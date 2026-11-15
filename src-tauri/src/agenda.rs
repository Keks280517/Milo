//! Agenda: eigene Termine (lokal gespeichert) und abonnierte Kalender (ICS-Adressen, z. B. die
//! „geheime Adresse“ eines Google-Kalenders). Abos werden nur gelesen, nie veraendert, und hoechstens
//! alle zehn Minuten neu geladen.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use chrono::{DateTime, Datelike, Duration as Span, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Event {
    pub id: String,
    pub title: String,
    /// Beginn und Ende in ms seit 1970.
    pub start_ms: i64,
    pub end_ms: i64,
    pub all_day: bool,
    pub location: String,
    /// "local" = eigener Termin (loeschbar), sonst der Name des Abos.
    pub source: String,
}

fn file(app: &AppHandle) -> PathBuf {
    app.path().app_config_dir().unwrap_or_default().join("agenda.json")
}

static LOCK: Mutex<()> = Mutex::new(());
/// Zwischenspeicher der Abos: Adresse, Zeitpunkt, Termine.
static CACHE: Mutex<Vec<(String, Instant, Vec<Event>)>> = Mutex::new(Vec::new());
const REFRESH: Duration = Duration::from_secs(600);

fn load_local(app: &AppHandle) -> Vec<Event> {
    std::fs::read_to_string(file(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn store_local(app: &AppHandle, list: &[Event]) -> Result<()> {
    let path = file(app);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(list)?)?;
    Ok(())
}

/// Termine, die sich mit [from, to) ueberschneiden, nach Beginn sortiert.
#[tauri::command]
pub async fn agenda_list(app: AppHandle, from: i64, to: i64) -> Result<Vec<Event>, String> {
    let mut all: Vec<Event> = load_local(&app);
    let urls = app.state::<crate::settings::SettingsStore>().get().agenda_ics;
    for url in urls.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match subscription(url).await {
            Ok(events) => all.extend(events),
            Err(err) => log::warn!("Kalender-Abo nicht lesbar: {err}"),
        }
    }
    let mut hits: Vec<Event> = all.into_iter().filter(|e| e.end_ms > from && e.start_ms < to).collect();
    hits.sort_by_key(|e| (e.start_ms, e.end_ms));
    Ok(hits)
}

#[tauri::command]
pub fn agenda_add(app: AppHandle, title: String, start_ms: i64, end_ms: i64, all_day: bool, location: Option<String>) -> Result<Event, String> {
    add(&app, title, start_ms, end_ms, all_day, location.unwrap_or_default()).map_err(|e| e.to_string())
}

pub fn add(app: &AppHandle, title: String, start_ms: i64, end_ms: i64, all_day: bool, location: String) -> Result<Event> {
    let title: String = title.trim().chars().filter(|c| !c.is_control()).take(120).collect();
    if title.is_empty() {
        bail!(tr!("Der Termin braucht einen Namen.", "The event needs a name."));
    }
    if end_ms < start_ms || end_ms - start_ms > 31 * 86_400_000 {
        bail!(tr!("Ende liegt vor dem Beginn oder zu weit danach.", "The end is before the start or too far after it."));
    }
    let _guard = LOCK.lock().unwrap();
    let mut list = load_local(app);
    let event = Event {
        id: format!("l{}", Utc::now().timestamp_millis()),
        title,
        start_ms,
        end_ms: end_ms.max(start_ms + 60_000),
        all_day,
        location: location.trim().chars().take(120).collect(),
        source: "local".into(),
    };
    list.push(event.clone());
    // Alte eigene Termine (aelter als ein Jahr) fallen weg.
    let cutoff = Utc::now().timestamp_millis() - 365 * 86_400_000;
    list.retain(|e| e.end_ms > cutoff);
    store_local(app, &list)?;
    Ok(event)
}

#[tauri::command]
pub fn agenda_delete(app: AppHandle, id: String) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap();
    let mut list = load_local(&app);
    list.retain(|e| e.id != id);
    store_local(&app, &list).map_err(|e| e.to_string())
}

async fn subscription(url: &str) -> Result<Vec<Event>> {
    if let Some((_, at, events)) = CACHE.lock().unwrap().iter().find(|(u, _, _)| u == url) {
        if at.elapsed() < REFRESH {
            return Ok(events.clone());
        }
    }
    let fetch = url.replacen("webcal://", "https://", 1);
    if !fetch.starts_with("https://") {
        bail!(tr!("Nur https- oder webcal-Adressen.", "Only https or webcal addresses."));
    }
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?;
    let text = client.get(&fetch).send().await?.error_for_status()?.text().await?;
    let name = calendar_name(&text).unwrap_or_else(|| tr!("Kalender", "Calendar").into());
    let now = Utc::now().timestamp_millis();
    // Wiederholungen werden fuer gut ein Jahr ausgerollt.
    let events = parse_ics(&text, &name, now - 60 * 86_400_000, now + 400 * 86_400_000);
    let mut cache = CACHE.lock().unwrap();
    cache.retain(|(u, _, _)| u != url);
    cache.push((url.to_string(), Instant::now(), events.clone()));
    Ok(events)
}

