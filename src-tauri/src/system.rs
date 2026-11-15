//! Schmale Schicht ueber Windows: etwas oeffnen, Medientasten druecken, Fenster schliessen,
//! herunterfahren. Es werden nur normale, dokumentierte Windows-Funktionen benutzt.

use std::process::Command;

use anyhow::{anyhow, bail, Result};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Mute,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerAction {
    Lock,
    /// Bricht ein angekuendigtes Herunterfahren oder Neustarten ab.
    Abort,
}

/// Sekunden zwischen Bestaetigung und tatsaechlichem Herunterfahren; in dieser Zeit ist Abbrechen moeglich.
pub const POWER_DELAY_SECONDS: u32 = 30;

fn hidden(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn power(action: PowerAction) -> Result<()> {
    power_in(action, POWER_DELAY_SECONDS)
}

/// Sperren oder ein bei Windows angekuendigtes Herunterfahren abbrechen. Herunterfahren selbst
/// plant Milo in power.rs und loest es erst am Ende mit `power_now` aus.
pub fn power_in(action: PowerAction, _seconds: u32) -> Result<()> {
    let status = match action {
        PowerAction::Abort => hidden("shutdown").arg("/a").status()?,
        PowerAction::Lock => hidden("rundll32.exe").arg("user32.dll,LockWorkStation").status()?,
    };
    if !status.success() {
        bail!("{}", tr!(format!("Windows hat den Befehl abgelehnt (Code {}).", status.code().unwrap_or(-1)), format!("Windows rejected the command (Code {}).", status.code().unwrap_or(-1))));
    }
    Ok(())
}

/// Faehrt sofort herunter (oder startet neu), ohne Zwang: Programme duerfen noch nachfragen.
/// So gibt es keine Windows-Meldung „Sie werden in Kuerze abgemeldet“ – den Countdown zeigt Milo.
pub fn power_now(restart: bool) -> Result<()> {
    let status = hidden("shutdown").args([if restart { "/r" } else { "/s" }, "/t", "0"]).status()?;
    if !status.success() {
        bail!("{}", tr!(format!("Windows hat den Befehl abgelehnt (Code {}).", status.code().unwrap_or(-1)), format!("Windows rejected the command (Code {}).", status.code().unwrap_or(-1))));
    }
    Ok(())
}

/// Faehrt sofort und mit Zwang herunter (oder startet neu): offene Programme halten es nicht auf.
/// Nur fuer den Waechter, wenn ein geplantes Herunterfahren nicht von selbst passiert ist.
pub fn force_power(restart: bool) -> Result<()> {
    let status = hidden("shutdown").args([if restart { "/r" } else { "/s" }, "/f", "/t", "0"]).status()?;
    if !status.success() {
        bail!("{}", tr!(format!("Windows hat den Befehl abgelehnt (Code {}).", status.code().unwrap_or(-1)), format!("Windows rejected the command (Code {}).", status.code().unwrap_or(-1))));
    }
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_MEDIA_NEXT_TRACK,
        VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
    };
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GWL_EXSTYLE,
        GW_OWNER, SW_SHOWNORMAL, WM_CLOSE, WS_EX_TOOLWINDOW,
    };
    use windows::core::BOOL;

    pub fn shell_open(target: &str) -> Result<()> {
        let target = HSTRING::from(target);
        let result = unsafe { ShellExecuteW(None, windows::core::w!("open"), &target, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
        // Laut Windows-Dokumentation bedeutet ein Wert ueber 32 Erfolg.
        if result.0 as isize > 32 {
            Ok(())
        } else {
            Err(anyhow!("{}", tr!(format!("Windows konnte das nicht öffnen (Code {}).", result.0 as isize), format!("Windows couldn't open that (Code {}).", result.0 as isize))))
        }
    }

    pub fn press(key: MediaKey) -> Result<()> {
        let vk: VIRTUAL_KEY = match key {
            MediaKey::PlayPause => VK_MEDIA_PLAY_PAUSE,
            MediaKey::Next => VK_MEDIA_NEXT_TRACK,
            MediaKey::Previous => VK_MEDIA_PREV_TRACK,
            MediaKey::VolumeUp => VK_VOLUME_UP,
            MediaKey::VolumeDown => VK_VOLUME_DOWN,
            MediaKey::Mute => VK_VOLUME_MUTE,
        };
        let key_event = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        };
        let inputs = [key_event(KEYBD_EVENT_FLAGS(0)), key_event(KEYEVENTF_KEYUP)];
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() { Ok(()) } else { Err(anyhow!(tr!("Der Tastendruck wurde von Windows blockiert.", "Windows blocked the key press."))) }
    }

    /// Sichtbare Hauptfenster mit Titel und Programmname (ohne Endung).
    pub fn windows() -> Vec<AppWindow> {
        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let list = unsafe { &mut *(lparam.0 as *mut Vec<AppWindow>) };
            unsafe {
                let tool = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0;
                let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid());
                if !IsWindowVisible(hwnd).as_bool() || tool || owned {
                    return BOOL(1);
                }
                let mut title = [0u16; 512];
                let len = GetWindowTextW(hwnd, &mut title);
                if len <= 0 {
                    return BOOL(1);
                }
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                list.push(AppWindow {
                    handle: hwnd.0 as isize,
                    title: String::from_utf16_lossy(&title[..len as usize]),
                    process: process_name(pid).unwrap_or_default(),
                    pid,
                });
            }
            BOOL(1)
        }
        let mut list: Vec<AppWindow> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut _ as isize));
        }
        list
    }

    pub fn process_name(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 1024];
            let mut len = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), windows::core::PWSTR(buffer.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(handle);
            if !ok {
                return None;
            }
            let path = String::from_utf16_lossy(&buffer[..len as usize]);
            std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned())
        }
    }

    /// Bittet ein Fenster hoeflich, sich zu schliessen (wie ein Klick auf das X); ungespeicherte
    /// Arbeit kann das Programm dadurch noch abfragen.
    pub fn close(handle: isize) -> Result<()> {
        unsafe { PostMessageW(Some(HWND(handle as *mut core::ffi::c_void)), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .map_err(|e| anyhow!("{}", tr!(format!("Das Fenster ließ sich nicht schließen: {e}"), format!("The window couldn't be closed: {e}"))))
    }
}

#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn shell_open(_target: &str) -> Result<()> {
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
    pub fn press(_key: MediaKey) -> Result<()> {
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
    pub fn windows() -> Vec<AppWindow> {
        Vec::new()
    }
    pub fn close(_handle: isize) -> Result<()> {
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
}

#[derive(Debug, Clone)]
pub struct AppWindow {
    pub handle: isize,
    pub title: String,
    /// Name der Programmdatei ohne Endung, z. B. "Spotify".
    pub process: String,
    pub pid: u32,
}

/// Oeffnet eine Datei, einen Ordner, eine Adresse oder einen Startmenue-Eintrag mit dem Standardprogramm.
pub fn shell_open(target: &str) -> Result<()> {
    native::shell_open(target)
}

pub fn press(key: MediaKey) -> Result<()> {
    native::press(key)
}

pub fn windows() -> Vec<AppWindow> {
    native::windows()
}

pub fn close_window(handle: isize) -> Result<()> {
    native::close(handle)
}

/// Macht aus einer Eingabe eine sichere Webadresse: nur http/https, fehlendes Schema wird ergaenzt.
pub fn web_url(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() || input.contains(char::is_whitespace) {
        bail!(tr!("Das ist keine gültige Webadresse.", "That's not a valid web address."));
    }
    let candidate = if input.contains("://") { input.to_string() } else { format!("https://{input}") };
    let url = url::Url::parse(&candidate).map_err(|_| anyhow!(tr!("Das ist keine gültige Webadresse.", "That's not a valid web address.")))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!(tr!("Ich öffne nur Webadressen (http oder https).", "I only open web addresses (http or https)."));
    }
    if !url.host_str().is_some_and(|h| h.contains('.') || h == "localhost") {
        bail!(tr!("Das ist keine gültige Webadresse.", "That's not a valid web address."));
    }
    Ok(url.into())
}

/// Baut die Adresse einer Suche auf einer bekannten Seite.
pub fn search_url(query: &str, site: &str) -> String {
    let base = match site {
        "youtube" => "https://www.youtube.com/results?search_query=",
        "wikipedia" => "https://de.wikipedia.org/w/index.php?search=",
        "bilder" => "https://www.google.com/search?tbm=isch&q=",
        "maps" => "https://www.google.com/maps/search/",
        "twitch" => "https://www.twitch.tv/search?term=",
        _ => "https://www.google.com/search?q=",
    };
    let encoded: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{base}{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_url_adds_scheme_and_rejects_dangerous_input() {
        assert_eq!(web_url("netflix.com").unwrap(), "https://netflix.com/");
        assert_eq!(web_url(" https://www.youtube.com/watch?v=1 ").unwrap(), "https://www.youtube.com/watch?v=1");
        assert!(web_url("file:///C:/Windows/System32/cmd.exe").is_err());
        assert!(web_url("javascript:alert(1)").is_err());
        assert!(web_url("ms-settings://x").is_err());
        assert!(web_url("nur ein satz").is_err());
        assert!(web_url("youtube").is_err());
        assert!(web_url("").is_err());
    }

    #[test]
    fn search_url_encodes_query() {
        assert_eq!(search_url("Lofi Musik & mehr", "youtube"), "https://www.youtube.com/results?search_query=Lofi+Musik+%26+mehr");
        assert_eq!(search_url("Wetter Köln", "google"), "https://www.google.com/search?q=Wetter+K%C3%B6ln");
        assert!(search_url("x", "unbekannt").starts_with("https://www.google.com/search?q="));
    }
}
