//! Desktop-Ebene: Deko-Objekte und Milo selbst lassen sich aus der Insel ziehen, fallen mit
//! Schwerkraft auf die Taskleiste und lassen sich dort wieder greifen.
//!
//! Dafuer gibt es ein durchsichtiges Fenster ueber dem Arbeitsbereich des Insel-Monitors (bis zur
//! Oberkante der Taskleiste). Es laesst Klicks nur dort zu, wo gerade ein Objekt liegt; die Seite
//! meldet diese Flaechen (`desk_hits`). Waehrend etwas getragen wird, fuehrt dieser Kern die Maus:
//! er liest Position und linke Maustaste direkt bei Windows und meldet beides an die Seite. So geht
//! das Tragen auch aus der Insel heraus und ueber alle Fenster hinweg.
//!
//! Minispiele (Rechtsklick auf Milo): waehrend eines Spiels faengt das Fenster die ganze Maus ab,
//! und Escape wird hier bei Windows abgefragt, weil die Ebene nie den Tastaturfokus bekommt.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::settings::{DeskItem, SettingsStore, DECOR};

pub const DESK_LABEL: &str = "desk";

#[derive(Default)]
pub struct DeskState {
    /// Flaechen mit Objekten, physische Pixel relativ zum Fenster: [x, y, b, h].
    hits: Mutex<Vec<[f64; 4]>>,
    /// Was gerade getragen wird ("milo" oder ein Deko-Objekt).
    carrying: Mutex<Option<String>>,
    /// Milo ist auf dem Desktop.
    pub milo_out: AtomicBool,
    hidden: AtomicBool,
    /// Ein Minispiel laeuft: ganzes Fenster klickbar, Escape beendet.
    playing: AtomicBool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeskStart {
    pub items: Vec<DeskItem>,
    pub carrying: Option<String>,
    pub milo: bool,
    /// Ein Ausflug wartet auf die Seite: (Dauer in Sekunden, Startpunkt als Anteil der Breite).
    pub trip: Option<(f64, f64)>,
}

fn valid(kind: &str) -> bool {
    kind == "milo" || DECOR.contains(&kind)
}

/// Fenster anlegen (falls noetig), auf den Arbeitsbereich des Insel-Monitors legen und zeigen.
pub(crate) fn ensure(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let overlay = app.get_webview_window(crate::overlay::OVERLAY_LABEL).ok_or(tr!("Die Insel ist nicht offen.", "The island isn't open."))?;
    let monitor = overlay
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| overlay.primary_monitor().ok().flatten())
        .ok_or(tr!("Kein Bildschirm gefunden.", "No screen found."))?;
    let work = monitor.work_area();
    let window = match app.get_webview_window(DESK_LABEL) {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, DESK_LABEL, WebviewUrl::App("desk".into()))
            .title("Milo Desktop")
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .visible(false)
            .build()
            .map_err(|e| e.to_string())?,
    };
    window.set_position(PhysicalPosition::new(work.position.x, work.position.y)).map_err(|e| e.to_string())?;
    window.set_size(PhysicalSize::new(work.size.width, work.size.height)).map_err(|e| e.to_string())?;
    window.set_ignore_cursor_events(true).map_err(|e| e.to_string())?;
    if !app.state::<DeskState>().hidden.load(Ordering::Relaxed) {
        window.show().map_err(|e| e.to_string())?;
    }
    Ok(window)
}

/// Beim Start: liegen Dinge auf dem Desktop, die Ebene oeffnen.
pub fn open_if_needed(app: &AppHandle) {
    if !app.state::<SettingsStore>().get().desk_items.is_empty() {
        if let Err(err) = ensure(app) {
            log::warn!("Desktop-Ebene: {err}");
        }
    }
}

/// Startet das Tragen. `from_x`: kommt das Objekt vom Desktop, sein Platz (Anteil der Breite),
/// damit genau dieses aus der Liste faellt. Asynchron: legt unter Windows ein synchroner Befehl ein
/// Fenster an, blockiert er den Hauptthread (die ganze App haengt).
#[tauri::command]
pub async fn desk_carry(app: AppHandle, kind: String, from_x: Option<f64>, from_island: Option<bool>) -> Result<(), String> {
    if !valid(&kind) {
        return Err(tr!("Unbekanntes Objekt.", "Unknown object.").into());
    }
    // Aus der Insel gezogen: dort verschwindet es (es liegt gleich auf dem Desktop).
    if from_island == Some(true) && kind != "milo" {
        let store = app.state::<SettingsStore>();
        let mut settings = store.get();
        settings.decor.retain(|d| d != &kind);
        let saved = store.set(settings)?;
        let _ = app.emit("milo://settings", &saved);
    }
    if let Some(x) = from_x {
        if kind != "milo" {
            let store = app.state::<SettingsStore>();
            let mut settings = store.get();
            if let Some(index) = settings
                .desk_items
                .iter()
                .enumerate()
                .filter(|(_, d)| d.kind == kind)
                .min_by(|a, b| (a.1.x - x).abs().total_cmp(&(b.1.x - x).abs()))
                .map(|(i, _)| i)
            {
                settings.desk_items.remove(index);
                let saved = store.set(settings)?;
                let _ = app.emit("milo://settings", &saved);
            }
        }
    }
    if kind == "milo" {
        app.state::<DeskState>().milo_out.store(true, Ordering::Relaxed);
    }
    let window = ensure(&app)?;
    let _ = window.show();
    *app.state::<DeskState>().carrying.lock().unwrap() = Some(kind.clone());
    let _ = app.emit_to(DESK_LABEL, "milo://desk-carry", json!({ "kind": kind }));
    Ok(())
}

