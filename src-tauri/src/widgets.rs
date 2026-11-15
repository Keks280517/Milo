//! Eigene Widgets: kleine Anzeigen (RAM, CPU, Uhr, Wetter, Claude ...) in eigenen, durchsichtigen
//! Fenstern am oberen Bildschirmrand, neben der Insel. Sie lassen sich nur waagerecht entlang der
//! Oberkante verschieben, sperren (dann verrutscht nichts beim Anklicken) und verschwinden wie die
//! Insel bei Vollbild und im Spielemodus. Danach tauchen sie an derselben Stelle wieder auf.
//!
//! Ein Widget ist nur eine Beschreibung aus festen Bausteinen (keine freien Skripte). Die KI-Hilfe
//! uebersetzt eine Beschreibung des Nutzers ebenfalls nur in diese Bausteine.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const PREFIX: &str = "widget-";
pub const EVENT: &str = "milo://widgets";
const MAX_WIDGETS: usize = 12;
const MAX_BLOCKS: usize = 4;
/// Abstand zur Insel und zwischen Widgets (logische Pixel).
const GAP: f64 = 10.0;
/// So breit wird die Insel aufgeklappt; Widgets bleiben ausserhalb.
/// Breite der offenen Insel je Groessenstufe (wie `PANEL_WIDTHS` in Island.svelte).
const ISLAND_WIDTHS: [f64; 3] = [760.0, 880.0, 1000.0];
/// Platz um die sichtbare Flaeche fuer Hohlkehlen und Schatten (logische Pixel).
const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 14.0;

