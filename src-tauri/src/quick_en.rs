//! Englische Schnellbefehle („open YouTube“, „set a timer for 10 minutes“, „shut down in 20 minutes“).
//! Gleiches Prinzip wie `quick`: sicher erkannte Alltagssaetze laufen ohne Sprachmodell, alles andere
//! (`None`) geht ans Modell. Wird nach dem deutschen Erkenner gefragt, damit beide Sprachen gehen.

use chrono::{Local, Timelike};
use serde_json::{json, Value};

use crate::quick::Quick;

fn tool(name: &'static str, args: Value) -> Option<Quick> {
    Some(Quick::Tool { name, args, times: 1 })
}

/// Kleinbuchstaben, Apostrophe zusammengezogen („what's“ → „whats“), Satzzeichen weg, Fuellwoerter weg.
fn clean(text: &str) -> String {
    let lower = text.to_lowercase().replace(['\'', '’'], "");
    let mut out = String::with_capacity(lower.len());
    for ch in lower.chars() {
        out.push(if ch.is_alphanumeric() || ch == ':' || ch == '%' { ch } else { ' ' });
    }
    out.split_whitespace().filter(|w| !FILLERS.contains(w)).collect::<Vec<_>>().join(" ")
}

const FILLERS: &[&str] = &["please", "just", "quickly", "now", "hey", "milo", "mylo", "milo", "okay", "ok", "real"];

const LEADS: &[&str] = &["can you", "could you", "would you", "will you", "i want you to", "id like you to", "i would like you to", "i need you to", "go ahead and"];

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

fn has(w: &[&str], any: &[&str]) -> bool {
    w.iter().any(|x| any.contains(x))
}

fn phrase(t: &str, any: &[&str]) -> bool {
    let padded = format!(" {t} ");
    any.iter().any(|p| padded.contains(&format!(" {p} ")))
}

/// Zahl aus Ziffern oder einem englischen Zahlwort bis 100 („twenty five“ als zwei Woerter kommt ueber `duration`).
pub fn number(word: &str) -> Option<u32> {
    let word = word.trim_end_matches('%');
    if let Ok(n) = word.parse() {
        return Some(n);
    }
    Some(match word {
        "zero" => 0,
        "a" | "an" | "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        "hundred" => 100,
        _ => return None,
    })
}

fn unit_seconds(word: &str) -> Option<u64> {
    match word {
        "second" | "seconds" | "sec" | "secs" | "s" => Some(1),
        "minute" | "minutes" | "min" | "mins" | "m" => Some(60),
        "hour" | "hours" | "hr" | "hrs" | "h" => Some(3600),
        _ => None,
    }
}

/// Zusammengeschrieben: „10min“, „5minutes“, „2h“.
fn joined(word: &str) -> Option<(u32, u64)> {
    const UNITS: [&str; 12] = ["seconds", "second", "minutes", "minute", "hours", "hour", "secs", "mins", "hrs", "sec", "min", "h"];
    UNITS.iter().find_map(|unit| {
        let n = word.strip_suffix(unit)?;
        (!n.is_empty()).then_some(())?;
        Some((n.parse().ok()?, unit_seconds(unit)?))
    })
}

