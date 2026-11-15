//! Werkzeuge, die das Modell aufrufen kann. Jedes Werkzeug hat eine Beschreibung fuer das Modell
//! (`definitions`) und eine Ausfuehrung (`execute`). Riskante Aktionen fragen vorher nach.

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::apps::{self, AppIndex};
use crate::memory::Memory;
use crate::gemini::{Call, Thinking};
use crate::limits::Tier;
use crate::settings::{Settings, SettingsStore};
use crate::system::{self, MediaKey, PowerAction};
use crate::{browser, confirm, docs, media, pc, screens, scripts, timers, web, youtube};

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Link {
    pub title: String,
    pub url: String,
}

pub struct Outcome {
    /// Ergebnistext fuer das Modell.
    pub result: String,
    /// Kurze Zeile fuer Oberflaeche und Protokoll.
    pub summary: String,
    pub ok: bool,
    /// Quellen, die in der Oberflaeche anklickbar erscheinen.
    pub links: Vec<Link>,
    /// Daten fuer eine fertige Ansicht in der Oberflaeche (Wetter, Timer, Quellen ...).
    /// Das Modell muss nichts gestalten, die Vorlage wird nur gefuellt.
    pub card: Option<Value>,
    /// Der Nutzer hat die Sicherheitsabfrage abgelehnt.
    pub declined: bool,
    /// Wie sich die Aktion im Protokoll rueckgaengig machen laesst (siehe `actionlog::undo`).
    pub undo: Option<Value>,
}

impl Outcome {
    pub fn done(summary: impl Into<String>, result: impl Into<String>) -> Self {
        Self { result: result.into(), summary: summary.into(), ok: true, links: Vec::new(), card: None, declined: false, undo: None }
    }

    pub fn fail(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            // Deutlich formuliert, damit das Modell danach keinen Erfolg behauptet.
            result: format!("FEHLGESCHLAGEN: {message} Es wurde nichts ausgeführt. Sag das dem Nutzer ehrlich."),
            summary: message,
            ok: false,
            links: Vec::new(),
            card: None,
            declined: false,
            undo: None,
        }
    }

    /// Der Nutzer hat in der Sicherheitsabfrage auf Abbrechen gedrueckt (oder nicht geantwortet).
    pub fn declined(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            result: format!(
                "NICHT AUSGEFÜHRT: Der Nutzer hat die Sicherheitsabfrage abgelehnt. {message} Bestätige ihm nur kurz, dass nichts passiert ist."
            ),
            summary: message,
            ok: false,
            links: Vec::new(),
            card: None,
            declined: true,
            undo: None,
        }
    }

    pub fn with_card(mut self, card: Value) -> Self {
        self.card = Some(card);
        self
    }

    pub fn with_undo(mut self, undo: Value) -> Self {
        self.undo = Some(undo);
        self
    }

    /// Zustand fuer die Anzeige: ok, error oder declined.
    pub fn status(&self) -> &'static str {
        match (self.ok, self.declined) {
            (true, _) => "ok",
            (false, true) => "declined",
            (false, false) => "error",
        }
    }
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": { "type": "object", "properties": properties, "required": required },
        },
    })
}