/// Mini-Menue in der Insel: ein Objekt direkt auf den Desktop legen (x = Anteil der Breite).
/// Es verschwindet aus der Insel und faellt auf der Desktop-Ebene von oben auf die Taskleiste.
#[tauri::command]
pub async fn desk_place(app: AppHandle, kind: String, x: f64) -> Result<(), String> {
    if kind == "milo" || !valid(&kind) {
        return Err(tr!("Unbekanntes Objekt.", "Unknown object.").into());
    }
    let store = app.state::<SettingsStore>();
    let mut settings = store.get();
    settings.decor.retain(|d| d != &kind);
    settings.desk_items.push(DeskItem { kind, x: if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.5 }, locked: false });
    let saved = store.set(settings)?;
    ensure(&app)?;
    let _ = app.emit("milo://settings", &saved);
    Ok(())
}

/// Ein Objekt ist gelandet: auf dem Desktop (x = Anteil der Breite) oder zurueck in der Insel.
#[tauri::command]
pub fn desk_drop_home(app: AppHandle, kind: String, x: f64, home: bool) -> Result<(), String> {
    if !valid(&kind) {
        return Err(tr!("Unbekanntes Objekt.", "Unknown object.").into());
    }
    if kind == "milo" {
        if home {
            app.state::<DeskState>().milo_out.store(false, Ordering::Relaxed);
            let _ = app.emit_to(crate::overlay::OVERLAY_LABEL, "milo://milo-home", ());
        }
        return Ok(());
    }
    let store = app.state::<SettingsStore>();
    let mut settings = store.get();
    if home {
        if !settings.decor.contains(&kind) {
            settings.decor.push(kind);
        }
    } else {
        settings.desk_items.push(DeskItem { kind, x: x.clamp(0.0, 1.0), locked: false });
    }
    let saved = store.set(settings)?;
    let _ = app.emit("milo://settings", &saved);
    Ok(())
}

/// Rechtsklick-Menue auf dem Desktop: Objekt sperren, entsperren, in die Insel zurueck oder entfernen.
/// `x` (Anteil der Breite) bestimmt, welches von mehreren gleichen Objekten gemeint ist.
#[tauri::command]
pub fn desk_item(app: AppHandle, kind: String, x: f64, action: String) -> Result<(), String> {
    let store = app.state::<SettingsStore>();
    let mut settings = store.get();
    let Some(index) = settings
        .desk_items
        .iter()
        .enumerate()
        .filter(|(_, d)| d.kind == kind)
        .min_by(|a, b| (a.1.x - x).abs().total_cmp(&(b.1.x - x).abs()))
        .map(|(i, _)| i)
    else {
        return Err(tr!("Das Objekt liegt nicht mehr da.", "The object isn't there anymore.").into());
    };
    match action.as_str() {
        "lock" => settings.desk_items[index].locked = true,
        "unlock" => settings.desk_items[index].locked = false,
        "home" => {
            settings.desk_items.remove(index);
            if !settings.decor.contains(&kind) {
                settings.decor.push(kind);
            }
        }
        "remove" => {
            settings.desk_items.remove(index);
        }
        _ => return Err(tr!("Unbekannte Aktion.", "Unknown action.").into()),
    }
    let saved = store.set(settings)?;
    let _ = app.emit("milo://settings", &saved);
    Ok(())
}

/// Die Seite meldet, wo Objekte liegen (dort darf geklickt werden).
#[tauri::command]
pub fn desk_hits(state: tauri::State<'_, DeskState>, rects: Vec<[f64; 4]>) {
    *state.hits.lock().unwrap() = rects.into_iter().take(64).collect();
}

/// Die Seite ist bereit: was liegt da, was wird gerade getragen, ist Milo draussen?
#[tauri::command]
pub fn desk_ready(app: AppHandle) -> DeskStart {
    let state = app.state::<DeskState>();
    DeskStart {
        items: app.state::<SettingsStore>().get().desk_items,
        carrying: state.carrying.lock().unwrap().clone(),
        milo: state.milo_out.load(Ordering::Relaxed),
        trip: crate::pet::take_trip(&app),
    }
}

/// Milo soll vom Desktop zurueck (Knopf in der Insel): die Seite laesst ihn verschwinden.
#[tauri::command]
pub fn desk_take(app: AppHandle) {
    if app.get_webview_window(DESK_LABEL).is_some() && app.state::<DeskState>().milo_out.load(Ordering::Relaxed) {
        let _ = app.emit_to(DESK_LABEL, "milo://desk-recall", ());
    } else {
        app.state::<DeskState>().milo_out.store(false, Ordering::Relaxed);
        let _ = app.emit_to(crate::overlay::OVERLAY_LABEL, "milo://milo-home", ());
    }
}