/// Dauer in Sekunden („10 minutes“, „half an hour“, „an hour and a half“, „twenty five minutes“) und die Woerter dazu.
fn duration(w: &[&str]) -> Option<(u64, Vec<usize>)> {
    let mut total = 0u64;
    let mut used = Vec::new();
    let mut i = 0;
    while i < w.len() {
        let word = w[i];
        if word == "half" && w.get(i + 1) == Some(&"an") && w.get(i + 2).is_some_and(|x| x.starts_with("hour")) {
            total += 1800;
            used.extend([i, i + 1, i + 2]);
            i += 3;
            continue;
        }
        if word == "quarter" && w.get(i + 1) == Some(&"of") && w.get(i + 2) == Some(&"an") && w.get(i + 3).is_some_and(|x| x.starts_with("hour")) {
            total += 900;
            used.extend([i, i + 1, i + 2, i + 3]);
            i += 4;
            continue;
        }
        // „and a half“ nach einer Stunde
        if word == "and" && w.get(i + 1) == Some(&"a") && w.get(i + 2) == Some(&"half") && total >= 3600 {
            total += 1800;
            used.extend([i, i + 1, i + 2]);
            i += 3;
            continue;
        }
        if let Some((n, unit)) = joined(word) {
            total += n as u64 * unit;
            used.push(i);
        } else if let Some(unit) = unit_seconds(word) {
            if i > 0 && !used.contains(&(i - 1)) {
                // „twenty five minutes“: zwei Zahlwoerter davor
                if let (Some(ones), Some(tens)) = (number(w[i - 1]), i.checked_sub(2).and_then(|j| number(w[j]))) {
                    if tens >= 20 && tens % 10 == 0 && ones < 10 && !used.contains(&(i - 2)) {
                        total += (tens + ones) as u64 * unit;
                        used.extend([i - 2, i - 1, i]);
                        i += 1;
                        continue;
                    }
                }
                if let Some(n) = number(w[i - 1]) {
                    total += n as u64 * unit;
                    used.extend([i - 1, i]);
                }
            }
        }
        i += 1;
    }
    (total > 0).then_some((total, used))
}

/// Uhrzeit nach „at“ („at 6 pm“, „at 18:30“, „at 7:15 am“, „at 7 oclock“) als HH:MM.
fn clock(w: &[&str]) -> Option<(String, Vec<usize>)> {
    let at = w.iter().position(|x| *x == "at")?;
    let first = *w.get(at + 1)?;
    let mut used = vec![at, at + 1];
    let (mut hour, minute) = match first.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (number(first)?, 0),
    };
    let next = w.get(at + 2).copied();
    match next {
        Some("pm" | "p") => {
            used.push(at + 2);
            if hour < 12 {
                hour += 12;
            }
        }
        Some("am" | "a") => {
            used.push(at + 2);
            if hour == 12 {
                hour = 0;
            }
        }
        Some("oclock") => used.push(at + 2),
        _ if first.contains(':') => {}
        _ => return None, // „at 5“ allein ist zu unsicher
    }
    (hour < 24 && minute < 60).then(|| (format!("{hour:02}:{minute:02}"), used))
}

fn minutes_until(hhmm: &str) -> f64 {
    let (h, m) = hhmm.split_once(':').map(|(h, m)| (h.parse::<i64>().unwrap_or(0), m.parse::<i64>().unwrap_or(0))).unwrap_or((0, 0));
    let now = Local::now();
    let mut diff = h * 60 + m - (now.hour() as i64 * 60 + now.minute() as i64);
    if diff <= 0 {
        diff += 24 * 60;
    }
    diff as f64
}