pub const SHAPES: &[&str] = &["round", "pill", "card"];
pub const SIZES: &[&str] = &["s", "m", "l"];
pub const STYLES: &[&str] = &["hanging", "floating"];
pub const BLOCKS: &[&str] = &[
    "ram", "ramfree", "cpu", "cpugraph", "disk", "diskio", "net", "netgraph", "uptime", "topapp", "battery",
    "clock", "seconds", "analog", "binary", "date", "week", "dayofyear", "worldclock",
    "countdown", "holiday", "weekend", "workday", "agenda",
    "dayprogress", "weekprogress", "monthprogress", "yearprogress", "moon",
    "weather", "feels", "wind", "humidity", "rain", "forecast", "sun", "daylight",
    "timer", "media", "note", "quote", "quota", "claude", "claudelimit", "text",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Block {
    pub kind: String,
    /// Angabe fuer einige Bausteine: `text` (was dasteht), `disk` (Laufwerksbuchstabe),
    /// `worldclock` (Zeitzone wie Asia/Tokyo), `countdown` ("Name|JJJJ-MM-TT"), `diskio` (Laufwerk, leer = alle).
    pub text: String,
    /// Eigene kurze Beschriftung statt der eingebauten (z. B. "Arbeit" statt "RAM").
    pub label: String,
}

impl Default for Block {
    fn default() -> Self {
        Self { kind: "clock".into(), text: String::new(), label: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Widget {
    pub id: String,
    pub name: String,
    pub shape: String,
    pub size: String,
    pub style: String,
    pub accent: String,
    pub blocks: Vec<Block>,
    /// Linke Kante in physischen Pixeln (Bildschirmkoordinaten); `None` = neben der Insel.
    pub x: Option<i32>,
    pub locked: bool,
}

impl Default for Widget {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Widget".into(),
            shape: "pill".into(),
            size: "m".into(),
            style: "hanging".into(),
            accent: "#64d2ff".into(),
            blocks: vec![Block { kind: "ram".into(), text: String::new(), label: String::new() }],
            x: None,
            locked: false,
        }
    }
}

/// Platz fuer Bausteine: rund 2 (zwei Werte werden zu Ringen ineinander), Pille 2, Karte 4.
pub fn room(shape: &str) -> usize {
    match shape {
        "card" => MAX_BLOCKS,
        _ => 2,
    }
}

/// Entwurf der KI: passt der Wunsch nicht in die Form, wird es eine Karte, statt Bausteine
/// wegzulassen; runde Widgets mit zwei Werten werden mindestens mittelgross.
fn fit(mut widget: Widget) -> Widget {
    widget.blocks.retain(|b| BLOCKS.contains(&b.kind.as_str()));
    if widget.blocks.len() > room(&widget.shape) {
        widget.shape = "card".into();
        if widget.blocks.len() > 3 && widget.size == "s" {
            widget.size = "m".into();
        }
    }
    if widget.shape == "round" && widget.blocks.len() == 2 && widget.size == "s" {
        widget.size = "m".into();
    }
    widget
}

/// Sichtbare Groesse in logischen Pixeln.
pub fn face_size(shape: &str, size: &str) -> (f64, f64) {
    let step = match size {
        "s" => 0,
        "l" => 2,
        _ => 1,
    };
    match shape {
        "round" => [(72.0, 72.0), (96.0, 96.0), (128.0, 128.0)][step],
        "card" => [(150.0, 88.0), (200.0, 118.0), (260.0, 156.0)][step],
        _ => [(150.0, 40.0), (210.0, 48.0), (290.0, 58.0)][step],
    }
}

/// Fenstergroesse in logischen Pixeln (sichtbare Flaeche plus Rand).
fn window_size(widget: &Widget) -> (f64, f64) {
    let (w, h) = face_size(&widget.shape, &widget.size);
    (w + 2.0 * PAD_X, h + PAD_Y)
}

/// Bringt eine Beschreibung in gueltige Form (auch die von der KI).
pub fn sanitize(mut widget: Widget) -> Widget {
    let pick = |value: &str, allowed: &[&str], fallback: &str| if allowed.contains(&value) { value.to_string() } else { fallback.to_string() };
    widget.shape = pick(&widget.shape, SHAPES, "pill");
    widget.size = pick(&widget.size, SIZES, "m");
    widget.style = pick(&widget.style, STYLES, "hanging");
    widget.name = widget.name.trim().chars().take(40).collect();
    if widget.name.is_empty() {
        widget.name = "Widget".into();
    }
    let hex = widget.accent.trim();
    widget.accent = if hex.len() == 7 && hex.starts_with('#') && hex[1..].chars().all(|c| c.is_ascii_hexdigit()) { hex.to_lowercase() } else { "#64d2ff".into() };
    widget.blocks.retain(|b| BLOCKS.contains(&b.kind.as_str()));
    for block in &mut widget.blocks {
        block.text = block.text.trim().chars().take(60).collect();
        block.label = block.label.trim().chars().take(16).collect();
    }
    widget.blocks.truncate(room(&widget.shape));
    if widget.blocks.is_empty() {
        widget.blocks.push(Block::default());
    }
    widget.id.retain(|c| c.is_ascii_alphanumeric());
    widget
}

// ---------- Speicher ----------

fn file(app: &AppHandle) -> PathBuf {
    app.path().app_config_dir().unwrap_or_default().join("widgets.json")
}

static LOCK: Mutex<()> = Mutex::new(());

pub fn load(app: &AppHandle) -> Vec<Widget> {
    let list: Vec<Widget> = std::fs::read_to_string(file(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    list.into_iter().map(sanitize).filter(|w| !w.id.is_empty()).collect()
}

fn store(app: &AppHandle, list: &[Widget]) -> Result<()> {
    let path = file(app);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(list)?)?;
    Ok(())
}

fn update(app: &AppHandle, id: &str, change: impl FnOnce(&mut Widget)) -> Result<Widget> {
    let _guard = LOCK.lock().unwrap();
    let mut list = load(app);
    let Some(widget) = list.iter_mut().find(|w| w.id == id) else { bail!(tr!("Dieses Widget gibt es nicht mehr.", "That widget doesn't exist anymore.")) };
    change(widget);
    let changed = widget.clone();
    store(app, &list)?;
    Ok(changed)
}

// ---------- Fenster ----------

fn label(id: &str) -> String {
    format!("{PREFIX}{id}")
}

/// Bildschirm, an dessen Oberkante die Insel haengt (Hauptbildschirm).
fn island_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    app.primary_monitor().ok().flatten().or_else(|| app.available_monitors().ok()?.into_iter().next())
}

/// Erlaubte linke Kante fuer ein Fenster der Breite `width` (physisch), gewuenscht `x`:
/// auf dem Bildschirm, in dessen Breite die Mitte faellt, und nie unter der Insel.
pub fn clamp_x(x: i32, width: i32, monitors: &[(i32, i32, f64)], island: Option<(i32, i32)>) -> (i32, usize) {
    let center = x + width / 2;
    let index = monitors.iter().position(|(left, w, _)| center >= *left && center < left + *w as i32).unwrap_or_else(|| {
        // Ausserhalb aller Bildschirme: den naechstgelegenen nehmen.
        monitors.iter().enumerate().min_by_key(|(_, (left, w, _))| (center - (left + w / 2)).abs()).map(|(i, _)| i).unwrap_or(0)
    });
    let Some(&(left, mon_width, _)) = monitors.get(index) else { return (x, 0) };
    let mut x = x.clamp(left, (left + mon_width - width).max(left));
    if let Some((from, to)) = island {
        // Liegt das Widget ganz oder teilweise unter der Insel, zur naeheren Seite schieben.
        if x < to && x + width > from {
            let to_left = from - width;
            x = if (x + width / 2) < (from + to) / 2 && to_left >= left { to_left } else if to + width <= left + mon_width { to } else { to_left.max(left) };
        }
    }
    (x, index)
}

/// Monitore als (linke Kante, Breite, Skalierung) und der Bereich der Insel (physisch).
fn geometry(app: &AppHandle) -> (Vec<(i32, i32, f64, i32)>, Option<(i32, i32)>) {
    let monitors: Vec<(i32, i32, f64, i32)> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| (m.position().x, m.size().width as i32, m.scale_factor(), m.position().y))
        .collect();
    let size = app.state::<crate::settings::SettingsStore>().get().island_size.min(2) as usize;
    let island = island_monitor(app).map(|m| {
        let half = ((ISLAND_WIDTHS[size] / 2.0 + GAP) * m.scale_factor()).round() as i32;
        let center = m.position().x + m.size().width as i32 / 2;
        (center - half, center + half)
    });
    (monitors, island)
}

/// Legt das Fenster an die gespeicherte Stelle der Oberkante (oder neben die Insel).
fn place(app: &AppHandle, window: &WebviewWindow, widget: &Widget) -> Result<i32> {
    let (monitors, island) = geometry(app);
    if monitors.is_empty() {
        bail!(tr!("Kein Bildschirm gefunden.", "No screen found."));
    }
    let flat: Vec<(i32, i32, f64)> = monitors.iter().map(|(l, w, s, _)| (*l, *w, *s)).collect();
    // Skalierung des Zielbildschirms: zuerst grob bestimmen, dann mit der echten Breite klemmen.
    let guess = widget.x.unwrap_or_else(|| island.map(|(_, to)| to).unwrap_or(0));
    let (_, index) = clamp_x(guess, 1, &flat, island);
    let scale = flat[index].2;
    let (w, h) = window_size(widget);
    let (width, height) = ((w * scale).round() as i32, (h * scale).round() as i32);
    let (x, index) = clamp_x(guess, width, &flat, island);
    window.set_size(PhysicalSize::new(width as u32, height as u32))?;
    window.set_position(PhysicalPosition::new(x, monitors[index].3))?;
    Ok(x)
}

static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Oeffnet (oder aktualisiert) das Fenster eines Widgets.
fn show(app: &AppHandle, widget: &Widget) -> Result<()> {
    let window = match app.get_webview_window(&label(&widget.id)) {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, label(&widget.id), WebviewUrl::App("widget".into()))
            .title(format!("Milo-Widget {}", widget.name))
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .focused(false)
            .visible(false)
            .drag_and_drop(false)
            .build()?,
    };
    place(app, &window, widget)?;
    if !HIDDEN.load(Ordering::Relaxed) {
        window.show()?;
    }
    let _ = app.emit_to(label(&widget.id), "milo://widget", widget);
    Ok(())
}

