//! Schnellbefehle: alltaegliche Saetze („öffne YouTube“, „Timer 10 Minuten“, „fahr in 20 Minuten runter“)
//! werden hier ohne Sprachmodell erkannt und sofort ausgefuehrt. Das spart das Laden und Nachdenken
//! des Modells (sonst bis zu 20 Sekunden). Erkennt die Liste einen Satz nicht sicher, gibt sie `None`
//! zurueck und das Modell uebernimmt wie bisher. Die Sicherheitsabfragen der Werkzeuge gelten unveraendert.

use chrono::{Datelike, Local, Timelike};
use serde_json::{json, Value};

/// Ein erkannter Schnellbefehl.
#[derive(Debug, Clone, PartialEq)]
pub enum Quick {
    /// Programm oder Webseite oeffnen; was davon, entscheidet die Ausfuehrung anhand der installierten Programme.
    Open(String),
    /// Wie `Open`, aber auf einem bestimmten Bildschirm („auf dem zweiten Bildschirm“).
    OpenOn(String, String),
    /// Ein Werkzeugaufruf, `times` Mal hintereinander (z. B. fuenfmal „lauter“).
    Tool { name: &'static str, args: Value, times: u8 },
    /// Antwort ohne Werkzeug (Uhrzeit, Datum).
    Say(String),
}

fn tool(name: &'static str, args: Value) -> Option<Quick> {
    Some(Quick::Tool { name, args, times: 1 })
}

// ---------- Text aufbereiten ----------

/// Kleinbuchstaben, Satzzeichen weg (ausser dem Doppelpunkt in Uhrzeiten), einfache Leerzeichen.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.to_lowercase().chars() {
        if ch.is_alphanumeric() || ch == ':' || ch == '%' {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    let words: Vec<&str> = out.split_whitespace().filter(|w| !FILLERS.contains(w)).collect();
    words.join(" ")
}

/// Woerter, die fuer die Bedeutung keine Rolle spielen.
const FILLERS: &[&str] = &["bitte", "mal", "doch", "einfach", "kurz", "jetzt", "gerade", "schnell", "hey", "milo", "mylo", "milo", "okay", "ok"];

/// Anreden am Satzanfang („kannst du …“).
const LEADS: &[&str] = &["kannst du", "könntest du", "würdest du", "kann ich", "ich möchte dass du", "ich will dass du", "du sollst"];

fn strip_leads(t: &str) -> &str {
    let mut rest = t;
    loop {
        let before = rest;
        for lead in LEADS {
            if let Some(r) = rest.strip_prefix(lead) {
                rest = r.trim_start();
            }
        }
        if rest == before {
            return rest;
        }
    }
}

fn words(t: &str) -> Vec<&str> {
    t.split(' ').filter(|w| !w.is_empty()).collect()
}

fn has(words: &[&str], any: &[&str]) -> bool {
    words.iter().any(|w| any.contains(w))
}

fn has_prefix(words: &[&str], prefixes: &[&str]) -> bool {
    words.iter().any(|w| prefixes.iter().any(|p| w.starts_with(p)))
}

fn phrase(t: &str, any: &[&str]) -> bool {
    let padded = format!(" {t} ");
    any.iter().any(|p| padded.contains(&format!(" {p} ")))
}

// ---------- Zahlen, Dauern, Uhrzeiten ----------

/// Zahl aus Ziffern oder einem deutschen Zahlwort bis 100 („fünfundzwanzig“).
pub fn number(word: &str) -> Option<u32> {
    let word = word.trim_end_matches('%');
    if let Ok(n) = word.parse() {
        return Some(n);
    }
    let simple = |w: &str| -> Option<u32> {
        Some(match w {
            "null" => 0,
            "ein" | "eine" | "einen" | "einer" | "eins" => 1,
            "zwei" => 2,
            "drei" => 3,
            "vier" => 4,
            "fünf" | "fuenf" => 5,
            "sechs" => 6,
            "sieben" => 7,
            "acht" => 8,
            "neun" => 9,
            "zehn" => 10,
            "elf" => 11,
            "zwölf" | "zwoelf" => 12,
            "dreizehn" => 13,
            "vierzehn" => 14,
            "fünfzehn" | "fuenfzehn" => 15,
            "sechzehn" => 16,
            "siebzehn" => 17,
            "achtzehn" => 18,
            "neunzehn" => 19,
            "zwanzig" => 20,
            "dreißig" | "dreissig" => 30,
            "vierzig" => 40,
            "fünfzig" | "fuenfzig" => 50,
            "sechzig" => 60,
            "siebzig" => 70,
            "achtzig" => 80,
            "neunzig" => 90,
            "hundert" | "einhundert" => 100,
            _ => return None,
        })
    };
    if let Some(n) = simple(word) {
        return Some(n);
    }
    // „fünfundzwanzig“ = fünf + zwanzig
    let (ones, tens) = word.split_once("und")?;
    let (ones, tens) = (simple(ones)?, simple(tens)?);
    (ones < 10 && tens >= 20 && tens % 10 == 0).then_some(tens + ones)
}

fn unit_seconds(word: &str) -> Option<u64> {
    match word {
        "sekunde" | "sekunden" | "sek" | "s" => Some(1),
        "minute" | "minuten" | "min" | "minütigen" | "minütiger" => Some(60),
        "stunde" | "stunden" | "std" | "h" => Some(3600),
        _ => None,
    }
}

/// Zahl und Einheit in einem Wort („10min“, „5minuten“, „zehnminuten“, „2std“).
fn joined_duration(word: &str) -> Option<(u32, u64)> {
    const UNITS: [&str; 10] = ["sekunden", "sekunde", "minuten", "minute", "stunden", "stunde", "sek", "min", "std", "h"];
    UNITS.iter().find_map(|unit| {
        let number_part = word.strip_suffix(unit)?;
        if number_part.is_empty() {
            return None;
        }
        Some((number(number_part)?, unit_seconds(unit)?))
    })
}

/// Dauer im Satz in Sekunden („10 Minuten“, „eine halbe Stunde“, „1 Stunde 30 Minuten“) und die Woerter, die dazu gehoeren.
fn duration(w: &[&str]) -> Option<(u64, Vec<usize>)> {
    let mut total = 0u64;
    let mut used = Vec::new();
    for i in 0..w.len() {
        match w[i] {
            "viertelstunde" => {
                total += 900;
                used.push(i);
            }
            "halbe" | "halben" if w.get(i + 1).is_some_and(|n| n.starts_with("stunde")) => {
                total += 1800;
                used.extend([i, i + 1]);
                if i > 0 && matches!(w[i - 1], "eine" | "einer" | "ne") {
                    used.push(i - 1);
                }
            }
            "anderthalb" | "eineinhalb" if w.get(i + 1).is_some_and(|n| n.starts_with("stunde")) => {
                total += 5400;
                used.extend([i, i + 1]);
            }
            word if joined_duration(word).is_some() => {
                // Zusammengeschrieben: „10min“, „10minuten“, „zehnminuten“.
                let (n, unit) = joined_duration(word).unwrap_or_default();
                total += n as u64 * unit;
                used.push(i);
            }
            word => {
                if let (Some(unit), Some(prev)) = (unit_seconds(word), i.checked_sub(1)) {
                    if used.contains(&i) {
                        continue;
                    }
                    if let Some(n) = number(w[prev]) {
                        total += n as u64 * unit;
                        used.extend([prev, i]);
                    }
                }
            }
        }
    }
    (total > 0).then_some((total, used))
}

/// Uhrzeit nach „um“ („um 18 Uhr“, „um 7:30“, „um 7 Uhr 15“) als HH:MM und die zugehoerigen Woerter.
fn clock(w: &[&str]) -> Option<(String, Vec<usize>)> {
    let at = w.iter().position(|x| *x == "um")?;
    let first = *w.get(at + 1)?;
    let mut used = vec![at, at + 1];
    let (hour, mut minute) = match first.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (number(first)?, 0),
    };
    if w.get(at + 2) == Some(&"uhr") {
        used.push(at + 2);
        if let Some(m) = w.get(at + 3).and_then(|m| number(m)) {
            if m < 60 {
                minute = m;
                used.push(at + 3);
            }
        }
    } else if !first.contains(':') {
        return None; // „um 5“ ohne „Uhr“ ist zu unsicher
    }
    (hour < 24 && minute < 60).then(|| (format!("{hour:02}:{minute:02}"), used))
}