fn when_minutes(w: &[&str]) -> f64 {
    if let Some((seconds, _)) = duration(w) {
        return seconds as f64 / 60.0;
    }
    clock(w).map(|(hhmm, _)| minutes_until(&hhmm)).unwrap_or(0.0)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

const PC: &[&str] = &["pc", "computer", "laptop", "system", "windows", "machine"];
const SCREEN_WORDS: &[&str] = &["screen", "monitor", "display"];

/// Erkennt einen englischen Schnellbefehl oder liefert `None` (dann entscheidet das Modell).
pub fn parse(text: &str) -> Option<Quick> {
    let original = text.trim();
    if original.is_empty() || original.chars().count() > 160 {
        return None;
    }
    let cleaned = clean(original);
    let t = strip_leads(&cleaned);
    let w: Vec<&str> = t.split(' ').filter(|x| !x.is_empty()).collect();
    if w.is_empty() {
        return None;
    }
    if let Some(found) = note(original, t) {
        return Some(found);
    }
    let after_name = original.trim_start_matches(|c: char| !c.is_alphanumeric());
    let after_name = ["hey milo", "hey mylo", "hey milo", "milo", "mylo", "milo", "okay", "ok", "hey"]
        .iter()
        .find_map(|n| after_name.to_lowercase().starts_with(n).then(|| after_name[n.len()..].trim_start_matches(|c: char| !c.is_alphanumeric())))
        .unwrap_or(after_name);
    if after_name.contains(',') || after_name.contains(';') || phrase(t, &["and", "then", "afterwards", "also"]) && !phrase(t, &["and a half"]) {
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
        .or_else(|| timer(t, &w))
        .or_else(|| volume(t, &w))
        .or_else(|| screenshot_or_playing(t, &w))
        .or_else(|| media(t, &w))
        .or_else(|| folder(&w))
        .or_else(|| weather(t, &w))
        .or_else(|| time_or_date(t))
        .or_else(|| youtube(&w))
        .or_else(|| close(&w))
        .or_else(|| open(t, &w))
}

/// „open YouTube on the second screen“, „play lofi on youtube on the left monitor“
fn on_screen(w: &[&str]) -> Option<Quick> {
    let i = w.iter().position(|x| SCREEN_WORDS.iter().any(|s| x.starts_with(s)))?;
    let (spec, mut used) = if let Some(n) = w.get(i + 1).and_then(|x| number(x)).filter(|n| (1..=4).contains(n)) {
        (n.to_string(), vec![i, i + 1])
    } else if i > 0 && ["first", "second", "third", "fourth", "left", "right", "main", "primary", "other", "external", "1st", "2nd", "3rd"].contains(&w[i - 1]) {
        (w[i - 1].to_string(), vec![i - 1, i])
    } else {
        return None;
    };
    let mut j = used.iter().copied().min().unwrap_or(i);
    while j > 0 && ["on", "the", "my", "to"].contains(&w[j - 1]) {
        j -= 1;
        used.push(j);
    }
    let rest: Vec<&str> = w.iter().enumerate().filter(|(k, _)| !used.contains(k)).map(|(_, x)| *x).collect();
    if rest.is_empty() {
        return None;
    }
    let t = rest.join(" ");
    match open(&t, &rest).or_else(|| youtube(&rest)).or_else(|| search(&t))? {
        Quick::Open(target) => Some(Quick::OpenOn(target, spec)),
        Quick::Tool { name, mut args, times } if ["play_youtube", "browser_search"].contains(&name) => {
            args["screen"] = json!(spec);
            Some(Quick::Tool { name, args, times })
        }
        _ => None,
    }
}

fn note(original: &str, t: &str) -> Option<Quick> {
    const TRIGGERS: &[&str] = &["remember that", "remember", "make a note that", "make a note", "take a note that", "take a note", "note that", "note down", "write down"];
    let trigger = TRIGGERS.iter().find(|p| t.starts_with(*p))?;
    let lower = original.to_lowercase();
    let first_word = trigger.split(' ').next().unwrap_or(trigger);
    let start = lower.find(first_word)?;
    // Den Ausloeser im Original ueberspringen (Woerter zaehlen, Gross- und Kleinschreibung bleibt).
    let skip = trigger.split(' ').count();
    let mut content = original[start..].split_whitespace().skip(skip).collect::<Vec<_>>().join(" ");
    content = content.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
    let content = content.trim().trim_end_matches(['.', '!']).trim().to_string();
    if content.chars().count() < 4 || ["this", "that", "it"].contains(&content.to_lowercase().as_str()) || t.starts_with("remember when") || t.starts_with("remember what") {
        return None;
    }
    tool("save_note", json!({ "text": format!("{}.", capitalize(&content)) }))
}

fn power(t: &str, w: &[&str]) -> Option<Quick> {
    let mentions = phrase(t, &["shut down", "shutdown", "turn off", "power off", "switch off", "restart", "reboot", "sleep", "standby", "hibernate"]);
    if mentions && has(w, &["cancel", "abort", "stop", "dont"]) && !has(w, &["music", "song", "video"]) {
        return tool("system_power", json!({ "action": "abort" }));
    }
    if has(w, &["volume", "music", "song", "scroll", "page", "file", "download", "timer"]) {
        return None;
    }
    let minutes = when_minutes(w);
    let target_pc = has(w, PC) || w.len() <= 6;
    if (phrase(t, &["shut down", "shutdown", "power off"]) || (phrase(t, &["turn off", "switch off"]) && has(w, PC))) && target_pc {
        return tool("system_power", json!({ "action": "shutdown", "in_minutes": minutes }));
    }
    if has(w, &["restart", "reboot"]) && target_pc {
        return tool("system_power", json!({ "action": "restart", "in_minutes": minutes }));
    }
    if (phrase(t, &["go to sleep", "put the pc to sleep", "put the computer to sleep", "sleep mode", "standby"]) || t == "sleep") && !has(w, &["screen", "monitor"]) {
        return tool("system_power", json!({ "action": "sleep", "in_minutes": minutes }));
    }
    if (w[0] == "lock" && (w.len() == 1 || has(w, PC) || has(w, &["screen"]))) || t == "lock" {
        return tool("system_power", json!({ "action": "lock" }));
    }
    if has(w, &["screen", "monitor", "display", "screens", "monitors"]) && phrase(t, &["turn off", "switch off"]) {
        return tool("system_power", json!({ "action": "screen_off" }));
    }
    None
}

fn timer(t: &str, w: &[&str]) -> Option<Quick> {
    let timer_word = |x: &&str| ["timer", "alarm", "countdown", "reminder"].contains(x);
    let is_timer = w.iter().any(timer_word);
    let remind = t.starts_with("remind me");
    let wake = t.starts_with("wake me");
    if is_timer && has(w, &["cancel", "stop", "delete", "remove", "end", "clear", "off", "dismiss"]) {
        let stop = ["cancel", "stop", "delete", "remove", "end", "clear", "off", "dismiss", "the", "my", "a", "timer", "alarm", "countdown", "reminder", "for"];
        let label: Vec<&str> = w.iter().copied().filter(|x| !stop.contains(x)).collect();
        return tool("cancel_timer", json!({ "label": label.join(" ") }));
    }
    if !(is_timer || remind || wake) {
        return None;
    }
    let (mut args, used) = if let Some((seconds, used)) = duration(w) {
        (json!({ "duration_seconds": seconds }), used)
    } else if let Some((hhmm, used)) = clock(w) {
        (json!({ "at_time": hhmm }), used)
    } else {
        return None;
    };
    // Bezeichnung: bei Erinnerungen nach „to“/„about“, sonst nach dem letzten „for“, das nicht zur Dauer gehoert.
    let markers: &[&str] = if remind { &["to", "about", "that"] } else { &["for"] };
    let skip = ["the", "a", "an", "me", "in", "at", "set", "my", "of", "up", "timer"];
    let label = w
        .iter()
        .enumerate()
        .filter(|(i, x)| markers.contains(x) && !used.contains(&(i + 1)))
        .map(|(i, _)| i)
        .next_back()
        .map(|start| {
            w.iter()
                .enumerate()
                .skip(start + 1)
                .filter(|(i, x)| !used.contains(i) && !skip.contains(*x) && !timer_word(*x))
                .map(|(_, x)| *x)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let label = if label.is_empty() && wake { "Alarm".to_string() } else { capitalize(&label) };
    if !label.is_empty() {
        args["label"] = json!(label);
    }
    tool("set_timer", args)
}

fn volume(t: &str, w: &[&str]) -> Option<Quick> {
    if has(w, &["volume", "sound"]) {
        let value = w.iter().enumerate().find_map(|(i, x)| {
            let n = number(x)?;
            let marked = x.ends_with('%') || w.get(i + 1).is_some_and(|n| *n == "percent" || *n == "%") || (i > 0 && ["to", "at", "volume"].contains(&w[i - 1]));
            (marked && n <= 100 && !["a", "an", "one"].contains(x)).then_some(n)
        });
        if let Some(percent) = value {
            return tool("set_volume", json!({ "percent": percent }));
        }
    }
    if has(w, &["mute", "unmute", "silence"]) {
        return tool("media_control", json!({ "action": "mute" }));
    }
    let steps = if has(w, &["lot", "much", "way"]) { 10 } else if has(w, &["bit", "little", "slightly", "tad"]) { 2 } else { 5 };
    let up = has(w, &["louder"]) || phrase(t, &["volume up", "turn it up", "turn up the volume", "turn the volume up", "raise the volume"]);
    let down = has(w, &["quieter", "softer"]) || phrase(t, &["volume down", "turn it down", "turn down the volume", "turn the volume down", "lower the volume"]);
    if up {
        return Some(Quick::Tool { name: "media_control", args: json!({ "action": "volume_up" }), times: steps });
    }
    if down {
        return Some(Quick::Tool { name: "media_control", args: json!({ "action": "volume_down" }), times: steps });
    }
    None
}

fn screenshot_or_playing(t: &str, w: &[&str]) -> Option<Quick> {
    let shot = w.iter().any(|x| x.starts_with("screenshot") || *x == "screengrab") || phrase(t, &["screen shot", "screen capture"]);
    let looking = has(w, &["what", "whats", "describe", "explain", "read", "see"]);
    if shot && !looking {
        let screen = w
            .iter()
            .position(|x| SCREEN_WORDS.iter().any(|s| x.starts_with(s)) && !x.starts_with("screenshot"))
            .and_then(|i| if i > 0 { Some(w[i - 1].to_string()) } else { None })
            .unwrap_or_default();
        return tool("save_screenshot", json!({ "screen": screen }));
    }
    if phrase(t, &["whats playing", "what is playing", "what song is this", "what song is playing", "whats this song", "what am i listening to", "which song is this"]) {
        return tool("now_playing", json!({}));
    }
    None
}

const MEDIA_THINGS: &[&str] = &["song", "track", "video", "episode", "title"];

fn media(t: &str, w: &[&str]) -> Option<Quick> {
    if (w[0] == "skip" && !has(w, &["forward", "ahead", "back"])) || (has(w, &["next"]) && (has(w, MEDIA_THINGS) || w.len() == 1)) {
        return tool("media_control", json!({ "action": "next" }));
    }
    if (has(w, &["previous", "last"]) && (has(w, MEDIA_THINGS) || w.len() == 1)) || phrase(t, &["go back a song", "back a track"]) {
        return tool("media_control", json!({ "action": "previous" }));
    }
    let seek = has(w, &["rewind", "fast"]) || (duration(w).is_some() && has(w, &["forward", "ahead", "back", "backward", "backwards"]) && w.len() <= 6);
    if seek && !has(w, &["timer"]) {
        let back = has(w, &["rewind", "back", "backward", "backwards"]);
        let seconds = duration(w).map(|(s, _)| s).unwrap_or(10).clamp(1, 3600);
        return tool("media_control", json!({ "action": if back { "rewind" } else { "forward" }, "seconds": seconds }));
    }
    if w.len() <= 4 {
        let ok = w.iter().all(|x| ["the", "music", "video", "song", "playback", "show", "episode", "it", "stop", "pause", "play", "resume", "continue", "unpause"].contains(x));
        if ok {
            if has(w, &["pause", "stop"]) {
                return tool("media_control", json!({ "action": "pause" }));
            }
            if has(w, &["play", "resume", "continue", "unpause"]) {
                return tool("media_control", json!({ "action": "play" }));
            }
        }
    }
    None
}

fn folder(w: &[&str]) -> Option<Quick> {
    let verb = has(w, &["open", "show", "go"]);
    let explicit = has(w, &["folder", "my"]);
    let name = w.iter().find_map(|x| match *x {
        x if x.starts_with("download") => Some("downloads"),
        x if x.starts_with("document") => Some("dokumente"),
        "desktop" => Some("desktop"),
        "pictures" | "photos" | "images" => Some("bilder"),
        "music" => Some("musik"),
        "videos" => Some("videos"),
        _ => None,
    })?;
    let clear = matches!(name, "downloads" | "dokumente" | "desktop");
    (verb && (explicit || clear) && w.len() <= 6).then(|| tool("open_folder", json!({ "name": name }))).flatten()
}

fn weather(t: &str, w: &[&str]) -> Option<Quick> {
    if !has(w, &["weather", "temperature"]) && !phrase(t, &["is it raining", "will it rain"]) {
        return None;
    }
    if has(w, &["tomorrow", "weekend", "week", "next", "yesterday", "evening", "afternoon", "later", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"]) {
        return None;
    }
    let skip = ["today", "outside", "right", "like", "the", "currently", "at", "moment", "there"];
    let place = w
        .iter()
        .rposition(|x| *x == "in")
        .map(|i| w[i + 1..].iter().copied().filter(|x| !skip.contains(x)).map(capitalize).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    tool("get_weather", json!({ "location": place }))
}

fn time_or_date(t: &str) -> Option<Quick> {
    let now = Local::now();
    let short = t.split(' ').count() <= 7;
    if short && (t.contains("what time is it") || t.contains("whats the time") || t.contains("what is the time") || t == "time" || t.contains("tell me the time")) {
        return Some(Quick::Say(format!("It's {}.", now.format("%-I:%M %p"))));
    }
    if short && (t.contains("what day is it") || t.contains("whats the date") || t.contains("what is the date") || t.contains("todays date") || t.contains("what day is today")) {
        return Some(Quick::Say(format!("Today is {}.", now.format("%A, %B %-d, %Y"))));
    }
    None
}

fn search(t: &str) -> Option<Quick> {
    for (prefix, site) in [("search youtube for ", "youtube"), ("search on youtube for ", "youtube"), ("search google for ", "google"), ("search on google for ", "google"), ("google ", "google")] {
        if let Some(query) = t.strip_prefix(prefix) {
            return (!query.is_empty()).then(|| tool("browser_search", json!({ "query": query, "site": site }))).flatten();
        }
    }
    None
}

/// „play lofi on youtube“, „play the trailer of X on youtube“
fn youtube(w: &[&str]) -> Option<Quick> {
    if !has(w, &["youtube"]) || has(w, &["channel"]) || !has(w, &["play", "show", "watch"]) {
        return None;
    }
    let drop = ["play", "show", "watch", "me", "on", "youtube", "a", "the", "video", "of", "from"];
    let query: Vec<&str> = w.iter().copied().filter(|x| !drop.contains(x)).collect();
    (!query.is_empty()).then(|| tool("play_youtube", json!({ "query": query.join(" ") }))).flatten()
}

const ARTICLES: &[&str] = &["the", "my", "a", "an", "app", "application", "program", "website", "site", "page"];

fn close(w: &[&str]) -> Option<Quick> {
    if !["close", "quit", "exit"].contains(&w[0]) {
        return None;
    }
    let target: Vec<&str> = w[1..].iter().copied().filter(|x| !ARTICLES.contains(x)).collect();
    if target.is_empty() || target.len() > 3 || has(&target, &["timer", "window", "all", "everything", "tab", "tabs"]) {
        return None;
    }
    tool("close_app", json!({ "name": target.join(" ") }))
}

fn open(t: &str, w: &[&str]) -> Option<Quick> {
    let go = t.starts_with("go to ");
    if !(["open", "launch", "start", "run"].contains(&w[0]) || go) {
        return None;
    }
    let start = if go { 2 } else { 1 };
    if start >= w.len() {
        return None;
    }
    let target: Vec<&str> = w[start..].iter().copied().filter(|x| !ARTICLES.contains(x)).collect();
    let unsure = ["file", "document", "note", "notes", "timer", "folder", "picture", "photo", "pdf", "new", "last", "recent", "mail", "email", "again", "in", "with", "of", "from", "on", "for", "game", "music", "playing"];
    if target.is_empty() || target.len() > 4 || has(&target, &unsure) {
        return None;
    }
    Some(Quick::Open(target.join(" ")))
}

fn workspace(t: &str) -> Option<Quick> {
    let save = ["save workspace as ", "save the workspace as ", "save workspace ", "save the workspace "].iter().find_map(|p| t.strip_prefix(p)).map(|r| ("save", r));
    let (action, name) = match save {
        Some(found) => found,
        None => {
            let r = t.strip_prefix("open workspace ").or_else(|| t.strip_prefix("workspace "))?;
            ("open", r.strip_suffix(" open").unwrap_or(r))
        }
    };
    let name = name.trim();
    if name.is_empty() || name.split_whitespace().count() > 4 {
        return None;
    }
    tool("workspace", json!({ "action": action, "name": capitalize(name) }))
}

/// Mehrere Schritte im Browser („open Netflix, click Alex and play The Office“) erledigt das Modell mit Klick-Werkzeugen.
pub fn is_browser_task(text: &str) -> bool {
    let t = clean(text);
    let w: Vec<&str> = t.split(' ').filter(|x| !x.is_empty()).collect();
    let streaming = has(&w, &["netflix", "disney", "prime", "crunchyroll", "twitch", "hulu", "max", "paramount", "peacock"]);
    let page = streaming || has(&w, &["browser", "website", "youtube", "page", "site"]);
    let click = w.iter().any(|x| ["click", "press", "select", "choose", "tap", "type", "search"].iter().any(|p| x.starts_with(p)))
        || has(&w, &["profile", "season", "episode", "continue", "searchbar"]);
    let play = w.iter().any(|x| ["play", "watch", "start"].iter().any(|p| x.starts_with(p)));
    let steps = text.contains(',') || phrase(&t, &["and", "then", "afterwards"]);
    let live = w.iter().any(|x| x.starts_with("live") || x.starts_with("stream"));
    let screen = w.iter().any(|x| SCREEN_WORDS.iter().any(|s| x.starts_with(s)));
    page && ((click && (steps || streaming)) || (streaming && (play || live || screen)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(text: &str) -> (String, Value) {
        match parse(text) {
            Some(Quick::Tool { name, args, .. }) => (name.to_string(), args),
            other => panic!("\"{text}\" gave {other:?}"),
        }
    }

    #[test]
    fn timers() {
        let (name, args) = call("Set a timer for 10 minutes");
        assert_eq!(name, "set_timer");
        assert_eq!(args["duration_seconds"], 600);
        let (_, args) = call("set a timer for 25 minutes for pasta");
        assert_eq!(args["duration_seconds"], 1500);
        assert_eq!(args["label"], "Pasta");
        let (_, args) = call("timer twenty five minutes");
        assert_eq!(args["duration_seconds"], 1500);
        let (_, args) = call("Remind me in half an hour to call mom");
        assert_eq!(args["duration_seconds"], 1800);
        assert_eq!(args["label"], "Call mom");
        let (_, args) = call("wake me up at 7 am");
        assert_eq!(args["at_time"], "07:00");
        let (_, args) = call("set a timer for an hour and a half");
        assert_eq!(args["duration_seconds"], 5400);
        let (name, args) = call("cancel the pasta timer");
        assert_eq!(name, "cancel_timer");
        assert_eq!(args["label"], "pasta");
    }

    #[test]
    fn power_and_volume() {
        let (_, args) = call("shut down the pc in 20 minutes");
        assert_eq!(args["action"], "shutdown");
        assert_eq!(args["in_minutes"], 20.0);
        assert_eq!(call("restart the computer").1["action"], "restart");
        assert_eq!(call("cancel the shutdown").1["action"], "abort");
        assert_eq!(call("lock the pc").1["action"], "lock");
        assert_eq!(call("set the volume to 30").1["percent"], 30);
        assert_eq!(call("volume 40%").1["percent"], 40);
        assert_eq!(call("turn it up a bit").1["action"], "volume_up");
        assert_eq!(call("mute").1["action"], "mute");
    }

    #[test]
    fn media_open_and_more() {
        assert_eq!(call("next song").1["action"], "next");
        assert_eq!(call("pause the music").1["action"], "pause");
        assert_eq!(call("skip forward 30 seconds").1["seconds"], 30);
        assert_eq!(call("Play lofi beats on YouTube").1["query"], "lofi beats");
        assert_eq!(call("What's the weather in New York?").1["location"], "New York");
        assert_eq!(call("Remember that the wifi password is on the router").1["text"], "The wifi password is on the router.");
        assert_eq!(call("open my downloads").1["name"], "downloads");
        assert!(matches!(parse("Open Spotify"), Some(Quick::Open(t)) if t == "spotify"));
        assert!(matches!(parse("open YouTube on the second screen"), Some(Quick::OpenOn(t, s)) if t == "youtube" && s == "second"));
        assert!(matches!(parse("what time is it"), Some(Quick::Say(_))));
        assert_eq!(parse("open spotify and play some jazz"), None);
        assert_eq!(parse("what's the weather tomorrow"), None);
        assert_eq!(parse("tell me a joke"), None);
    }
}
