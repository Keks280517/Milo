//! Text-Werkzeuge: markierten Text in jedem Programm per Tastenkombination umschreiben lassen
//! (korrigieren, formeller, kuerzer, uebersetzen ...).
//!
//! Ablauf: Milo merkt sich das Fenster, kopiert die Markierung (Strg+C) und zeigt ein kleines
//! Menue. Nach der Wahl bearbeitet das schnelle Gemini-Modell den Text, Milo fuegt das Ergebnis
//! an derselben Stelle ein (Strg+V) und legt danach die alte Zwischenablage zurueck. Ist nichts
//! markiert, wird der Inhalt der Zwischenablage bearbeitet und das Ergebnis nur hineingelegt.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::gemini::Thinking;
use crate::limits::Tier;

pub const EVENT: &str = "milo://textwork";
/// Laengere Texte schneidet Milo ab, damit die Anfrage schnell bleibt.
const MAX_CHARS: usize = 12_000;

/// Was gerade bearbeitet wird.
#[derive(Default)]
struct Job {
    /// Fenster, aus dem der Text kam (als Zahl, damit der Zustand threadsicher bleibt).
    window: isize,
    text: String,
    /// Der Text kam aus einer Markierung (dann wird das Ergebnis dort eingefuegt).
    from_selection: bool,
    /// Inhalt der Zwischenablage vor dem Kopieren, wird danach zurueckgelegt.
    previous: Option<String>,
}

#[derive(Default)]
pub struct TextWork {
    job: Mutex<Option<Job>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub text: String,
    pub from_selection: bool,
    pub chars: usize,
}

/// Anweisung je Aktion. `None`: unbekannte Aktion.
pub fn instruction(action: &str) -> Option<&'static str> {
    Some(match action {
        "fix" => tr!("Korrigiere Rechtschreibung, Grammatik und Zeichensetzung. Ändere sonst nichts am Inhalt oder Stil.", "Fix spelling, grammar and punctuation. Change nothing else about the content or style. Keep the text in its original language."),
        "formal" => tr!("Formuliere den Text höflich und förmlich (Sie-Form, sachlich), ohne Inhalt hinzuzufügen oder wegzulassen.", "Rewrite the text politely and formally, without adding or leaving out content. Keep the text in its original language."),
        "casual" => tr!("Formuliere den Text locker und freundlich (Du-Form), ohne Inhalt hinzuzufügen oder wegzulassen.", "Rewrite the text in a relaxed, friendly tone, without adding or leaving out content. Keep the text in its original language."),
        "shorter" => tr!("Kürze den Text auf etwa die Hälfte. Alle wichtigen Aussagen bleiben erhalten.", "Shorten the text to about half. Keep all important points. Keep the text in its original language."),
        "english" => tr!("Übersetze den Text ins Englische. Natürlich klingend, gleiche Tonlage.", "Translate the text into English. Natural sounding, same tone."),
        "german" => tr!("Übersetze den Text ins Deutsche. Natürlich klingend, gleiche Tonlage.", "Translate the text into German. Natural sounding, same tone."),
        "summary" => tr!("Fasse den Text in wenigen kurzen Stichpunkten auf Deutsch zusammen.", "Summarize the text in a few short bullet points in English."),
        "explain" => tr!("Erkläre kurz und verständlich auf Deutsch, was dieser Text bedeutet oder worum es geht.", "Briefly and clearly explain in English what this text means or what it is about."),
        _ => return None,
    })
}

