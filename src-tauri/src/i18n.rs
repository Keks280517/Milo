//! Sprache der Oberflaeche und von Milos Antworten: Deutsch oder Englisch.
//!
//! Die Einstellung `language` ist "de", "en" oder "auto" (Windows-Anzeigesprache). Sie wird beim
//! Laden und Speichern der Einstellungen hier abgelegt, damit jede Stelle im Kern ohne AppHandle
//! fragen kann. Texte stehen mit `tr!("Deutsch", "English")` direkt nebeneinander.

use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);

/// Englisch aktiv?
pub fn en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// Einstellung uebernehmen ("de", "en" oder "auto").
pub fn apply(setting: &str) {
    // In Tests bleibt es bei Deutsch: sonst haengt das Ergebnis davon ab, ob vorher ein Test
    // Einstellungen geladen hat (die Sprache ist global).
    if cfg!(test) {
        return;
    }
    let en = match setting {
        "en" => true,
        "de" => false,
        _ => !system_is_german(),
    };
    EN.store(en, Ordering::Relaxed);
}

/// Sprache fuer Anweisungen an die Sprachmodelle ("Antworte auf ...").
pub fn reply_language() -> &'static str {
    if en() {
        "English"
    } else {
        "Deutsch"
    }
}

#[cfg(windows)]
fn system_is_german() -> bool {
    // Primaere Sprach-ID 0x07 = Deutsch (alle Varianten).
    unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() & 0x3ff == 0x07 }
}

#[cfg(not(windows))]
fn system_is_german() -> bool {
    std::env::var("LANG").map(|l| l.starts_with("de")).unwrap_or(false)
}

/// `tr!("Deutsch", "English")`: der passende Text zur eingestellten Sprache.
#[macro_export]
macro_rules! tr {
    ($de:expr, $en:expr $(,)?) => {
        if $crate::i18n::en() {
            $en
        } else {
            $de
        }
    };
}