/// Alle Widgets oeffnen (Start, Ende des Spielemodus).
pub fn open_all(app: &AppHandle) {
    for widget in load(app) {
        if let Err(err) = show(app, &widget) {
            log::warn!("Widget {} nicht geöffnet: {err}", widget.name);
        }
    }
}

/// Alle Widget-Fenster schliessen (Spielemodus): kostet dann weder Speicher noch Rechenzeit.
pub fn close_all(app: &AppHandle) {
    for (name, window) in app.webview_windows() {
        if name.starts_with(PREFIX) {
            let _ = window.destroy();
        }
    }
}

/// Vollbild auf dem Bildschirm der Insel: Widgets verstecken wie die Insel, danach wieder zeigen.
pub fn set_hidden(app: &AppHandle, hidden: bool) {
    if HIDDEN.swap(hidden, Ordering::Relaxed) == hidden {
        return;
    }
    for (name, window) in app.webview_windows() {
        if name.starts_with(PREFIX) {
            let _ = if hidden { window.hide() } else { window.show() };
        }
    }
}

// ---------- Befehle ----------

#[tauri::command]
pub fn widgets_list(app: AppHandle) -> Vec<Widget> {
    load(&app)
}

#[tauri::command]
pub fn widget_get(app: AppHandle, id: String) -> Option<Widget> {
    load(&app).into_iter().find(|w| w.id == id)
}