/// Bei diesen Aktionen ersetzt das Ergebnis den markierten Text; die anderen werden nur angezeigt.
pub fn replaces(action: &str) -> bool {
    matches!(action, "fix" | "formal" | "casual" | "shorter" | "english" | "german")
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

    /// Oeffnet die Zwischenablage; andere Programme halten sie manchmal kurz fest.
    fn open() -> Result<()> {
        for _ in 0..20 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        bail!(tr!("Die Zwischenablage ist gerade von einem anderen Programm belegt.", "Another app is using the clipboard right now."))
    }

    pub fn read() -> Option<String> {
        // Von einem Passwortmanager kopiert: nie lesen.
        if crate::guard::clipboard_protected() {
            return None;
        }
        open().ok()?;
        let text = unsafe {
            GetClipboardData(CF_UNICODETEXT.0 as u32).ok().and_then(|handle| {
                let memory = HGLOBAL(handle.0);
                let ptr = GlobalLock(memory) as *const u16;
                if ptr.is_null() {
                    return None;
                }
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                let _ = GlobalUnlock(memory);
                Some(text)
            })
        };
        unsafe {
            let _ = CloseClipboard();
        }
        text
    }

    pub fn write(text: &str) -> Result<()> {
        open()?;
        let result = (|| unsafe {
            EmptyClipboard()?;
            let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let memory = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2)?;
            let ptr = GlobalLock(memory) as *mut u16;
            if ptr.is_null() {
                bail!(tr!("Kein Speicher für die Zwischenablage.", "No memory for the clipboard."));
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len());
            let _ = GlobalUnlock(memory);
            // Gehoert danach der Zwischenablage, nicht mehr uns.
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0)))?;
            Ok(())
        })();
        unsafe {
            let _ = CloseClipboard();
        }
        result
    }

    pub fn clear() {
        if open().is_ok() {
            unsafe {
                let _ = EmptyClipboard();
                let _ = CloseClipboard();
            }
        }
    }

    fn held(vk: VIRTUAL_KEY) -> bool {
        (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
    }

    /// Wartet, bis der Nutzer die Tastenkombination losgelassen hat; sonst wird aus Strg+C z. B. Strg+Alt+C.
    pub fn wait_for_release() {
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(1500) && [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN].iter().any(|k| held(*k)) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Strg + Taste druecken (C = kopieren, V = einfuegen).
    pub fn ctrl(key: u8) {
        let event = |vk: VIRTUAL_KEY, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } },
        };
        let k = VIRTUAL_KEY(key as u16);
        let inputs = [event(VK_CONTROL, false), event(k, false), event(k, true), event(VK_CONTROL, true)];
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    }

    pub fn foreground() -> isize {
        unsafe { GetForegroundWindow() }.0 as isize
    }

    pub fn focus(window: isize) {
        if window != 0 {
            unsafe {
                let _ = SetForegroundWindow(HWND(window as *mut _));
            }
        }
    }
}

#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn read() -> Option<String> {
        None
    }
    pub fn write(_: &str) -> Result<()> {
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
    pub fn clear() {}
    pub fn wait_for_release() {}
    pub fn ctrl(_: u8) {}
    pub fn foreground() -> isize {
        0
    }
    pub fn focus(_: isize) {}
}

/// Liest die Zwischenablage ohne PowerShell (schnell).
pub fn clipboard_read() -> Option<String> {
    native::read()
}

/// Legt Text in die Zwischenablage, ohne PowerShell.
pub fn clipboard_write(text: &str) -> Result<()> {
    native::write(text)
}

/// Kopiert die Markierung des vorderen Fensters. Liefert Fenster, Text, Herkunft und alte Zwischenablage.
fn capture() -> Result<Job> {
    let window = native::foreground();
    let previous = native::read();
    native::wait_for_release();
    native::clear();
    native::ctrl(b'C');
    let started = Instant::now();
    let mut copied = None;
    while started.elapsed() < Duration::from_millis(700) {
        std::thread::sleep(Duration::from_millis(40));
        if let Some(text) = native::read().filter(|t| !t.trim().is_empty()) {
            copied = Some(text);
            break;
        }
    }
    match copied {
        Some(text) => Ok(Job { window, text, from_selection: true, previous }),
        None => {
            // Nichts markiert: alte Zwischenablage wiederherstellen und sie selbst bearbeiten.
            if let Some(old) = &previous {
                let _ = native::write(old);
            }
            let text = previous.clone().filter(|t| !t.trim().is_empty()).ok_or_else(|| anyhow!(tr!("Es ist nichts markiert und die Zwischenablage ist leer.", "Nothing is selected and the clipboard is empty.")))?;
            Ok(Job { window, text, from_selection: false, previous: None })
        }
    }
}

