//! Timer und Erinnerungen. Sie werden in der Datenbank gehalten und ueberstehen einen Neustart.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Result};
use chrono::{Local, NaiveTime, TimeZone};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::memory::{Memory, StoredTimer};
use crate::settings::SettingsStore;
use crate::sound;

pub const EVENT: &str = "milo://timers";
const MAX_DURATION: Duration = Duration::from_secs(7 * 24 * 3600);
/// So lange klingelt ein Timer hoerbar; danach blinkt er nur noch, bis er bestaetigt wird.
const RING_SOUND: Duration = Duration::from_secs(60);
/// Spaetestens so oft schaut der Waechter auf die Uhr. So bemerkt er verlaengerte Timer und
/// geht auch nach dem Ruhezustand des PCs richtig.
const RECHECK: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerView {
    pub id: i64,
    pub label: String,
    pub due_ms: i64,
    pub total_ms: i64,
    /// Abgelaufen und klingelt, bis der Nutzer ihn bestaetigt.
    pub ringing: bool,
}

#[derive(Default)]
pub struct Timers {
    ringing: Mutex<Vec<i64>>,
    /// Zaehlt die Klingel-Starts, damit das Verstummen nach `RING_SOUND` nur den eigenen Start trifft.
    ring_epoch: AtomicU64,
}

fn now_ms() -> i64 {
    Local::now().timestamp_millis()
}

fn stored(app: &AppHandle, id: i64) -> Option<StoredTimer> {
    app.state::<Memory>().timers().unwrap_or_default().into_iter().find(|t| t.id == id)
}

fn snapshot(app: &AppHandle) -> Vec<TimerView> {
    let ringing = app.state::<Timers>().ringing.lock().unwrap().clone();
    app.state::<Memory>()
        .timers()
        .unwrap_or_default()
        .into_iter()
        .map(|t| TimerView { ringing: ringing.contains(&t.id), id: t.id, label: t.label, due_ms: t.due_ms, total_ms: t.total_ms })
        .collect()
}

fn broadcast(app: &AppHandle) {
    let _ = app.emit(EVENT, snapshot(app));
}

/// Wartet bis zur Faelligkeit und laesst den Timer dann klingeln, sofern er noch existiert.
fn watch(app: AppHandle, id: i64) {
    tauri::async_runtime::spawn(async move {
        let label = loop {
            let Some(timer) = stored(&app, id) else {
                return;
            };
            let wait = timer.due_ms - now_ms();
            if wait <= 0 {
                break timer.label;
            }
            tokio::time::sleep(Duration::from_millis(wait as u64).min(RECHECK)).await;
        };
        let timers = app.state::<Timers>();
        {
            let mut ringing = timers.ringing.lock().unwrap();
            if ringing.contains(&id) {
                return;
            }
            ringing.push(id);
        }
        log::info!("Timer „{label}“ ist abgelaufen");
        broadcast(&app);
        if app.state::<SettingsStore>().get().timer_sound {
            let epoch = timers.ring_epoch.fetch_add(1, Ordering::SeqCst) + 1;
            sound::start_ringing();
            tokio::time::sleep(RING_SOUND).await;
            if timers.ring_epoch.load(Ordering::SeqCst) == epoch {
                sound::stop_ringing();
            }
        }
    });
}

/// Nimmt einen Timer aus der Klingel-Liste; verstummt, wenn keiner mehr klingelt.
fn silence(app: &AppHandle, id: i64) -> bool {
    let timers = app.state::<Timers>();
    let (was_ringing, quiet) = {
        let mut ringing = timers.ringing.lock().unwrap();
        let before = ringing.len();
        ringing.retain(|r| *r != id);
        (ringing.len() != before, ringing.is_empty())
    };
    if quiet {
        timers.ring_epoch.fetch_add(1, Ordering::SeqCst);
        sound::stop_ringing();
    }
    was_ringing
}

/// Stellt einen Timer und liefert seine Faelligkeit.
pub fn start(app: &AppHandle, label: &str, duration: Duration) -> Result<StoredTimer> {
    if duration < Duration::from_secs(1) {
        bail!(tr!("Die Dauer muss mindestens eine Sekunde sein.", "The duration has to be at least one second."));
    }
    if duration > MAX_DURATION {
        bail!(tr!("Timer gehen höchstens über sieben Tage.", "Timers can run for at most seven days."));
    }
    let total = duration.as_millis() as i64;
    let label = label.trim();
    let timer = app.state::<Memory>().add_timer(if label.is_empty() { "Timer" } else { label }, now_ms() + total, total)?;
    watch(app.clone(), timer.id);
    broadcast(app);
    Ok(timer)
}

