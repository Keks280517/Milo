//! Schutzregeln, damit Milos KI-Werkzeuge nie an Passwörter kommen:
//!
//! - Was ein Passwortmanager als geheim markiert in die Zwischenablage legt, wird nie gelesen.
//! - Skripte, die Prozessspeicher oder gespeicherte Anmeldedaten auslesen wollen, werden abgewiesen.

#[cfg(windows)]
mod native {
    use windows::Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW};

    pub fn clipboard_protected() -> bool {
        unsafe {
            ["ExcludeClipboardContentFromMonitorProcessing", "Clipboard Viewer Ignore"].iter().any(|name| {
                let w = windows::core::HSTRING::from(*name);
                let fmt = RegisterClipboardFormatW(&w);
                fmt != 0 && IsClipboardFormatAvailable(fmt).is_ok()
            })
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub fn clipboard_protected() -> bool {
        false
    }
}

/// Enthält die Zwischenablage etwas, das ein Passwortmanager als geheim
/// markiert hat (1Password, Bitwarden, KeePass ...)?
pub fn clipboard_protected() -> bool {
    native::clipboard_protected()
}

/// Skripte, die an Prozessspeicher oder gespeicherte Anmeldedaten wollen, werden gar nicht erst vorgelegt.
pub fn script_forbidden(code: &str) -> Option<&'static str> {
    let l = code.to_lowercase();
    const DUMP: &[&str] = &[
        "minidump",
        "readprocessmemory",
        "procdump",
        "comsvcs",
        "createdump",
        "debugactiveprocess",
        "openprocess",
        "mimikatz",
        "credread",
        "cmdkey /list",
        "get-storedcredential",
        "passwordvault",
        "windows.security.credentials",
        "credentialmanager",
        "vaultcmd",
    ];
    if DUMP.iter().any(|t| l.contains(t)) {
        return Some(tr!("Das Skript will Prozessspeicher oder gespeicherte Anmeldedaten auslesen. So etwas führe ich nicht aus.", "The script wants to read process memory or stored credentials. I don't run things like that."));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_scripts() {
        assert!(script_forbidden("rundll32 comsvcs.dll, MiniDump 1234 x.dmp full").is_some());
        assert!(script_forbidden("cmdkey /list").is_some());
        assert!(script_forbidden("[Windows.Security.Credentials.PasswordVault]::new()").is_some());
        assert!(script_forbidden("Get-Process | Sort-Object CPU").is_none());
    }

    /// Live-Prüfung (manuell: cargo test -- --ignored live_)
    #[test]
    #[ignore]
    fn live_guards() {
        println!("Zwischenablage geschützt: {}", clipboard_protected());
        println!("Zwischenablage gelesen: {:?}", crate::textwork::clipboard_read());
    }
}