/// Tastenkombination gedrueckt: Text holen und das Menue zeigen.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let offer = match capture() {
            Ok(job) => {
                let text: String = job.text.chars().take(MAX_CHARS).collect();
                let offer = Offer { chars: job.text.chars().count(), text: text.chars().take(400).collect(), from_selection: job.from_selection };
                *app.state::<TextWork>().job.lock().unwrap() = Some(Job { text, ..job });
                Ok(offer)
            }
            Err(err) => Err(err.to_string()),
        };
        if let Some(win) = app.get_webview_window(crate::overlay::OVERLAY_LABEL) {
            let _ = win.show();
        }
        let _ = app.emit(EVENT, json!({ "offer": offer.as_ref().ok(), "error": offer.as_ref().err() }));
    });
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    pub result: String,
    /// Das Ergebnis wurde an Stelle der Markierung eingefuegt.
    pub pasted: bool,
    /// Das Ergebnis liegt in der Zwischenablage.
    pub copied: bool,
}

/// Bearbeitet den gemerkten Text mit der gewaehlten Aktion.
#[tauri::command]
pub async fn text_transform(app: AppHandle, work: State<'_, TextWork>, action: String) -> Result<Done, String> {
    let Some(instruction) = instruction(&action) else {
        return Err(tr!(format!("Die Aktion „{action}“ kenne ich nicht."), format!("I don't know the action \"{action}\".")));
    };
    let Some(job) = work.job.lock().unwrap().take() else {
        return Err(tr!("Es ist kein Text ausgewählt.", "No text is selected.").into());
    };
    let system = format!(
        "Du bearbeitest einen Text für den Nutzer. {instruction} Gib nur das Ergebnis aus: keine Einleitung, keine Anführungszeichen drumherum, \
keine Erklärung. Behalte Absätze und Zeilenumbrüche bei."
    );
    let result = crate::agent::ask_once(&app, Tier::Easy, &system, vec![json!({ "text": job.text })], Thinking::Low).await;
    let result = match result {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => {
            restore(&job);
            return Err(tr!("Gemini hat nichts zurückgegeben.", "Gemini returned nothing.").into());
        }
        Err(err) => {
            restore(&job);
            return Err(crate::agent::friendly(&err));
        }
    };
    app.state::<crate::memory::Memory>().log_action("cloud", &tr!(format!("Text-Werkzeug „{action}“: {} Zeichen an Gemini", job.text.chars().count()), format!("Text tool \"{action}\": {} characters to Gemini", job.text.chars().count())), true);
    let replace = replaces(&action) && job.from_selection;
    let text = result.clone();
    let done = tokio::task::spawn_blocking(move || -> Result<Done> {
        native::write(&text)?;
        if !replace {
            return Ok(Done { result: text, pasted: false, copied: true });
        }
        native::focus(job.window);
        std::thread::sleep(Duration::from_millis(180));
        native::ctrl(b'V');
        // Erst zurueklegen, wenn das Zielprogramm eingefuegt hat.
        std::thread::sleep(Duration::from_millis(500));
        restore(&job);
        Ok(Done { result: text, pasted: true, copied: false })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    Ok(done)
}

/// Bricht ab und legt die alte Zwischenablage zurueck.
#[tauri::command]
pub fn text_cancel(work: State<'_, TextWork>) {
    if let Some(job) = work.job.lock().unwrap().take() {
        restore(&job);
    }
}

fn restore(job: &Job) {
    if job.from_selection {
        match &job.previous {
            Some(old) => {
                let _ = native::write(old);
            }
            None => native::clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_an_instruction() {
        for action in ["fix", "formal", "casual", "shorter", "english", "german", "summary", "explain"] {
            assert!(instruction(action).is_some(), "{action}");
        }
        assert!(instruction("delete").is_none());
        assert!(replaces("fix") && !replaces("summary") && !replaces("explain"));
    }
}
