//! Morgen-Briefing und Tagesrueckblick.
//!
//! Morgens beim ersten Kontakt (ab 5 Uhr) fasst Milo den Tag zusammen: Datum, Wetter am
//! gespeicherten Ort, laufende Timer und was gestern notiert wurde. Das geht ohne KI und sofort.
//! Abends (ab der eingestellten Stunde) oder auf „Feierabend“ fragt Milo nach dem Tag; die
//! naechste Antwort speichert er als Notiz, die das Briefing am naechsten Morgen aufgreift.

use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::memory::{Memory, Note};
use crate::settings::SettingsStore;

/// Beginn eines Tagesrueckblicks in den Notizen; daran erkennt das Briefing ihn wieder.
pub const REVIEW_PREFIX: &str = "Tagesrückblick";
/// Englischer Marker fuer den Tagesrueckblick; erkannt werden beide.
pub const REVIEW_PREFIX_EN: &str = "Day review";

fn is_review(text: &str) -> bool {
    text.starts_with(REVIEW_PREFIX) || text.starts_with(REVIEW_PREFIX_EN)
}
const WEEKDAYS: [&str; 7] = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];
const MONTHS: [&str; 12] = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Seen {
    briefing: String,
    review: String,
}

pub struct Daily {
    path: PathBuf,
    seen: Mutex<Seen>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// "briefing" oder "review".
    pub kind: &'static str,
    pub text: String,
}

impl Daily {
    pub fn load(path: PathBuf) -> Self {
        let seen = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { path, seen: Mutex::new(seen) }
    }

    fn mark(&self, update: impl FnOnce(&mut Seen)) {
        let mut seen = self.seen.lock().unwrap();
        update(&mut seen);
        if let Ok(text) = serde_json::to_string(&*seen) {
            let _ = std::fs::write(&self.path, text);
        }
    }
}

/// Was ist heute faellig? `hour`: aktuelle Stunde, `today`: Datum als Text.
pub fn due(seen_briefing: &str, seen_review: &str, today: &str, hour: u32, briefing: bool, review: bool, evening_hour: u8) -> Option<&'static str> {
    if review && hour >= evening_hour as u32 && seen_review != today {
        return Some("review");
    }
    if briefing && (5..evening_hour as u32).contains(&hour) && seen_briefing != today {
        return Some("briefing");
    }
    None
}

/// Notizen eines bestimmten Tages (Datum am Anfang von `created_at`).
fn notes_of(notes: &[Note], day: NaiveDate) -> Vec<&Note> {
    let prefix = day.format("%Y-%m-%d").to_string();
    notes.iter().filter(|n| n.created_at.starts_with(&prefix)).collect()
}

/// Text des Briefings aus den gesammelten Teilen.
pub fn briefing_text(name: &str, now: chrono::DateTime<Local>, weather: Option<String>, timers: usize, yesterday: &[&Note]) -> String {
    let greeting = match now.hour() {
        5..=10 => tr!("Guten Morgen", "Good morning"),
        11..=17 => tr!("Hallo", "Hello"),
        _ => tr!("Guten Abend", "Good evening"),
    };
    let who = if name.is_empty() { String::new() } else { format!(", {name}") };
    let mut lines = vec![if crate::i18n::en() {
        format!("{greeting}{who}! Today is {}.", now.format("%A, %B %-d"))
    } else {
        format!(
            "{greeting}{who}! Heute ist {}, der {}. {}.",
            WEEKDAYS[now.weekday().num_days_from_monday() as usize],
            now.day(),
            MONTHS[now.month0() as usize]
        )
    }];
    if let Some(weather) = weather {
        lines.push(weather);
    }
    match timers {
        0 => {}
        1 => lines.push(tr!("Es läuft ein Timer.", "One timer is running.").into()),
        n => lines.push(tr!(format!("Es laufen {n} Timer."), format!("{n} timers are running."))),
    }
    let review = yesterday.iter().find(|n| is_review(&n.text));
    if let Some(review) = review {
        let said = review.text.split_once(':').map(|(_, rest)| rest.trim()).unwrap_or(&review.text);
        lines.push(tr!(format!("Gestern Abend hast du gesagt: „{}“", said.chars().take(220).collect::<String>()), format!("Last night you said: \"{}\"", said.chars().take(220).collect::<String>())));
    }
    let others: Vec<&&Note> = yesterday.iter().filter(|n| !is_review(&n.text)).take(3).collect();
    if !others.is_empty() {
        let list: Vec<String> = others.iter().map(|n| n.text.chars().take(90).collect()).collect();
        lines.push(tr!(format!("Gestern notiert: {}.", list.join("; ")), format!("Noted yesterday: {}.", list.join("; "))));
    }
    lines.join(" ")
}

/// Kurzer Wettersatz fuer das Briefing.
fn weather_line(weather: &crate::web::Weather) -> String {
    let today = weather.days.first();
    let mut line = tr!(format!("In {} sind es gerade {:.0} Grad, {}.", weather.place, weather.now.temp, weather.now.text.to_lowercase()), format!("It's {1:.0} degrees in {0} right now, {2}.", weather.place, weather.now.temp, weather.now.text.to_lowercase()));
    if let Some(day) = today {
        line.push_str(&tr!(format!(" Heute bis {:.0} Grad", day.max), format!(" Up to {:.0} degrees today", day.max)));
        if day.rain >= 30.0 {
            line.push_str(&tr!(format!(", Regenwahrscheinlichkeit {:.0} %", day.rain), format!(", {:.0} % chance of rain", day.rain)));
        }
        line.push('.');
    }
    line
}