/// Entfernt einen Timer (laufend oder klingelnd).
pub fn remove(app: &AppHandle, id: i64) -> bool {
    silence(app, id);
    let removed = app.state::<Memory>().remove_timer(id).unwrap_or(false);
    broadcast(app);
    removed
}

/// Gibt einem Timer mehr Zeit. Ein klingelnder Timer schlummert und laeuft neu ueber `extra`.
pub fn extend(app: &AppHandle, id: i64, extra: Duration) -> Result<()> {
    let Some(timer) = stored(app, id) else {
        bail!(tr!("Diesen Timer gibt es nicht mehr.", "That timer doesn't exist anymore."));
    };
    let extra_ms = extra.as_millis() as i64;
    let memory = app.state::<Memory>();
    if silence(app, id) {
        memory.set_timer(id, now_ms() + extra_ms, extra_ms)?;
        watch(app.clone(), id);
    } else {
        let left = (timer.due_ms - now_ms()).max(0) + extra_ms;
        if left > MAX_DURATION.as_millis() as i64 {
            bail!(tr!("Timer gehen höchstens über sieben Tage.", "Timers can run for at most seven days."));
        }
        memory.set_timer(id, timer.due_ms + extra_ms, timer.total_ms + extra_ms)?;
    }
    broadcast(app);
    Ok(())
}

/// Nach dem Start: gespeicherte Timer wieder ueberwachen. Bereits verpasste klingeln sofort.
pub fn restore(app: &AppHandle) {
    for timer in app.state::<Memory>().timers().unwrap_or_default() {
        watch(app.clone(), timer.id);
    }
}

/// Dauer bis zur naechsten Uhrzeit `text` ("18:30", "18.30", "18"), heute oder morgen.
pub fn until_clock_time(text: &str) -> Result<Duration> {
    let cleaned = text.trim().to_lowercase().replace("uhr", "").replace('.', ":");
    let cleaned = cleaned.trim();
    let time = NaiveTime::parse_from_str(cleaned, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(&format!("{cleaned}:00"), "%H:%M"))
        .map_err(|_| anyhow::anyhow!("{}", tr!(format!("„{text}“ ist keine Uhrzeit, die ich verstehe (Beispiel: 18:30)."), format!("\"{text}\" isn't a time I understand (example: 18:30)."))))?;
    let now = Local::now();
    let mut target = now.date_naive().and_time(time);
    if target <= now.naive_local() {
        target += chrono::Duration::days(1);
    }
    let target = Local.from_local_datetime(&target).earliest().ok_or_else(|| anyhow::anyhow!(tr!("Diese Uhrzeit gibt es heute nicht.", "That time doesn't exist today.")))?;
    Ok((target - now).to_std().unwrap_or(Duration::from_secs(1)))
}