/// Minispiel an oder aus. An: das Fenster nimmt alle Mausereignisse, Escape wird gemeldet.
#[tauri::command]
pub fn desk_game(app: AppHandle, on: bool) {
    app.state::<DeskState>().playing.store(on, Ordering::Relaxed);
}

/// Vollbild: die Ebene verschwindet mit der Insel und kommt danach wieder.
pub fn set_hidden(app: &AppHandle, hidden: bool) {
    app.state::<DeskState>().hidden.store(hidden, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window(DESK_LABEL) {
        let _ = if hidden { window.hide() } else { window.show() };
    }
}

/// Spielemodus und Beenden: das Fenster ganz schliessen (Milo kehrt heim).
pub fn close(app: &AppHandle) {
    let state = app.state::<DeskState>();
    state.milo_out.store(false, Ordering::Relaxed);
    state.playing.store(false, Ordering::Relaxed);
    *state.carrying.lock().unwrap() = None;
    if let Some(window) = app.get_webview_window(DESK_LABEL) {
        let _ = window.destroy();
    }
}

/// Fuehrt die Maus: Klick-Durchlass ueber leeren Stellen, beim Tragen Position und Loslassen melden.
pub fn spawn_pointer(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ignoring: Option<bool> = None;
        let mut escape_down = false;
        let mut last_hover = (f64::NAN, f64::NAN);
        loop {
            let Some(win) = app.get_webview_window(DESK_LABEL) else {
                ignoring = None;
                std::thread::sleep(Duration::from_millis(500));
                continue;
            };
            let state = app.state::<DeskState>();
            let carrying = state.carrying.lock().unwrap().clone();
            std::thread::sleep(Duration::from_millis(if carrying.is_some() { 12 } else { 30 }));
            let (Ok(cursor), Ok(origin)) = (app.cursor_position(), win.outer_position()) else { continue };
            let (x, y) = (cursor.x - origin.x as f64, cursor.y - origin.y as f64);
            if state.playing.load(Ordering::Relaxed) && carrying.is_none() {
                if ignoring != Some(false) && win.set_ignore_cursor_events(false).is_ok() {
                    ignoring = Some(false);
                }
                // Nur die Flanke melden: einmal Druecken beendet das Spiel einmal.
                let down = native::escape_down();
                if down && !escape_down {
                    let _ = win.emit("milo://desk-escape", ());
                }
                escape_down = down;
                continue;
            }
            escape_down = false;
            // Milo ist draussen: er schaut dem Zeiger nach und kommt neugierig naeher.
            if state.milo_out.load(Ordering::Relaxed) && (x, y) != last_hover {
                last_hover = (x, y);
                let _ = win.emit("milo://desk-hover", [x, y]);
            }
            if let Some(kind) = carrying {
                if ignoring != Some(true) && win.set_ignore_cursor_events(true).is_ok() {
                    ignoring = Some(true);
                }
                if native::left_button_down() {
                    let _ = win.emit("milo://desk-pointer", [x, y]);
                } else {
                    *state.carrying.lock().unwrap() = None;
                    let over = over_island(&app, cursor.x, cursor.y);
                    let _ = win.emit("milo://desk-drop", json!({ "kind": kind, "x": x, "y": y, "overIsland": over }));
                }
                continue;
            }
            let inside = state.hits.lock().unwrap().iter().any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]);
            if ignoring != Some(!inside) && win.set_ignore_cursor_events(!inside).is_ok() {
                ignoring = Some(!inside);
            }
        }
    });
}

/// Liegt der Bildschirmpunkt ueber der sichtbaren Insel?
fn over_island(app: &AppHandle, x: f64, y: f64) -> bool {
    let Some(overlay) = app.get_webview_window(crate::overlay::OVERLAY_LABEL) else { return false };
    let Ok(origin) = overlay.outer_position() else { return false };
    let rect = *app.state::<crate::overlay::OverlayState>().hit.lock().unwrap();
    // Etwas grosszuegiger als die Insel selbst, damit das Zurueckwerfen leicht faellt.
    let grown = crate::overlay::HitRect { x: rect.x - 40.0, y: rect.y, w: rect.w + 80.0, h: rect.h + 50.0 };
    grown.contains(x - origin.x as f64, y - origin.y as f64)
}

#[cfg(windows)]
mod native {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_RBUTTON};
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};

    pub fn left_button_down() -> bool {
        unsafe {
            // Bei vertauschten Maustasten ist die "linke" physisch die rechte.
            let key = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
            (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0
        }
    }

    pub fn escape_down() -> bool {
        unsafe { (GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000) != 0 }
    }
}

#[cfg(not(windows))]
mod native {
    pub fn left_button_down() -> bool {
        false
    }

    pub fn escape_down() -> bool {
        false
    }
}
