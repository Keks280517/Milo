//! Zaehler fuer die kostenlosen Gemini-Kontingente. Milo weiss dadurch vor jeder Anfrage, ob ein
//! Modell noch frei ist, und nimmt sonst gleich das naechste, statt erst gegen die Grenze zu laufen.
//!
//! Gezaehlt wird je Modell: Anfragen im gleitenden 60-Sekunden-Fenster, Token in diesem Fenster und
//! Anfragen am Tag. Der Tag wechselt wie bei Google um Mitternacht kalifornischer Zeit.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use chrono_tz::America::Los_Angeles;
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

/// Wie lange Milo hoechstens wartet, bis ein Modell in der Minute wieder frei ist, statt zu wechseln.
pub const MAX_WAIT_MS: i64 = 12_000;

/// Wie schwer eine Anfrage ist; danach richtet sich die Reihenfolge der Modelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    Easy,
    Hard,
    Report,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ModelUse {
    today: u32,
    tokens_today: u64,
    /// Google hat heute „Kontingent aufgebraucht“ gemeldet; bis zum Tageswechsel nicht mehr fragen.
    exhausted: bool,
    /// Zeitpunkte (ms) und Token der Anfragen der letzten Minute. Wird nicht gespeichert.
    #[serde(skip)]
    recent: VecDeque<(i64, u64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct State {
    /// Kalifornisches Datum, zu dem die Tageszaehler gehoeren.
    day: String,
    models: HashMap<String, ModelUse>,
}

/// Ergebnis der Pruefung eines Modells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Room {
    Free,
    /// In dieser Minute voll, in so vielen Millisekunden wieder frei.
    WaitMs(i64),
    /// Fuer heute aufgebraucht.
    Full,
}

/// Gewaehltes Modell fuer eine Anfrage.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub model: String,
    /// Vorher so lange warten (Minutengrenze).
    pub wait_ms: i64,
    /// Hinweis fuer den Nutzer, wenn nicht das eigentlich passende Modell genommen wird.
    pub note: Option<String>,
}

/// Eine Zeile fuer die Verbrauchsanzeige.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub model: String,
    /// Wofuer Milo das Modell nimmt: "leicht", "schwer", "schwer (2)".
    pub role: String,
    pub today: u32,
    pub rpd: u32,
    pub minute: u32,
    pub rpm: u32,
    pub tokens_today: u64,
    pub tokens_minute: u64,
    pub tpm: u32,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub rows: Vec<UsageRow>,
    /// Naechster Tageswechsel in ms seit 1970.
    pub reset_ms: i64,
}

pub struct Limits {
    path: Option<PathBuf>,
    state: Mutex<State>,
}

/// Kalifornisches Datum zu einem Zeitpunkt.
pub fn pacific_day(now: DateTime<Utc>) -> String {
    now.with_timezone(&Los_Angeles).format("%Y-%m-%d").to_string()
}

/// Naechste Mitternacht in Kalifornien, als UTC-Zeitpunkt.
pub fn next_reset(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&Los_Angeles).date_naive() + ChronoDuration::days(1);
    let midnight = local.and_hms_opt(0, 0, 0).unwrap();
    Los_Angeles
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now + ChronoDuration::hours(24))
}

/// Reserve, die fuer den Tag uebrig bleibt, damit nichts mitten in einer Aufgabe abbricht.
fn reserve(rpd: u32) -> u32 {
    if rpd >= 100 { 5 } else if rpd >= 10 { 2 } else { 0 }
}

impl Limits {
    pub fn load(path: PathBuf) -> Self {
        let state = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { path: Some(path), state: Mutex::new(state) }
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        Self { path: None, state: Mutex::new(State::default()) }
    }