/// Dauer bis zu einem Tag („morgen“, „übermorgen“, „montag“, „2026-11-20“, englisch auch) und einer
/// Uhrzeit (Standard 09:00). Liegt das Ziel in der Vergangenheit, ist es ein Fehler.
pub fn until_day_time(day: &str, time: &str) -> Result<Duration> {
    use chrono::{Datelike, NaiveDate};
    let day = day.trim().to_lowercase();
    let today = Local::now().date_naive();
    const WEEK: [&[&str]; 7] = [
        &["montag", "monday"],
        &["dienstag", "tuesday"],
        &["mittwoch", "wednesday"],
        &["donnerstag", "thursday"],
        &["freitag", "friday"],
        &["samstag", "saturday"],
        &["sonntag", "sunday"],
    ];
    let date = match day.as_str() {
        "" | "heute" | "today" => today,
        "morgen" | "tomorrow" => today + chrono::Duration::days(1),
        "übermorgen" | "uebermorgen" => today + chrono::Duration::days(2),
        other => {
            if let Some(index) = WEEK.iter().position(|names| names.iter().any(|n| other.contains(n))) {
                let now = today.weekday().num_days_from_monday() as i64;
                let mut ahead = (index as i64 - now).rem_euclid(7);
                if ahead == 0 {
                    ahead = 7;
                }
                today + chrono::Duration::days(ahead)
            } else {
                NaiveDate::parse_from_str(other, "%Y-%m-%d")
                    .or_else(|_| NaiveDate::parse_from_str(other, "%d.%m.%Y"))
                    .map_err(|_| anyhow::anyhow!("{}", tr!(format!("Den Tag „{day}“ verstehe ich nicht."), format!("I don't understand the day \"{day}\"."))))?
            }
        }
    };
    let time = if time.trim().is_empty() { "09:00".to_string() } else { time.trim().replace('.', ":") };
    let clock = NaiveTime::parse_from_str(&time, "%H:%M").map_err(|_| anyhow::anyhow!("{}", tr!(format!("„{time}“ ist keine Uhrzeit (Beispiel: 18:30)."), format!("\"{time}\" isn't a time (example: 18:30)."))))?;
    let target = Local.from_local_datetime(&date.and_time(clock)).earliest().ok_or_else(|| anyhow::anyhow!(tr!("Diese Uhrzeit gibt es an dem Tag nicht.", "That time doesn't exist on that day.")))?;
    let left = target - Local::now();
    if left <= chrono::Duration::zero() {
        bail!(tr!("Dieser Zeitpunkt ist schon vorbei.", "That moment has already passed."));
    }
    Ok(left.to_std().unwrap_or(Duration::from_secs(1)))
}

/// Dauer als gut lesbarer deutscher Text, z. B. "1 Stunde 30 Minuten".
pub fn describe(duration: Duration) -> String {
    let total = duration.as_secs();
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    let part = |n: u64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(part(hours, tr!("Stunde", "hour"), tr!("Stunden", "hours")));
    }
    if minutes > 0 {
        parts.push(part(minutes, tr!("Minute", "minute"), tr!("Minuten", "minutes")));
    }
    if seconds > 0 && hours == 0 {
        parts.push(part(seconds, tr!("Sekunde", "second"), tr!("Sekunden", "seconds")));
    }
    if parts.is_empty() { tr!("0 Sekunden", "0 seconds").into() } else { parts.join(" ") }
}

#[tauri::command]
pub fn list_timers(app: AppHandle) -> Vec<TimerView> {
    snapshot(&app)
}

#[tauri::command]
pub fn cancel_timer(app: AppHandle, id: i64) -> bool {
    remove(&app, id)
}

/// Timer direkt aus der Oberflaeche stellen (Schnellwahl), ohne das Modell zu bemuehen.
#[tauri::command]
pub fn start_timer(app: AppHandle, label: String, seconds: u64) -> Result<TimerView, String> {
    let timer = start(&app, &label, Duration::from_secs(seconds)).map_err(|e| e.to_string())?;
    Ok(TimerView { id: timer.id, label: timer.label, due_ms: timer.due_ms, total_ms: timer.total_ms, ringing: false })
}

#[tauri::command]
pub fn extend_timer(app: AppHandle, id: i64, seconds: u64) -> Result<(), String> {
    extend(&app, id, Duration::from_secs(seconds.clamp(1, 24 * 3600))).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_durations_in_german() {
        assert_eq!(describe(Duration::from_secs(600)), "10 Minuten");
        assert_eq!(describe(Duration::from_secs(5400)), "1 Stunde 30 Minuten");
        assert_eq!(describe(Duration::from_secs(45)), "45 Sekunden");
        assert_eq!(describe(Duration::from_secs(61)), "1 Minute 1 Sekunde");
        assert_eq!(describe(Duration::from_secs(7200)), "2 Stunden");
        assert_eq!(describe(Duration::from_secs(0)), "0 Sekunden");
    }

    #[test]
    fn clock_time_is_always_in_the_next_24_hours() {
        for text in ["18:30", "18.30", "18", "7 Uhr", " 00:05 ", "18:30 Uhr"] {
            let d = until_clock_time(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert!(d > Duration::ZERO && d <= Duration::from_secs(24 * 3600), "{text}: {d:?}");
        }
    }

    #[test]
    fn clock_time_rejects_nonsense() {
        assert!(until_clock_time("später").is_err());
        assert!(until_clock_time("25:00").is_err());
        assert!(until_clock_time("").is_err());
    }
}