// ---------- ICS lesen ----------

/// Entfaltet umbrochene Zeilen (Fortsetzung beginnt mit Leerzeichen oder Tab).
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else {
            lines.push(raw.trim_end_matches('\r').to_string());
        }
    }
    lines
}

fn calendar_name(text: &str) -> Option<String> {
    unfold(text).into_iter().find_map(|l| l.strip_prefix("X-WR-CALNAME:").map(|n| unescape(n).chars().take(40).collect()))
}

fn unescape(value: &str) -> String {
    value.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// Liest einen Zeitwert. Liefert ms und ob es ein ganzer Tag ist.
fn parse_time(params: &str, value: &str) -> Option<(i64, bool)> {
    let value = value.trim();
    if params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") || value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        let local = Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).earliest()?;
        return Some((local.timestamp_millis(), true));
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let naive = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some((Utc.from_utc_datetime(&naive).timestamp_millis(), false));
    }
    let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    if let Some(zone) = params.split(';').find_map(|p| p.strip_prefix("TZID=")) {
        if let Ok(tz) = zone.trim_matches('"').parse::<chrono_tz::Tz>() {
            if let Some(t) = tz.from_local_datetime(&naive).earliest() {
                return Some((t.timestamp_millis(), false));
            }
        }
    }
    Some((Local.from_local_datetime(&naive).earliest()?.timestamp_millis(), false))
}

/// Dauer wie PT1H30M oder P1D in ms.
fn parse_duration(value: &str) -> Option<i64> {
    let mut total = 0i64;
    let mut number = String::new();
    let mut in_time = false;
    for c in value.trim().trim_start_matches(['+', '-']).chars() {
        match c {
            'P' => {}
            'T' => in_time = true,
            '0'..='9' => number.push(c),
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                total += n * match (unit, in_time) {
                    ('W', _) => 7 * 86_400_000,
                    ('D', _) => 86_400_000,
                    ('H', true) => 3_600_000,
                    ('M', true) => 60_000,
                    ('S', true) => 1000,
                    _ => return None,
                };
            }
        }
    }
    Some(total)
}

struct Rule {
    freq: String,
    interval: i64,
    count: Option<usize>,
    until: Option<i64>,
}

fn parse_rule(value: &str) -> Option<Rule> {
    let mut rule = Rule { freq: String::new(), interval: 1, count: None, until: None };
    for part in value.split(';') {
        let (key, val) = part.split_once('=')?;
        match key {
            "FREQ" => rule.freq = val.to_string(),
            "INTERVAL" => rule.interval = val.parse().unwrap_or(1).max(1),
            "COUNT" => rule.count = val.parse().ok(),
            "UNTIL" => rule.until = parse_time("", val).map(|(t, _)| t),
            // BYDAY und Co. werden nicht ausgewertet; dann wiederholt sich der Termin am selben Wochentag.
            _ => {}
        }
    }
    matches!(rule.freq.as_str(), "DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY").then_some(rule)
}

/// Naechstes Vorkommen nach `start` gemaess Regel (in lokaler Zeit gerechnet, damit Sommerzeit stimmt).
fn step(start: DateTime<Local>, rule: &Rule, n: i64) -> Option<DateTime<Local>> {
    let naive = start.naive_local();
    let next = match rule.freq.as_str() {
        "DAILY" => naive + Span::days(n * rule.interval),
        "WEEKLY" => naive + Span::weeks(n * rule.interval),
        "MONTHLY" => {
            let months = naive.month0() as i64 + n * rule.interval;
            let year = naive.year() + (months / 12) as i32;
            naive.with_day(1)?.with_year(year)?.with_month0((months % 12) as u32)?.with_day(naive.day())?
        }
        "YEARLY" => naive.with_year(naive.year() + (n * rule.interval) as i32)?,
        _ => return None,
    };
    Local.from_local_datetime(&next).earliest()
}

pub fn parse_ics(text: &str, source: &str, from: i64, to: i64) -> Vec<Event> {
    let mut out = Vec::new();
    let mut current: Option<Vec<(String, String, String)>> = None;
    for line in unfold(text) {
        if line == "BEGIN:VEVENT" {
            current = Some(Vec::new());
            continue;
        }
        if line == "END:VEVENT" {
            if let Some(props) = current.take() {
                out.extend(event_of(&props, source, from, to));
            }
            continue;
        }
        if let Some(props) = current.as_mut() {
            let Some((head, value)) = line.split_once(':') else { continue };
            let (name, params) = head.split_once(';').unwrap_or((head, ""));
            props.push((name.to_ascii_uppercase(), params.to_string(), value.to_string()));
        }
    }
    out.truncate(2000);
    out
}