    fn save(&self, state: &State) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(state) {
            if let Err(err) = std::fs::write(path, text) {
                log::warn!("Kontingent-Zaehler nicht speicherbar: {err}");
            }
        }
    }

    /// Setzt die Tageszaehler zurueck, wenn in Kalifornien ein neuer Tag begonnen hat, und
    /// vergisst Anfragen, die laenger als eine Minute her sind.
    fn roll(state: &mut State, now: DateTime<Utc>) {
        let day = pacific_day(now);
        if state.day != day {
            state.day = day;
            for usage in state.models.values_mut() {
                usage.today = 0;
                usage.tokens_today = 0;
                usage.exhausted = false;
            }
        }
        let cutoff = now.timestamp_millis() - 60_000;
        for usage in state.models.values_mut() {
            while usage.recent.front().is_some_and(|(t, _)| *t <= cutoff) {
                usage.recent.pop_front();
            }
        }
    }

    /// Hat das Modell Platz fuer `calls` weitere Anfragen heute und eine jetzt gleich?
    fn room_in(state: &State, settings: &Settings, model: &str, calls: u32, now: DateTime<Utc>) -> Room {
        let limit = settings.limit_for(model);
        let fresh = ModelUse::default();
        let usage = state.models.get(model).unwrap_or(&fresh);
        if usage.exhausted {
            return Room::Full;
        }
        // Eine laengere Aufgabe braucht mehrere Aufrufe; sie soll nicht mitten drin am Tageslimit scheitern.
        let usable = limit.rpd.saturating_sub(reserve(limit.rpd)).max(1);
        if usage.today + calls.max(1) > usable {
            return Room::Full;
        }
        let tokens: u64 = usage.recent.iter().map(|(_, t)| t).sum();
        let busy = usage.recent.len() as u32 >= limit.rpm || tokens >= limit.tpm as u64;
        if busy {
            let oldest = usage.recent.front().map(|(t, _)| *t).unwrap_or(now.timestamp_millis());
            return Room::WaitMs((oldest + 60_000 - now.timestamp_millis()).max(200));
        }
        Room::Free
    }

    /// Prueft ein einzelnes Modell (fuer Tests und die Anzeige).
    pub fn room(&self, settings: &Settings, model: &str, calls: u32) -> Room {
        self.room_at(settings, model, calls, Utc::now())
    }

    fn room_at(&self, settings: &Settings, model: &str, calls: u32, now: DateTime<Utc>) -> Room {
        let mut state = self.state.lock().unwrap();
        Self::roll(&mut state, now);
        Self::room_in(&state, settings, model, calls, now)
    }

    /// Reihenfolge der Modelle je Stufe, ohne doppelte.
    pub fn chain(settings: &Settings, tier: Tier) -> Vec<String> {
        let list = match tier {
            Tier::Easy => [&settings.model_easy, &settings.model_hard_2, &settings.model_hard],
            Tier::Hard | Tier::Report => [&settings.model_hard, &settings.model_hard_2, &settings.model_easy],
        };
        let mut out: Vec<String> = Vec::new();
        for model in list {
            if !out.contains(model) {
                out.push(model.clone());
            }
        }
        out
    }

    /// Waehlt das erste Modell der Reihenfolge, das jetzt Platz hat. `calls`: wie viele Aufrufe
    /// die Aufgabe voraussichtlich braucht. `skip`: Modelle, die in dieser Anfrage schon versagt haben.
    pub fn pick(&self, settings: &Settings, tier: Tier, calls: u32, skip: &[String]) -> Option<Pick> {
        self.pick_at(settings, tier, calls, skip, Utc::now())
    }

    fn pick_at(&self, settings: &Settings, tier: Tier, calls: u32, skip: &[String], now: DateTime<Utc>) -> Option<Pick> {
        let mut state = self.state.lock().unwrap();
        Self::roll(&mut state, now);
        // Wechsel zwischen gleich starken Modellen bleibt still; nur der Rueckfall auf das
        // leichte Modell bei einer schweren Aufgabe wird erwaehnt.
        let note = |model: &str| -> Option<String> {
            (tier != Tier::Easy && model == settings.model_easy)
                .then(|| tr!("Das Flash-Kontingent ist für heute aufgebraucht, ich antworte mit dem einfacheren Modell.", "The Flash quota is used up for today, I'll answer with the simpler model.").to_string())
        };
        for model in Self::chain(settings, tier) {
            if skip.contains(&model) {
                continue;
            }
            match Self::room_in(&state, settings, &model, calls, now) {
                Room::Free => return Some(Pick { note: note(&model), model, wait_ms: 0 }),
                // Kurz auf das bessere Modell warten ist besser als sofort ein schwaecheres.
                Room::WaitMs(ms) if ms <= MAX_WAIT_MS => return Some(Pick { note: note(&model), model, wait_ms: ms }),
                _ => {}
            }
        }
        None
    }

    /// Eine Anfrage an `model` beginnt jetzt.
    pub fn begin(&self, model: &str) {
        self.begin_at(model, Utc::now());
    }

    fn begin_at(&self, model: &str, now: DateTime<Utc>) {
        let mut state = self.state.lock().unwrap();
        Self::roll(&mut state, now);
        let usage = state.models.entry(model.to_string()).or_default();
        usage.today += 1;
        usage.recent.push_back((now.timestamp_millis(), 0));
        self.save(&state);
    }

    /// Token der zuletzt begonnenen Anfrage nachtragen (aus `usageMetadata`).
    pub fn add_tokens(&self, model: &str, tokens: u64) {
        let mut state = self.state.lock().unwrap();
        let usage = state.models.entry(model.to_string()).or_default();
        usage.tokens_today += tokens;
        if let Some(last) = usage.recent.back_mut() {
            last.1 += tokens;
        }
        self.save(&state);
    }

    /// Google meldet „aufgebraucht“: bis zum Tageswechsel nicht mehr fragen.
    pub fn mark_exhausted(&self, model: &str) {
        let mut state = self.state.lock().unwrap();
        Self::roll(&mut state, Utc::now());
        state.models.entry(model.to_string()).or_default().exhausted = true;
        self.save(&state);
    }

    /// Stand fuer die Verbrauchsanzeige.
    pub fn usage(&self, settings: &Settings) -> Usage {
        let now = Utc::now();
        let mut state = self.state.lock().unwrap();
        Self::roll(&mut state, now);
        let roles = [(&settings.model_easy, "leicht"), (&settings.model_hard, "schwer"), (&settings.model_hard_2, "schwer (2)")];
        let mut rows: Vec<UsageRow> = Vec::new();
        for (model, role) in roles {
            if let Some(row) = rows.iter_mut().find(|r| &r.model == model) {
                row.role = format!("{}, {role}", row.role);
                continue;
            }
            let limit = settings.limit_for(model);
            let usage = state.models.get(model.as_str()).cloned().unwrap_or_default();
            rows.push(UsageRow {
                model: model.clone(),
                role: role.into(),
                today: usage.today,
                rpd: limit.rpd,
                minute: usage.recent.len() as u32,
                rpm: limit.rpm,
                tokens_today: usage.tokens_today,
                tokens_minute: usage.recent.iter().map(|(_, t)| t).sum(),
                tpm: limit.tpm,
                exhausted: usage.exhausted,
            });
        }
        Usage { rows, reset_ms: next_reset(now).timestamp_millis() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn day_changes_at_midnight_in_california() {
        // 16. Oktober 2026, Sommerzeit in Kalifornien (UTC-7): Mitternacht = 07:00 UTC = 09:00 in Deutschland.
        assert_eq!(pacific_day(at(2026, 10, 16, 6, 59)), "2026-10-15");
        assert_eq!(pacific_day(at(2026, 10, 16, 7, 0)), "2026-10-16");
        assert_eq!(next_reset(at(2026, 10, 16, 12, 0)), at(2026, 10, 17, 7, 0));
        // Nach der Zeitumstellung in den USA (1. November) liegt Mitternacht bei 08:00 UTC.
        assert_eq!(next_reset(at(2026, 11, 10, 12, 0)), at(2026, 11, 11, 8, 0));
    }

    #[test]
    fn minute_window_makes_the_model_wait_then_free() {
        let limits = Limits::in_memory();
        let s = Settings::default();
        let t0 = at(2026, 10, 16, 12, 0);
        for i in 0..5 {
            limits.begin_at("gemini-3.8-flash", t0 + ChronoDuration::seconds(i));
        }
        let later = t0 + ChronoDuration::seconds(10);
        assert_eq!(limits.room_at(&s, "gemini-3.8-flash", 1, later), Room::WaitMs(50_000));
        assert_eq!(limits.room_at(&s, "gemini-3.8-flash", 1, t0 + ChronoDuration::seconds(61)), Room::Free);
    }

    #[test]
    fn daily_limit_keeps_a_reserve_and_resets_next_day() {
        let limits = Limits::in_memory();
        let s = Settings::default();
        let mut t = at(2026, 10, 16, 12, 0);
        for _ in 0..17 {
            limits.begin_at("gemini-3.8-flash", t);
            t += ChronoDuration::minutes(1);
        }
        // 20 pro Tag, 2 Reserve: eine einzelne Anfrage geht noch, ein Bericht mit 8 Aufrufen nicht.
        assert_eq!(limits.room_at(&s, "gemini-3.8-flash", 1, t), Room::Free);
        assert_eq!(limits.room_at(&s, "gemini-3.8-flash", 8, t), Room::Full);
        limits.begin_at("gemini-3.8-flash", t);
        assert_eq!(limits.room_at(&s, "gemini-3.8-flash", 1, t + ChronoDuration::minutes(1)), Room::Full);
        // Neuer Tag in Kalifornien: alles wieder frei.
        assert_eq!(limits.room_at(&s, "gemini-3.8-flash", 8, at(2026, 10, 17, 7, 1)), Room::Free);
    }

    #[test]
    fn exhausted_model_is_skipped_until_reset() {
        let limits = Limits::in_memory();
        let s = Settings::default();
        limits.begin_at("gemini-3.8-flash", Utc::now());
        limits.mark_exhausted("gemini-3.8-flash");
        let pick = limits.pick(&s, Tier::Hard, 1, &[]).unwrap();
        assert_eq!(pick.model, "gemini-3.7-flash");
        assert_eq!(pick.note, None, "gleich starkes Modell: kein Hinweis noetig");
    }

    #[test]
    fn chains_follow_the_tier_and_fall_back() {
        let s = Settings::default();
        assert_eq!(Limits::chain(&s, Tier::Easy), vec!["gemini-3.5-flash-lite", "gemini-3.7-flash", "gemini-3.8-flash"]);
        assert_eq!(Limits::chain(&s, Tier::Hard), vec!["gemini-3.8-flash", "gemini-3.7-flash", "gemini-3.5-flash-lite"]);
        let limits = Limits::in_memory();
        let first = limits.pick(&s, Tier::Easy, 1, &[]).unwrap();
        assert_eq!((first.model.as_str(), first.wait_ms, first.note.is_none()), ("gemini-3.5-flash-lite", 0, true));
        // Beide Flash-Modelle aufgebraucht: schwere Anfragen bekommen das leichte Modell mit Hinweis.
        limits.mark_exhausted("gemini-3.8-flash");
        limits.mark_exhausted("gemini-3.7-flash");
        let fallback = limits.pick(&s, Tier::Hard, 1, &[]).unwrap();
        assert_eq!(fallback.model, "gemini-3.5-flash-lite");
        assert!(fallback.note.unwrap().contains("aufgebraucht"));
        // In dieser Anfrage gescheiterte Modelle werden uebersprungen.
        assert_eq!(limits.pick(&s, Tier::Easy, 1, &["gemini-3.5-flash-lite".into()]), None);
    }

    #[test]
    fn short_waits_keep_the_better_model() {
        let limits = Limits::in_memory();
        let s = Settings::default();
        let t0 = Utc::now();
        for _ in 0..5 {
            limits.begin_at("gemini-3.8-flash", t0 - ChronoDuration::seconds(55));
        }
        let pick = limits.pick_at(&s, Tier::Hard, 1, &[], t0).unwrap();
        assert_eq!(pick.model, "gemini-3.8-flash");
        assert!(pick.wait_ms > 0 && pick.wait_ms <= MAX_WAIT_MS);
        // Laengeres Warten: lieber gleich das zweite Flash-Modell.
        let limits = Limits::in_memory();
        for _ in 0..5 {
            limits.begin_at("gemini-3.8-flash", t0 - ChronoDuration::seconds(5));
        }
        assert_eq!(limits.pick_at(&s, Tier::Hard, 1, &[], t0).unwrap().model, "gemini-3.7-flash");
    }

    #[test]
    fn tokens_count_into_the_minute_and_the_usage_view() {
        let limits = Limits::in_memory();
        let s = Settings::default();
        limits.begin("gemini-3.5-flash-lite");
        limits.add_tokens("gemini-3.5-flash-lite", 1200);
        let usage = limits.usage(&s);
        let lite = usage.rows.iter().find(|r| r.model == "gemini-3.5-flash-lite").unwrap();
        assert_eq!((lite.today, lite.minute, lite.tokens_today, lite.rpd), (1, 1, 1200, 500));
        assert_eq!(usage.rows.len(), 3);
        assert!(usage.reset_ms > Utc::now().timestamp_millis());
    }
}