/// „um 7“ ohne „Uhr“ (nur bei Weckern und Erinnerungen an einem Tag sicher genug).
fn bare_hour(w: &[&str]) -> Option<(String, Vec<usize>)> {
    let at = w.iter().position(|x| *x == "um")?;
    let hour = number(w.get(at + 1)?)?;
    (hour < 24).then(|| (format!("{hour:02}:00"), vec![at, at + 1]))
}

/// Minuten bis zur naechsten Uhrzeit HH:MM (heute oder morgen).
fn minutes_until(hhmm: &str) -> f64 {
    let (h, m) = hhmm.split_once(':').map(|(h, m)| (h.parse::<i64>().unwrap_or(0), m.parse::<i64>().unwrap_or(0))).unwrap_or((0, 0));
    let now = Local::now();
    let now_min = now.hour() as i64 * 60 + now.minute() as i64;
    let mut diff = h * 60 + m - now_min;
    if diff <= 0 {
        diff += 24 * 60;
    }
    diff as f64
}

/// Wann etwas passiert, in Minuten: aus einer Dauer („in 20 Minuten“) oder einer Uhrzeit („um 23 Uhr“); sonst sofort.
fn when_minutes(w: &[&str]) -> f64 {
    if let Some((seconds, _)) = duration(w) {
        return seconds as f64 / 60.0;
    }
    if let Some((hhmm, _)) = clock(w) {
        return minutes_until(&hhmm);
    }
    0.0
}

// ---------- Erkennung ----------

const PC: &[&str] = &["pc", "computer", "rechner", "laptop", "system", "windows", "kiste"];
const MEDIA_THINGS: &[&str] = &["lied", "song", "titel", "track", "video", "folge", "episode"];
const WEEKDAYS: &[&str] = &["montag", "dienstag", "mittwoch", "donnerstag", "freitag", "samstag", "sonntag"];

/// Erkennt einen Schnellbefehl (Deutsch, sonst Englisch) oder liefert `None`, wenn das Sprachmodell entscheiden soll.
pub fn parse(text: &str) -> Option<Quick> {
    parse_de(text).or_else(|| crate::quick_en::parse(text))
}

fn parse_de(text: &str) -> Option<Quick> {
    let original = text.trim();
    if original.is_empty() || original.chars().count() > 160 {
        return None;
    }
    let cleaned = clean(original);
    let t = strip_leads(&cleaned);
    let w = words(t);
    if w.is_empty() {
        return None;
    }

    // Notizen duerfen „und“ und Kommas enthalten, deshalb vor der Pruefung auf mehrere Auftraege.
    if let Some(note) = note(original, t) {
        return Some(note);
    }
    // Mehrere Auftraege in einem Satz erledigt das Modell (oder der Cloud-Helfer im Browser).
    let rest_of_sentence = original.trim_start_matches(|c: char| !c.is_alphanumeric());
    let after_name = strip_name(rest_of_sentence);
    if after_name.contains(',') || after_name.contains(';') || phrase(t, &["und", "dann", "danach", "anschließend", "außerdem"]) {
        return None;
    }
    if w.len() > 12 {
        return None;
    }

    if let Some(found) = on_screen(&w) {
        return Some(found);
    }
    workspace(t)
        .or_else(|| search(t))
        .or_else(|| power(t, &w))
        .or_else(|| focus(t, &w))
        .or_else(|| timer(t, &w))
        .or_else(|| volume(t, &w))
        .or_else(|| screenshot_or_playing(t, &w))
        .or_else(|| media(t, &w))
        .or_else(|| folder(&w))
        .or_else(|| weather(&w))
        .or_else(|| time_or_date(t))
        .or_else(|| youtube_or_search(t, &w))
        .or_else(|| close(t, &w))
        .or_else(|| open(t, &w))
}

/// Woerter, die einen Bildschirm benennen, und was davor stehen kann.
const SCREEN_WORDS: &[&str] = &["bildschirm", "bildschirms", "monitor", "monitors", "display", "screen"];
const SCREEN_FILL: &[&str] = &["auf", "dem", "den", "meinem", "meinen", "am", "im", "zum"];

/// Wo im Satz ein Bildschirm genannt wird: (Bildschirmangabe, Indizes der Woerter, die dazu gehoeren).
fn screen_phrase(w: &[&str]) -> Option<(String, Vec<usize>)> {
    let ordinal = |x: &str| {
        x.parse::<u32>().is_ok()
            || ["erst", "zweit", "dritt", "viert", "link", "recht", "ander", "haupt", "neben", "extern", "obere", "untere"].iter().any(|p| x.starts_with(p))
    };
    for (i, word) in w.iter().enumerate() {
        // Zusammengeschrieben: „zweitbildschirm“, „hauptmonitor“.
        if let Some(prefix) = SCREEN_WORDS.iter().find_map(|s| word.strip_suffix(s)).filter(|p| !p.is_empty() && ordinal(p)) {
            let mut used = vec![i];
            let mut j = i;
            while j > 0 && SCREEN_FILL.contains(&w[j - 1]) {
                j -= 1;
                used.push(j);
            }
            return Some((prefix.to_string(), used));
        }
        if !SCREEN_WORDS.contains(word) {
            continue;
        }
        // „Bildschirm 2“, „Monitor zwei“
        if let Some(n) = w.get(i + 1).and_then(|x| number(x)).filter(|n| (1..=4).contains(n)) {
            let mut used = vec![i, i + 1];
            let mut j = i;
            while j > 0 && SCREEN_FILL.contains(&w[j - 1]) {
                j -= 1;
                used.push(j);
            }
            return Some((n.to_string(), used));
        }
        if i == 0 || !ordinal(w[i - 1]) {
            continue;
        }
        let mut used = vec![i, i - 1];
        let mut j = i - 1;
        while j > 0 && SCREEN_FILL.contains(&w[j - 1]) {
            j -= 1;
            used.push(j);
        }
        return Some((w[i - 1].to_string(), used));
    }
    None
}

