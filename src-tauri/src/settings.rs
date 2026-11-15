//! Einstellungen: eine JSON-Datei im Konfigurationsordner der App.

use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

pub const ARM_STYLES: [&str; 3] = ["capsule", "hands", "spring"];
/// Wuerfel der Leiste unten in der Insel (wie `SHELF_TABS` in Island.svelte).
pub const SHELF_TABS: [&str; 12] = ["home", "chat", "media", "agenda", "timer", "weather", "files", "notes", "system", "claude", "usage", "history"];

/// Karten, die die Startseite zeigen kann (wie `HOME_CARDS` in HomeView.svelte).
pub const HOME_CARDS: [&str; 9] = ["media", "agenda", "weather", "timer", "system", "claude", "notes", "usage", "files"];
/// Deko-Objekte, die es gibt (wie `DECOR` in decor.ts).
pub const DECOR: [&str; 16] = [
    "plant", "mailbox", "lantern", "cactus", "duck", "mug", "books", "lamp", "candle", "aquarium", "hourglass", "bonsai", "radio", "crystal", "globe", "teddy",
];

/// Kostenloses Kontingent eines Gemini-Modells (aus AI Studio abgelesen).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelLimit {
    pub model: String,
    /// Anfragen pro Minute.
    pub rpm: u32,
    /// Token pro Minute.
    pub tpm: u32,
    /// Anfragen pro Tag.
    pub rpd: u32,
}

impl Default for ModelLimit {
    fn default() -> Self {
        Self { model: String::new(), rpm: 5, tpm: 250_000, rpd: 20 }
    }
}