/// Legt ein Widget an oder aendert es. Neue Widgets kommen neben die Insel.
#[tauri::command]
pub async fn widget_save(app: AppHandle, widget: Widget) -> Result<Widget, String> {
    let mut widget = sanitize(widget);
    let saved = {
        let _guard = LOCK.lock().unwrap();
        let mut list = load(&app);
        match list.iter_mut().find(|w| !widget.id.is_empty() && w.id == widget.id) {
            Some(existing) => {
                // Lage und Sperre gehoeren dem Fenster, nicht dem Editor.
                widget.x = existing.x;
                widget.locked = existing.locked;
                *existing = widget.clone();
            }
            None => {
                if list.len() >= MAX_WIDGETS {
                    return Err(tr!(format!("Mehr als {MAX_WIDGETS} Widgets gehen nicht."), format!("More than {MAX_WIDGETS} widgets are not possible.")));
                }
                widget.id = format!("w{}", chrono::Local::now().timestamp_millis());
                // Rechts neben die Insel, hinter das letzte Widget, das schon dort haengt.
                let (_, island) = geometry(&app);
                let gap = island_monitor(&app).map(|m| (GAP * m.scale_factor()).round() as i32).unwrap_or(10);
                widget.x = island.map(|(_, to)| {
                    app.webview_windows()
                        .iter()
                        .filter(|(name, _)| name.starts_with(PREFIX))
                        .filter_map(|(_, w)| Some(w.outer_position().ok()?.x + w.outer_size().ok()?.width as i32))
                        .filter(|right| *right > to)
                        .max()
                        .map(|right| right + gap)
                        .unwrap_or(to)
                });
                list.push(widget.clone());
            }
        }
        store(&app, &list).map_err(|e| e.to_string())?;
        widget
    };
    show(&app, &saved).map_err(|e| e.to_string())?;
    // Die tatsaechliche Stelle festhalten (sie kann am Bildschirmrand angepasst worden sein).
    if let Some(window) = app.get_webview_window(&label(&saved.id)) {
        if let Ok(position) = window.outer_position() {
            let _ = update(&app, &saved.id, |w| w.x = Some(position.x));
        }
    }
    let _ = app.emit(EVENT, load(&app));
    Ok(saved)
}

#[tauri::command]
pub fn widget_delete(app: AppHandle, id: String) -> Result<(), String> {
    {
        let _guard = LOCK.lock().unwrap();
        let mut list = load(&app);
        list.retain(|w| w.id != id);
        store(&app, &list).map_err(|e| e.to_string())?;
    }
    if let Some(window) = app.get_webview_window(&label(&id)) {
        let _ = window.destroy();
    }
    let _ = app.emit(EVENT, load(&app));
    Ok(())
}

/// Linke Kante beim Beginn des Ziehens, je Widget.
static DRAG_START: Mutex<Option<(String, i32)>> = Mutex::new(None);

/// Waehrend des Ziehens: `offset` physische Pixel waagerecht seit Beginn des Ziehens (nie nach oben
/// oder unten). Unter die Insel kommt das Widget nie: bis zur Mitte der Insel bleibt es an ihrem
/// Rand, danach springt es auf die andere Seite. So laesst es sich auch links neben die Insel legen.
#[tauri::command]
pub fn widget_drag(app: AppHandle, id: String, offset: i32, start: bool) -> Result<(), String> {
    let window = app.get_webview_window(&label(&id)).ok_or(tr!("Widget nicht offen.", "Widget not open."))?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let origin = {
        let mut slot = DRAG_START.lock().unwrap();
        if start || slot.as_ref().is_none_or(|(who, _)| *who != id) {
            *slot = Some((id.clone(), position.x));
        }
        slot.as_ref().map(|(_, x)| *x).unwrap_or(position.x)
    };
    let width = window.outer_size().map_err(|e| e.to_string())?.width as i32;
    let (monitors, island) = geometry(&app);
    let flat: Vec<(i32, i32, f64)> = monitors.iter().map(|(l, w, s, _)| (*l, *w, *s)).collect();
    let (x, index) = clamp_x(origin + offset, width, &flat, island);
    window.set_position(PhysicalPosition::new(x, monitors.get(index).map(|m| m.3).unwrap_or(position.y))).map_err(|e| e.to_string())
}

