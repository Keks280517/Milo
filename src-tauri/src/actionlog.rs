//! Aktionsprotokoll: jeder Werkzeugaufruf von Milo landet mit Angaben, Dauer und Ergebnis in der
//! Tabelle `actions` (siehe `memory.rs`). Wo es sicher geht, merkt sich der Eintrag, wie er sich
//! rueckgaengig machen laesst: Lautstaerke zurueck, geplantes Herunterfahren abbrechen, verschobene
//! Datei zuruecklegen, kopierte Datei in den Papierkorb.
//!
//! Angaben zu Passwort-Werkzeugen und alles, was nach Passwort oder Schluessel aussieht, wird nie
//! gespeichert.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::memory::{Action, Memory};
use crate::tools::Outcome;

const DETAIL_CHARS: usize = 240;

/// Werkzeuge, deren Angaben nie ins Protokoll kommen.
fn secret_tool(name: &str) -> bool {
    name.contains("password") || name.contains("vault")
}

fn secret_key(key: &str) -> bool {
    let key = key.to_lowercase();
    ["passw", "secret", "token", "key", "pin", "kennwort"].iter().any(|s| key.contains(s))
}

/// Kurzform der Aufrufangaben, z. B. `percent: 30 · screen: 2`.
pub fn detail_of(tool: &str, args: &Value) -> String {
    if secret_tool(tool) {
        return String::new();
    }
    let Some(map) = args.as_object() else { return String::new() };
    let text = map
        .iter()
        .filter(|(k, _)| !secret_key(k))
        .map(|(k, v)| {
            let value = match v {
                Value::String(s) => s.lines().next().unwrap_or("").to_string(),
                other => other.to_string(),
            };
            format!("{k}: {value}")
        })
        .collect::<Vec<_>>()
        .join(" · ");
    if text.chars().count() > DETAIL_CHARS { format!("{}…", text.chars().take(DETAIL_CHARS).collect::<String>()) } else { text }
}

/// Traegt einen Werkzeugaufruf ins Protokoll ein.
pub fn record(app: &AppHandle, tool: &str, args: &Value, outcome: &Outcome, took: Duration) {
    let undo = if outcome.ok { outcome.undo.as_ref().map(Value::to_string) } else { None };
    app.state::<Memory>().log_entry(tool, &outcome.summary, outcome.ok, &detail_of(tool, args), took.as_millis() as i64, undo.as_deref());
}

#[tauri::command]
pub fn action_log(memory: State<'_, Memory>, limit: Option<usize>) -> Result<Vec<Action>, String> {
    memory.recent_actions(limit.unwrap_or(300).min(1000)).map_err(|e| e.to_string())
}

/// Macht einen Eintrag rueckgaengig. Nur auf Klick des Nutzers.
#[tauri::command]
pub async fn undo_action(app: AppHandle, id: i64) -> Result<String, String> {
    let memory = app.state::<Memory>();
    let (undo, summary) = memory.undo_of(id).map_err(|e| e.to_string())?.ok_or(tr!("Das lässt sich nicht (mehr) rückgängig machen.", "That can't be undone (anymore)."))?;
    let undo: Value = serde_json::from_str(&undo).map_err(|e| e.to_string())?;
    let result = run_undo(&app, &undo).await;
    let ok = result.is_ok();
    let text = match &result {
        Ok(text) => text.clone(),
        Err(err) => tr!(format!("Rückgängig fehlgeschlagen: {err}"), format!("Undo failed: {err}")),
    };
    memory.log_entry("undo", &tr!(format!("Rückgängig: {summary}"), format!("Undo: {summary}")), ok, &text, 0, None);
    if ok {
        memory.mark_undone(id).map_err(|e| e.to_string())?;
    }
    result.map_err(|e| e.to_string())
}

async fn run_undo(app: &AppHandle, undo: &Value) -> Result<String> {
    match undo["kind"].as_str().unwrap_or("") {
        "volume" => {
            let percent = undo["percent"].as_u64().unwrap_or(50).min(100) as u32;
            tokio::task::spawn_blocking(move || crate::pc::set_volume(percent)).await??;
            Ok(tr!(format!("Lautstärke wieder auf {percent} %."), format!("Volume back to {percent} %.")))
        }
        "power_cancel" => match crate::power::cancel(app) {
            Some(_) => Ok(tr!("Geplanter Befehl abgebrochen.", "Scheduled command cancelled.").into()),
            None => bail!(tr!("Es ist nichts mehr geplant.", "Nothing is scheduled anymore.")),
        },
        "move" => {
            let (from, to) = (PathBuf::from(text(undo, "from")), PathBuf::from(text(undo, "to")));
            if !to.exists() {
                bail!("{}", tr!(format!("Die Datei liegt nicht mehr in {}.", to.display()), format!("The file isn't in {} anymore.", to.display())));
            }
            if from.exists() {
                bail!(tr!("Am alten Ort liegt inzwischen schon eine Datei gleichen Namens.", "A file with the same name is now in the old location."));
            }
            crate::browse::move_path(&to, &from)?;
            Ok(tr!(format!("Zurück nach {}.", from.parent().map(|p| p.display().to_string()).unwrap_or_default()), format!("Back to {}.", from.parent().map(|p| p.display().to_string()).unwrap_or_default())))
        }
        "copy" => {
            let path = PathBuf::from(text(undo, "path"));
            if !path.exists() {
                bail!(tr!("Die Kopie gibt es nicht mehr.", "The copy doesn't exist anymore."));
            }
            to_recycle_bin(&path)?;
            Ok(tr!("Kopie in den Papierkorb gelegt.", "Copy moved to the Recycle Bin.").into())
        }
        other => bail!("{}", tr!(format!("Unbekannte Art „{other}“."), format!("Unknown kind \"{other}\"."))),
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").to_string()
}

/// Legt eine Datei oder einen Ordner in den Papierkorb (wiederherstellbar, nichts wird endgueltig geloescht).
#[cfg(windows)]
pub fn to_recycle_bin(path: &Path) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW};
    // Doppelt nullterminierte Liste mit genau einem Pfad.
    let mut from: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
    from.extend([0, 0]);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    // SAFETY: `from` lebt bis nach dem Aufruf; die Struktur ist vollstaendig initialisiert.
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted.as_bool() {
        bail!("{}", tr!(format!("Windows hat das Verschieben in den Papierkorb abgelehnt (Code {code})."), format!("Windows refused to move it to the Recycle Bin (Code {code}).")));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn to_recycle_bin(_path: &Path) -> Result<()> {
    bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn details_are_short_and_never_secret() {
        assert_eq!(detail_of("set_volume", &json!({ "percent": 30 })), "percent: 30");
        assert_eq!(detail_of("password_fill", &json!({ "site": "bank" })), "");
        assert_eq!(detail_of("browser_type", &json!({ "text": "hallo", "password": "geheim" })), "text: hallo");
        let long = detail_of("run_script", &json!({ "code": "x".repeat(500) }));
        assert!(long.chars().count() <= DETAIL_CHARS + 1 && long.ends_with('…'));
        assert_eq!(detail_of("run_script", &json!({ "code": "zeile1\nzeile2" })), "code: zeile1");
    }
}