/// Werkzeugbeschreibungen (JSON-Schema); fuer Gemini wandelt `gemini::declaration` sie um.
pub fn definitions(_settings: &Settings) -> Vec<Value> {
    let mut tools = vec![
        tool(
            "open_app",
            "Startet ein auf dem PC installiertes Programm oder Spiel, auf Wunsch auf einem bestimmten Bildschirm.",
            json!({
                "name": { "type": "string", "description": "Name des Programms, so wie der Nutzer es nennt, z. B. 'Spotify' oder 'Rechner'" },
                "screen": { "type": "string", "description": "Nur wenn der Nutzer einen Bildschirm nennt: '2', 'zweiter', 'links', 'rechts', 'Haupt'. Öffnet ein neues Fenster maximiert auf diesem Bildschirm." },
            }),
            &["name"],
        ),
        tool(
            "close_app",
            "Schließt ein laufendes Programm. Der Nutzer wird vorher gefragt. 'Browser' schließt alle Browserfenster, egal welcher Browser.",
            json!({ "name": { "type": "string", "description": "Name des Programms, so wie es läuft (z. B. 'Brave', 'Discord'), oder 'Browser'" } }),
            &["name"],
        ),
        tool(
            "open_website",
            "Öffnet eine Webseite im Standardbrowser (neuer Tab), mit screen in einem neuen Fenster auf dem gewünschten Bildschirm.",
            json!({
                "url": { "type": "string", "description": "Adresse, z. B. https://www.netflix.com" },
                "screen": { "type": "string", "description": "Nur wenn der Nutzer einen Bildschirm nennt: '2', 'zweiter', 'links', 'rechts', 'Haupt'. Öffnet ein neues Fenster maximiert auf diesem Bildschirm." },
            }),
            &["url"],
        ),
        tool(
            "browser_search",
            "Öffnet eine Suche sichtbar im Browser, wenn der Nutzer die Ergebnisse selbst sehen will (z. B. Videos auf YouTube).",
            json!({
                "query": { "type": "string", "description": "Suchbegriffe" },
                "site": { "type": "string", "enum": ["google", "youtube", "wikipedia", "bilder", "maps", "twitch"], "description": "Wo gesucht wird" },
                "screen": { "type": "string", "description": "Nur wenn der Nutzer einen Bildschirm nennt: '2', 'zweiter', 'links', 'rechts', 'Haupt'. Öffnet ein neues Fenster maximiert auf diesem Bildschirm." },
            }),
            &["query"],
        ),
        tool(
            "set_timer",
            "Stellt einen Timer oder eine Erinnerung. Entweder eine Dauer ODER eine Uhrzeit angeben.",
            json!({
                "duration_seconds": { "type": "integer", "description": "Dauer in Sekunden, z. B. 600 für 10 Minuten" },
                "at_time": { "type": "string", "description": "Uhrzeit im Format HH:MM, z. B. 18:00 für 'um 18 Uhr'" },
                "day": { "type": "string", "description": "Nur für Erinnerungen an einem anderen Tag: 'morgen', 'übermorgen', ein Wochentag ('montag') oder ein Datum YYYY-MM-DD. Ohne at_time gilt 09:00." },
                "label": { "type": "string", "description": "Wofür der Timer ist, z. B. 'Pizza'" },
            }),
            &[],
        ),
        tool(
            "set_alarm",
            "Stellt einen Wecker zu einer Uhrzeit, einmalig oder wiederholt an Wochentagen (z. B. werktags um 7).",
            json!({
                "time": { "type": "string", "description": "Uhrzeit HH:MM" },
                "days": { "type": "array", "items": { "type": "integer" }, "description": "Wochentage 0=Montag bis 6=Sonntag; leer = einmalig" },
                "label": { "type": "string", "description": "Bezeichnung, z. B. 'Aufstehen'" },
            }),
            &["time"],
        ),
        tool(
            "focus_mode",
            "Startet oder beendet einen Fokus-Modus (Pomodoro): Konzentrationszeit mit anschließender Pause. Während des Fokus ist Milo still.",
            json!({
                "action": { "type": "string", "enum": ["start", "stop"], "description": "start oder stop" },
                "minutes": { "type": "integer", "description": "Fokus-Dauer in Minuten, Standard 25" },
            }),
            &[],
        ),
        tool("list_timers", "Listet die laufenden Timer auf.", json!({}), &[]),
        tool(
            "cancel_timer",
            "Bricht einen Timer ab.",
            json!({ "label": { "type": "string", "description": "Bezeichnung des Timers; weglassen, wenn nur einer läuft" } }),
            &[],
        ),
        tool(
            "save_note",
            "Merkt sich etwas dauerhaft (Notiz im lokalen Gedächtnis).",
            json!({ "text": { "type": "string", "description": "Was gemerkt werden soll, als vollständiger Satz" } }),
            &["text"],
        ),
        tool(
            "search_notes",
            "Sucht im Gedächtnis nach früher gemerkten Dingen. Immer benutzen, wenn der Nutzer nach etwas fragt, das er dir gesagt haben könnte.",
            json!({ "query": { "type": "string", "description": "Stichwörter; leer lassen für die neuesten Notizen" } }),
            &[],
        ),
        tool("list_files", "Listet die Dateien auf, die der Nutzer dir gegeben hat.", json!({}), &[]),
        tool(
            "read_file",
            "Liest den Inhalt einer Datei, die der Nutzer dir gegeben hat. Immer benutzen, bevor du Fragen zu einer Datei beantwortest oder sie zusammenfasst.",
            json!({
                "name": { "type": "string", "description": "Dateiname oder ein Teil davon; leer = die zuletzt erhaltene Datei" },
                "part": { "type": "integer", "description": "Bei langen Dateien: welcher Abschnitt (1, 2, 3 ...). Standard 1" },
            }),
            &[],
        ),
        tool(
            "media_control",
            "Steuert die laufende Musik- oder Videowiedergabe und die Lautstärke.",
            json!({
                "action": { "type": "string", "enum": ["play", "pause", "next", "previous", "rewind", "forward", "volume_up", "volume_down", "mute"] },
                "seconds": { "type": "integer", "description": "Nur bei rewind/forward: wie viele Sekunden, Standard 10" },
            }),
            &["action"],
        ),
        tool(
            "system_power",
            "Fährt den PC herunter, startet ihn neu, versetzt ihn in den Energiesparmodus, sperrt ihn oder schaltet den Bildschirm aus, auf Wunsch zeitversetzt. Läuft ohne Rückfrage; bis zum Zeitpunkt kann der Nutzer abbrechen.",
            json!({
                "action": { "type": "string", "enum": ["shutdown", "restart", "sleep", "lock", "screen_off", "abort"], "description": "sleep = Energie sparen; abort bricht einen geplanten Befehl ab" },
                "in_minutes": { "type": "integer", "description": "Nach wie vielen Minuten, z. B. 5 für 'in fünf Minuten'; weglassen für sofort" },
            }),
            &["action"],
        ),
    ];
    tools.push(tool(
        "workspace",
        "Arbeitsplätze: merkt sich die offenen Programme samt Fensterlage unter einem Namen (save), stellt sie wieder her und startet fehlende Programme (open), listet sie (list) oder löscht einen (delete).",
        json!({
            "action": { "type": "string", "enum": ["save", "open", "list", "delete"] },
            "name": { "type": "string", "description": "Name des Arbeitsplatzes, z. B. Schule oder Zocken" },
        }),
        &["action"],
    ));
    tools.push(tool(
        "window_control",
        "Ordnet Fenster an: auf einen anderen Bildschirm schieben, maximieren, minimieren, wiederherstellen, linke oder rechte Bildschirmhälfte, nach vorne holen (zu einem Programm wechseln) oder den Desktop zeigen.",
        json!({
            "action": { "type": "string", "enum": ["move", "maximize", "minimize", "restore", "left_half", "right_half", "focus", "show_desktop"] },
            "target": { "type": "string", "description": "Programm oder Fenstertitel, z. B. 'Spotify', 'Discord' oder 'Browser'; leer = das vorderste Fenster" },
            "screen": { "type": "string", "description": "Bei move, maximize, left_half, right_half: '1', '2', 'zweiter', 'links', 'rechts'. Leer = der Bildschirm, auf dem das Fenster schon ist" },
        }),
        &["action"],
    ));
    tools.push(tool(
        "set_volume",
        "Stellt die Lautstärke des PCs auf einen genauen Wert in Prozent.",
        json!({ "percent": { "type": "integer", "description": "0 bis 100" } }),
        &["percent"],
    ));
    tools.push(tool(
        "open_folder",
        "Öffnet einen bekannten Ordner im Explorer: downloads, dokumente, bilder, musik, videos oder desktop.",
        json!({ "name": { "type": "string" } }),
        &["name"],
    ));
    tools.push(tool(
        "system_info",
        "Liefert Speicherplatz der Laufwerke, Arbeitsspeicher, Prozessorauslastung, die größten Programme, Akku und Laufzeit.",
        json!({}),
        &[],
    ));
    tools.push(tool(
        "clipboard",
        "Liest die Zwischenablage (nur wenn der Nutzer ausdrücklich danach fragt) oder legt Text hinein.",
        json!({
            "action": { "type": "string", "enum": ["read", "write"] },
            "text": { "type": "string", "description": "Nur bei write: der Text" },
        }),
        &["action"],
    ));
    tools.push(tool(
        "browser_click",
        "Klickt im geöffneten Browser auf ein Element mit diesem sichtbaren Text, z. B. ein Netflix-Profil ('Philipp'), eine Serie ('The Middle') oder einen Knopf ('Abspielen'). Für mehrere Schritte nacheinander mehrmals aufrufen. Klickt nie auf Kaufen, Löschen, Senden oder Konto-Einstellungen.",
        json!({ "target": { "type": "string", "description": "Sichtbarer Text oder Name des Elements" } }),
        &["target"],
    ));
    tools.push(tool(
        "browser_type",
        "Tippt Text in das Feld, das im Browser gerade ausgewählt ist (z. B. nach einem Klick auf das Suchfeld). Nie Passwörter.",
        json!({
            "text": { "type": "string" },
            "enter": { "type": "boolean", "description": "Danach Enter drücken" },
        }),
        &["text"],
    ));
    tools.push(tool(
        "run_script",
        "Führt ein kurzes PowerShell-Skript auf dem PC aus, wenn es für den Wunsch kein eigenes Werkzeug gibt (z. B. Dateien zählen, Systeminformationen auslesen). Der Nutzer sieht das Skript vorher und muss zustimmen. Keine Administratorrechte.",
        json!({
            "code": { "type": "string", "description": "Das vollständige PowerShell-Skript" },
            "purpose": { "type": "string", "description": "In einem Satz, was das Skript tut" },
        }),
        &["code", "purpose"],
    ));
    tools.push(tool(
        "look_at_screen",
        "Macht ein Bildschirmfoto und beantwortet eine Frage dazu. Nur benutzen, wenn der Nutzer ausdrücklich möchte, dass du auf seinen Bildschirm schaust.",
        json!({ "question": { "type": "string", "description": "Was der Nutzer über den Bildschirminhalt wissen will" } }),
        &["question"],
    ));
    {
        tools.push(tool(
            "get_weather",
            "Liefert das aktuelle Wetter und die Vorhersage der nächsten Tage für einen Ort.",
            json!({ "location": { "type": "string", "description": "Stadt, z. B. 'Hamburg'" } }),
            &["location"],
        ));
        tools.push(tool(
            "play_youtube",
            "Sucht auf YouTube und öffnet direkt das erste passende Video, auf Wunsch von einem bestimmten Kanal.",
            json!({
                "query": { "type": "string", "description": "Wonach gesucht wird" },
                "channel": { "type": "string", "description": "Name des Kanals, falls genannt" },
                "screen": { "type": "string", "description": "Nur wenn der Nutzer einen Bildschirm nennt: '2', 'zweiter', 'links', 'rechts', 'Haupt'. Öffnet ein neues Fenster maximiert auf diesem Bildschirm." },
            }),
            &["query"],
        ));
        tools.push(tool(
            "web_research",
            "Recherchiert im Internet und liefert dir Fundstellen als Text. Benutze das für aktuelle Informationen (Nachrichten, Preise, Ergebnisse, Termine) oder wenn du dir bei einer Tatsache nicht sicher bist.",
            json!({ "query": { "type": "string", "description": "Suchanfrage in wenigen Stichwörtern" } }),
            &["query"],
        ));
    }
    tools.push(tool(
        "browser_read",
        "Liest den sichtbaren Text der Seite, die gerade im Browser offen ist (z. B. um zu prüfen, was dort steht, oder um einen Artikel zusammenzufassen).",
        json!({}),
        &[],
    ));
    tools.push(tool(
        "browser_list",
        "Listet die anklickbaren Elemente (Knöpfe, Links, Profile, Titel) der Seite im Browser auf. Hilft, den genauen Namen für browser_click zu finden.",
        json!({}),
        &[],
    ));
    tools.push(tool(
        "browser_scroll",
        "Blättert die Seite im Browser nach unten oder oben.",
        json!({
            "direction": { "type": "string", "enum": ["down", "up"] },
            "steps": { "type": "integer", "description": "Wie weit, 1 bis 30; Standard 5" },
        }),
        &["direction"],
    ));
    tools.push(tool(
        "browser_key",
        "Drückt eine Taste im Browser: enter, escape, tab, space (Video anhalten/weiter), back (zurück), forward, reload, new_tab, close_tab, next_tab, previous_tab, fullscreen (Video-Vollbild), page_down, page_up.",
        json!({ "key": { "type": "string", "enum": browser::KEYS } }),
        &["key"],
    ));
    tools.push(tool(
        "twitch_live",
        "Prüft, ob Twitch-Kanäle gerade live sind, und liefert Streamtitel und Adresse. Mehrere Kandidaten auf einmal prüfen, z. B. für offizielle Kanäle: ['valorant', 'valorant_emea', 'valorant_americas', 'valorant_pacific'].",
        json!({ "channels": { "type": "array", "items": { "type": "string" }, "description": "Kanalnamen wie in twitch.tv/<name>, bis zu 8" } }),
        &["channels"],
    ));
    tools.push(tool(
        "find_links",
        "Schnelle Websuche, die nur Titel und Adressen liefert. Nimm das, um die richtige Seite zu finden (offizieller Kanal, Shop-Seite, Anleitung) und sie dann direkt mit open_website zu öffnen.",
        json!({ "query": { "type": "string", "description": "Suchanfrage" } }),
        &["query"],
    ));
    tools.push(tool(
        "browser_tabs",
        "Tabs im Browser: auflisten, zu einem Tab wechseln oder einen Tab schließen (nach Titel).",
        json!({
            "action": { "type": "string", "enum": ["list", "switch", "close"] },
            "title": { "type": "string", "description": "Bei switch und close: Titel oder Teil des Titels, z. B. 'YouTube'" },
        }),
        &["action"],
    ));
    tools.push(tool(
        "browser_search_site",
        &format!(
            "Sucht direkt auf einer bekannten Seite und öffnet gleich die Trefferliste, ohne Startseite, Vorschauen und Suchfeld. \
             Danach liefert es die anklickbaren Treffer; klick dann mit browser_click auf den passenden Titel und anschließend auf Abspielen. \
             Bekannte Seiten: {}.",
            browser::search_site_names().join(", ")
        ),
        json!({
            "site": { "type": "string", "description": "Seite, z. B. 'Netflix', 'YouTube', 'Prime Video'" },
            "query": { "type": "string", "description": "Wonach gesucht wird, z. B. 'Türkisch für Anfänger'" },
        }),
        &["site", "query"],
    ));
    tools.push(tool(
        "browser_navigate",
        "Öffnet eine Adresse im aktuellen Browser-Tab (statt in einem neuen Tab).",
        json!({ "url": { "type": "string" } }),
        &["url"],
    ));
    tools.push(tool(
        "save_screenshot",
        "Speichert ein Bildschirmfoto als PNG im Ordner Bilder\\Milo. Nur auf ausdrücklichen Wunsch.",
        json!({ "screen": { "type": "string", "description": "Welcher Bildschirm ('1', '2', 'links' ...); leer = Hauptbildschirm" } }),
        &[],
    ));
    tools.push(tool(
        "now_playing",
        "Sagt, welche Musik oder welches Video gerade läuft (Titel, Interpret, Programm).",
        json!({}),
        &[],
    ));
    tools.push(tool(
        "deep_research",
        "Gründliche Recherche zu einem Unterthema für einen Bericht: liest mehrere Webseiten und den passenden Wikipedia-Artikel und liefert verdichtete Notizen mit Quellen.",
        json!({ "topic": { "type": "string", "description": "Unterthema als Suchanfrage, z. B. 'Deutschland Geschichte 20. Jahrhundert'" } }),
        &["topic"],
    ));
    tools.push(tool(
        "create_word_document",
        "Speichert einen fertigen Text als Word-Datei (.docx) im Ordner Dokumente\\Milo und öffnet sie.",
        json!({
            "title": { "type": "string", "description": "Titel des Dokuments" },
            "subtitle": { "type": "string", "description": "Optionaler Untertitel" },
            "sections": {
                "type": "array",
                "description": "Abschnitte in der richtigen Reihenfolge",
                "items": {
                    "type": "object",
                    "properties": {
                        "heading": { "type": "string" },
                        "paragraphs": { "type": "array", "items": { "type": "string" }, "description": "Absätze als ganze Sätze" },
                        "bullets": { "type": "array", "items": { "type": "string" }, "description": "Optionale Aufzählungspunkte" },
                    },
                    "required": ["heading", "paragraphs"],
                },
            },
            "sources": {
                "type": "array",
                "items": { "type": "object", "properties": { "title": { "type": "string" }, "url": { "type": "string" } } },
            },
        }),
        &["title", "sections"],
    ));
    tools
}