/// Ziehen beendet: Stelle merken.
#[tauri::command]
pub fn widget_drop(app: AppHandle, id: String) -> Result<(), String> {
    *DRAG_START.lock().unwrap() = None;
    let window = app.get_webview_window(&label(&id)).ok_or(tr!("Widget nicht offen.", "Widget not open."))?;
    let x = window.outer_position().map_err(|e| e.to_string())?.x;
    update(&app, &id, |w| w.x = Some(x)).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn widget_lock(app: AppHandle, id: String, locked: bool) -> Result<Widget, String> {
    let widget = update(&app, &id, |w| w.locked = locked).map_err(|e| e.to_string())?;
    let _ = app.emit_to(label(&id), "milo://widget", &widget);
    let _ = app.emit(EVENT, load(&app));
    Ok(widget)
}

/// Rechtsklick auf ein Widget, "Bearbeiten": die Insel oeffnet den Editor.
#[tauri::command]
pub fn widget_edit(app: AppHandle, id: String) {
    let _ = app.emit_to(crate::overlay::OVERLAY_LABEL, "milo://widget-edit", id);
}

#[derive(Debug, Clone, Serialize)]
pub struct Battery {
    pub present: bool,
    pub percent: u8,
    pub charging: bool,
}

#[tauri::command]
pub fn battery_status() -> Battery {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: liest nur den Akkustand in eine lokale Struktur.
        if unsafe { GetSystemPowerStatus(&mut status) }.is_ok() {
            // 128 = kein Akku, 255 = unbekannt.
            let present = status.BatteryFlag & 128 == 0 && status.BatteryFlag != 255 && status.BatteryLifePercent <= 100;
            return Battery { present, percent: status.BatteryLifePercent.min(100), charging: status.ACLineStatus == 1 };
        }
    }
    Battery { present: false, percent: 0, charging: false }
}