/// „Öffne YouTube auf dem zweiten Bildschirm“, „spiel Lofi auf YouTube auf dem linken Monitor“.
fn on_screen(w: &[&str]) -> Option<Quick> {
    let (spec, used) = screen_phrase(w)?;
    let rest: Vec<&str> = w.iter().enumerate().filter(|(i, _)| !used.contains(i)).map(|(_, x)| *x).collect();
    if rest.is_empty() {
        return None;
    }
    let t = rest.join(" ");
    match open(&t, &rest).or_else(|| youtube_or_search(&t, &rest)).or_else(|| search(&t))? {
        Quick::Open(target) => Some(Quick::OpenOn(target, spec)),
        Quick::Tool { name, mut args, times } if ["play_youtube", "browser_search"].contains(&name) => {
            args["screen"] = json!(spec);
            Some(Quick::Tool { name, args, times })
        }
        _ => None,
    }
}

/// „Mach einen Screenshot (vom zweiten Bildschirm)“ und „Was läuft gerade?“.
fn screenshot_or_playing(t: &str, w: &[&str]) -> Option<Quick> {
    let shot = has_prefix(w, &["screenshot", "bildschirmfoto", "bildschirmaufnahme"]);
    let looking = has(w, &["was", "schau", "siehst", "beschreib", "beschreibe", "erklär", "erkläre", "lies"]);
    if shot && !looking {
        let screen = screen_phrase(w).map(|(spec, _)| spec).unwrap_or_default();
        return tool("save_screenshot", json!({ "screen": screen }));
    }
    // „gerade“ ist schon als Fuellwort entfernt: „was läuft gerade“ kommt als „was läuft“ an.
    let playing = t == "was läuft" || phrase(t, &["was läuft da", "was läuft hier", "welches lied", "welcher song", "welcher titel läuft", "wie heißt das lied", "wie heißt der song", "was höre ich", "was hör ich"]);
    if playing && !has(w, &["fernsehen", "tv", "kino", "heute", "abend", "netflix"]) {
        return tool("now_playing", json!({}));
    }
    None
}

/// Entfernt eine Anrede wie „Milo,“ am Satzanfang, damit ihr Komma nicht als zweiter Auftrag gilt.
fn strip_name(text: &str) -> &str {
    let lower = text.to_lowercase();
    for name in ["hey milo", "hey mylo", "hey milo", "milo", "mylo", "milo", "okay", "ok", "hey"] {
        if lower.starts_with(name) {
            return text[name.len()..].trim_start_matches(|c: char| !c.is_alphanumeric());
        }
    }
    text
}

fn note(original: &str, t: &str) -> Option<Quick> {
    const TRIGGERS: &[&str] = &["merk dir", "merke dir", "notier dir", "notiere dir", "notier", "notiere", "schreib dir auf", "speicher dir"];
    let trigger = TRIGGERS.iter().find(|p| t.starts_with(*p))?;
    // Inhalt aus dem Originaltext holen, damit Gross- und Kleinschreibung erhalten bleiben.
    let lower = original.to_lowercase();
    let start = lower.find(trigger)? + trigger.len();
    let mut content = original.get(start..)?.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
    for lead in ["dass ", "das ", "folgendes "] {
        if content.to_lowercase().starts_with(lead) && content.chars().count() > lead.len() + 3 {
            content = content[lead.len()..].to_string();
        }
    }
    let content = content.trim().trim_end_matches(['.', '!']).trim().to_string();
    if content.chars().count() < 4 || ["das", "dies", "es"].contains(&content.to_lowercase().as_str()) {
        return None;
    }
    let mut chars = content.chars();
    let first = chars.next()?.to_uppercase().collect::<String>();
    tool("save_note", json!({ "text": format!("{first}{}.", chars.as_str()) }))
}

fn power(t: &str, w: &[&str]) -> Option<Quick> {
    let shut_words = ["herunterfahren", "runterfahren", "ausschalten", "herunter", "runter", "neustart", "neustarten", "energiesparmodus", "energiesparen", "standby"];
    let cancel_words = ["abbrechen", "abbruch", "abbrich", "brich", "stopp", "stoppe", "stoppen", "nicht", "cancel"];
    let mentions_power = has(w, &shut_words) || phrase(t, &["neu starten", "energie sparen"]);
    if mentions_power && has(w, &cancel_words) {
        return tool("system_power", json!({ "action": "abort" }));
    }
    let off_topic = has(w, &["lautstärke", "leiser", "lauter", "musik", "scroll", "scrolle", "seite", "rollladen", "rollo", "datei", "download", "herunterladen", "runterladen"]);
    if off_topic {
        return None;
    }
    let minutes = when_minutes(w);
    let drive = has_prefix(w, &["fahr", "fähr"]);
    let shutdown = has(w, &["herunterfahren", "runterfahren"])
        || (drive && has(w, &["herunter", "runter"]))
        || (has(w, &["schalte", "schalt", "mach", "ausschalten"]) && has(w, PC) && (has(w, &["aus"]) || has(w, &["ausschalten"])))
        || t == "pc aus";
    if shutdown {
        return tool("system_power", json!({ "action": "shutdown", "in_minutes": minutes }));
    }
    let restart = has(w, &["neustart", "neustarten"]) || (has(w, &["starte", "start", "starten"]) && w.last() == Some(&"neu"));
    if restart && (has(w, PC) || w[0].starts_with("neustart")) {
        return tool("system_power", json!({ "action": "restart", "in_minutes": minutes }));
    }
    if has(w, &["energiesparmodus", "energiesparen", "standby", "ruhemodus", "schlafmodus"]) || phrase(t, &["energie sparen"]) {
        return tool("system_power", json!({ "action": "sleep", "in_minutes": minutes }));
    }
    if (has_prefix(w, &["sperr"]) && (has(w, PC) || has(w, &["bildschirm"]))) || t == "sperren" {
        return tool("system_power", json!({ "action": "lock" }));
    }
    if has(w, &["bildschirm", "monitor", "display", "bildschirme", "monitore"]) && has(w, &["aus", "ausschalten"]) && !has(w, &["wie", "was", "sieht", "siehst", "schau"]) {
        return tool("system_power", json!({ "action": "screen_off" }));
    }
    None
}