pub fn default_limits() -> Vec<ModelLimit> {
    vec![
        ModelLimit { model: "gemini-3.5-flash-lite".into(), rpm: 15, tpm: 250_000, rpd: 500 },
        ModelLimit { model: "gemini-3.8-flash".into(), rpm: 5, tpm: 250_000, rpd: 20 },
        ModelLimit { model: "gemini-3.7-flash".into(), rpm: 5, tpm: 250_000, rpd: 20 },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Name des Assistenten, zugleich das Aktivierungswort.
    pub name: String,
    pub arm_style: String,
    /// Vor jeder Anfrage an Gemini nachfragen.
    pub confirm_cloud: bool,
    /// Der Nutzer hat einmal zugestimmt, dass Anfragen an Google Gemini gehen.
    pub consent_given: bool,
    /// Abgelaufene Timer klingeln hoerbar (sichtbar klingeln sie immer).
    pub timer_sound: bool,
    /// Zuletzt abgefragter Ort fuer die Wetter-Ansicht; leer = noch keiner.
    pub weather_place: String,
    /// Ab dieser Wertung (0 bis 100) gilt eine Anfrage als schwer und bekommt das Flash-Modell.
    pub cloud_threshold: u8,
    /// Gemini-Modell fuer leichte Anfragen, fuer schwere und das zweite fuer schwere (eigenes Kontingent).
    pub model_easy: String,
    pub model_hard: String,
    pub model_hard_2: String,
    /// Kostenlose Kontingente je Modell; danach weicht Milo auf das naechste Modell aus.
    pub limits: Vec<ModelLimit>,
    /// Zeigt im Chat zu jeder Anfrage die Wertung mit allen Teilen (Debug-Ansicht).
    pub show_rating: bool,
    /// Die Insel zieht sich zurueck, solange auf ihrem Monitor etwas im Vollbild laeuft.
    pub hide_in_fullscreen: bool,
    /// Tastenkombination, die das Zuhoeren startet und beendet; leer = keine.
    pub hotkey: String,
    /// Auf "Hey Milo" warten. Dafuer bleibt das Mikrofon offen (sichtbar angezeigt).
    pub wake_word: bool,
    /// 1 = streng (weniger Fehlausloesungen), 2 = normal, 3 = empfindlich.
    pub wake_level: u8,
    /// Spracherkenner: "parakeet" (schnell) oder "whisper" (genauer, langsamer).
    pub stt_model: String,
    /// Antworten auf gesprochene Anfragen vorlesen.
    pub speak_answers: bool,
    /// Spielemodus: solange ein Spiel laeuft, zieht sich Milo vollstaendig zurueck.
    pub game_mode: bool,
    /// Programme (Dateiname), die immer bzw. nie als Spiel gelten.
    pub games_always: Vec<String>,
    pub games_never: Vec<String>,
    /// Wie oft Milo von selbst etwas unternimmt: 0 = ruhig, 1 = normal, 2 = verspielt.
    pub liveliness: u8,
    /// Wie lange Milo nach dem Sprechen wartet, bevor er die Aufnahme beendet: 1 = kurz, 2 = normal, 3 = lang.
    pub speech_pause: u8,
    /// Name des Nutzers, damit Milo ihn richtig schreibt (z. B. bei Profilen); leer = unbekannt.
    pub user_name: String,
    /// Morgens beim ersten Kontakt eine kurze Uebersicht des Tages.
    pub briefing: bool,
    /// Abends kurz nach dem Tag fragen und die Antwort als Notiz speichern.
    pub evening_review: bool,
    /// Ab dieser Stunde fragt Milo nach dem Tag (0 bis 23).
    pub evening_hour: u8,
    /// Tastenkombination fuer die Text-Werkzeuge (markierten Text umschreiben); leer = keine.
    pub text_hotkey: String,
    /// Milo spaziert ab und zu von der Insel auf die Taskleiste.
    pub pet_mode: bool,
    /// Wie oft: 0 = selten, 1 = normal, 2 = oft.
    pub pet_frequency: u8,
    /// Sagt an, wenn Claude Code fertig ist oder eine Freigabe braucht.
    pub claude_voice: bool,
    /// Freigaben von Claude Code in Milo zeigen (Erlauben/Ablehnen).
    pub claude_approvals: bool,
    /// Groq als schnelles Modell mitbenutzen (nur mit hinterlegtem Groq-Schluessel).
    pub groq_enabled: bool,
    /// Groq-Modell fuer schnelle Antworten und Vorueberlegungen.
    pub groq_model: String,
    /// Der Nutzer hat einmal zugestimmt, dass Anfragen an Groq gehen.
    pub groq_consent: bool,
    /// Wo Milo denkt: "cloud" (Gemini/Groq), "mixed" (Leichtes lokal, Schweres in der Cloud)
    /// oder "local" (nur die KI auf diesem PC, nichts geht ins Internet).
    pub ai_mode: String,
    /// Lokales Modell (Ollama- oder LM-Studio-Name), leer = noch keins gewaehlt.
    pub local_model: String,
    /// Zeitpunkt (ms), seit dem die Pflanze auf der Insel waechst; 0 = noch nicht gepflanzt.
    pub plant_born_ms: i64,
    /// Deko-Objekte in der Insel, in dieser Reihenfolge (Blumentopf, Briefkasten, ...).
    pub decor: Vec<String>,
    /// Deko-Objekte, die auf dem Desktop liegen (x als Anteil der Bildschirmbreite).
    pub desk_items: Vec<DeskItem>,
    /// Kalender-Abos (ICS-Adressen), eine pro Zeile.
    pub agenda_ics: String,
    /// Akzentfarbe der Oberflaeche.
    pub accent: String,
    /// Groesse der offenen Insel: 0 = kompakt, 1 = normal, 2 = breit.
    pub island_size: u8,
    /// Glaswirkung: 0 = matt, 1 = normal, 2 = klar.
    pub glass: u8,
    /// Weniger Bewegung (kurze Ueberblendungen statt Federn).
    pub reduce_motion: bool,
    /// Karten der Startseite (Home), in dieser Reihenfolge.
    pub home_cards: Vec<String>,
    /// Eigene Groesse je Karte der Startseite (s, m, w, t, l, f); fehlt sie, gilt die Automatik.
    pub home_sizes: std::collections::BTreeMap<String, String>,
    /// Sprache: "auto" (Windows-Anzeigesprache), "de" oder "en".
    pub language: String,
    /// Eigene Plaetze der Deko in der geschlossenen Insel (Anteil der Breite, 0 bis 1).
    pub decor_positions: std::collections::BTreeMap<String, f64>,
    /// Gesperrte Deko in der Insel: laesst sich nicht verschieben oder herausziehen.
    pub decor_locked: Vec<String>,
    /// Eigene Reihenfolge der Wuerfel in der Leiste unten (leer = Standard).
    pub shelf_order: Vec<String>,
    /// Ausgeblendete Wuerfel in der Leiste unten (der Chat laesst sich nicht ausblenden).
    pub shelf_hidden: Vec<String>,
    /// Der Willkommens-Assistent (Sprache, Name, Schluessel, kurze Tour) ist durchlaufen.
    pub onboarded: bool,
    /// So viele Minuten vor einem Termin erscheint er in der Pille; 0 = nie.
    pub event_lead: u16,
    /// Uhrzeit in der ruhenden Pille (neben dem Wetter).
    pub pill_clock: bool,
    /// Die offene Insel klappt zu, sobald man woanders hinklickt; aus = nur ueber den Pfeil oder Esc.
    pub close_on_blur: bool,
    /// Verlauf der Zwischenablage (nur im Arbeitsspeicher, Passwoerter nie).
    pub clipboard_history: bool,
    /// Nach einem Bildschirmfoto (Win+Umschalt+S) eine Karte in der Pille.
    pub screenshot_card: bool,
    /// Fertige Downloads in der Pille melden.
    pub download_card: bool,
    /// Akku von PC und Bluetooth-Geraeten unter 15 % melden.
    pub battery_alert: bool,
    /// Punkt in der Pille, solange ein Programm Mikrofon oder Kamera benutzt.
    pub privacy_dots: bool,
    /// Kurz vor Regen (am Wetter-Ort) in der Pille warnen.
    pub rain_alert: bool,
    /// Tastenkombination fuer eine Schnellnotiz direkt in der Pille; leer = aus.
    pub note_hotkey: String,
    /// Wecker (einmalig oder an Wochentagen).
    pub alarms: Vec<Alarm>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Alarm {
    pub id: String,
    /// "HH:MM"
    pub time: String,
    /// Wochentage 0 = Montag bis 6 = Sonntag; leer = einmalig.
    pub days: Vec<u8>,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DeskItem {
    pub kind: String,
    pub x: f64,
    /// Gesperrt: bleibt liegen, laesst sich nicht greifen.
    pub locked: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "Milo".into(),
            arm_style: "capsule".into(),
            confirm_cloud: false,
            consent_given: false,
            timer_sound: true,
            weather_place: String::new(),
            cloud_threshold: 60,
            model_easy: "gemini-3.5-flash-lite".into(),
            model_hard: "gemini-3.8-flash".into(),
            model_hard_2: "gemini-3.7-flash".into(),
            limits: default_limits(),
            show_rating: false,
            hide_in_fullscreen: true,
            hotkey: "Ctrl+Alt+K".into(),
            wake_word: false,
            wake_level: 2,
            stt_model: "parakeet".into(),
            speak_answers: false,
            game_mode: true,
            games_always: Vec::new(),
            games_never: Vec::new(),
            liveliness: 1,
            speech_pause: 2,
            user_name: String::new(),
            briefing: true,
            evening_review: true,
            evening_hour: 21,
            text_hotkey: "Ctrl+Alt+T".into(),
            pet_mode: true,
            pet_frequency: 1,
            claude_voice: true,
            claude_approvals: true,
            groq_enabled: true,
            groq_model: crate::groq::DEFAULT_MODEL.into(),
            groq_consent: false,
            ai_mode: "cloud".into(),
            local_model: String::new(),
            plant_born_ms: 0,
            decor: DECOR.iter().take(3).map(|s| s.to_string()).collect(),
            desk_items: Vec::new(),
            agenda_ics: String::new(),
            accent: "#64d2ff".into(),
            island_size: 1,
            glass: 1,
            reduce_motion: false,
            home_cards: HOME_CARDS.iter().take(3).map(|s| s.to_string()).collect(),
            home_sizes: Default::default(),
            language: "en".into(),
            decor_positions: Default::default(),
            decor_locked: Vec::new(),
            shelf_order: Vec::new(),
            shelf_hidden: Vec::new(),
            onboarded: false,
            event_lead: 30,
            pill_clock: true,
            close_on_blur: true,
            clipboard_history: true,
            screenshot_card: true,
            download_card: true,
            battery_alert: true,
            privacy_dots: true,
            rain_alert: true,
            note_hotkey: "Ctrl+Alt+N".into(),
            alarms: Vec::new(),
        }
    }
}

/// Erlaubte Zeichen eines Modellnamens; leer = ungueltig.
fn model_name(name: &str) -> Option<String> {
    let clean: String = name.trim().chars().filter(|c| c.is_ascii_alphanumeric() || "-._".contains(*c)).take(60).collect();
    (!clean.is_empty()).then_some(clean)
}

impl Settings {
    /// Kontingent eines Modells; unbekannte Modelle bekommen vorsichtige Werte.
    pub fn limit_for(&self, model: &str) -> ModelLimit {
        self.limits.iter().find(|l| l.model == model).cloned().unwrap_or(ModelLimit { model: model.to_string(), ..ModelLimit::default() })
    }

    /// Bringt ungueltige Werte in den erlaubten Bereich, statt die Datei abzulehnen.
    pub fn sanitized(mut self) -> Self {
        let defaults = Self::default();
        self.name = self.name.trim().chars().take(24).collect();
        if self.name.is_empty() {
            self.name = defaults.name;
        }
        self.weather_place = self.weather_place.trim().chars().take(80).collect();
        self.groq_model = self.groq_model.trim().chars().filter(|c| !c.is_whitespace() && !c.is_control()).take(80).collect();
        if self.groq_model.is_empty() {
            self.groq_model = defaults.groq_model.clone();
        }
        if !["cloud", "mixed", "local"].contains(&self.ai_mode.as_str()) {
            self.ai_mode = defaults.ai_mode.clone();
        }
        self.local_model = self.local_model.trim().chars().filter(|c| !c.is_whitespace() && !c.is_control()).take(120).collect();
        self.cloud_threshold = self.cloud_threshold.clamp(20, 95);
        self.wake_level = self.wake_level.clamp(1, 3);
        self.liveliness = self.liveliness.min(2);
        self.speech_pause = self.speech_pause.clamp(1, 3);
        self.user_name = self.user_name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
        let names = |list: &mut Vec<String>| {
            *list = list.iter().map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty() && n.len() <= 80 && !n.contains(['\\', '/'])).take(100).collect();
            list.dedup();
        };
        names(&mut self.games_always);
        names(&mut self.games_never);
        if self.stt_model != "whisper" {
            self.stt_model = defaults.stt_model;
        }
        let keys = |k: &str| k.trim().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '+').take(40).collect::<String>();
        self.hotkey = keys(&self.hotkey);
        self.text_hotkey = keys(&self.text_hotkey);
        if !self.text_hotkey.is_empty() && self.text_hotkey.eq_ignore_ascii_case(&self.hotkey) {
            self.text_hotkey.clear();
        }
        self.model_easy = model_name(&self.model_easy).unwrap_or(defaults.model_easy);
        self.model_hard = model_name(&self.model_hard).unwrap_or(defaults.model_hard);
        self.model_hard_2 = model_name(&self.model_hard_2).unwrap_or(defaults.model_hard_2);
        self.limits = self
            .limits
            .into_iter()
            .filter_map(|l| {
                let model = model_name(&l.model)?;
                Some(ModelLimit { model, rpm: l.rpm.clamp(1, 10_000), tpm: l.tpm.clamp(1000, 100_000_000), rpd: l.rpd.clamp(1, 1_000_000) })
            })
            .take(20)
            .collect();
        self.evening_hour = self.evening_hour.min(23);
        self.pet_frequency = self.pet_frequency.min(2);
        if !ARM_STYLES.contains(&self.arm_style.as_str()) {
            self.arm_style = defaults.arm_style;
        }
        let mut seen = Vec::new();
        self.decor.retain(|d| DECOR.contains(&d.as_str()) && !seen.contains(d) && {
            seen.push(d.clone());
            true
        });
        self.desk_items.retain(|d| DECOR.contains(&d.kind.as_str()) && d.x.is_finite());
        self.desk_items.truncate(24);
        for item in &mut self.desk_items {
            item.x = item.x.clamp(0.0, 1.0);
        }
        self.agenda_ics = self.agenda_ics.lines().map(str::trim).filter(|l| l.starts_with("https://") || l.starts_with("webcal://")).take(10).collect::<Vec<_>>().join("
");
        let hex = self.accent.trim();
        if !(hex.len() == 7 && hex.starts_with('#') && hex[1..].chars().all(|c| c.is_ascii_hexdigit())) {
            self.accent = defaults.accent;
        }
        self.island_size = self.island_size.min(2);
        self.event_lead = self.event_lead.min(240);
        self.alarms.truncate(30);
        for alarm in &mut self.alarms {
            alarm.days.retain(|d| *d < 7);
            alarm.days.sort_unstable();
            alarm.days.dedup();
            alarm.label = alarm.label.trim().chars().filter(|c| !c.is_control()).take(40).collect();
        }
        self.glass = self.glass.min(2);
        let mut seen_cards = Vec::new();
        self.home_cards.retain(|c| HOME_CARDS.contains(&c.as_str()) && !seen_cards.contains(c) && {
            seen_cards.push(c.clone());
            true
        });
        if !["auto", "de", "en"].contains(&self.language.as_str()) {
            self.language = "auto".into();
        }
        self.home_sizes.retain(|k, v| HOME_CARDS.contains(&k.as_str()) && ["s", "m", "w", "t", "l", "f"].contains(&v.as_str()));
        self.decor_positions.retain(|k, v| DECOR.contains(&k.as_str()) && v.is_finite());
        for v in self.decor_positions.values_mut() {
            *v = v.clamp(0.0, 1.0);
        }
        self.decor_locked.retain(|k| DECOR.contains(&k.as_str()));
        self.decor_locked.dedup();
        let mut seen_tabs = Vec::new();
        self.shelf_order.retain(|c| SHELF_TABS.contains(&c.as_str()) && !seen_tabs.contains(c) && {
            seen_tabs.push(c.clone());
            true
        });
        self.shelf_hidden.retain(|c| SHELF_TABS.contains(&c.as_str()) && c != "chat");
        self.shelf_hidden.sort();
        self.shelf_hidden.dedup();
        self
    }
}