/// KI-Hilfe: aus einer Beschreibung drei unterschiedliche Widgets aus den vorhandenen Bausteinen
/// machen. Gespeichert wird nichts; der Nutzer waehlt einen Entwurf und prueft ihn im Baukasten.
#[tauri::command]
pub async fn widget_design(app: AppHandle, description: String) -> Result<Vec<Widget>, String> {
    let description: String = description.trim().chars().take(400).collect();
    if description.is_empty() {
        return Err(tr!("Beschreib kurz, was das Widget zeigen soll.", "Briefly describe what the widget should show.").into());
    }
    let system = format!(
        "Du baust Desktop-Widgets aus festen Bausteinen. Antworte NUR mit einer JSON-Liste aus genau drei Objekten, ohne Erklärung, ohne Codeblock. \
Die drei Entwürfe sollen sich deutlich unterscheiden (z. B. andere Form, Optik, Farbe oder Anordnung), aber alle zur Beschreibung passen.\n\
Jedes Objekt hat die\
 Felder: name (kurz, auf {lang}), shape ({shapes}), size ({sizes}), style ({styles}), accent (Hexfarbe wie #64d2ff), blocks (Liste von {{\"kind\": ..., \"text\": ..., \"label\": ...}}).\n\
Bausteine (kind), alle mit echten Daten vom PC. In Klammern: (%) = Anteil, wird Balken oder Ring; (Kurve) = kleiner Verlauf; (gross) = braucht Platz.\n\
PC: ram = Arbeitsspeicher (%), ramfree = freier Arbeitsspeicher in GB, cpu = Prozessorlast (%), cpugraph = Verlauf der Prozessorlast (Kurve), \
disk = Belegung eines Laufwerks (%, text = Buchstabe wie C oder D), \
diskio = Laufwerksaktivität Lesen/Schreiben gerade (text = Buchstabe oder leer für alle; das ist \"Speicheraktivität\"), \
net = Netz runter/hoch, netgraph = Verlauf des Downloads (Kurve), uptime = wie lange der PC läuft, topapp = Programm mit der meisten Prozessorlast, battery = Akku (%).\n\
Zeit: clock = Uhrzeit, seconds = Uhrzeit mit Sekunden, analog = Analoguhr mit Zeigern (gross, am besten allein in round), binary = Binäruhr aus Lichtpunkten, \
date = Datum, week = Kalenderwoche, dayofyear = Tag im Jahr, worldclock = Uhrzeit anderswo (text = IANA-Zeitzone wie Asia/Tokyo oder America/New_York), \
dayprogress, weekprogress, monthprogress, yearprogress = wie viel vom Tag, der Woche, dem Monat, dem Jahr vorbei ist (%), moon = Mondphase.\n\
Planen: agenda = nächster Termin aus Milos Agenda, countdown = Tage bis zu einem Datum (text = \"Name|JJJJ-MM-TT\", z. B. \"Urlaub|2026-12-24\"), \
holiday = nächster gesetzlicher Feiertag, weekend = Zeit bis zum Wochenende (%), workday = Feierabend-Countdown (%, text = Arbeitszeit wie \"9-17\" oder \"8:30-16:30\"), \
timer = nächster Timer.\n\
Wetter am gespeicherten Ort: weather = Temperatur mit Symbol, feels = gefühlte Temperatur, wind = Wind, humidity = Luftfeuchte (%), \
rain = Regenwahrscheinlichkeit heute (%), forecast = Tiefst- und Höchstwert morgen mit Symbol, sun = nächster Sonnenauf- oder -untergang, daylight = verbleibendes Tageslicht (%).\n\
Milo: media = was gerade läuft (Musik/Video), note = neueste Notiz, quote = Sprichwort des Tages (lang, nur in card), quota = heute verbrauchtes Gemini-Kontingent, \
claude = Status von Claude Code, claudelimit = verbrauchtes 5-Stunden-Limit von Claude (%), text = fester Text (Feld text).\n\
Denk mit: ein Arbeits-Widget passt zu workday, agenda, clock; ein Wetter-Widget zu weather, rain, wind, sun; Gaming zu cpu, cpugraph, ram, netgraph.\n\
Wenn der Nutzer etwas will, das kein Baustein liefert (z. B. Aktienkurse, Nachrichten), nimm nur, was es gibt, und erfinde nichts.\n\
Gestaltung: Es soll am Ende schön und lesbar aussehen. round = 1 oder 2 Werte (zwei Prozentwerte wie ram, cpu, disk, battery werden zu zwei Ringen ineinander), \
pill = 1 oder 2 kurze Werte nebeneinander, card = 2 bis 4 Werte untereinander. Will der Nutzer mehr, als in die gewünschte Form passt, nimm card. \
Bei round mit 2 Werten nimm size m oder l. hanging = hängt wie die Insel am oberen Rand, floating = schwebt mit kleinem Abstand. \
Jeder Baustein darf ein kurzes eigenes label haben (höchstens 14 Zeichen), wenn es passender klingt als die Standardbeschriftung. \
Die Farbe darf zum Thema passen.\n\
Nutze nur diese Werte. Erfinde keine anderen Bausteine; was nicht geht, lässt du weg. \
Name und Beschriftungen (label) schreibst du auf {lang}.",
        lang = crate::i18n::reply_language(),
        shapes = SHAPES.join(", "),
        sizes = SIZES.join(", "),
        styles = STYLES.join(", ")
    );
    let answer = crate::agent::ask_once(&app, crate::limits::Tier::Easy, &system, vec![json!({ "text": description })], crate::gemini::Thinking::Low)
        .await
        .map_err(|e| e.to_string())?;
    let designs = parse_designs(&answer);
    if designs.is_empty() {
        return Err(tr!("Die KI hat kein brauchbares Widget geliefert. Versuch es mit einer anderen Beschreibung oder nutze den Baukasten.", "The AI didn't deliver a usable widget. Try a different description or use the builder.").into());
    }
    Ok(designs)
}

/// Ein Entwurf aus der KI-Antwort, bereinigt.
fn design_of(value: Value) -> Option<Widget> {
    let mut widget: Widget = serde_json::from_value(value).ok()?;
    widget.id = String::new();
    widget.x = None;
    widget.locked = false;
    Some(sanitize(fit(widget)))
}

/// Liest das JSON aus der Antwort (auch wenn es in einem Codeblock oder Text steckt).
pub fn parse_design(answer: &str) -> Option<Widget> {
    let start = answer.find('{')?;
    let end = answer.rfind('}')?;
    design_of(serde_json::from_str(answer.get(start..=end)?).ok()?)
}