/// Wird bei jedem ersten Anzeichen von Nutzung gefragt. Liefert hoechstens eine Nachricht pro Art und Tag.
#[tauri::command]
pub async fn daily_check(app: AppHandle) -> Option<Message> {
    let settings = app.state::<SettingsStore>().get();
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let kind = {
        let daily = app.state::<Daily>();
        let seen = daily.seen.lock().unwrap();
        due(&seen.briefing, &seen.review, &today, now.hour(), settings.briefing, settings.evening_review, settings.evening_hour)?
    };
    let daily = app.state::<Daily>();
    if kind == "review" {
        daily.mark(|s| s.review = today.clone());
        return Some(start_review(&app));
    }
    daily.mark(|s| s.briefing = today.clone());
    let weather = if settings.weather_place.is_empty() {
        None
    } else {
        crate::web::weather(&settings.weather_place).await.ok().map(|w| weather_line(&w))
    };
    let timers = crate::timers::list_timers(app.clone()).iter().filter(|t| !t.ringing).count();
    let notes = app.state::<Memory>().recent_notes(50).unwrap_or_default();
    let yesterday = notes_of(&notes, now.date_naive() - ChronoDuration::days(1));
    Some(Message { kind: "briefing", text: briefing_text(&settings.user_name, now, weather, timers, &yesterday) })
}

/// Fragt nach dem Tag; die naechste Eingabe wird als Rueckblick gespeichert.
pub fn start_review(app: &AppHandle) -> Message {
    // Auch wer selbst „Feierabend“ sagt, wird heute nicht noch einmal gefragt.
    let today = Local::now().format("%Y-%m-%d").to_string();
    app.state::<Daily>().mark(|s| s.review = today);
    app.state::<crate::agent::Agent>().awaiting_review.store(true, std::sync::atomic::Ordering::SeqCst);
    Message { kind: "review", text: tr!("Feierabend! Wie war dein Tag? Erzähl kurz, ich merke es mir und erinnere dich morgen früh daran.", "Done for today! How was your day? Tell me briefly, I'll remember it and remind you tomorrow morning.").into() }
}

/// Speichert die Antwort auf die Abendfrage.
pub fn save_review(app: &AppHandle, text: &str) -> String {
    let date = Local::now().format("%d.%m.%Y");
    match app.state::<Memory>().add_note(&format!("{} {date}: {}", tr!(REVIEW_PREFIX, REVIEW_PREFIX_EN), text.trim())) {
        Ok(_) => tr!("Danke, hab ich mir gemerkt. Schönen Abend!", "Thanks, got it. Have a nice evening!").into(),
        Err(err) => tr!(format!("Das konnte ich leider nicht speichern ({err})."), format!("Sorry, I couldn't save that ({err}).")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn note(text: &str, at: &str) -> Note {
        Note { id: 1, text: text.into(), created_at: at.into(), updated_at: at.into(), rich: String::new() }
    }

    #[test]
    fn at_most_one_briefing_and_one_review_per_day() {
        assert_eq!(due("", "", "2026-10-16", 8, true, true, 21), Some("briefing"));
        assert_eq!(due("2026-10-16", "", "2026-10-16", 8, true, true, 21), None);
        assert_eq!(due("", "", "2026-10-16", 4, true, true, 21), None, "nachts kein Briefing");
        assert_eq!(due("2026-10-16", "", "2026-10-16", 21, true, true, 21), Some("review"));
        assert_eq!(due("2026-10-16", "2026-10-16", "2026-10-16", 22, true, true, 21), None);
        assert_eq!(due("", "", "2026-10-16", 8, false, true, 21), None, "abgeschaltet");
        assert_eq!(due("", "", "2026-10-16", 22, true, false, 21), None, "abends kein spaetes Briefing");
    }

    #[test]
    fn briefing_mentions_weather_timers_and_yesterday() {
        let now = Local.with_ymd_and_hms(2026, 10, 16, 8, 0, 0).unwrap();
        let notes = vec![
            note("Tagesrückblick 15.10.2026: Präsentation fertig, morgen Zahnarzt", "2026-10-15 21:10"),
            note("Milch kaufen", "2026-10-15 12:00"),
            note("Alt", "2026-10-10 12:00"),
        ];
        let yesterday = notes_of(&notes, NaiveDate::from_ymd_opt(2026, 10, 15).unwrap());
        assert_eq!(yesterday.len(), 2);
        let text = briefing_text("Alex", now, Some("In Berlin sind es 9 Grad.".into()), 2, &yesterday);
        assert!(text.starts_with("Guten Morgen, Alex! Heute ist Freitag, der 16. Oktober."));
        assert!(text.contains("Berlin"));
        assert!(text.contains("Es laufen 2 Timer."));
        assert!(text.contains("„Präsentation fertig, morgen Zahnarzt“"));
        assert!(text.contains("Milch kaufen"));
        assert!(!text.contains("Alt"));
    }
}