fn text(args: &Value, key: &str) -> String {
    match &args[key] {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Zahl aus einem Argument; Modelle liefern Zahlen gelegentlich als Text.
fn number(args: &Value, key: &str) -> Option<f64> {
    match &args[key] {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Kurze Beschreibung eines Aufrufs fuer die Anzeige, waehrend er laeuft.
pub fn describe(call: &Call) -> String {
    let args = &call.args;
    match call.name.as_str() {
        "open_app" => tr!(format!("Öffne {} …", text(args, "name")), format!("Opening {} …", text(args, "name"))),
        "close_app" => tr!(format!("Schließe {} …", text(args, "name")), format!("Closing {} …", text(args, "name"))),
        "open_website" => tr!(format!("Öffne {} …", text(args, "url")), format!("Opening {} …", text(args, "url"))),
        "browser_search" => tr!(format!("Suche nach „{}“ …", text(args, "query")), format!("Searching for \"{}\" …", text(args, "query"))),
        "set_timer" => tr!("Stelle Timer …", "Setting a timer …").into(),
        "list_timers" => tr!("Schaue nach den Timern …", "Checking the timers …").into(),
        "cancel_timer" => tr!("Breche Timer ab …", "Cancelling the timer …").into(),
        "save_note" => tr!("Merke mir das …", "Remembering that …").into(),
        "search_notes" => tr!("Schaue im Gedächtnis nach …", "Checking my memory …").into(),
        "list_files" => tr!("Schaue nach deinen Dateien …", "Checking your files …").into(),
        "read_file" => tr!("Lese die Datei …", "Reading the file …").into(),
        "media_control" => tr!("Steuere die Wiedergabe …", "Controlling playback …").into(),
        "system_power" => tr!("Systembefehl …", "System command …").into(),
        "workspace" => match text(args, "action").as_str() {
            "save" => tr!(format!("Merke mir den Arbeitsplatz „{}“ …", text(args, "name")), format!("Saving the workspace \"{}\" …", text(args, "name"))),
            "open" => tr!(format!("Stelle den Arbeitsplatz „{}“ her …", text(args, "name")), format!("Restoring the workspace \"{}\" …", text(args, "name"))),
            _ => tr!("Arbeitsplätze …", "Workspaces …").into(),
        },
        "run_script" => tr!("Bereite ein Skript vor …", "Preparing a script …").into(),
        "set_volume" => tr!(format!("Stelle die Lautstärke auf {} % …", text(args, "percent")), format!("Setting the volume to {} % …", text(args, "percent"))),
        "open_folder" => tr!(format!("Öffne {} …", text(args, "name")), format!("Opening {} …", text(args, "name"))),
        "system_info" => tr!("Schaue nach dem PC …", "Checking the PC …").into(),
        "clipboard" => tr!("Zwischenablage …", "Clipboard …").into(),
        "browser_click" => tr!(format!("Klicke auf „{}“ …", text(args, "target")), format!("Clicking \"{}\" …", text(args, "target"))),
        "browser_type" => tr!("Tippe im Browser …", "Typing in the browser …").into(),
        "play_youtube" => tr!(format!("Suche „{}“ auf YouTube …", text(args, "query")), format!("Searching YouTube for \"{}\" …", text(args, "query"))),
        "look_at_screen" => tr!("Schaue auf den Bildschirm …", "Looking at the screen …").into(),
        "get_weather" => tr!(format!("Hole das Wetter für {} …", text(args, "location")), format!("Getting the weather for {} …", text(args, "location"))),
        "web_research" => tr!(format!("Recherchiere „{}“ …", text(args, "query")), format!("Researching \"{}\" …", text(args, "query"))),
        "deep_research" => tr!(format!("Recherchiere gründlich: {} …", text(args, "topic")), format!("Researching in depth: {} …", text(args, "topic"))),
        "create_word_document" => tr!(format!("Schreibe die Word-Datei „{}“ …", text(args, "title")), format!("Writing the Word file \"{}\" …", text(args, "title"))),
        "browser_read" => tr!("Lese die Seite …", "Reading the page …").into(),
        "browser_list" => tr!("Schaue, was auf der Seite anklickbar ist …", "Checking what can be clicked on the page …").into(),
        "browser_scroll" => tr!("Blättere …", "Scrolling …").into(),
        "browser_key" => tr!(format!("Drücke {} …", text(args, "key")), format!("Pressing {} …", text(args, "key"))),
        "window_control" => tr!("Ordne das Fenster an …", "Arranging the window …").into(),
        "twitch_live" => tr!("Schaue, wer auf Twitch live ist …", "Checking who is live on Twitch …").into(),
        "find_links" => tr!(format!("Suche „{}“ …", text(args, "query")), format!("Searching \"{}\" …", text(args, "query"))),
        "browser_tabs" => tr!("Schaue nach den Tabs …", "Checking the tabs …").into(),
        "browser_navigate" => tr!(format!("Öffne {} …", text(args, "url")), format!("Opening {} …", text(args, "url"))),
        "browser_search_site" => tr!(format!("Suche „{}“ auf {} …", text(args, "query"), text(args, "site")), format!("Searching \"{}\" on {} …", text(args, "query"), text(args, "site"))),
        "save_screenshot" => tr!("Mache ein Bildschirmfoto …", "Taking a screenshot …").into(),
        "now_playing" => tr!("Schaue, was gerade läuft …", "Checking what is playing …").into(),
        other => format!("{other} …"),
    }
}

pub async fn execute(app: &AppHandle, call: &Call) -> Outcome {
    let name = call.name.as_str();
    let started = std::time::Instant::now();
    let outcome = run(app, name, &call.args).await;
    crate::actionlog::record(app, name, &call.args, &outcome, started.elapsed());
    outcome
}

async fn run(app: &AppHandle, name: &str, args: &Value) -> Outcome {
    match name {
        "open_app" => open_app(app, &text(args, "name"), &text(args, "screen")).await,
        "window_control" => window_control(&text(args, "action"), &text(args, "target"), &text(args, "screen")).await,
        "run_script" => run_script(app, &text(args, "code"), &text(args, "purpose")).await,
        "set_volume" => match number(args, "percent") {
            Some(percent) if (0.0..=100.0).contains(&percent) => match tokio::task::spawn_blocking(move || pc::set_volume(percent.round() as u32)).await {
                Ok(Ok(before)) => Outcome::done(tr!(format!("Lautstärke {percent:.0} %"), format!("Volume {percent:.0} %")), tr!(format!("Lautstärke von {before} % auf {percent:.0} % gestellt."), format!("Volume changed from {before} % to {percent:.0} %.")))
                    .with_undo(json!({ "kind": "volume", "percent": before })),
                Ok(Err(err)) => Outcome::fail(err.to_string()),
                Err(err) => Outcome::fail(err.to_string()),
            },
            _ => Outcome::fail(tr!("Die Lautstärke muss zwischen 0 und 100 Prozent liegen.", "The volume has to be between 0 and 100 percent.")),
        },
        "open_folder" => {
            let name = text(args, "name");
            match pc::known_folder(&name) {
                Some(path) => match system::shell_open(&path.to_string_lossy()) {
                    Ok(()) => Outcome::done(tr!(format!("Ordner {} geöffnet", path.display()), format!("Opened the folder {}", path.display())), tr!(format!("Der Ordner {} wurde geöffnet.", path.display()), format!("The folder {} was opened.", path.display()))),
                    Err(err) => Outcome::fail(err.to_string()),
                },
                None => Outcome::fail(tr!(format!("Den Ordner „{name}“ kenne ich nicht. Ich kann Downloads, Dokumente, Bilder, Musik, Videos und Desktop öffnen."), format!("I don't know the folder \"{name}\". I can open Downloads, Documents, Pictures, Music, Videos and Desktop."))),
            }
        }
        "system_info" => match pc::system_info().await {
            Ok(report) => Outcome::done(tr!("Systeminfos gelesen", "Read system info"), report),
            Err(err) => Outcome::fail(err.to_string()),
        },
        "clipboard" => match text(args, "action").as_str() {
            "write" => {
                let content = text(args, "text");
                if content.is_empty() {
                    return Outcome::fail(tr!("Was soll in die Zwischenablage?", "What should go on the clipboard?"));
                }
                let written = match crate::textwork::clipboard_write(&content) {
                    Ok(()) => Ok(()),
                    Err(_) => pc::clipboard_write(&content).await,
                };
                match written {
                    Ok(()) => Outcome::done(tr!("In die Zwischenablage gelegt", "Copied to the clipboard"), tr!("Der Text liegt jetzt in der Zwischenablage.", "The text is now on the clipboard.")),
                    Err(err) => Outcome::fail(err.to_string()),
                }
            }
            _ if crate::guard::clipboard_protected() => Outcome::fail(
                tr!("In der Zwischenablage liegt etwas Geschütztes (z. B. ein Passwort aus einem Passwortmanager). Das lese ich nicht.", "There's something protected on the clipboard (e.g. a password from a password manager). I won't read it."),
            ),
            _ => match Ok::<String, anyhow::Error>(crate::textwork::clipboard_read().unwrap_or_default()) {
                Ok(content) if content.trim().is_empty() => Outcome::done(tr!("Zwischenablage ist leer", "Clipboard is empty"), tr!("Die Zwischenablage enthält keinen Text.", "The clipboard doesn't contain any text.")),
                Ok(content) => Outcome::done(tr!("Zwischenablage gelesen", "Read the clipboard"), format!("Inhalt der Zwischenablage:\n{}", content.chars().take(6000).collect::<String>())),
                Err(err) => Outcome::fail(err.to_string()),
            },
        },
        "browser_click" => browser_click(app, &text(args, "target")).await,
        "browser_type" => browser_type(app, &text(args, "text"), args["enter"].as_bool().unwrap_or(false)).await,
        "play_youtube" => {
            let (query, channel) = (text(args, "query"), text(args, "channel"));
            if query.is_empty() {
                return Outcome::fail(tr!("Wonach soll ich auf YouTube suchen?", "What should I search for on YouTube?"));
            }
            let search = if channel.is_empty() { query.clone() } else { format!("{query} {channel}") };
            match youtube::search(&search).await {
                Ok(videos) => match youtube::pick(&videos, &channel) {
                    Some(video) => {
                        let url = format!("https://www.youtube.com/watch?v={}", video.id);
                        match open_url(&url, &text(args, "screen")).await {
                            Ok(place) => Outcome::done(
                                tr!(format!("„{}“ von {} geöffnet{place}", video.title, video.channel), format!("Opened \"{}\" by {}{place}", video.title, video.channel)),
                                tr!(format!("Geöffnet{place}: „{}“ vom Kanal {}.", video.title, video.channel), format!("Opened{place}: \"{}\" from the channel {}.", video.title, video.channel)),
                            )
                            .with_card(json!({ "type": "link", "title": format!("{} · {}", video.title, video.channel), "url": url })),
                            Err(err) => Outcome::fail(err.to_string()),
                        }
                    }
                    None => Outcome::fail(tr!(format!("Ich finde zu „{query}“ kein Video vom Kanal „{channel}“."), format!("I can't find a video for \"{query}\" from the channel \"{channel}\"."))),
                },
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "look_at_screen" => look_at_screen(app, &text(args, "question")).await,
        "close_app" => close_app(app, &text(args, "name")).await,
        "open_website" => match system::web_url(&text(args, "url")) {
            Ok(url) => match open_url(&url, &text(args, "screen")).await {
                Ok(place) => Outcome::done(tr!(format!("{url} geöffnet{place}"), format!("Opened {url}{place}")), tr!(format!("Die Seite {url} wurde im Browser geöffnet{place}."), format!("The page {url} was opened in the browser{place}.")))
                    .with_card(json!({ "type": "link", "title": "Webseite geöffnet", "url": url })),
                Err(err) => Outcome::fail(err.to_string()),
            },
            Err(err) => Outcome::fail(err.to_string()),
        },
        "browser_search" => {
            let query = text(args, "query");
            if query.is_empty() {
                return Outcome::fail(tr!("Wonach soll ich suchen?", "What should I search for?"));
            }
            let site = text(args, "site");
            let url = system::search_url(&query, &site);
            match open_url(&url, &text(args, "screen")).await {
                Ok(place) => Outcome::done(tr!(format!("Suche nach „{query}“ geöffnet{place}"), format!("Opened the search for \"{query}\"{place}")), tr!(format!("Die Suche wurde im Browser geöffnet{place}."), format!("The search was opened in the browser{place}.")))
                    .with_card(json!({ "type": "link", "title": format!("Suche: {query}"), "url": url })),
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "set_timer" => set_timer(app, args),
        "set_alarm" => {
            let days: Vec<u8> = args.get("days").and_then(|d| d.as_array()).map(|list| list.iter().filter_map(|d| d.as_u64()).filter(|d| *d < 7).map(|d| d as u8).collect()).unwrap_or_default();
            match crate::alarms::add(app, &text(args, "time"), days, &text(args, "label")) {
                Ok(alarm) => {
                    let when = crate::alarms::describe(&alarm);
                    Outcome::done(tr!(format!("Wecker {when}"), format!("Alarm {when}")), tr!(format!("Wecker gestellt: {when}."), format!("Alarm set: {when}.")))
                }
                Err(err) => Outcome::fail(err),
            }
        }
        "focus_mode" => {
            let stop = text(args, "action") == "stop";
            let minutes = number(args, "minutes").filter(|m| *m >= 1.0 && *m <= 240.0).unwrap_or(25.0).round() as u64;
            let _ = tauri::Emitter::emit(app, "milo://focus", json!({ "action": if stop { "stop" } else { "start" }, "minutes": minutes }));
            if stop {
                Outcome::done(tr!("Fokus beendet", "Focus ended"), tr!("Der Fokus-Modus ist beendet.", "Focus mode has ended."))
            } else {
                Outcome::done(tr!(format!("Fokus: {minutes} Minuten"), format!("Focus: {minutes} minutes")), tr!(format!("Fokus-Modus für {minutes} Minuten gestartet, danach 5 Minuten Pause."), format!("Focus mode started for {minutes} minutes, then a 5-minute break.")))
            }
        }
        "list_timers" => {
            let list = timers::list_timers(app.clone());
            if list.is_empty() {
                return Outcome::done(tr!("Keine Timer aktiv", "No timers running"), tr!("Es läuft kein Timer.", "No timer is running."));
            }
            let now = chrono::Local::now().timestamp_millis();
            let lines: Vec<String> = list
                .iter()
                .map(|t| {
                    let left = Duration::from_millis((t.due_ms - now).max(0) as u64);
                    if t.ringing { tr!(format!("„{}“: abgelaufen, klingelt", t.label), format!("\"{}\": up, ringing", t.label)) } else { tr!(format!("„{}“: noch {}", t.label, timers::describe(left)), format!("\"{}\": {} left", t.label, timers::describe(left))) }
                })
                .collect();
            Outcome::done(format!("{} Timer", list.len()), lines.join("\n"))
        }
        "cancel_timer" => cancel_timer(app, &text(args, "label")),
        "save_note" => {
            let note = text(args, "text");
            if note.is_empty() {
                return Outcome::fail(tr!("Die Notiz ist leer.", "The note is empty."));
            }
            match app.state::<Memory>().add_note(&note) {
                Ok(id) => Outcome::done(tr!("Notiz gespeichert", "Note saved"), tr!(format!("Gemerkt: {note}"), format!("Remembered: {note}")))
                    .with_card(json!({ "type": "note", "id": id, "text": note })),
                Err(err) => Outcome::fail(tr!(format!("Die Notiz ließ sich nicht speichern: {err}"), format!("The note couldn't be saved: {err}"))),
            }
        }
        "search_notes" => match app.state::<Memory>().search_notes(&text(args, "query"), 8) {
            Ok(notes) if notes.is_empty() => Outcome::done(tr!("Nichts gefunden", "Nothing found"), tr!("Dazu ist nichts im Gedächtnis gespeichert.", "Nothing about that is stored in memory.")),
            Ok(notes) => {
                let lines: Vec<String> = notes.iter().map(|n| format!("- ({}) {}", n.created_at, n.text)).collect();
                Outcome::done(tr!(format!("{} Notiz(en) gefunden", notes.len()), format!("Found {} note(s)", notes.len())), lines.join("\n"))
                    .with_card(json!({ "type": "notes", "items": notes }))
            }
            Err(err) => Outcome::fail(tr!(format!("Das Gedächtnis ließ sich nicht durchsuchen: {err}"), format!("Memory couldn't be searched: {err}"))),
        },
        "list_files" => match crate::files::list(&app.state::<Memory>(), 30) {
            Ok(files) if files.is_empty() => Outcome::done(tr!("Keine Dateien", "No files"), tr!("Der Nutzer hat dir noch keine Datei gegeben.", "The user hasn't given you a file yet.")),
            Ok(files) => {
                let lines: Vec<String> = files
                    .iter()
                    .map(|f| format!("- {} ({}, erhalten {}{})", f.name, f.kind, f.added_at, if f.chars == 0 { tr!(", ohne lesbaren Text", ", no readable text") } else { "" }))
                    .collect();
                Outcome::done(format!("{} Datei(en)", files.len()), lines.join("\n")).with_card(json!({ "type": "files", "items": files }))
            }
            Err(err) => Outcome::fail(tr!(format!("Die Dateiliste ließ sich nicht lesen: {err}"), format!("The file list couldn't be read: {err}"))),
        },
        "read_file" => {
            /// Zeichen je Abschnitt; passt mit Antwort bequem in den Kontext des Modells.
            const PART: usize = 6000;
            let name = text(args, "name");
            match crate::files::find(&app.state::<Memory>(), &name) {
                Ok(Some((file, _))) if file.chars == 0 => Outcome::fail(tr!(format!("„{}“ enthält keinen Text, den ich lesen kann. {}", file.name, file.note), format!("\"{}\" contains no text I can read. {}", file.name, file.note))),
                Ok(Some((file, content))) => {
                    let parts = file.chars.div_ceil(PART).max(1);
                    let part = (number(args, "part").unwrap_or(1.0).max(1.0) as usize).min(parts);
                    let more = if part < parts { format!(" Es gibt {parts} Abschnitte; für den nächsten rufe read_file mit part={} auf.", part + 1) } else { String::new() };
                    Outcome::done(
                        tr!(format!("„{}“ gelesen{}", file.name, if parts > 1 { tr!(format!(" (Abschnitt {part} von {parts})"), format!(" (part {part} of {parts})")) } else { String::new() }), format!("Read \"{}\"{}", file.name, if parts > 1 { tr!(format!(" (Abschnitt {part} von {parts})"), format!(" (part {part} of {parts})")) } else { String::new() })),
                        format!("Inhalt der Datei „{}“, Abschnitt {part} von {parts}.{more}\n\n{}", file.name, crate::files::slice(&content, (part - 1) * PART, PART)),
                    )
                    .with_card(json!({ "type": "files", "items": [file] }))
                }
                Ok(None) => Outcome::fail(if name.is_empty() { tr!("Du hast mir noch keine Datei gegeben.", "You haven't given me a file yet.").to_string() } else { tr!(format!("Eine Datei namens „{name}“ habe ich nicht."), format!("I don't have a file called \"{name}\".")) }),
                Err(err) => Outcome::fail(tr!(format!("Die Datei ließ sich nicht lesen: {err}"), format!("The file couldn't be read: {err}"))),
            }
        }
        "media_control" if matches!(text(args, "action").as_str(), "rewind" | "forward") => {
            let seconds = number(args, "seconds").unwrap_or(10.0).clamp(1.0, 3600.0) as i64;
            let back = text(args, "action") == "rewind";
            let delta = if back { -seconds * 1000 } else { seconds * 1000 };
            match tokio::task::spawn_blocking(move || media::seek_by(delta)).await {
                Ok(Ok(_)) => {
                    let done = format!("{seconds} Sekunden {}", if back { tr!("zurückgespult", "back") } else { tr!("vorgespult", "forward") });
                    Outcome::done(done.clone(), format!("{done}."))
                }
                Ok(Err(err)) => Outcome::fail(err),
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "media_control" => {
            let (key, done) = match text(args, "action").as_str() {
                "play" => (MediaKey::PlayPause, tr!("Wiedergabe gestartet", "Playback started")),
                "pause" => (MediaKey::PlayPause, tr!("Wiedergabe pausiert", "Playback paused")),
                "next" => (MediaKey::Next, tr!("Nächster Titel", "Next track")),
                "previous" => (MediaKey::Previous, tr!("Vorheriger Titel", "Previous track")),
                "volume_up" => (MediaKey::VolumeUp, tr!("Lauter", "Louder")),
                "volume_down" => (MediaKey::VolumeDown, tr!("Leiser", "Quieter")),
                "mute" => (MediaKey::Mute, tr!("Ton umgeschaltet", "Sound toggled")),
                other => return Outcome::fail(tr!(format!("Die Aktion „{other}“ kenne ich nicht."), format!("I don't know the action \"{other}\"."))),
            };
            // Lautstaerke in drei Schritten aendern, ein einzelner ist kaum hoerbar.
            let presses = if matches!(key, MediaKey::VolumeUp | MediaKey::VolumeDown) { 3 } else { 1 };
            match (0..presses).try_for_each(|_| system::press(key)) {
                Ok(()) => Outcome::done(done, format!("{done}.")),
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "system_power" => system_power(app, &text(args, "action"), number(args, "in_minutes").unwrap_or(0.0)).await,
        "workspace" => workspace(app, &text(args, "action"), &text(args, "name")).await,
        "get_weather" => {
            let place = text(args, "location");
            match web::weather(&place).await {
                Ok(weather) => {
                    // Den Ort merken, damit die Wetter-Ansicht ihn spaeter von selbst zeigen kann.
                    let store = app.state::<SettingsStore>();
                    let mut settings = store.get();
                    if settings.weather_place != weather.place {
                        settings.weather_place = weather.place.clone();
                        let _ = store.set(settings);
                    }
                    let mut card = serde_json::to_value(&weather).unwrap_or_default();
                    card["type"] = json!("weather");
                    Outcome::done(tr!(format!("Wetter für {} abgerufen", weather.place), format!("Got the weather for {}", weather.place)), weather.report()).with_card(card)
                }
                Err(err) => Outcome::fail(online_error(&err)),
            }
        }
        "web_research" => {
            let query = text(args, "query");
            match web::research(&query).await {
                Ok(found) => Outcome::done(
                    tr!(format!("{} Quellen gefunden", found.sources.len()), format!("Found {} sources", found.sources.len())),
                    format!(
                        "Suchergebnisse für „{query}“. Antworte nur mit dem, was hier steht, und nenne die Quelle. Steht die Antwort nicht darin, sag das.\n\n{}",
                        found.text
                    ),
                )
                .with_card(json!({ "type": "sources", "query": query, "items": found.sources })),
                Err(err) => Outcome::fail(online_error(&err)),
            }
        }
        "browser_read" => match tokio::task::spawn_blocking(|| browser::page_text(12_000)).await {
            Ok(Ok((page, text))) if text.trim().is_empty() => Outcome::fail(tr!(format!("Auf der Seite „{page}“ finde ich keinen lesbaren Text."), format!("I can't find any readable text on the page \"{page}\"."))),
            Ok(Ok((page, text))) => Outcome::done(tr!(format!("Seite „{page}“ gelesen"), format!("Read the page \"{page}\"")), format!("Text der Seite „{page}“:\n{text}")),
            Ok(Err(err)) => Outcome::fail(err.to_string()),
            Err(err) => Outcome::fail(err.to_string()),
        },
        "browser_list" => match tokio::task::spawn_blocking(|| browser::clickable(80)).await {
            Ok(Ok((page, names))) if names.is_empty() => Outcome::fail(tr!(format!("Auf „{page}“ sehe ich gerade nichts Anklickbares. Vielleicht lädt die Seite noch."), format!("I don't see anything clickable on \"{page}\" right now. The page may still be loading."))),
            Ok(Ok((page, names))) => Outcome::done(tr!(format!("{} Elemente auf „{page}“", names.len()), format!("{} elements on \"{page}\"", names.len())), format!("Anklickbar auf „{page}“:\n{}", names.join("\n"))),
            Ok(Err(err)) => Outcome::fail(err.to_string()),
            Err(err) => Outcome::fail(err.to_string()),
        },
        "browser_scroll" => {
            let down = text(args, "direction") != "up";
            let steps = number(args, "steps").unwrap_or(5.0).clamp(1.0, 30.0) as u32;
            match tokio::task::spawn_blocking(move || browser::scroll(down, steps)).await {
                Ok(Ok(page)) => Outcome::done(if down { tr!("Nach unten geblättert", "Scrolled down") } else { tr!("Nach oben geblättert", "Scrolled up") }, format!("Auf „{page}“ geblättert.")),
                Ok(Err(err)) => Outcome::fail(err.to_string()),
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "browser_key" => {
            let key = text(args, "key");
            let pressed = key.clone();
            match tokio::task::spawn_blocking(move || browser::press(&pressed)).await {
                Ok(Ok(page)) => Outcome::done(tr!(format!("Taste {key} gedrückt"), format!("Pressed {key}")), format!("Taste {key} auf „{page}“ gedrückt.")),
                Ok(Err(err)) => Outcome::fail(err.to_string()),
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "twitch_live" => {
            let channels: Vec<String> = match &args["channels"] {
                Value::Array(list) => list.iter().filter_map(|c| c.as_str().map(String::from)).collect(),
                Value::String(one) => one.split(',').map(|c| c.trim().to_string()).collect(),
                _ => Vec::new(),
            };
            if channels.is_empty() {
                return Outcome::fail(tr!("Welche Twitch-Kanäle soll ich prüfen?", "Which Twitch channels should I check?"));
            }
            match crate::twitch::check(&channels).await {
                Ok(found) if found.is_empty() => Outcome::fail(tr!("Das sind keine gültigen Twitch-Kanalnamen.", "Those aren't valid Twitch channel names.")),
                Ok(found) => {
                    let live = found.iter().filter(|c| c.live).count();
                    let lines: Vec<String> = found.iter().map(|c| c.describe()).collect();
                    Outcome::done(tr!(format!("{live} von {} Kanälen live", found.len()), format!("{live} of {} channels live", found.len())), lines.join("\n"))
                }
                Err(err) => Outcome::fail(online_error(&err)),
            }
        }
        "find_links" => {
            let query = text(args, "query");
            match web::find_links(&query).await {
                Ok(hits) => {
                    let lines: Vec<String> = hits
                        .iter()
                        .enumerate()
                        .map(|(i, h)| format!("{}. {} – {}{}", i + 1, h.title, h.url, if h.snippet.is_empty() { String::new() } else { format!("\n   {}", h.snippet.chars().take(160).collect::<String>()) }))
                        .collect();
                    Outcome::done(tr!(format!("{} Treffer zu „{query}“", hits.len()), format!("{} results for \"{query}\"", hits.len())), format!("Treffer für „{query}“:\n{}", lines.join("\n")))
                }
                Err(err) => Outcome::fail(online_error(&err)),
            }
        }
        "browser_tabs" => {
            let (action, title) = (text(args, "action"), text(args, "title"));
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<(String, String)> {
                match action.as_str() {
                    "switch" | "close" => {
                        if title.is_empty() {
                            anyhow::bail!(tr!("Welchen Tab meinst du?", "Which tab do you mean?"));
                        }
                        let name = browser::switch_tab(&title)?;
                        if action == "close" {
                            std::thread::sleep(Duration::from_millis(250));
                            browser::press("close_tab")?;
                            Ok((tr!(format!("Tab „{name}“ geschlossen"), format!("Closed the tab \"{name}\"")), tr!(format!("Der Tab „{name}“ wurde geschlossen."), format!("The tab \"{name}\" was closed."))))
                        } else {
                            Ok((tr!(format!("Zu „{name}“ gewechselt"), format!("Switched to \"{name}\"")), tr!(format!("Der Tab „{name}“ ist jetzt vorne."), format!("The tab \"{name}\" is now in front."))))
                        }
                    }
                    _ => {
                        let (page, tabs) = browser::tabs()?;
                        if tabs.is_empty() {
                            anyhow::bail!("{}", tr!(format!("Ich sehe im Browserfenster „{page}“ keine Tabs."), format!("I don't see any tabs in the browser window \"{page}\".")));
                        }
                        Ok((tr!(format!("{} Tabs offen", tabs.len()), format!("{} tabs open", tabs.len())), format!("Offene Tabs:\n{}", tabs.iter().enumerate().map(|(i, t)| format!("{}. {t}", i + 1)).collect::<Vec<_>>().join("\n"))))
                    }
                }
            })
            .await;
            match result {
                Ok(Ok((summary, detail))) => Outcome::done(summary, detail),
                Ok(Err(err)) => Outcome::fail(err.to_string()),
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "browser_search_site" => browser_search_site(app, &text(args, "site"), &text(args, "query")).await,
        "browser_navigate" => match system::web_url(&text(args, "url")) {
            Ok(url) => {
                let target = url.clone();
                match tokio::task::spawn_blocking(move || browser::navigate(&target)).await {
                    Ok(Ok(_)) => Outcome::done(tr!(format!("{url} geöffnet"), format!("Opened {url}")), tr!(format!("Im aktuellen Tab wird {url} geladen."), format!("{url} is loading in the current tab.")))
                        .with_card(json!({ "type": "link", "title": "Webseite geöffnet", "url": url })),
                    Ok(Err(err)) => Outcome::fail(err.to_string()),
                    Err(err) => Outcome::fail(err.to_string()),
                }
            }
            Err(err) => Outcome::fail(err.to_string()),
        },
        "save_screenshot" => save_screenshot(&text(args, "screen")).await,
        "now_playing" => match app.state::<media::Media>().current() {
            Some(now) if !now.title.is_empty() => {
                let by = if now.artist.is_empty() { String::new() } else { tr!(format!(" von {}", now.artist), format!(" by {}", now.artist)) };
                let state = if now.playing { "läuft" } else { tr!("ist pausiert", "is paused") };
                let from = if now.app.is_empty() { String::new() } else { format!(" ({})", now.app) };
                Outcome::done(format!("„{}“{by}", now.title), tr!(format!("Gerade {state}: „{}“{by}{from}.", now.title), format!("Right now {state}: \"{}\"{by}{from}.", now.title)))
            }
            _ => Outcome::done(tr!("Nichts läuft", "Nothing playing"), tr!("Gerade läuft keine Musik und kein Video, das Windows meldet.", "No music or video that Windows reports is playing right now.")),
        },
        "deep_research" => deep_research(app, &text(args, "topic")).await,
        "create_word_document" => create_word_document(app, args).await,
        other => Outcome::fail(tr!(format!("Das Werkzeug „{other}“ kenne ich nicht."), format!("I don't know the tool \"{other}\"."))),
    }
}

/// Gruendliche Recherche: Quellen lesen und vom schnellen Modell zu Notizen verdichten lassen,
/// damit das schreibende Modell nur das Wesentliche bekommt.
async fn deep_research(app: &AppHandle, topic: &str) -> Outcome {
    let sources = match web::deep_research(topic).await {
        Ok(sources) => sources,
        Err(err) => return Outcome::fail(online_error(&err)),
    };
    let links: Vec<Link> = sources.iter().map(|s| Link { title: s.hit.title.clone(), url: s.hit.url.clone() }).collect();
    let mut material = String::new();
    for (i, source) in sources.iter().enumerate() {
        material.push_str(&format!("[{}] {} ({})\n{}\n\n", i + 1, source.hit.title, source.hit.url, source.text));
    }
    let system = "Du verdichtest Rechercheergebnisse zu sachlichen Notizen auf Deutsch für einen Bericht. \
Behalte alle wichtigen Fakten, Zahlen, Daten und Namen; lass Werbung, Navigation und Wiederholungen weg. \
Schreibe nach jedem Fakt die Quellennummer in eckigen Klammern, z. B. [2]. Erfinde nichts. Höchstens etwa 700 Wörter.";
    let prompt = format!("Thema: {topic}\n\nQuellen:\n{material}");
    let notes = match crate::agent::ask_once(app, Tier::Easy, system, vec![json!({ "text": prompt })], Thinking::Low).await {
        Ok(notes) if !notes.trim().is_empty() => notes,
        // Ohne Verdichtung gehen die gekuerzten Auszuege direkt weiter.
        _ => material.chars().take(9000).collect(),
    };
    let list: Vec<String> = links.iter().enumerate().map(|(i, l)| format!("[{}] {} – {}", i + 1, l.title, l.url)).collect();
    let mut outcome = Outcome::done(
        tr!(format!("{} Quellen zu „{topic}“ gelesen", links.len()), format!("Read {} sources on \"{topic}\"", links.len())),
        format!("Notizen zu „{topic}“:\n{notes}\n\nQuellen:\n{}", list.join("\n")),
    )
    .with_card(json!({ "type": "sources", "query": topic, "items": sources.iter().map(|s| &s.hit).collect::<Vec<_>>() }));
    outcome.links = links;
    outcome
}

/// Word-Datei im Ordner Dokumente\Milo anlegen und oeffnen.
async fn create_word_document(app: &AppHandle, args: &Value) -> Outcome {
    let doc: docs::Document = match serde_json::from_value(args.clone()) {
        Ok(doc) => doc,
        Err(err) => return Outcome::fail(tr!(format!("Der Inhalt für die Word-Datei ist unvollständig ({err})."), format!("The content for the Word file is incomplete ({err})."))),
    };
    let dir = pc::known_folder("dokumente").unwrap_or_else(|| std::env::temp_dir()).join("Milo");
    let title = doc.title.clone();
    let path = match tokio::task::spawn_blocking(move || docs::save(&doc, &dir)).await {
        Ok(Ok(path)) => path,
        Ok(Err(err)) => return Outcome::fail(tr!(format!("Die Word-Datei ließ sich nicht speichern: {err}"), format!("The Word file couldn't be saved: {err}"))),
        Err(err) => return Outcome::fail(err.to_string()),
    };
    let shown = path.display().to_string();
    if let Err(err) = system::shell_open(&shown) {
        log::warn!("Word-Datei nicht geöffnet: {err}");
    }
    app.state::<Memory>().log_action("file_out", &tr!(format!("Word-Datei „{title}“ gespeichert"), format!("Saved the Word file \"{title}\"")), true);
    Outcome::done(tr!(format!("Word-Datei „{title}“ gespeichert"), format!("Saved the Word file \"{title}\"")), tr!(format!("Die Datei wurde gespeichert und geöffnet: {shown}"), format!("The file was saved and opened: {shown}")))
        .with_card(json!({ "type": "document", "title": title, "path": shown }))
}

fn online_error(err: &anyhow::Error) -> String {
    if err.chain().any(|e| e.downcast_ref::<reqwest::Error>().is_some_and(|r| r.is_connect() || r.is_timeout())) {
        tr!("Ich komme gerade nicht ins Internet.", "I can't reach the internet right now.").into()
    } else {
        err.to_string()
    }
}

async fn open_app(app: &AppHandle, name: &str, screen: &str) -> Outcome {
    if name.is_empty() {
        return Outcome::fail(tr!("Welches Programm soll ich öffnen?", "Which app should I open?"));
    }
    if screens::is_browser_word(name) {
        if let Some(exe) = screens::default_browser() {
            let label = screens::browser_name(&screens::stem(&exe));
            return match std::process::Command::new(&exe).spawn() {
                Ok(_) => Outcome::done(format!("{label} geöffnet"), format!("Der Standardbrowser {label} wurde gestartet.")).with_card(json!({ "type": "app", "name": label })),
                Err(err) => Outcome::fail(tr!(format!("{label} ließ sich nicht starten: {err}"), format!("{label} couldn't be started: {err}"))),
            };
        }
    }
    let index = app.state::<AppIndex>();
    index.ensure_fresh(Duration::from_secs(600)).await;
    let mut found = index.find(name);
    if found.is_none() {
        // Vielleicht wurde das Programm gerade erst installiert: Liste einmal frisch einlesen.
        index.ensure_fresh(Duration::ZERO).await;
        found = index.find(name);
    }
    let Some(entry) = found else {
        let similar = index.suggestions(name, 3);
        let hint = if similar.is_empty() { String::new() } else { tr!(format!(" Ähnlich heißen: {}.", similar.join(", ")), format!(" Similar names: {}.", similar.join(", "))) };
        return Outcome::fail(tr!(format!("Ich finde kein Programm namens „{name}“.{hint}"), format!("I can't find an app called \"{name}\".{hint}")));
    };
    let before: std::collections::HashSet<isize> = screens::other_windows().iter().map(|w| w.handle).collect();
    if let Err(err) = apps::launch(&entry) {
        return Outcome::fail(tr!(format!("„{}“ ließ sich nicht starten: {err}", entry.name), format!("\"{}\" couldn't be started: {err}", entry.name)));
    }
    let mut place = String::new();
    if !screen.is_empty() {
        let (wanted, spec, label) = (name.to_string(), screen.to_string(), entry.name.clone());
        let moved = tokio::task::spawn_blocking(move || place_new_app_window(&before, &wanted, &label, &spec)).await;
        match moved {
            Ok(Ok(number)) => place = tr!(format!(" auf Bildschirm {number}"), format!(" on screen {number}")),
            Ok(Err(err)) => {
                return Outcome::done(tr!(format!("{} geöffnet", entry.name), format!("Opened {}", entry.name)), tr!(format!("Das Programm „{}“ wurde gestartet, aber nicht verschoben: {err}", entry.name), format!("The app \"{}\" was started but not moved: {err}", entry.name)))
                    .with_card(json!({ "type": "app", "name": entry.name }));
            }
            Err(err) => log::warn!("{err}"),
        }
    }
    Outcome::done(tr!(format!("{} geöffnet{place}", entry.name), format!("Opened {}{place}", entry.name)), tr!(format!("Das Programm „{}“ wurde gestartet{place}.", entry.name), format!("The app \"{}\" was started{place}.", entry.name)))
        .with_card(json!({ "type": "app", "name": entry.name }))
}

/// Oeffnet eine Adresse; mit Bildschirmangabe in einem neuen Fenster dort. Liefert den Zusatz fuer die Meldung.
async fn open_url(url: &str, screen: &str) -> anyhow::Result<String> {
    if screen.is_empty() {
        system::shell_open(url)?;
        return Ok(String::new());
    }
    let (url, spec) = (url.to_string(), screen.to_string());
    let opened = tokio::task::spawn_blocking(move || screens::open_url_on(&url, &spec)).await??;
    Ok(if opened.moved_existing {
        tr!(format!(" (im vorhandenen Browserfenster, das jetzt auf Bildschirm {} liegt)", opened.screen), format!(" (in the existing browser window, now on screen {})", opened.screen))
    } else {
        tr!(format!(" in einem neuen Fenster auf Bildschirm {}", opened.screen), format!(" in a new window on screen {}", opened.screen))
    })
}

/// Sucht nach dem Start eines Programms sein Fenster (ein neues, sonst ein schon offenes mit passendem Namen)
/// und legt es maximiert auf den gewuenschten Bildschirm.
fn place_new_app_window(before: &std::collections::HashSet<isize>, wanted: &str, label: &str, spec: &str) -> anyhow::Result<usize> {
    let list = screens::list();
    let screen = screens::resolve(&list, spec)?.clone();
    let names = [apps::normalize(wanted), apps::normalize(label)];
    let fits = |w: &system::AppWindow| {
        let (process, title) = (apps::normalize(&w.process), apps::normalize(&w.title));
        names.iter().any(|n| !n.is_empty() && (process.contains(n.as_str()) || title.contains(n.as_str()) || n.contains(process.as_str()) && !process.is_empty()))
    };
    // Erst auf ein neues, passendes Fenster warten; Startbildschirme verschwinden oft wieder, deshalb kurz nachpruefen.
    let mut found = screens::wait_for_new_window(before, Duration::from_secs(12), |w| fits(w));
    if found.is_none() {
        // Lief das Programm schon, holt Windows nur das vorhandene Fenster nach vorne.
        found = find_windows(wanted).into_iter().next().or_else(|| find_windows(label).into_iter().next());
    }
    let window = found.ok_or_else(|| anyhow::anyhow!(tr!("Ich finde das Fenster nicht.", "I can't find the window.")))?;
    std::thread::sleep(Duration::from_millis(600));
    let window = screens::other_windows().into_iter().find(|w| w.handle == window.handle).or_else(|| find_windows(wanted).into_iter().next()).unwrap_or(window);
    screens::place(window.handle, &screen, screens::Placement::Maximize)?;
    screens::activate(window.handle);
    Ok(screen.number)
}

/// Offene Fenster, die zu einem Programmnamen oder Fenstertitel passen. „Browser“ meint alle
/// Browserfenster (bekannte Browser und der Standardbrowser), egal ob Brave, Chrome oder Edge.
fn find_windows(name: &str) -> Vec<system::AppWindow> {
    window_matches(name, &screens::other_windows())
}

/// Programmnamen, wie Nutzer und Modell sie sagen, auf den Dateinamen abbilden.
fn process_alias(wanted: &str) -> &str {
    match wanted {
        "edge" | "microsoft edge" => "msedge",
        "google chrome" | "chrome browser" => "chrome",
        "brave browser" => "brave",
        "mozilla firefox" => "firefox",
        "explorer" | "datei explorer" | "windows explorer" => "explorer",
        other => other,
    }
}

/// Die Auswahl selbst, getrennt vom Lesen der Fenster (damit testbar).
pub fn window_matches(name: &str, windows: &[system::AppWindow]) -> Vec<system::AppWindow> {
    let wanted = apps::normalize(name);
    if wanted.is_empty() {
        return Vec::new();
    }
    if screens::is_browser_word(name) {
        let default = screens::default_browser().map(|exe| screens::stem(&exe));
        return windows.iter().filter(|w| screens::is_browser_process(&w.process, default.as_deref())).cloned().collect();
    }
    let wanted = process_alias(&wanted).to_string();
    windows
        .iter()
        .filter(|w| {
            let process = apps::normalize(&w.process);
            let title = apps::normalize(&w.title);
            process == wanted
                || (!wanted.is_empty() && process.contains(&wanted))
                || (process.chars().count() >= 4 && wanted.contains(&process))
                || title.split(' ').any(|t| t == wanted)
                || (wanted.chars().count() >= 5 && title.contains(&wanted))
        })
        .cloned()
        .collect()
}

/// Was gerade offen ist, als kurze Liste fuer Fehlermeldungen ans Modell („Brave (3 Fenster), Discord“).
fn open_programs() -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for w in screens::other_windows() {
        let name = if w.process.is_empty() { continue } else { w.process.clone() };
        // Unsichtbare Helfer von Windows sind keine Programme, die man schliessen will.
        if ["textinputhost", "applicationframehost", "shellexperiencehost", "searchhost", "startmenuexperiencehost"].contains(&name.to_lowercase().as_str()) {
            continue;
        }
        match counts.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(&name)) {
            Some((_, count)) => *count += 1,
            None => counts.push((name, 1)),
        }
    }
    let parts: Vec<String> = counts.iter().map(|(n, c)| if *c > 1 { tr!(format!("{n} ({c} Fenster)"), format!("{n} ({c} windows)")) } else { n.clone() }).collect();
    parts.join(", ")
}

/// Fehlermeldung, wenn zu einem Namen kein Fenster passt: mit der Liste, was wirklich offen ist,
/// damit das Modell nicht raten muss (sonst versucht es „Google Chrome“, obwohl Brave laeuft).
fn no_window(name: &str) -> String {
    let open = open_programs();
    if open.is_empty() {
        tr!(format!("Ich sehe kein offenes Fenster von „{name}“, und gerade sind keine anderen Programme offen."), format!("I don't see an open window of \"{name}\", and no other apps are open right now."))
    } else {
        format!("Ich sehe kein offenes Fenster von „{name}“. Offen sind gerade: {open}. Falls eins davon gemeint ist, ruf das Werkzeug noch einmal mit genau diesem Namen auf; sonst sag dem Nutzer, dass es nicht offen ist.")
    }
}

/// Fenster anordnen: verschieben, maximieren, Haelften, nach vorne holen, Desktop zeigen.
async fn window_control(action: &str, target: &str, screen: &str) -> Outcome {
    let (action, target, screen) = (action.to_string(), target.to_string(), screen.to_string());
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        if action == "show_desktop" {
            screens::show_desktop();
            return Ok(tr!("Desktop gezeigt", "Showed the desktop").into());
        }
        let window = if target.trim().is_empty() {
            screens::other_windows().into_iter().next()
        } else {
            find_windows(&target).into_iter().next()
        }
        .ok_or_else(|| anyhow::anyhow!("{}", no_window(if target.is_empty() { tr!("dem Programm", "the app") } else { &target })))?;
        let list = screens::list();
        let on_screen = || -> anyhow::Result<screens::Screen> {
            if screen.trim().is_empty() {
                Ok(screens::screen_of(&list, window.handle).or(list.first()).cloned().ok_or_else(|| anyhow::anyhow!(tr!("Ich erkenne keinen Bildschirm.", "I can't detect a screen.")))?)
            } else {
                Ok(screens::resolve(&list, &screen)?.clone())
            }
        };
        let name = if window.process.is_empty() { window.title.clone() } else { window.process.clone() };
        Ok(match action.as_str() {
            "move" | "maximize" | "left_half" | "right_half" => {
                let target_screen = on_screen()?;
                let how = match action.as_str() {
                    "move" => screens::Placement::Move,
                    "left_half" => screens::Placement::LeftHalf,
                    "right_half" => screens::Placement::RightHalf,
                    _ => screens::Placement::Maximize,
                };
                // „Auf den zweiten Bildschirm“ ohne weitere Angabe: dort maximiert.
                let how = if how == screens::Placement::Move && !screen.is_empty() { screens::Placement::Maximize } else { how };
                screens::place(window.handle, &target_screen, how)?;
                screens::activate(window.handle);
                let what = match how {
                    screens::Placement::LeftHalf => tr!("linke Hälfte von", "the left half of"),
                    screens::Placement::RightHalf => tr!("rechte Hälfte von", "the right half of"),
                    _ => "",
                };
                format!("{name} liegt jetzt auf {what} Bildschirm {}", target_screen.number).replace("auf  ", "auf ")
            }
            "minimize" => {
                screens::minimize(window.handle);
                tr!(format!("{name} minimiert"), format!("{name} minimized"))
            }
            "restore" => {
                screens::restore(window.handle);
                screens::activate(window.handle);
                tr!(format!("{name} wiederhergestellt"), format!("{name} restored"))
            }
            "focus" => {
                screens::activate(window.handle);
                tr!(format!("{name} ist jetzt vorne"), format!("{name} is now in front"))
            }
            other => anyhow::bail!("{}", tr!(format!("Die Aktion „{other}“ kenne ich nicht."), format!("I don't know the action \"{other}\"."))),
        })
    })
    .await;
    match result {
        Ok(Ok(done)) => Outcome::done(done.clone(), format!("{done}.")),
        Ok(Err(err)) => Outcome::fail(err.to_string()),
        Err(err) => Outcome::fail(err.to_string()),
    }
}

/// Skript: erst pruefen, dann dem Nutzer vollstaendig zeigen, erst nach Zustimmung ausfuehren.
async fn run_script(app: &AppHandle, code: &str, purpose: &str) -> Outcome {
    if let Err(err) = scripts::check(code) {
        return Outcome::fail(err.to_string());
    }
    let title = if purpose.is_empty() { tr!("Dieses Skript ausführen?", "Run this script?").to_string() } else { tr!(format!("Skript ausführen: {purpose}"), format!("Run script: {purpose}")) };
    if !confirm::ask(app, &title, code, tr!("Ausführen", "Run"), true).await {
        return Outcome::declined(tr!("Das Skript wurde nicht ausgeführt.", "The script wasn't run."));
    }
    match scripts::run(code).await {
        Ok(output) if output.ok => {
            let shown = if output.text.is_empty() { tr!("(keine Ausgabe)", "(no output)").to_string() } else { output.text };
            Outcome::done(tr!("Skript ausgeführt", "Script ran"), tr!(format!("Das Skript lief ohne Fehler. Ausgabe:\n{shown}"), format!("The script ran without errors. Output:\n{shown}")))
                .with_card(json!({ "type": "script", "code": code, "output": shown, "ok": true }))
        }
        Ok(output) => Outcome::fail(tr!(format!("Das Skript meldet einen Fehler: {}", output.text), format!("The script reports an error: {}", output.text))),
        Err(err) => Outcome::fail(err.to_string()),
    }
}

/// Bildschirmfoto in voller Groesse als PNG in Bilder\Milo speichern.
async fn save_screenshot(screen: &str) -> Outcome {
    let spec = screen.to_string();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<std::path::PathBuf> {
        let list = screens::list();
        let target = if spec.trim().is_empty() { list.first().cloned() } else { Some(screens::resolve(&list, &spec)?.clone()) };
        let png = match target {
            Some(s) => scripts::capture(s.bounds.left, s.bounds.top, s.bounds.width(), s.bounds.height(), 8192)?,
            None => scripts::screenshot(8192)?,
        };
        let dir = pc::known_folder("bilder").unwrap_or_else(std::env::temp_dir).join("Milo");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("Bildschirmfoto {}.png", chrono::Local::now().format("%Y-%m-%d %H-%M-%S")));
        std::fs::write(&path, png)?;
        Ok(path)
    })
    .await;
    match result {
        Ok(Ok(path)) => {
            let shown = path.display().to_string();
            Outcome::done(tr!("Bildschirmfoto gespeichert", "Screenshot saved"), tr!(format!("Das Bildschirmfoto liegt unter {shown}."), format!("The screenshot is at {shown}.")))
                .with_card(json!({ "type": "image", "title": path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), "path": shown }))
        }
        Ok(Err(err)) => Outcome::fail(err.to_string()),
        Err(err) => Outcome::fail(err.to_string()),
    }
}

/// Bildschirmfoto: wird nie gespeichert, sondern direkt an Gemini geschickt und danach verworfen.
async fn look_at_screen(app: &AppHandle, question: &str) -> Outcome {
    let png = match tokio::task::spawn_blocking(|| scripts::screenshot(1600)).await {
        Ok(Ok(png)) => png,
        Ok(Err(err)) => return Outcome::fail(err.to_string()),
        Err(err) => return Outcome::fail(err.to_string()),
    };
    use base64::Engine;
    let image = base64::engine::general_purpose::STANDARD.encode(&png);
    let ask = if question.is_empty() { tr!("Beschreibe kurz, was auf diesem Bildschirmfoto zu sehen ist.", "Briefly describe what can be seen in this screenshot.") } else { question };
    let system = tr!("Du bekommst ein Foto des Bildschirms des Nutzers. Antworte auf Deutsch, knapp und nur anhand dessen, was wirklich zu sehen ist.", "You get a photo of the user's screen. Answer in English, briefly and only based on what is actually visible.");
    let parts = vec![json!({ "inline_data": { "mime_type": "image/png", "data": image } }), json!({ "text": ask })];
    match crate::agent::ask_once(app, Tier::Easy, system, parts, Thinking::Low).await {
        Ok(answer) if !answer.is_empty() => Outcome::done(tr!("Bildschirm angesehen", "Looked at the screen"), tr!(format!("Auf dem Bildschirm ist zu sehen: {answer}"), format!("On the screen: {answer}"))),
        Ok(_) => Outcome::fail(tr!("Ich konnte auf dem Bildschirmfoto nichts erkennen.", "I couldn't make anything out in the screenshot.")),
        Err(err) => Outcome::fail(tr!(format!("Das Bildschirmfoto ließ sich nicht auswerten ({err})."), format!("The screenshot couldn't be analyzed ({err})."))),
    }
}

async fn close_app(app: &AppHandle, name: &str) -> Outcome {
    if name.is_empty() {
        return Outcome::fail(tr!("Welches Programm soll ich schließen?", "Which app should I close?"));
    }
    let matches = find_windows(name);
    if matches.is_empty() {
        return Outcome::fail(no_window(name));
    }
    let titles: Vec<String> = matches.iter().map(|w| format!("{} ({})", w.title, w.process)).collect();
    let approved = confirm::ask(app, &tr!(format!("„{name}“ schließen?"), format!("Close \"{name}\"?")), &titles.join("\n"), tr!("Schließen", "Close"), true).await;
    if !approved {
        return Outcome::declined(tr!("Nichts wurde geschlossen.", "Nothing was closed."));
    }
    let closed = matches.iter().filter(|w| system::close_window(w.handle).is_ok()).count();
    Outcome::done(tr!(format!("{closed} Fenster von {name} geschlossen"), format!("Closed {closed} window(s) of {name}")), tr!(format!("{closed} Fenster wurden zum Schließen aufgefordert."), format!("{closed} window(s) were asked to close.")))
}

#[cfg(test)]
mod window_tests {
    use super::*;

    fn w(process: &str, title: &str) -> system::AppWindow {
        system::AppWindow { handle: title.len() as isize, title: title.into(), process: process.into(), pid: 1 }
    }

    #[test]
    fn browser_means_brave_too() {
        let open = vec![w("brave", "Netflix - Brave"), w("brave", "YouTube - Brave"), w("Discord", "Freunde - Discord")];
        let found = window_matches("browser", &open);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|x| x.process == "brave"));
        assert_eq!(window_matches("meinen Browser", &open).len(), 2);
        assert_eq!(window_matches("Brave", &open).len(), 2);
    }

    #[test]
    fn long_names_and_aliases_find_the_process() {
        let open = vec![w("chrome", "Google - Chrome"), w("msedge", "Bing - Edge"), w("Spotify", "Spotify Premium")];
        assert_eq!(window_matches("Google Chrome", &open)[0].process, "chrome");
        assert_eq!(window_matches("Microsoft Edge", &open)[0].process, "msedge");
        assert_eq!(window_matches("Edge", &open)[0].process, "msedge");
        assert_eq!(window_matches("spotify", &open)[0].process, "Spotify");
        assert!(window_matches("Firefox", &open).is_empty());
        assert!(window_matches("", &open).is_empty());
    }
}

fn set_timer(app: &AppHandle, args: &Value) -> Outcome {
    let at_time = text(args, "at_time");
    let day = text(args, "day");
    let duration = if !day.is_empty() && !matches!(day.to_lowercase().as_str(), "heute" | "today") {
        match timers::until_day_time(&day, &at_time) {
            Ok(d) => d,
            Err(err) => return Outcome::fail(err.to_string()),
        }
    } else if !at_time.is_empty() {
        match timers::until_clock_time(&at_time) {
            Ok(d) => d,
            Err(err) => return Outcome::fail(err.to_string()),
        }
    } else {
        match number(args, "duration_seconds") {
            Some(seconds) if seconds >= 1.0 => Duration::from_secs_f64(seconds),
            _ => return Outcome::fail(tr!("Für den Timer fehlt die Dauer oder die Uhrzeit.", "The timer needs a duration or a time.")),
        }
    };
    match timers::start(app, &text(args, "label"), duration) {
        Ok(timer) => {
            let due = chrono::DateTime::from_timestamp_millis(timer.due_ms)
                .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_default();
            Outcome::done(
                tr!(format!("Timer „{}“: {}", timer.label, timers::describe(duration)), format!("Timer \"{}\": {}", timer.label, timers::describe(duration))),
                tr!(format!("Timer „{}“ gestellt auf {}, er endet um {due} Uhr.", timer.label, timers::describe(duration)), format!("Timer \"{}\" set for {}; it ends at {due}.", timer.label, timers::describe(duration))),
            )
            .with_card(json!({ "type": "timer", "id": timer.id, "label": timer.label, "totalMs": timer.total_ms, "dueMs": timer.due_ms }))
        }
        Err(err) => Outcome::fail(err.to_string()),
    }
}

fn cancel_timer(app: &AppHandle, label: &str) -> Outcome {
    let list = timers::list_timers(app.clone());
    let wanted = apps::normalize(label);
    let hit = if wanted.is_empty() {
        (list.len() == 1).then(|| &list[0])
    } else {
        list.iter().find(|t| apps::normalize(&t.label).contains(&wanted))
    };
    match hit {
        Some(timer) => {
            timers::remove(app, timer.id);
            Outcome::done(tr!(format!("Timer „{}“ abgebrochen", timer.label), format!("Timer \"{}\" cancelled", timer.label)), tr!(format!("Der Timer „{}“ wurde abgebrochen.", timer.label), format!("The timer \"{}\" was cancelled.", timer.label)))
        }
        None if list.is_empty() => Outcome::fail(tr!("Es läuft kein Timer.", "No timer is running.")),
        None => {
            let names: Vec<String> = list.iter().map(|t| format!("„{}“", t.label)).collect();
            Outcome::fail(tr!(format!("Welchen Timer meinst du? Es laufen: {}.", names.join(", ")), format!("Which timer do you mean? Running: {}.", names.join(", "))))
        }
    }
}

/// Wartezeit als lesbarer Text: "in 5 Minuten", "in 30 Sekunden".
pub fn wait_text(seconds: u64) -> String {
    match seconds {
        s if s >= 120 => tr!(format!("in {} Minuten", s / 60), format!("in {} minutes", s / 60)),
        60..=119 => tr!("in einer Minute", "in one minute").into(),
        s => tr!(format!("in {s} Sekunden"), format!("in {s} seconds")),
    }
}

async fn workspace(app: &AppHandle, action: &str, name: &str) -> Outcome {
    use crate::workspaces;
    match action {
        "save" => match workspaces::save(app, name) {
            Ok(saved) => {
                let names: Vec<String> = saved.spots.iter().map(|s| s.process.clone()).collect();
                Outcome::done(
                    tr!(format!("Arbeitsplatz „{}“ gemerkt ({} Fenster)", saved.name, saved.spots.len()), format!("Saved the workspace \"{}\" ({} windows)", saved.name, saved.spots.len())),
                    tr!(format!("Arbeitsplatz „{}“ gespeichert mit: {}.", saved.name, names.join(", ")), format!("Saved the workspace \"{}\" with: {}.", saved.name, names.join(", "))),
                )
            }
            Err(err) => Outcome::fail(err.to_string()),
        },
        "open" => {
            let (app, name) = (app.clone(), name.to_string());
            match tokio::task::spawn_blocking(move || workspaces::open(&app, &name)).await {
                Ok(Ok((placed, missing))) => {
                    let mut text = tr!(format!("{} Fenster an ihren Platz gelegt", placed.len()), format!("Put {} window(s) back in place", placed.len()));
                    if !missing.is_empty() {
                        text += &tr!(format!(", nicht geklappt: {}", missing.join(", ")), format!(", didn\'t work: {}", missing.join(", ")));
                    }
                    Outcome::done(text.clone(), format!("{text}."))
                }
                Ok(Err(err)) => Outcome::fail(err.to_string()),
                Err(err) => Outcome::fail(err.to_string()),
            }
        }
        "delete" => match workspaces::remove(app, name) {
            Ok(true) => Outcome::done(tr!(format!("Arbeitsplatz „{name}“ gelöscht"), format!("Deleted the workspace \"{name}\"")), tr!("Gelöscht.", "Deleted.")),
            Ok(false) => Outcome::fail(tr!(format!("Einen Arbeitsplatz „{name}“ gibt es nicht."), format!("There's no workspace called \"{name}\"."))),
            Err(err) => Outcome::fail(err.to_string()),
        },
        _ => {
            let all = workspaces::load(app);
            if all.is_empty() {
                Outcome::done(tr!("Keine Arbeitsplätze", "No workspaces"), tr!("Es sind noch keine Arbeitsplätze gespeichert.", "No workspaces have been saved yet."))
            } else {
                let list: Vec<String> = all.iter().map(|w| tr!(format!("{} ({} Fenster)", w.name, w.spots.len()), format!("{} ({} windows)", w.name, w.spots.len()))).collect();
                Outcome::done(tr!(format!("{} Arbeitsplätze", all.len()), format!("{} workspaces", all.len())), tr!(format!("Gespeicherte Arbeitsplätze: {}.", list.join(", ")), format!("Saved workspaces: {}.", list.join(", "))))
            }
        }
    }
}

async fn system_power(app: &AppHandle, action: &str, minutes: f64) -> Outcome {
    use crate::power::{self, Action};
    let minutes = minutes.clamp(0.0, 24.0 * 60.0);
    let planned = match action {
        "shutdown" => Some(Action::Shutdown),
        "restart" => Some(Action::Restart),
        "sleep" => Some(Action::Sleep),
        _ => None,
    };
    // Herunterfahren, Neustart und Energie sparen ohne Rueckfrage (Wunsch des Nutzers): bis zum
    // Zeitpunkt zeigt die Insel eine Karte mit Abbrechen, und Milo achtet darauf, dass es wirklich passiert.
    if let Some(kind) = planned {
        let seconds = (minutes * 60.0).round() as u64;
        return match power::schedule(app, kind, seconds) {
            Ok(replaced) => {
                let plan = power::current();
                let due = plan.as_ref().map(|p| p.due_ms).unwrap_or_else(|| chrono::Local::now().timestamp_millis());
                let left = ((due - chrono::Local::now().timestamp_millis()).max(0) as u64 + 500) / 1000;
                let when = wait_text(left);
                let done = kind.label();
                let note = if replaced { tr!(" Der vorher geplante Zeitpunkt wurde dabei ersetzt.", " The previously planned time was replaced.") } else { "" };
                let card = json!({ "type": "power", "title": done, "kind": action, "atMs": due });
                Outcome::done(
                    format!("{done} {when}{}", if replaced { tr!(" (ersetzt den alten Plan)", " (replaced the old plan)") } else { "" }),
                    tr!(format!("{done} {when} ist geplant.{note} Der Nutzer kann es mit dem Knopf „Abbrechen“ oder mit system_power abort stoppen."), format!("{done} {when} is scheduled.{note} The user can stop it with the \"Cancel\" button or with system_power abort.")),
                )
                .with_card(card)
                .with_undo(json!({ "kind": "power_cancel" }))
            }
            Err(err) => Outcome::fail(err.to_string()),
        };
    }
    match action {
        "lock" => match system::power(PowerAction::Lock) {
            Ok(()) => Outcome::done(tr!("PC gesperrt", "PC locked"), tr!("Der PC ist gesperrt.", "The PC is locked.")),
            Err(err) => Outcome::fail(err.to_string()),
        },
        "screen_off" => match pc::screen_off() {
            Ok(()) => Outcome::done(tr!("Bildschirm aus", "Screen off"), tr!("Der Bildschirm ist aus; eine Mausbewegung schaltet ihn wieder ein.", "The screen is off; moving the mouse turns it back on.")),
            Err(err) => Outcome::fail(err.to_string()),
        },
        "abort" => match power::cancel(app) {
            Some(what) => {
                let label = what.map(|a| a.label()).unwrap_or(tr!("Geplanter Befehl", "Scheduled command"));
                Outcome::done(tr!(format!("{label} abgebrochen (Sprache)"), format!("{label} cancelled (voice)")), tr!(format!("{label} wurde abgebrochen, der PC läuft normal weiter."), format!("{label} was cancelled, the PC keeps running normally.")))
            }
            None => Outcome::fail(tr!("Es war nichts geplant, das ich abbrechen könnte.", "Nothing was scheduled that I could cancel.")),
        },
        other => Outcome::fail(tr!(format!("Die Aktion „{other}“ kenne ich nicht."), format!("I don't know the action \"{other}\"."))),
    }
}

/// Fragt vor einem Klick im Browser nach, ausser auf bekannten Streaming-Seiten.
/// Direkt auf einer bekannten Seite suchen: Trefferliste oeffnen (im offenen Browserfenster, sonst
/// im Standardbrowser), Laden abwarten, Zeiger von der Seite nehmen, anklickbare Treffer liefern.
async fn browser_search_site(app: &AppHandle, site: &str, query: &str) -> Outcome {
    let Some(url) = browser::search_url(site, query) else {
        return Outcome::fail(tr!(
            format!("Auf „{site}“ kann ich nicht direkt suchen. Bekannt sind: {}. Sonst öffne die Seite mit open_website und nutze ihr Suchfeld.", browser::search_site_names().join(", ")),
            format!("I can't search \"{site}\" directly. Known sites: {}. Otherwise open the site with open_website and use its search box.", browser::search_site_names().join(", "))
        ));
    };
    let target = url.clone();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<(String, Vec<String>)> {
        let before = browser::current_title();
        if before.is_empty() {
            system::shell_open(&target)?;
        } else {
            browser::navigate(&target)?;
        }
        let page = browser::wait_for_page(&before, Duration::from_secs(10));
        browser::park_mouse();
        let (page2, names) = browser::clickable(60).unwrap_or((page.clone(), Vec::new()));
        Ok((if page2.is_empty() { page } else { page2 }, names))
    })
    .await;
    match result {
        Ok(Ok((page, names))) => {
            let lower = names.join("\n").to_lowercase();
            // Netflix und Co. zeigen ohne gewaehltes Profil erst die Profilauswahl statt der Treffer.
            let profiles = lower.contains("profil") || lower.contains("wer schaut") || lower.contains("who's watching") || lower.contains("profile");
            let user = app.state::<SettingsStore>().get().user_name;
            let hint = if profiles {
                profile_hint(&user)
            } else {
                "\nKlick jetzt mit browser_click auf den passenden Titel (den Titel selbst, nicht das große Banner oben), dann im geöffneten Fenster auf Abspielen/Wiedergabe.".to_string()
            };
            Outcome::done(
                tr!(format!("„{query}“ auf {site} gesucht"), format!("Searched \"{query}\" on {site}")),
                format!("Trefferseite „{page}“ ({url}) geöffnet. Anklickbar:\n{}{hint}", names.join("\n")),
            )
            .with_card(json!({ "type": "link", "title": "Suche geöffnet", "url": url }))
        }
        Ok(Err(err)) => Outcome::fail(err.to_string()),
        Err(err) => Outcome::fail(err.to_string()),
    }
}

/// Hinweis fuer die Profilauswahl (Netflix, Disney+ …): mit dem bekannten Namen des Nutzers.
fn profile_hint(user: &str) -> String {
    let user = user.trim();
    if user.is_empty() {
        "\nAchtung: Das ist die Profilauswahl. Gibt es nur ein Profil (außer „Kinder“), klick es an. Sonst frag den Nutzer kurz, welches Profil er ist, und klick es dann an. Danach rufe browser_search_site noch einmal auf.".into()
    } else {
        format!("\nAchtung: Das ist die Profilauswahl. Der Nutzer heißt „{user}“: klick das Profil mit diesem Namen an (ähnliche Schreibweise reicht). Gibt es keins, aber nur ein Profil außer „Kinder“, nimm das. Sonst frag kurz nach. Danach rufe browser_search_site noch einmal auf.")
    }
}

async fn browser_click(app: &AppHandle, target: &str) -> Outcome {
    if target.is_empty() {
        return Outcome::fail(tr!("Worauf soll ich klicken?", "What should I click?"));
    }
    if !browser::allowed_target(target) {
        return Outcome::fail(format!("Auf „{target}“ klicke ich aus Sicherheitsgründen nicht. Das machst du bitte selbst."));
    }
    let handle = app.clone();
    let wanted = target.to_string();
    let result = tokio::task::spawn_blocking(move || {
        let clicked = browser::click(&wanted, |name, page| {
            if browser::trusted_page(page) {
                return true;
            }
            let question = format!("Im Browser auf „{name}“ klicken?");
            tauri::async_runtime::block_on(confirm::ask(&handle, &question, &format!("Seite: {page}"), "Klicken", false))
        })?;
        // Bleibt der Zeiger ueber einem Titel stehen, oeffnen Streamingseiten grosse Vorschauen.
        browser::park_mouse();
        // Was jetzt anklickbar ist, damit der naechste Klick nicht geraten werden muss. Ein frisch
        // geoeffnetes Fenster (Vorschau einer Serie) steht dabei vorn.
        std::thread::sleep(Duration::from_millis(900));
        let next = browser::clickable(40).map(|(_, names)| names).unwrap_or_default();
        Ok::<_, anyhow::Error>((clicked, next))
    })
    .await;
    match result {
        Ok(Ok((clicked, next))) => {
            let lower = next.join("\n").to_lowercase();
            let profiles = lower.contains("wer schaut") || lower.contains("who's watching") || lower.contains("profil verwalten") || lower.contains("manage profiles");
            let mut detail = format!("Auf „{}“ geklickt (Seite: {}).", clicked.name, clicked.page);
            if !next.is_empty() {
                detail.push_str(&format!(" Jetzt anklickbar (ein geöffnetes Fenster steht vorn):\n{}", next.join("\n")));
            }
            if profiles {
                detail.push_str(&profile_hint(&app.state::<SettingsStore>().get().user_name));
            }
            Outcome::done(format!("Auf „{}“ geklickt", clicked.name), detail)
        }
        Ok(Err(err)) if err.to_string() == "ABGELEHNT" => Outcome::declined("Es wurde nicht geklickt."),
        Ok(Err(err)) => Outcome::fail(err.to_string()),
        Err(err) => Outcome::fail(err.to_string()),
    }
}

async fn browser_type(app: &AppHandle, content: &str, enter: bool) -> Outcome {
    if content.is_empty() {
        return Outcome::fail("Was soll ich tippen?");
    }
    let handle = app.clone();
    let typed = content.to_string();
    let result = tokio::task::spawn_blocking(move || {
        browser::type_text(&typed, enter, |page| {
            if browser::trusted_page(page) {
                return true;
            }
            tauri::async_runtime::block_on(confirm::ask(&handle, "Im Browser tippen?", &format!("Seite: {page}\nText: {typed}"), "Tippen", false))
        })
    })
    .await;
    match result {
        Ok(Ok(page)) => Outcome::done("Im Browser getippt", format!("Text eingegeben (Seite: {page}).")),
        Ok(Err(err)) if err.to_string() == "ABGELEHNT" => Outcome::declined("Es wurde nichts eingetippt."),
        Ok(Err(err)) => Outcome::fail(err.to_string()),
        Err(err) => Outcome::fail(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tools_are_offered() {
        let names: Vec<String> = definitions(&Settings::default()).iter().map(|t| t["function"]["name"].as_str().unwrap().to_string()).collect();
        for wanted in ["web_research", "get_weather", "open_app", "browser_read", "browser_key", "deep_research", "create_word_document"] {
            assert!(names.contains(&wanted.to_string()), "{wanted}");
        }
        // Gemini lehnt doppelte Namen ab.
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn every_definition_is_well_formed() {
        for def in definitions(&Settings::default()) {
            let f = &def["function"];
            assert!(f["name"].as_str().is_some_and(|n| !n.is_empty()));
            assert!(f["description"].as_str().is_some_and(|d| d.len() > 10));
            assert_eq!(f["parameters"]["type"], "object");
            for required in f["parameters"]["required"].as_array().unwrap() {
                assert!(f["parameters"]["properties"].get(required.as_str().unwrap()).is_some(), "{}", f["name"]);
            }
        }
    }

    #[test]
    fn argument_helpers_accept_numbers_as_text() {
        let args = json!({ "duration_seconds": "600", "label": 5, "x": null });
        assert_eq!(number(&args, "duration_seconds"), Some(600.0));
        assert_eq!(text(&args, "label"), "5");
        assert_eq!(text(&args, "x"), "");
        assert_eq!(number(&args, "fehlt"), None);
    }

    #[test]
    fn describe_names_the_target() {
        let call = Call { name: "open_app".into(), args: json!({ "name": "Spotify" }) };
        assert_eq!(describe(&call), "Öffne Spotify …");
    }
}