/// Bis zu drei Entwuerfe; liefert die KI nur ein Objekt, wird es als einziger Entwurf genommen.
pub fn parse_designs(answer: &str) -> Vec<Widget> {
    let list = answer.find('[').zip(answer.rfind(']')).and_then(|(start, end)| answer.get(start..=end)).and_then(|text| serde_json::from_str::<Vec<Value>>(text).ok());
    match list {
        Some(items) => items.into_iter().filter_map(design_of).take(3).collect(),
        None => parse_design(answer).into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn designs_are_limited_to_known_blocks() {
        let w = parse_design("Hier: ```json\n{\"name\":\"PC\",\"shape\":\"round\",\"size\":\"xl\",\"accent\":\"rot\",\"blocks\":[{\"kind\":\"cpu\"},{\"kind\":\"ram\"},{\"kind\":\"bitcoin\"}]}\n```").unwrap();
        assert_eq!((w.shape.as_str(), w.size.as_str(), w.accent.as_str()), ("round", "m", "#64d2ff"));
        assert_eq!(w.blocks.iter().map(|b| b.kind.as_str()).collect::<Vec<_>>(), vec!["cpu", "ram"]);
        // Mehr Werte als die Form fasst: wird eine Karte, nichts faellt weg.
        let many = parse_design("{\"shape\":\"round\",\"blocks\":[{\"kind\":\"ram\"},{\"kind\":\"diskio\"},{\"kind\":\"cpu\",\"label\":\"Rechenlast der ganzen Zeit\"}]}").unwrap();
        assert_eq!((many.shape.as_str(), many.blocks.len()), ("card", 3));
        assert_eq!(many.blocks[2].label.chars().count(), 16);
        let ring = parse_design("{\"shape\":\"round\",\"size\":\"s\",\"blocks\":[{\"kind\":\"ram\"},{\"kind\":\"disk\"}]}").unwrap();
        assert_eq!((ring.shape.as_str(), ring.size.as_str(), ring.blocks.len()), ("round", "m", 2));
        let card = sanitize(Widget { shape: "card".into(), blocks: (0..6).map(|_| Block::default()).collect(), ..Default::default() });
        assert_eq!(card.blocks.len(), MAX_BLOCKS);
        let empty = sanitize(Widget { blocks: vec![Block { kind: "virus".into(), ..Default::default() }], ..Default::default() });
        assert_eq!(empty.blocks.len(), 1);
        assert!(parse_design("kein json").is_none());
        let three = parse_designs("```json\n[{\"name\":\"A\",\"shape\":\"pill\"},{\"name\":\"B\",\"shape\":\"round\"},{\"name\":\"C\",\"shape\":\"card\"},{\"name\":\"D\"}]\n```");
        assert_eq!(three.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), vec!["A", "B", "C"]);
        assert_eq!(parse_designs("{\"name\":\"Nur eins\"}").len(), 1);
    }

    #[test]
    fn widgets_stay_on_the_top_edge_and_off_the_island() {
        // Zwei Bildschirme nebeneinander, Insel mittig auf dem ersten (900..1020 ist zu schmal, nur zum Testen).
        let monitors = [(0, 1920, 1.0), (1920, 1920, 1.0)];
        let island = Some((600, 1320));
        // Links neben der Insel bleibt links.
        assert_eq!(clamp_x(100, 200, &monitors, island), (100, 0));
        // In die Insel hinein: zur naeheren Seite.
        assert_eq!(clamp_x(650, 200, &monitors, island), (400, 0));
        assert_eq!(clamp_x(1200, 200, &monitors, island), (1320, 0));
        // Ueber den Rand hinaus: am Bildschirm angehalten.
        assert_eq!(clamp_x(-500, 200, &monitors, island), (0, 0));
        // Mitte auf dem zweiten Bildschirm: dorthin.
        assert_eq!(clamp_x(2500, 200, &monitors, island), (2500, 1));
        assert_eq!(clamp_x(5000, 200, &monitors, island), (3640, 1));
        // Ueber die Inselmitte hinweg gezogen: auf der anderen Seite.
        assert_eq!(clamp_x(800, 200, &monitors, island), (400, 0));
        assert_eq!(clamp_x(900, 200, &monitors, island), (1320, 0));
    }
}