fn event_of(props: &[(String, String, String)], source: &str, from: i64, to: i64) -> Vec<Event> {
    let get = |name: &str| props.iter().find(|(n, _, _)| n == name);
    if get("STATUS").is_some_and(|(_, _, v)| v == "CANCELLED") || get("RECURRENCE-ID").is_some() {
        return Vec::new();
    }
    let Some((start, all_day)) = get("DTSTART").and_then(|(_, p, v)| parse_time(p, v)) else { return Vec::new() };
    let end = get("DTEND")
        .and_then(|(_, p, v)| parse_time(p, v).map(|(t, _)| t))
        .or_else(|| get("DURATION").and_then(|(_, _, v)| parse_duration(v)).map(|d| start + d))
        .unwrap_or(start + if all_day { 86_400_000 } else { 3_600_000 });
    let length = (end - start).max(0);
    let title = get("SUMMARY").map(|(_, _, v)| unescape(v)).unwrap_or_else(|| tr!("(ohne Titel)", "(untitled)").into());
    let location = get("LOCATION").map(|(_, _, v)| unescape(v)).unwrap_or_default();
    let uid = get("UID").map(|(_, _, v)| v.clone()).unwrap_or_else(|| format!("{start}"));
    let make = |s: i64, i: usize| Event {
        id: format!("{source}:{uid}:{i}"),
        title: title.chars().take(120).collect(),
        start_ms: s,
        end_ms: s + length,
        all_day,
        location: location.chars().take(120).collect(),
        source: source.to_string(),
    };
    let Some(rule) = get("RRULE").and_then(|(_, _, v)| parse_rule(v)) else {
        return if start + length > from && start < to { vec![make(start, 0)] } else { Vec::new() };
    };
    let Some(first) = Local.timestamp_millis_opt(start).single() else { return Vec::new() };
    let mut list = Vec::new();
    for n in 0..1500i64 {
        if rule.count.is_some_and(|c| n as usize >= c) {
            break;
        }
        let Some(at) = step(first, &rule, n) else { break };
        let s = at.timestamp_millis();
        if s > to || rule.until.is_some_and(|u| s > u) {
            break;
        }
        if s + length > from {
            list.push(make(s, n as usize));
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nX-WR-CALNAME:Privat\r\nBEGIN:VEVENT\r\nUID:a1\r\nSUMMARY:Design review\\, Team\r\nDTSTART:20261111T093000Z\r\nDTEND:20261111T103000Z\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b2\r\nSUMMARY:Sport\r\nDTSTART;TZID=Europe/Berlin:20261102T180000\r\nDURATION:PT1H\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:c3\r\nSUMMARY:Feiertag\r\nDTSTART;VALUE=DATE:20261225\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:d4\r\nSTATUS:CANCELLED\r\nSUMMARY:Weg\r\nDTSTART:20261111T120000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn reads_simple_recurring_and_all_day_events() {
        assert_eq!(calendar_name(SAMPLE).as_deref(), Some("Privat"));
        let events = parse_ics(SAMPLE, "Privat", 0, i64::MAX / 2);
        let review = events.iter().find(|e| e.title.starts_with("Design")).unwrap();
        assert_eq!(review.title, "Design review, Team");
        assert_eq!(review.end_ms - review.start_ms, 3_600_000);
        assert_eq!(events.iter().filter(|e| e.title == "Sport").count(), 4);
        let sport: Vec<_> = events.iter().filter(|e| e.title == "Sport").collect();
        assert_eq!(sport[1].start_ms - sport[0].start_ms, 7 * 86_400_000);
        // 18:00 Berliner Zeit im November = 17:00 UTC.
        assert_eq!(Utc.timestamp_millis_opt(sport[0].start_ms).unwrap().format("%H:%M").to_string(), "17:00");
        let holiday = events.iter().find(|e| e.title == "Feiertag").unwrap();
        assert!(holiday.all_day);
        assert!(!events.iter().any(|e| e.title == "Weg"));
    }

    #[test]
    fn durations_and_folded_lines() {
        assert_eq!(parse_duration("PT1H30M"), Some(5_400_000));
        assert_eq!(parse_duration("P1D"), Some(86_400_000));
        let lines = unfold("SUMMARY:Lang\r\n  weiter\r\nX:1");
        assert_eq!(lines[0], "SUMMARY:Lang weiter");
    }
}
