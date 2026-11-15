//! Ausfluege auf den Desktop: Milo verlaesst ab und zu die Insel und spaziert ueber die Taskleiste.
//!
//! Der Ausflug laeuft auf derselben Desktop-Ebene wie ein herausgezogener Milo (siehe `desk.rs`).
//! Darum laesst er sich auch unterwegs greifen, werfen und per Rechtsklick (Minispiele) bedienen.
//! Die Insel entscheidet, wann Milo losgeht und wann er zurueckkommt (sobald der Nutzer ihn braucht).
//! Fasst der Nutzer ihn unterwegs an, wird aus dem Ausflug ein normaler Desktop-Besuch: Milo bleibt
//! dann, bis man ihn in die Insel zurueckholt.

use std::sync::atomic::Ordering;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::desk::{DeskState, DESK_LABEL};

#[derive(Default)]
pub struct PetState {
    /// Der laufende Auftrag (Dauer, Startpunkt), falls die Desktop-Seite ihn noch abholen muss.
    pub trip: std::sync::Mutex<Option<(f64, f64)>>,
    /// Ein Ausflug laeuft gerade.
    pub away: std::sync::atomic::AtomicBool,
}

/// Startet einen Ausflug von `seconds` Sekunden. `from`: Startpunkt als Anteil der Bildschirmbreite.
#[tauri::command]
pub async fn pet_out(app: AppHandle, seconds: f64, from: f64) -> Result<(), String> {
    if app.state::<DeskState>().milo_out.load(Ordering::Relaxed) {
        return Err(tr!("Milo ist schon auf dem Desktop.", "Milo is already on the desktop.").into());
    }
    let trip = (seconds.clamp(10.0, 180.0), from.clamp(0.0, 1.0));
    let state = app.state::<PetState>();
    *state.trip.lock().unwrap() = Some(trip);
    state.away.store(true, Ordering::Relaxed);
    app.state::<DeskState>().milo_out.store(true, Ordering::Relaxed);
    crate::desk::ensure(&app)?;
    // Die Seite braucht beim ersten Mal einen Augenblick; sonst holt sie sich den Auftrag ueber desk_ready.
    let _ = app.emit_to(DESK_LABEL, "milo://desk-visit", json!({ "seconds": trip.0, "from": trip.1 }));
    Ok(())
}

/// Die Desktop-Seite holt einen noch offenen Auftrag ab.
pub fn take_trip(app: &AppHandle) -> Option<(f64, f64)> {
    app.state::<PetState>().trip.lock().unwrap().take()
}

/// Die Desktop-Seite hat den Auftrag bekommen (Ereignis kam an): nicht noch einmal ausliefern.
#[tauri::command]
pub fn pet_ready(app: AppHandle) {
    take_trip(&app);
}

/// Die Insel ruft Milo zurueck; die Desktop-Seite verabschiedet ihn und meldet `pet_home`.
#[tauri::command]
pub fn pet_recall(app: AppHandle) {
    if !app.state::<PetState>().away.load(Ordering::Relaxed) {
        return;
    }
    if app.get_webview_window(DESK_LABEL).is_some() {
        let _ = app.emit_to(DESK_LABEL, "milo://desk-visit-end", ());
    } else {
        pet_home(app);
    }
}

/// Der Ausflug ist vorbei: Milo taucht wieder auf der Insel auf.
#[tauri::command]
pub fn pet_home(app: AppHandle) {
    let state = app.state::<PetState>();
    *state.trip.lock().unwrap() = None;
    if state.away.swap(false, Ordering::Relaxed) {
        app.state::<DeskState>().milo_out.store(false, Ordering::Relaxed);
        let _ = app.emit_to(crate::overlay::OVERLAY_LABEL, "milo://pet-back", ());
    }
}

/// Der Nutzer hat Milo unterwegs angefasst: ab jetzt ist er ein normaler Desktop-Besuch.
#[tauri::command]
pub fn pet_adopt(app: AppHandle) {
    let state = app.state::<PetState>();
    *state.trip.lock().unwrap() = None;
    if state.away.swap(false, Ordering::Relaxed) {
        let _ = app.emit_to(crate::overlay::OVERLAY_LABEL, "milo://pet-adopted", ());
    }
}

/// Spielemodus und Beenden: ein laufender Ausflug endet (die Desktop-Ebene schliesst sich selbst).
pub fn close(app: &AppHandle) {
    let state = app.state::<PetState>();
    *state.trip.lock().unwrap() = None;
    state.away.store(false, Ordering::Relaxed);
}