pub struct SettingsStore {
    path: PathBuf,
    value: RwLock<Settings>,
}

impl SettingsStore {
    /// Laedt `settings.json` aus `dir`. Eine kaputte Datei wird beiseitegelegt und durch
    /// Standardwerte ersetzt, damit die App trotzdem startet.
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("settings.json");
        let value = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(mut settings) => {
                    // Umbenennung: der alte Standardname wird einmalig zu Milo.
                    if settings.name == "Kubo" {
                        settings.name = "Milo".into();
                    }
                    settings.sanitized()
                }
                Err(err) => {
                    log::warn!("settings.json ist unlesbar ({err}), verwende Standardwerte");
                    let _ = std::fs::rename(&path, dir.join("settings.kaputt.json"));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        crate::i18n::apply(&value.language);
        Self { path, value: RwLock::new(value) }
    }

    pub fn get(&self) -> Settings {
        self.value.read().unwrap().clone()
    }

    pub fn set(&self, new: Settings) -> Result<Settings, String> {
        let new = new.sanitized();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Einstellungsordner nicht anlegbar: {e}"))?;
        }
        let text = serde_json::to_string_pretty(&new).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, text).map_err(|e| format!("Einstellungen nicht speicherbar: {e}"))?;
        crate::i18n::apply(&new.language);
        *self.value.write().unwrap() = new.clone();
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("milo-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_gives_defaults() {
        let store = SettingsStore::load(temp_dir("missing"));
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn roundtrip_persists() {
        let dir = temp_dir("roundtrip");
        let store = SettingsStore::load(dir.clone());
        let mut s = store.get();
        s.name = "Bixo".into();
        s.evening_hour = 20;
        store.set(s.clone()).unwrap();
        assert_eq!(SettingsStore::load(dir).get(), s);
    }

    #[test]
    fn corrupt_file_falls_back_and_is_kept_aside() {
        let dir = temp_dir("corrupt");
        std::fs::write(dir.join("settings.json"), "{ kaputt").unwrap();
        let store = SettingsStore::load(dir.clone());
        assert_eq!(store.get(), Settings::default());
        assert!(dir.join("settings.kaputt.json").exists());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = temp_dir("partial");
        std::fs::write(dir.join("settings.json"), r#"{"name":"Lumi","gibtEsNicht":1}"#).unwrap();
        let s = SettingsStore::load(dir).get();
        assert_eq!(s.name, "Lumi");
        assert_eq!(s.model_hard, Settings::default().model_hard);
        assert_eq!(s.limits, default_limits());
    }

    #[test]
    fn sanitize_clamps_bad_values() {
        let s = Settings {
            name: "   ".into(),
            evening_hour: 99,
            model_hard: " ! ".into(),
            arm_style: "tentakel".into(),
            hotkey: "Ctrl+Alt+K".into(),
            text_hotkey: "ctrl+alt+k".into(),
            limits: vec![ModelLimit { model: "x/../y".into(), rpm: 0, tpm: 0, rpd: 0 }, ModelLimit { model: "".into(), ..ModelLimit::default() }],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.name, "Milo");
        assert_eq!(s.evening_hour, 23);
        assert_eq!(s.model_hard, Settings::default().model_hard);
        assert_eq!(s.arm_style, "capsule");
        assert_eq!(s.text_hotkey, "", "gleiche Taste wie das Zuhoeren geht nicht");
        assert_eq!(s.limits.len(), 1);
        assert_eq!((s.limits[0].model.as_str(), s.limits[0].rpm, s.limits[0].rpd), ("x..y", 1, 1));
    }

    #[test]
    fn limits_for_known_and_unknown_models() {
        let s = Settings::default();
        assert_eq!(s.limit_for("gemini-3.5-flash-lite").rpd, 500);
        assert_eq!(s.limit_for("gemini-3.8-flash").rpm, 5);
        let other = s.limit_for("gemini-9-pro");
        assert_eq!((other.rpm, other.rpd), (5, 20));
    }
}
