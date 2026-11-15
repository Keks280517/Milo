//! Einmaliger Umzug von „Kubo“ auf „Milo“.
//!
//! Bis Version 0.4 lagen Milos Daten unter dem alten Namen (Ordner `de.kubo.assistant`, Datenbank
//! `kubo.db`, Schluessel „Kubo/…“, Dokumente\Kubo …). Beim ersten Start der umbenannten Fassung
//! zieht alles unter den neuen Namen um, bevor irgendein Fenster oder Dienst startet. Jeder Schritt
//! ist fuer sich abgesichert: was schon umgezogen ist oder fehlt, wird uebersprungen, und nichts
//! wird ueberschrieben. Nur in der installierten Fassung, nie in einem Entwicklungs-Build.
//!
//! Dies ist die einzige Stelle im Code, die den alten Namen noch kennt.

use std::path::{Path, PathBuf};

const OLD_ID: &str = "de.kubo.assistant";
const NEW_ID: &str = "de.milo.assistant";
const OLD_EXE: &str = "kubo.exe";
const OLD_DB: &str = "kubo.db";
const NEW_DB: &str = "milo.db";
const OLD_FOLDER: &str = "Kubo";
const NEW_FOLDER: &str = "Milo";
/// Schluessel in der Windows-Anmeldeinformationsverwaltung: alt -> neu.
const CREDENTIALS: [(&str, &str); 2] =
    [("Kubo/Gemini-API-Schluessel", "Milo/Gemini-API-Schluessel"), ("Kubo/Groq-API-Schluessel", "Milo/Groq-API-Schluessel")];
/// Name des alten Native-Messaging-Hosts der Browser-Erweiterung.
const OLD_HOST: &str = "de.kubo.milo";
/// Alter Ordner fuer die Claude-Verbindung unter `%USERPROFILE%\.claude`.
pub const OLD_CLAUDE_DIR: &str = "kubo";
const OLD_CLAUDE_BACKUP: &str = "settings.json.kubo-sicherung";
const NEW_CLAUDE_BACKUP: &str = "settings.json.milo-sicherung";

fn hidden(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Verschiebt `old` nach `new`. Gibt es `new` schon, wandert nur hinein, was dort noch fehlt;
/// ein danach leerer alter Ordner wird entfernt.
fn move_dir(old: &Path, new: &Path) {
    if !old.is_dir() {
        return;
    }
    if !new.exists() {
        match std::fs::rename(old, new) {
            Ok(()) => log::info!("Umzug: {} -> {}", old.display(), new.display()),
            Err(err) => log::warn!("Umzug {} fehlgeschlagen: {err}", old.display()),
        }
        return;
    }
    if let Ok(entries) = std::fs::read_dir(old) {
        for entry in entries.flatten() {
            let target = new.join(entry.file_name());
            if !target.exists() {
                if let Err(err) = std::fs::rename(entry.path(), &target) {
                    log::warn!("Umzug {} fehlgeschlagen: {err}", entry.path().display());
                }
            }
        }
    }
    let _ = std::fs::remove_dir(old);
}

fn move_file(old: &Path, new: &Path) {
    if old.is_file() && !new.exists() {
        if let Err(err) = std::fs::rename(old, new) {
            log::warn!("Umzug {} fehlgeschlagen: {err}", old.display());
        }
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_dir())
}

/// Alles vom alten Namen auf den neuen umziehen. Laeuft vor dem Start von Tauri.
pub fn run() {
    if cfg!(debug_assertions) {
        return;
    }
    // Eine noch laufende alte Fassung haelt Datenbank und Browser-Daten offen.
    let own = std::env::current_exe().ok();
    if own.as_ref().and_then(|p| p.file_name()).is_none_or(|n| !n.eq_ignore_ascii_case(OLD_EXE)) {
        let _ = hidden("taskkill").args(["/F", "/IM", OLD_EXE]).status();
        std::thread::sleep(std::time::Duration::from_millis(500));
        // Uebrig gebliebene alte Programmdatei neben der neuen (Installation ueber die alte).
        if let Some(stale) = own.as_ref().and_then(|p| p.parent()).map(|d| d.join(OLD_EXE)) {
            let _ = std::fs::remove_file(stale);
        }
    }

    for var in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(base) = env_dir(var) {
            move_dir(&base.join(OLD_ID), &base.join(NEW_ID));
        }
    }
    if let Some(data) = env_dir("APPDATA").map(|b| b.join(NEW_ID)) {
        for suffix in ["", "-wal", "-shm"] {
            move_file(&data.join(format!("{OLD_DB}{suffix}")), &data.join(format!("{NEW_DB}{suffix}")));
        }
    }

    for (old, new) in CREDENTIALS {
        if let Some(secret) = crate::gemini::vault::read(old) {
            if crate::gemini::vault::read(new).is_some() || crate::gemini::vault::write(new, &secret).is_ok() {
                crate::gemini::vault::delete(old);
                log::info!("Schlüssel {old} umgezogen");
            }
        }
    }

    for folder in ["dokumente", "bilder"] {
        if let Some(base) = crate::pc::known_folder(folder) {
            move_dir(&base.join(OLD_FOLDER), &base.join(NEW_FOLDER));
        }
    }

    #[cfg(windows)]
    for browser in [r"Google\Chrome", r"Microsoft\Edge", r"BraveSoftware\Brave-Browser", r"Chromium"] {
        let key = format!(r"HKCU\Software\{browser}\NativeMessagingHosts\{OLD_HOST}");
        let _ = hidden("reg").args(["delete", &key, "/f"]).status();
    }

    if let Some(claude) = env_dir("USERPROFILE").map(|h| h.join(".claude")) {
        move_file(&claude.join(OLD_CLAUDE_BACKUP), &claude.join(NEW_CLAUDE_BACKUP));
    }
}

/// Der alte Ordner fuer die Claude-Verbindung. Er wird erst geloescht, wenn Claudes Einstellungen
/// nicht mehr darauf zeigen und keine Claude-Sitzung mehr laeuft (die liest ihre Hooks nur beim Start).
pub fn old_claude_dir() -> Option<PathBuf> {
    env_dir("USERPROFILE").map(|h| h.join(".claude").join(OLD_CLAUDE_DIR)).filter(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_merges_without_overwriting() {
        let base = std::env::temp_dir().join(format!("milo-umzug-{}", std::process::id()));
        let (old, new) = (base.join("alt"), base.join("neu"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("a.txt"), "alt").unwrap();
        std::fs::write(old.join("b.txt"), "alt").unwrap();
        std::fs::write(new.join("b.txt"), "neu").unwrap();
        move_dir(&old, &new);
        assert_eq!(std::fs::read_to_string(new.join("a.txt")).unwrap(), "alt");
        assert_eq!(std::fs::read_to_string(new.join("b.txt")).unwrap(), "neu");
        // b.txt blieb im alten Ordner liegen, deshalb existiert er noch.
        assert!(old.join("b.txt").exists());
        let fresh = base.join("frisch");
        move_dir(&old, &fresh);
        assert!(fresh.join("b.txt").exists() && !old.exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
