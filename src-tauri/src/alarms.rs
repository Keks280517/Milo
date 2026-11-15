//! Wecker: zu einer Uhrzeit, einmalig oder an bestimmten Wochentagen. Die Liste steht in den
//! Einstellungen; ein Waechter schaut alle 15 Sekunden auf die Uhr und stellt zur Weckzeit einen
//! Timer, der sofort klingelt (mit Schlummern und Stopp in der Pille).

use std::collections::HashSet;
use std::time::Duration;

use chrono::{Datelike, Local, Timelike};
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{Alarm, SettingsStore};

/// Wochentage aus Woertern: „werktags“, „wochenende“, „täglich“, „montag“ ... (0 = Montag).
pub fn days_from_words(words: &[&str]) -> Option<Vec<u8>> {
    const NAMES: [&[&str]; 7] = [
        &["montag", "montags", "monday", "mondays"],
        &["dienstag", "dienstags", "tuesday", "tuesdays"],
        &["mittwoch", "mittwochs", "wednesday", "wednesdays"],
        &["donnerstag", "donnerstags", "thursday", "thursdays"],
        &["freitag", "freitags", "friday", "fridays"],
        &["samstag", "samstags", "saturday", "saturdays"],
        &["sonntag", "sonntags", "sunday", "sundays"],
    ];
    if words.iter().any(|w| ["werktags", "wochentags", "weekdays"].contains(w)) {
        return Some(vec![0, 1, 2, 3, 4]);
    }
    if words.iter().any(|w| ["wochenende", "wochenendes", "weekends", "weekend"].contains(w)) {
        return Some(vec![5, 6]);
    }
    if words.iter().any(|w| ["täglich", "taeglich", "daily"].contains(w)) || words.windows(2).any(|p| (p[0] == "jeden" || p[0] == "every") && (p[1] == "tag" || p[1] == "day")) {
        return Some((0..7).collect());
    }
    let days: Vec<u8> = (0..7u8).filter(|i| NAMES[*i as usize].iter().any(|n| words.contains(n))).collect();
    let repeating = words.iter().any(|w| ["jeden", "jede", "immer", "every"].contains(w) || w.ends_with("tags") || w.ends_with("days"));
    (repeating && !days.is_empty()).then_some(days)
}

/// Neuer Wecker in den Einstellungen (sie werden gespeichert und an alle Fenster gemeldet).
pub fn add(app: &AppHandle, time: &str, days: Vec<u8>, label: &str) -> Result<Alarm, String> {
    let (h, m) = time.split_once(':').and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?))).ok_or_else(|| tr!("Die Weckzeit verstehe ich nicht (Beispiel: 07:00).", "I don't understand the alarm time (example: 07:00).").to_string())?;
    if h > 23 || m > 59 {
        return Err(tr!("Die Weckzeit verstehe ich nicht (Beispiel: 07:00).", "I don't understand the alarm time (example: 07:00).").into());
    }
    let store = app.state::<SettingsStore>();
    let mut settings = store.get();
    let alarm = Alarm { id: format!("a{}", Local::now().timestamp_millis()), time: format!("{h:02}:{m:02}"), days, label: label.trim().to_string(), enabled: true };
    settings.alarms.push(alarm.clone());
    let saved = store.set(settings)?;
    let _ = app.emit("milo://settings", &saved);
    Ok(alarm)
}

/// Lesbare Beschreibung: „werktags um 07:00“.
pub fn describe(alarm: &Alarm) -> String {
    const SHORT_DE: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
    const SHORT_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let short = tr!(SHORT_DE, SHORT_EN);
    let days = match alarm.days.as_slice() {
        [] => tr!("einmal", "once").to_string(),
        [0, 1, 2, 3, 4] => tr!("werktags", "on weekdays").to_string(),
        [5, 6] => tr!("am Wochenende", "at weekends").to_string(),
        [0, 1, 2, 3, 4, 5, 6] => tr!("täglich", "every day").to_string(),
        list => list.iter().map(|d| short[*d as usize % 7]).collect::<Vec<_>>().join(", "),
    };
    tr!(format!("{days} um {}", alarm.time), format!("{days} at {}", alarm.time))
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        // Schon ausgeloest: Wecker-ID und Minute, damit er in derselben Minute nicht doppelt klingelt.
        let mut fired: HashSet<String> = HashSet::new();
        loop {
            let now = Local::now();
            let minute = now.format("%Y-%m-%d %H:%M").to_string();
            let hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
            let weekday = now.weekday().num_days_from_monday() as u8;
            let store = app.state::<SettingsStore>();
            let settings = store.get();
            let mut once_done = Vec::new();
            for alarm in settings.alarms.iter().filter(|a| a.enabled && a.time == hhmm) {
                if !(alarm.days.is_empty() || alarm.days.contains(&weekday)) {
                    continue;
                }
                if !fired.insert(format!("{}|{minute}", alarm.id)) {
                    continue;
                }
                let label = if alarm.label.is_empty() { tr!("Wecker", "Alarm").to_string() } else { alarm.label.clone() };
                if let Err(err) = crate::timers::start(&app, &format!("⏰ {label}"), Duration::from_secs(1)) {
                    log::warn!("Wecker nicht gestellt: {err}");
                }
                if alarm.days.is_empty() {
                    once_done.push(alarm.id.clone());
                }
            }
            // Einmalige Wecker schalten sich nach dem Klingeln ab.
            if !once_done.is_empty() {
                let mut next = settings.clone();
                for alarm in next.alarms.iter_mut().filter(|a| once_done.contains(&a.id)) {
                    alarm.enabled = false;
                }
                if let Ok(saved) = store.set(next) {
                    let _ = app.emit("milo://settings", &saved);
                }
            }
            fired.retain(|key| key.ends_with(&minute));
            std::thread::sleep(Duration::from_secs(15));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays_from_words() {
        assert_eq!(days_from_words(&["weck", "mich", "werktags", "um", "7"]), Some(vec![0, 1, 2, 3, 4]));
        assert_eq!(days_from_words(&["jeden", "tag", "um", "6"]), Some((0..7).collect()));
        assert_eq!(days_from_words(&["jeden", "montag"]), Some(vec![0]));
        assert_eq!(days_from_words(&["montags", "und", "freitags"]), Some(vec![0, 4]));
        assert_eq!(days_from_words(&["am", "montag"]), None);
        assert_eq!(days_from_words(&["um", "7"]), None);
    }

    #[test]
    fn descriptions() {
        let alarm = Alarm { id: "x".into(), time: "07:00".into(), days: vec![0, 1, 2, 3, 4], label: String::new(), enabled: true };
        assert!(describe(&alarm).ends_with("07:00"));
    }
}