/// Fokus-Modus: „Fokus 25 Minuten“, „Pomodoro“, „Fokus beenden“.
fn focus(t: &str, w: &[&str]) -> Option<Quick> {
    let about = w.iter().any(|x| x.starts_with("fokus") || x.starts_with("pomodoro") || *x == "konzentrationsmodus");
    if !about {
        return None;
    }
    if has(w, &["beenden", "beende", "stopp", "stop", "aus", "abbrechen", "ende"]) {
        return tool("focus_mode", json!({ "action": "stop" }));
    }
    let minutes = duration(w).map(|(s, _)| (s / 60).max(1)).unwrap_or(25);
    let _ = t;
    tool("focus_mode", json!({ "action": "start", "minutes": minutes }))
}

fn timer(t: &str, w: &[&str]) -> Option<Quick> {
    let timer_word = |x: &&str| x.ends_with("timer") || ["wecker", "countdown", "erinnerung", "alarm"].contains(x);
    let is_timer = w.iter().any(timer_word);
    let remind = t.starts_with("erinner") || phrase(t, &["erinnere mich", "erinner mich"]);
    let wake = t.starts_with("weck mich") || t.starts_with("wecke mich");

    if is_timer && has(w, &["abbrechen", "abbruch", "stopp", "stoppe", "stoppen", "aus", "ausschalten", "löschen", "lösche", "lösch", "beenden", "beende", "brich", "entfernen", "weg"]) {
        let stop = ["abbrechen", "abbruch", "stopp", "stoppe", "stoppen", "aus", "ausschalten", "löschen", "lösche", "lösch", "beenden", "beende", "brich", "entfernen", "weg", "ab", "den", "die", "das", "der", "mein", "meinen", "meine", "timer", "wecker", "countdown", "erinnerung", "alarm"];
        let label: Vec<&str> = w
            .iter()
            .copied()
            .filter(|x| !stop.contains(x))
            .map(|x| x.strip_suffix("timer").unwrap_or(x))
            .filter(|x| !x.is_empty())
            .collect();
        return tool("cancel_timer", json!({ "label": label.join(" ") }));
    }
    if !(is_timer || remind || wake || (t.starts_with("stell") && duration(w).is_some())) {
        return None;
    }
    // Wiederkehrender Wecker: „weck mich werktags um 7“, „Wecker jeden Tag um 6:30“.
    if wake || w.contains(&"wecker") {
        if let (Some(days), Some((hhmm, _))) = (crate::alarms::days_from_words(w), clock(w).or_else(|| bare_hour(w))) {
            return tool("set_alarm", json!({ "time": hhmm, "days": days }));
        }
    }
    // Erinnerung an einem anderen Tag: „morgen“, „übermorgen“, „am Montag“.
    let day_word = w.iter().position(|x| ["morgen", "übermorgen", "uebermorgen"].contains(x) || WEEKDAYS.contains(x));
    if let Some(at) = day_word.filter(|_| remind || wake || is_timer) {
        let at_time = clock(w).or_else(|| bare_hour(w));
        let mut used: Vec<usize> = at_time.as_ref().map(|(_, u)| u.clone()).unwrap_or_default();
        used.push(at);
        if at > 0 && ["am", "nächsten", "naechsten", "kommenden"].contains(&w[at - 1]) {
            used.push(at - 1);
        }
        let marker = if remind { ["an", "daran", "dass"].as_slice() } else { ["für", "fürs"].as_slice() };
        let skip = ["den", "die", "das", "der", "dem", "mich", "in", "auf", "nach", "von", "uhr", "ab", "jetzt", "dass", "ich", "früh", "abend", "am"];
        let label = w
            .iter()
            .position(|x| marker.contains(x))
            .map(|start| w.iter().enumerate().skip(start + 1).filter(|(i, x)| !used.contains(i) && !skip.contains(*x) && !timer_word(*x)).map(|(_, x)| *x).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let mut args = json!({ "day": w[at] });
        if let Some((hhmm, _)) = at_time {
            args["at_time"] = json!(hhmm);
        }
        let label = if label.is_empty() && wake { "Wecker".to_string() } else { capitalize(&label) };
        if !label.is_empty() {
            args["label"] = json!(label);
        }
        return tool("set_timer", args);
    }
    let (mut args, used) = if let Some((seconds, used)) = duration(w) {
        (json!({ "duration_seconds": seconds }), used)
    } else if let Some((hhmm, used)) = clock(w) {
        (json!({ "at_time": hhmm }), used)
    } else {
        return None; // ohne Dauer oder Uhrzeit fragt besser das Modell nach
    };
    // Bezeichnung: bei Erinnerungen das, was nach „an“ / „daran“ kommt, sonst nach „für“.
    let marker = if remind { ["an", "daran", "dass"].as_slice() } else { ["für", "fürs"].as_slice() };
    let label = w
        .iter()
        .position(|x| marker.contains(x))
        .map(|start| {
            let skip = ["den", "die", "das", "der", "dem", "mich", "in", "auf", "nach", "von", "uhr", "ab", "jetzt", "dass", "ich", "stellen", "stell"];
            w.iter()
                .enumerate()
                .skip(start + 1)
                .filter(|(i, x)| !used.contains(i) && !skip.contains(*x) && !timer_word(*x))
                .map(|(_, x)| *x)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let label = if label.is_empty() && wake { "Wecker".to_string() } else { capitalize(&label) };
    if !label.is_empty() {
        args["label"] = json!(label);
    }
    tool("set_timer", args)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// „Arbeitsplatz speichern als Schule“, „Arbeitsplatz Schule“, „Arbeitsplatz Schule öffnen“.
fn workspace(t: &str) -> Option<Quick> {
    let rest = t.strip_prefix("speichere den arbeitsplatz ").or_else(|| t.strip_prefix("speichere arbeitsplatz ")).map(|r| ("save", r));
    let rest = rest.or_else(|| t.strip_prefix("arbeitsplatz speichern ").map(|r| ("save", r)));
    let (action, name) = match rest {
        Some((action, r)) => (action, r.strip_prefix("als ").unwrap_or(r)),
        None => {
            let r = t.strip_prefix("arbeitsplatz ")?;
            let name = ["öffnen", "oeffnen", "laden", "starten", "wiederherstellen", "herstellen"].iter().find_map(|v| r.strip_suffix(*v)).unwrap_or(r);
            ("open", name)
        }
    };
    let name = name.trim().trim_matches(|c: char| c == '"' || c == '„' || c == '“');
    if name.is_empty() || name.split_whitespace().count() > 4 {
        return None;
    }
    // Der Text ist schon klein geschrieben; der Name bekommt einen grossen Anfangsbuchstaben.
    let mut chars = name.chars();
    let name: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    tool("workspace", json!({ "action": action, "name": name }))
}

fn volume(t: &str, w: &[&str]) -> Option<Quick> {
    let about_volume = has(w, &["lautstärke", "lautstaerke", "volume", "ton"]) || has(w, &["lauter", "leiser"]);
    if about_volume {
        // Genauer Wert: „Lautstärke auf 30“, „30 Prozent“, „leiser auf 20“
        let value = w.iter().enumerate().find_map(|(i, x)| {
            let n = number(x)?;
            let marked = x.ends_with('%') || w.get(i + 1).is_some_and(|n| *n == "prozent" || *n == "%") || (i > 0 && w[i - 1] == "auf");
            (marked && n <= 100).then_some(n)
        });
        if let Some(percent) = value {
            return tool("set_volume", json!({ "percent": percent }));
        }
    }
    if has(w, &["stumm", "stummschalten", "mute", "lautlos"]) || t == "ton aus" || t == "ton an" {
        return tool("media_control", json!({ "action": "mute" }));
    }
    // Eine Taste aendert die Lautstaerke um 2 %; fuenf Druecke sind ein spuerbarer Schritt.
    let steps = if has(w, &["viel", "deutlich", "ordentlich", "richtig"]) { 10 } else if has(w, &["bisschen", "etwas", "wenig", "klein"]) { 2 } else { 5 };
    if has(w, &["lauter"]) {
        return Some(Quick::Tool { name: "media_control", args: json!({ "action": "volume_up" }), times: steps });
    }
    if has(w, &["leiser"]) {
        return Some(Quick::Tool { name: "media_control", args: json!({ "action": "volume_down" }), times: steps });
    }
    None
}

fn media(t: &str, w: &[&str]) -> Option<Quick> {
    if has_prefix(w, &["überspring", "ueberspring", "skip"]) || (has_prefix(w, &["nächst", "naechst"]) && has(w, MEDIA_THINGS)) {
        return tool("media_control", json!({ "action": "next" }));
    }
    if (has_prefix(w, &["vorherig", "vorig", "letzt"]) && has(w, MEDIA_THINGS)) || phrase(t, &["lied zurück", "titel zurück", "song zurück"]) {
        return tool("media_control", json!({ "action": "previous" }));
    }
    let seek = has_prefix(w, &["spul", "vorspul", "zurückspul"])
        || (duration(w).is_some() && has(w, &["vor", "zurück", "vorwärts", "rückwärts", "nach vorne", "zurueck"]) && w.len() <= 5);
    if seek && !w.iter().any(|x| x.ends_with("timer")) {
        let back = has(w, &["zurück", "zurueck", "rückwärts", "zurückspulen"]) || w.iter().any(|x| x.starts_with("zurückspul"));
        let seconds = duration(w).map(|(s, _)| s).unwrap_or(10).clamp(1, 3600);
        return tool("media_control", json!({ "action": if back { "rewind" } else { "forward" }, "seconds": seconds }));
    }
    if w.len() <= 4 {
        let target_ok = w.iter().all(|x| ["die", "das", "den", "musik", "video", "film", "lied", "song", "wiedergabe", "serie", "folge", "an", "mach", "halt", "spiel", "es"].contains(x) || is_media_verb(x));
        if target_ok {
            if has_prefix(w, &["paus", "anhalten"]) || has(w, &["stopp", "stop", "stoppe", "stoppen"]) || t == "halt an" {
                return tool("media_control", json!({ "action": "pause" }));
            }
            if has(w, &["weiter", "weiterspielen", "fortsetzen", "play", "abspielen"]) || t == "musik an" {
                return tool("media_control", json!({ "action": "play" }));
            }
        }
    }
    None
}

fn is_media_verb(word: &str) -> bool {
    word.starts_with("paus") || ["anhalten", "stopp", "stop", "stoppe", "stoppen", "weiter", "weiterspielen", "fortsetzen", "play", "abspielen"].contains(&word)
}

fn folder(w: &[&str]) -> Option<Quick> {
    let verb = has(w, &["öffne", "oeffne", "öffnen", "zeig", "zeige", "geh", "gehe"]);
    let explicit = has(w, &["ordner", "meine", "meinen"]);
    let name = w.iter().find_map(|x| match *x {
        x if x.starts_with("download") => Some("downloads"),
        x if x.starts_with("dokument") => Some("dokumente"),
        "desktop" | "schreibtisch" => Some("desktop"),
        "bilder" | "fotos" | "bildern" => Some("bilder"),
        "musik" => Some("musik"),
        "videos" => Some("videos"),
        _ => None,
    })?;
    // Downloads, Dokumente und Desktop sind eindeutig; Musik, Bilder, Videos nur mit „Ordner“ oder „meine“.
    let clear = matches!(name, "downloads" | "dokumente" | "desktop");
    (verb && (explicit || clear) && w.len() <= 6).then(|| tool("open_folder", json!({ "name": name }))).flatten()
}

fn weather(w: &[&str]) -> Option<Quick> {
    if !has(w, &["wetter"]) || has(w, &["morgen", "übermorgen", "wochenende", "woche", "nächste", "nächsten", "gestern", "abend", "nachmittag", "später"]) || has(w, WEEKDAYS) {
        return None;
    }
    let skip = ["heute", "aus", "so", "da", "draußen", "momentan", "aktuell", "der", "die", "das"];
    let place = w
        .iter()
        .rposition(|x| *x == "in")
        .map(|i| w[i + 1..].iter().copied().filter(|x| !skip.contains(x)).map(|x| capitalize(x)).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    tool("get_weather", json!({ "location": place }))
}

fn time_or_date(t: &str) -> Option<Quick> {
    let now = Local::now();
    if t.split(' ').count() <= 6 && (t.contains("wie spät") || t.contains("wie viel uhr") || t.contains("wieviel uhr") || t == "uhrzeit") {
        return Some(Quick::Say(format!("Es ist {:02}:{:02} Uhr.", now.hour(), now.minute())));
    }
    if t.split(' ').count() <= 7 && (t.contains("welcher tag") || t.contains("welches datum") || t.contains("der wievielte") || t.contains("welchen tag") || t.contains("den wievielten")) {
        const DAYS: [&str; 7] = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];
        const MONTHS: [&str; 12] = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
        return Some(Quick::Say(format!(
            "Heute ist {}, der {}. {} {}.",
            DAYS[now.weekday().num_days_from_monday() as usize],
            now.day(),
            MONTHS[now.month0() as usize],
            now.year()
        )));
    }
    None
}

/// „such auf YouTube nach X“ und „google X“ zeigen die Ergebnisse im Browser.
fn search(t: &str) -> Option<Quick> {
    for (prefix, site) in [("such auf youtube nach ", "youtube"), ("suche auf youtube nach ", "youtube"), ("such bei google nach ", "google"), ("suche bei google nach ", "google"), ("google nach ", "google"), ("google ", "google")] {
        if let Some(query) = t.strip_prefix(prefix) {
            return (!query.is_empty()).then(|| tool("browser_search", json!({ "query": query, "site": site }))).flatten();
        }
    }
    None
}

/// „spiel X auf YouTube“ startet direkt das erste passende Video.
fn youtube_or_search(_t: &str, w: &[&str]) -> Option<Quick> {
    if !has(w, &["youtube"]) || has(w, &["kanal", "kanals", "channel"]) {
        return None;
    }
    let verbs = ["spiel", "spiele", "spielen", "zeig", "zeige", "abspielen", "starte", "öffne", "mir", "auf", "youtube", "video", "ein", "das", "den", "die", "an", "von"];
    if !has(w, &["spiel", "spiele", "spielen", "zeig", "zeige", "abspielen"]) {
        return None;
    }
    let query: Vec<&str> = w.iter().copied().filter(|x| !verbs.contains(x)).collect();
    (!query.is_empty()).then(|| tool("play_youtube", json!({ "query": query.join(" ") }))).flatten()
}

const ARTICLES: &[&str] = &["den", "die", "das", "der", "dem", "mir", "mein", "meine", "meinen", "programm", "app", "anwendung", "bitte"];

fn close(t: &str, w: &[&str]) -> Option<Quick> {
    let verb = matches!(w[0], "schließe" | "schließ" | "schliess" | "schliesse" | "schließen" | "beende" | "beend" | "beenden");
    let mach_zu = w[0] == "mach" && w.last() == Some(&"zu");
    if !(verb || mach_zu) {
        return None;
    }
    let end = if mach_zu { w.len() - 1 } else { w.len() };
    let target: Vec<&str> = w[1..end].iter().copied().filter(|x| !ARTICLES.contains(x)).collect();
    if target.is_empty() || target.len() > 3 || has(&target, &["timer", "fenster", "alle", "alles", "tab"]) || t.contains("herunter") {
        return None;
    }
    tool("close_app", json!({ "name": target.join(" ") }))
}

fn open(t: &str, w: &[&str]) -> Option<Quick> {
    let verb = matches!(w[0], "öffne" | "oeffne" | "öffnen" | "starte" | "start" | "starten" | "öffnest" | "startest");
    let mach_auf = w[0] == "mach" && w.last() == Some(&"auf");
    let go = t.starts_with("geh auf ") || t.starts_with("gehe auf ") || t.starts_with("geh zu ") || t.starts_with("gehe zu ");
    if !(verb || mach_auf || go) {
        return None;
    }
    let start = if go { 2 } else { 1 };
    let end = if mach_auf { w.len() - 1 } else { w.len() };
    if start >= end {
        return None;
    }
    let target: Vec<&str> = w[start..end].iter().copied().filter(|x| !ARTICLES.contains(x) && !["seite", "webseite", "website"].contains(x)).collect();
    let unsure = ["datei", "dokument", "notiz", "notizen", "timer", "ordner", "bild", "foto", "pdf", "neu", "letzte", "zuletzt", "mail", "email", "e", "wieder", "auf", "in", "mit", "im", "von", "eine", "einen", "ein"];
    if target.is_empty() || target.len() > 4 || has(&target, &unsure) {
        return None;
    }
    Some(Quick::Open(target.join(" ")))
}

// ---------- Webseiten ----------

/// Bekannte Seiten (normalisierter Name, Anzeigename, Adresse, zuerst als Programm versuchen?).
const SITES: &[(&str, &str, &str, bool)] = &[
    ("youtube", "YouTube", "https://www.youtube.com", false),
    ("netflix", "Netflix", "https://www.netflix.com", false),
    ("disney", "Disney+", "https://www.disneyplus.com", false),
    ("disney plus", "Disney+", "https://www.disneyplus.com", false),
    ("disneyplus", "Disney+", "https://www.disneyplus.com", false),
    ("prime", "Prime Video", "https://www.primevideo.com", false),
    ("prime video", "Prime Video", "https://www.primevideo.com", false),
    ("amazon prime", "Prime Video", "https://www.primevideo.com", false),
    ("amazon prime video", "Prime Video", "https://www.primevideo.com", false),
    ("amazon", "Amazon", "https://www.amazon.de", false),
    ("twitch", "Twitch", "https://www.twitch.tv", false),
    ("crunchyroll", "Crunchyroll", "https://www.crunchyroll.com", false),
    ("joyn", "Joyn", "https://www.joyn.de", false),
    ("google", "Google", "https://www.google.de", false),
    ("gmail", "Gmail", "https://mail.google.com", false),
    ("google maps", "Google Maps", "https://maps.google.com", false),
    ("maps", "Google Maps", "https://maps.google.com", false),
    ("wikipedia", "Wikipedia", "https://de.wikipedia.org", false),
    ("ebay", "eBay", "https://www.ebay.de", false),
    ("reddit", "Reddit", "https://www.reddit.com", false),
    ("instagram", "Instagram", "https://www.instagram.com", false),
    ("facebook", "Facebook", "https://www.facebook.com", false),
    ("tiktok", "TikTok", "https://www.tiktok.com", false),
    ("twitter", "X", "https://x.com", false),
    ("chatgpt", "ChatGPT", "https://chatgpt.com", false),
    ("gemini", "Gemini", "https://gemini.google.com", false),
    ("claude", "Claude", "https://claude.ai", true),
    ("spotify", "Spotify", "https://open.spotify.com", true),
    ("whatsapp", "WhatsApp", "https://web.whatsapp.com", true),
    ("discord", "Discord", "https://discord.com/app", true),
    ("outlook", "Outlook", "https://outlook.live.com", true),
];

/// Bekannte Webseite zu einem Namen: (Anzeigename, Adresse, zuerst als Programm versuchen).
pub fn site(name: &str) -> Option<(&'static str, &'static str, bool)> {
    let key = crate::apps::normalize(name);
    let key = key.strip_suffix(" com").or_else(|| key.strip_suffix(" de")).unwrap_or(&key);
    SITES.iter().find(|(k, ..)| *k == key).map(|(_, display, url, app_first)| (*display, *url, *app_first))
}

/// Sieht der Name wie eine Webadresse aus („heise.de“)?
pub fn looks_like_domain(name: &str) -> bool {
    !name.contains(' ') && name.contains('.') && name.rsplit('.').next().is_some_and(|tld| (2..=6).contains(&tld.len()) && tld.chars().all(|c| c.is_ascii_alphabetic()))
}

// ---------- Mehrschritt im Browser ----------

/// Mehrere Schritte im Browser („öffne Netflix, klick auf Philipp und spiel The Middle“): das kann
/// der Cloud-Helfer schneller und sicherer als das kleine lokale Modell.
pub fn is_browser_task(text: &str) -> bool {
    if crate::quick_en::is_browser_task(text) {
        return true;
    }
    let t = clean(text);
    let w = words(&t);
    let streaming = has(&w, &["netflix", "disney", "prime", "crunchyroll", "joyn", "twitch", "sky", "wow", "paramount", "rtl"]) || t.contains("amazon prime");
    let page = streaming || has(&w, &["browser", "webseite", "website", "youtube", "seite"]);
    let click = has_prefix(&w, &["klick", "drück", "wähl", "tipp", "gib", "such"]) || has(&w, &["profil", "staffel", "folge", "episode", "weiterschauen", "fortsetzen", "suchfeld"]);
    let play = has_prefix(&w, &["spiel", "schau", "abspiel", "start"]);
    let steps = text.contains(',') || phrase(&t, &["und", "dann", "danach"]);
    // Livestreams und Bildschirmangaben brauchen meist mehrere Schritte (Kanal pruefen, oeffnen, verschieben).
    let live = has_prefix(&w, &["livestream", "stream", "übertragung", "live"]);
    let screen = screen_phrase(&w).is_some();
    page && ((click && (steps || streaming)) || (streaming && (play || live || screen)))
}

#[cfg(test)]
mod tests {

    #[test]
    fn workspaces_by_voice() {
        let args = |text: &str| match parse(text) {
            Some(Quick::Tool { name: "workspace", args, .. }) => Some((args["action"].as_str().unwrap().to_string(), args["name"].as_str().unwrap().to_string())),
            _ => None,
        };
        assert_eq!(args("Arbeitsplatz speichern als Schule"), Some(("save".into(), "Schule".into())));
        assert_eq!(args("Speichere den Arbeitsplatz Zocken"), Some(("save".into(), "Zocken".into())));
        assert_eq!(args("Arbeitsplatz Schule öffnen"), Some(("open".into(), "Schule".into())));
        assert_eq!(args("Milo, Arbeitsplatz Schule"), Some(("open".into(), "Schule".into())));
        assert_eq!(args("Was ist ein guter Arbeitsplatz"), None);
    }
    use super::*;

    fn call(text: &str) -> (String, Value) {
        match parse(text) {
            Some(Quick::Tool { name, args, .. }) => (name.to_string(), args),
            other => panic!("„{text}“ ergab {other:?}"),
        }
    }

    fn none(text: &str) {
        assert_eq!(parse(text), None, "„{text}“ sollte ans Modell gehen");
    }

    #[test]
    fn numbers() {
        assert_eq!(number("20"), Some(20));
        assert_eq!(number("zwanzig"), Some(20));
        assert_eq!(number("fünfundzwanzig"), Some(25));
        assert_eq!(number("einen"), Some(1));
        assert_eq!(number("hallo"), None);
    }

    #[test]
    fn shutdown_variants() {
        for text in ["Fahr meinen PC in 20 Minuten runter", "fahr den Computer in zwanzig Minuten herunter", "Milo, fahr den PC in 20 Minuten herunter.", "Kannst du bitte in 20 Minuten den Rechner herunterfahren?"] {
            let (name, args) = call(text);
            assert_eq!(name, "system_power", "{text}");
            assert_eq!(args["action"], "shutdown", "{text}");
            assert_eq!(args["in_minutes"], 20.0, "{text}");
        }
        assert_eq!(call("Fahr den PC herunter").1["in_minutes"], 0.0);
        assert_eq!(call("PC in einer halben Stunde ausschalten").1["in_minutes"], 30.0);
        assert_eq!(call("Starte den PC neu").1["action"], "restart");
        assert_eq!(call("Neustart in 5 Minuten").1["in_minutes"], 5.0);
        assert_eq!(call("Herunterfahren abbrechen").1["action"], "abort");
        assert_eq!(call("Fahr den PC doch nicht runter").1["action"], "abort");
        assert_eq!(call("Sperr den PC").1["action"], "lock");
        assert_eq!(call("Bildschirm aus").1["action"], "screen_off");
        assert_eq!(call("Energiesparmodus in 10 Minuten").1["action"], "sleep");
        // So schreibt die Spracherkennung Zeiten manchmal zusammen.
        for text in ["Fahr den PC in 10min runter", "Fahr dich in 10-Minuten herunter", "fahr den Computer in zehnminuten runter", "In 10 Min. herunterfahren"] {
            assert_eq!(call(text).1["in_minutes"], 10.0, "{text}");
        }
    }

    #[test]
    fn shutdown_at_clock_time() {
        let (_, args) = call("Fahr den PC um 23:30 herunter");
        let minutes = args["in_minutes"].as_f64().unwrap();
        assert!(minutes > 0.0 && minutes <= 24.0 * 60.0);
    }

    #[test]
    fn timers() {
        let (name, args) = call("Stell einen Timer auf 10 Minuten");
        assert_eq!((name.as_str(), args["duration_seconds"].as_u64()), ("set_timer", Some(600)));
        assert_eq!(call("Timer 1 Stunde 30 Minuten").1["duration_seconds"], 5400);
        assert_eq!(call("Timer für eine halbe Stunde").1["duration_seconds"], 1800);
        let (_, args) = call("Timer für die Pizza 12 Minuten");
        assert_eq!((args["duration_seconds"].as_u64(), args["label"].as_str()), (Some(720), Some("Pizza")));
        let (_, args) = call("Erinnere mich in 20 Minuten an die Wäsche");
        assert_eq!((args["duration_seconds"].as_u64(), args["label"].as_str()), (Some(1200), Some("Wäsche")));
        let (_, args) = call("Erinnere mich um 18 Uhr an den Anruf");
        assert_eq!((args["at_time"].as_str(), args["label"].as_str()), (Some("18:00"), Some("Anruf")));
        let (_, args) = call("Weck mich um 7:30");
        assert_eq!((args["at_time"].as_str(), args["label"].as_str()), (Some("07:30"), Some("Wecker")));
        assert_eq!(call("Timer abbrechen").0, "cancel_timer");
        assert_eq!(call("Brich den Pizzatimer ab").1["label"], "pizza");
        none("Stell einen Timer");
        let (_, args) = call("Erinnere mich morgen um 8 Uhr an den Müll");
        assert_eq!((args["day"].as_str(), args["at_time"].as_str(), args["label"].as_str()), (Some("morgen"), Some("08:00"), Some("Müll")));
        let (_, args) = call("Erinnere mich am Montag an die Steuer");
        assert_eq!((args["day"].as_str(), args["label"].as_str()), (Some("montag"), Some("Steuer")));
        let (name, args) = call("Weck mich werktags um 7");
        assert_eq!((name.as_str(), args["time"].as_str()), ("set_alarm", Some("07:00")));
        assert_eq!(args["days"], json!([0, 1, 2, 3, 4]));
        let (name, args) = call("Fokus 50 Minuten");
        assert_eq!((name.as_str(), args["minutes"].as_u64()), ("focus_mode", Some(50)));
        assert_eq!(call("Pomodoro").1["minutes"], 25);
        assert_eq!(call("Fokus beenden").1["action"], "stop");
    }

    #[test]
    fn media_and_volume() {
        assert_eq!(call("Lautstärke auf 30 Prozent").1["percent"], 30);
        assert_eq!(call("Mach die Lautstärke auf zwanzig").1["percent"], 20);
        let lauter = parse("Mach lauter");
        assert_eq!(lauter, Some(Quick::Tool { name: "media_control", args: json!({ "action": "volume_up" }), times: 5 }));
        assert_eq!(call("leiser").1["action"], "volume_down");
        assert_eq!(call("Stumm").1["action"], "mute");
        assert_eq!(call("Pause").1["action"], "pause");
        assert_eq!(call("Pausiere die Musik").1["action"], "pause");
        assert_eq!(call("Weiter").1["action"], "play");
        assert_eq!(call("Nächstes Lied").1["action"], "next");
        assert_eq!(call("Vorheriges Lied").1["action"], "previous");
        let (_, args) = call("Spul 30 Sekunden zurück");
        assert_eq!((args["action"].as_str(), args["seconds"].as_u64()), (Some("rewind"), Some(30)));
        assert_eq!(call("10 Sekunden vor").1["action"], "forward");
    }

    #[test]
    fn open_and_close() {
        assert_eq!(parse("Öffne YouTube"), Some(Quick::Open("youtube".into())));
        assert_eq!(parse("Milo, öffne bitte Netflix"), Some(Quick::Open("netflix".into())));
        assert_eq!(parse("Starte Spotify"), Some(Quick::Open("spotify".into())));
        assert_eq!(parse("Mach Discord auf"), Some(Quick::Open("discord".into())));
        assert_eq!(parse("Starte den Rechner"), Some(Quick::Open("rechner".into())));
    }

    #[test]
    fn screenshots_and_now_playing() {
        assert_eq!(call("Mach einen Screenshot").0, "save_screenshot");
        let (name, args) = call("Mach einen Screenshot vom zweiten Bildschirm");
        assert_eq!((name.as_str(), args["screen"].as_str()), ("save_screenshot", Some("zweiten")));
        assert_eq!(call("Was läuft gerade?").0, "now_playing");
        assert_eq!(call("Welches Lied ist das").0, "now_playing");
        // Auf den Bildschirm schauen ist etwas anderes (Gemini), und Fernsehprogramm auch.
        none("Schau dir den Screenshot an, was siehst du");
        none("Was läuft heute Abend im Fernsehen");
    }

    #[test]
    fn opening_on_another_screen() {
        assert_eq!(parse("Öffne YouTube auf dem zweiten Bildschirm"), Some(Quick::OpenOn("youtube".into(), "zweiten".into())));
        assert_eq!(parse("Öffne Spotify auf meinem linken Monitor"), Some(Quick::OpenOn("spotify".into(), "linken".into())));
        assert_eq!(parse("Starte Discord auf Bildschirm 2"), Some(Quick::OpenOn("discord".into(), "2".into())));
        assert_eq!(parse("Öffne Twitch auf Monitor zwei"), Some(Quick::OpenOn("twitch".into(), "2".into())));
        assert_eq!(parse("Öffne Netflix auf dem Zweitbildschirm"), Some(Quick::OpenOn("netflix".into(), "zweit".into())));
        let (name, args) = call("Spiel Lofi Musik auf YouTube auf dem zweiten Bildschirm");
        assert_eq!((name.as_str(), args["screen"].as_str()), ("play_youtube", Some("zweiten")));
        // Der Bildschirm allein ist kein Ziel: „Bildschirm aus“ bleibt ein Energiebefehl.
        assert_eq!(call("Bildschirm aus").1["action"], "screen_off");
        // Streams auf einem bestimmten Bildschirm sind Mehrschritt-Aufgaben fuers Modell.
        assert!(is_browser_task("Öffne auf meinem zweiten Bildschirm den Livestream auf Twitch vom offiziellen Valorant Channel"));
        assert!(is_browser_task("Mach auf Twitch den Valorant Stream an"));
        assert_eq!(call("Schließe Spotify").1["name"], "spotify");
        assert_eq!(call("Öffne den Download Ordner").1["name"], "downloads");
        assert_eq!(call("Öffne meine Bilder").1["name"], "bilder");
        none("Öffne die Datei");
        none("Öffne die letzte Notiz");
    }

    #[test]
    fn weather_time_notes_search() {
        assert_eq!(call("Wie ist das Wetter?").1["location"], "");
        assert_eq!(call("Wie ist das Wetter in Hamburg").1["location"], "Hamburg");
        none("Wie wird das Wetter morgen?");
        assert!(matches!(parse("Wie spät ist es?"), Some(Quick::Say(s)) if s.starts_with("Es ist ")));
        assert!(matches!(parse("Welcher Tag ist heute"), Some(Quick::Say(s)) if s.starts_with("Heute ist ")));
        assert_eq!(call("Merk dir, dass der Zahnarzt am Freitag um 9 ist und ich Brot mitbringe").1["text"], "Der Zahnarzt am Freitag um 9 ist und ich Brot mitbringe.");
        none("Merk dir das");
        assert_eq!(call("Spiel Lofi Musik auf YouTube").1["query"], "lofi musik");
        assert_eq!(call("Google Wetter Mallorca").1["site"], "google");
        assert_eq!(call("Such auf YouTube nach Katzenvideos").1["site"], "youtube");
        none("Spiel das erste Video vom Kanal Gronkh auf YouTube");
    }

    #[test]
    fn leaves_hard_things_to_the_model() {
        none("Öffne Netflix, klicke auf Philipp und spiele The Middle");
        none("Öffne Spotify und spiel meine Playlist");
        none("Erklär mir, wie ein Herunterfahren unter Windows funktioniert und was dabei passiert");
        none("Schreib mir eine E-Mail an meinen Chef");
        none("Was ist die Hauptstadt von Australien?");
        none("Hallo, wie geht es dir?");
        none("Lade die Datei herunter");
        none("Scroll runter");
    }

    #[test]
    fn sites() {
        assert_eq!(site("YouTube").map(|s| s.1), Some("https://www.youtube.com"));
        assert_eq!(site("Disney+").map(|s| s.0), Some("Disney+"));
        assert_eq!(site("netflix.com").map(|s| s.0), Some("Netflix"));
        assert_eq!(site("Spotify").map(|s| s.2), Some(true));
        assert!(site("Steam").is_none());
        assert!(looks_like_domain("heise.de"));
        assert!(!looks_like_domain("rechner"));
    }

    #[test]
    fn browser_tasks() {
        assert!(is_browser_task("Öffne Netflix, klicke auf Philipp und spiele The Middle"));
        assert!(is_browser_task("Spiel The Middle auf Netflix"));
        assert!(is_browser_task("Geh auf Disney Plus, wähl mein Profil und starte die nächste Folge"));
        assert!(!is_browser_task("Öffne Netflix"));
        assert!(!is_browser_task("Spiel Lofi auf YouTube"));
        assert!(!is_browser_task("Stell einen Timer auf 10 Minuten"));
    }
}
