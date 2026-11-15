//! Starten mit Windows und vollstaendiges Entfernen.
//!
//! Autostart: eine Windows-Aufgabe bei der Anmeldung (keine Admin-Rechte, startet ohne die Wartezeit
//! der "Run"-Eintraege). Klappt sie nicht, ein Eintrag unter "Run" im Benutzerteil der Registrierung.
//! Entfernen: Milo kann seine eigenen Ordner nicht loeschen, solange es laeuft (Datenbank und
//! Browser-Daten sind geoeffnet). Deshalb schreibt es ein kleines Aufraeumskript in den
//! Temp-Ordner, beendet sich, und das Skript loescht danach Milos Daten, den Autostart-Eintrag
//! und startet das Deinstallationsprogramm, falls Milo installiert wurde.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use tauri::{AppHandle, Manager};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "Milo";
/// Name des Autostart-Eintrags vor der Umbenennung (Kubo -> Milo).
const LEGACY_RUN_NAME: &str = "Kubo";
/// Name der Windows-Aufgabe, die Milo bei der Anmeldung startet.
const TASK_NAME: &str = "Milo";
/// Nur Ordner mit diesem Namen werden je geloescht.
const APP_FOLDER: &str = "de.milo.assistant";

fn hidden(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Startet Milo mit Windows? (Aufgabe bei Anmeldung, aeltere Fassungen: Eintrag unter "Run")
pub fn autostart_enabled() -> bool {
    task_exists() || run_entry_exists(RUN_NAME)
}

fn run_entry_exists(name: &str) -> bool {
    hidden("reg").args(["query", RUN_KEY, "/v", name]).output().is_ok_and(|out| out.status.success())
}

fn task_exists() -> bool {
    hidden("schtasks").args(["/query", "/tn", TASK_NAME]).output().is_ok_and(|out| out.status.success())
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Aufgabe fuer die Anmeldung dieses Benutzers. Anders als ein "Run"-Eintrag wartet sie nicht,
/// bis Windows die uebrigen Autostart-Programme gestartet hat. Normale Prioritaet (Aufgaben laufen
/// sonst gedrosselt), kein Zeitlimit, auch im Akkubetrieb, nie doppelt.
pub fn task_xml(user: &str, exe: &Path) -> String {
    let user = xml_escape(user);
    let exe = xml_escape(&exe.display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Startet Milo bei der Anmeldung.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <Priority>4</Priority>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command></Exec></Actions>
</Task>
"#
    )
}

/// Legt die Anmelde-Aufgabe an (ersetzt eine vorhandene).
fn create_task(exe: &Path) -> Result<()> {
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) if !domain.is_empty() => format!("{domain}\\{name}"),
        (_, Ok(name)) => name,
        _ => bail!("Benutzername unbekannt"),
    };
    let file = std::env::temp_dir().join(format!("milo-autostart-{}.xml", std::process::id()));
    // schtasks erwartet UTF-16 mit BOM.
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(task_xml(&user, exe).encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&file, bytes)?;
    let out = hidden("schtasks").args(["/create", "/tn", TASK_NAME, "/xml"]).arg(&file).arg("/f").output();
    let _ = std::fs::remove_file(&file);
    let out = out?;
    if !out.status.success() {
        bail!("schtasks: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

pub fn set_autostart(enable: bool) -> Result<()> {
    if enable {
        let exe = std::env::current_exe().context(tr!("Programmpfad unbekannt", "Program path unknown"))?;
        match create_task(&exe) {
            Ok(()) => {
                // Sonst wuerde Windows Milo ueber den alten Weg ein zweites Mal (verspaetet) starten wollen.
                if run_entry_exists(RUN_NAME) {
                    let _ = hidden("reg").args(["delete", RUN_KEY, "/v", RUN_NAME, "/f"]).status();
                }
                return Ok(());
            }
            // Klappt die Aufgabe nicht, bleibt es beim "Run"-Eintrag.
            Err(err) => log::warn!("Anmelde-Aufgabe nicht angelegt, nehme den Run-Eintrag: {err}"),
        }
        let status = hidden("reg").args(["add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &format!("\"{}\"", exe.display()), "/f"]).status()?;
        if !status.success() {
            bail!(tr!("Der Autostart-Eintrag ließ sich nicht ändern.", "The autostart entry couldn't be changed."));
        }
        return Ok(());
    }
    let mut ok = true;
    if task_exists() {
        ok &= hidden("schtasks").args(["/delete", "/tn", TASK_NAME, "/f"]).status().is_ok_and(|s| s.success());
    }
    if run_entry_exists(RUN_NAME) {
        ok &= hidden("reg").args(["delete", RUN_KEY, "/v", RUN_NAME, "/f"]).status().is_ok_and(|s| s.success());
    }
    if !ok {
        bail!(tr!("Der Autostart-Eintrag ließ sich nicht ändern.", "The autostart entry couldn't be changed."));
    }
    Ok(())
}

/// Bringt einen vorhandenen Autostart auf den aktuellen Stand: alter Eintrag „Kubo“, ein "Run"-Eintrag
/// (startet spaet, nach allen anderen Programmen) oder eine Aufgabe, die auf ein anderes Programm zeigt.
pub fn refresh_autostart() {
    // Nur die installierte Fassung, nie ein Entwicklungs-Build.
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let legacy = run_entry_exists(LEGACY_RUN_NAME);
    let run = run_entry_exists(RUN_NAME);
    let task = task_exists();
    let task_points_here = task
        && hidden("schtasks")
            .args(["/query", "/tn", TASK_NAME, "/xml"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains(&xml_escape(&exe.display().to_string()).to_lowercase()));
    // Kein Autostart gewuenscht, oder schon richtig eingerichtet.
    if !legacy && !run && (!task || task_points_here) {
        return;
    }
    match set_autostart(true) {
        Ok(()) => {
            if legacy {
                let _ = hidden("reg").args(["delete", RUN_KEY, "/v", LEGACY_RUN_NAME, "/f"]).status();
            }
            log::info!("Autostart auf {} umgestellt (Aufgabe bei Anmeldung: {})", exe.display(), task_exists());
        }
        Err(err) => log::warn!("Autostart nicht umgestellt: {err}"),
    }
}

/// Ein Ordner darf nur geloescht werden, wenn er eindeutig Milos eigener Datenordner ist.
fn is_own_folder(path: &Path) -> bool {
    path.is_absolute() && path.file_name().is_some_and(|name| name == APP_FOLDER) && path.components().count() >= 4
}

/// Text des Aufraeumskripts. `folders` werden geloescht, `uninstaller` danach gestartet.
pub fn cleanup_script(pid: u32, folders: &[PathBuf], uninstaller: Option<&Path>) -> String {
    let mut lines = vec![
        "@echo off".to_string(),
        "rem Entfernt Milo, nachdem es sich beendet hat. Von Milo selbst erzeugt.".to_string(),
        ":warten".to_string(),
        format!("tasklist /FI \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul && (timeout /t 1 /nobreak >nul & goto warten)"),
        format!("reg delete \"{RUN_KEY}\" /v {RUN_NAME} /f >nul 2>&1"),
        format!("reg delete \"{RUN_KEY}\" /v {LEGACY_RUN_NAME} /f >nul 2>&1"),
        format!("schtasks /delete /tn \"{TASK_NAME}\" /f >nul 2>&1"),
    ];
    for folder in folders.iter().filter(|f| is_own_folder(f)) {
        lines.push(format!("rmdir /s /q \"{}\"", folder.display()));
    }
    if let Some(uninstaller) = uninstaller {
        lines.push(format!("start \"\" \"{}\" /S", uninstaller.display()));
    }
    lines.push("del \"%~f0\"".to_string());
    lines.join("\r\n") + "\r\n"
}

/// Entfernt Milo vollstaendig: Daten, Autostart, Verbindung zu Claude, den Gemini-Schluessel
/// und die Installation selbst, falls es eine gibt. Beendet danach die App.
pub fn wipe(app: &AppHandle) -> Result<()> {
    // Solange Milo noch laeuft: Claudes Einstellungen wieder bereinigen.
    if crate::claude::hooks_installed() {
        crate::claude::set_hooks(None)?;
    }
    // Milos Skripte und Meldungen fuer Claude (eigener Unterordner, sonst nichts in .claude).
    let _ = std::fs::remove_dir_all(crate::claude::milo_dir());
    // Der Gemini-Schluessel liegt in der Windows-Anmeldeinformationsverwaltung, nicht in Milos Ordnern.
    crate::gemini::delete_key();
    let folders: Vec<PathBuf> = [app.path().app_data_dir(), app.path().app_local_data_dir(), app.path().app_config_dir(), app.path().app_log_dir()]
        .into_iter()
        .flatten()
        // Der Protokollordner liegt unterhalb des Datenordners; es zaehlt der Ordner mit Milos Namen.
        .map(|p| if p.file_name().is_some_and(|n| n == APP_FOLDER) { p } else { p.parent().map(Path::to_path_buf).unwrap_or(p) })
        .filter(|p| is_own_folder(p))
        .fold(Vec::new(), |mut list, p| {
            if !list.contains(&p) {
                list.push(p);
            }
            list
        });
    // Installiert liegt neben der Programmdatei ein Deinstallationsprogramm; im Entwicklungsmodus nicht.
    let uninstaller = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join("uninstall.exe"))).filter(|p| p.is_file());
    let script = std::env::temp_dir().join(format!("milo-entfernen-{}.cmd", std::process::id()));
    std::fs::write(&script, cleanup_script(std::process::id(), &folders, uninstaller.as_deref()))?;
    hidden("cmd").args(["/c", "start", "", "/min", &script.to_string_lossy()]).spawn().context(tr!("Das Aufräumskript ließ sich nicht starten", "The cleanup script couldn't be started"))?;
    log::info!("Milo wird entfernt: {} Ordner", folders.len());
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_milos_own_folders_are_ever_deleted() {
        assert!(is_own_folder(Path::new(r"C:\Users\x\AppData\Roaming\de.milo.assistant")));
        assert!(!is_own_folder(Path::new(r"C:\Users\x\AppData\Roaming")));
        assert!(!is_own_folder(Path::new(r"C:\de.milo.assistant")));
        assert!(!is_own_folder(Path::new(r"de.milo.assistant")));
        assert!(!is_own_folder(Path::new(r"C:\Users\x\Documents")));
    }

    #[test]
    fn cleanup_script_waits_then_removes_only_safe_things() {
        let folders = vec![PathBuf::from(r"C:\Users\x\AppData\Roaming\de.milo.assistant"), PathBuf::from(r"C:\Users\x\Documents")];
        let script = cleanup_script(4711, &folders, Some(Path::new(r"C:\Programme\Milo\uninstall.exe")));
        assert!(script.contains("PID eq 4711"));
        assert!(script.contains(r#"rmdir /s /q "C:\Users\x\AppData\Roaming\de.milo.assistant""#));
        assert!(!script.contains("Documents"));
        assert!(script.contains("uninstall.exe\" /S"));
        assert!(script.contains("schtasks /delete /tn \"Milo\""));
        // Warten kommt vor dem Loeschen.
        assert!(script.find(":warten").unwrap() < script.find("rmdir").unwrap());
    }

    #[test]
    fn logon_task_starts_milo_promptly_and_never_stops_it() {
        let xml = task_xml(r"PC\Anna & Co", Path::new(r"C:\Users\x\AppData\Local\Milo\milo.exe"));
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains(r"<UserId>PC\Anna &amp; Co</UserId>"));
        assert!(xml.contains(r"<Command>C:\Users\x\AppData\Local\Milo\milo.exe</Command>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<Priority>4</Priority>"));
        assert!(!xml.contains("<Delay>"));
    }
}
