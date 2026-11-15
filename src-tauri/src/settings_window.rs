//! Eigenes Fenster fuer die Einstellungen: ein normales Windows-Fenster mit Titelleiste, in der
//! Taskleiste sichtbar, verschieb- und vergroesserbar. Unter Windows 11 mit Mica-Hintergrund.

use tauri::utils::config::WindowEffectsConfig;
use tauri::window::Effect;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "settings";

/// Oeffnet das Einstellungsfenster oder holt das offene nach vorn. `section` springt zu einem Bereich.
/// Asynchron, weil ein synchroner Befehl beim Anlegen eines Fensters unter Windows den Hauptthread blockiert.
#[tauri::command]
pub async fn settings_open(app: AppHandle, section: Option<String>, on_top: Option<bool>) -> Result<(), String> {
    // Die Tour zeigt das Fenster ueber der Insel; die Insel ist immer im Vordergrund und wuerde es
    // sonst verdecken. Beim normalen Oeffnen bleibt es ein gewoehnliches Fenster.
    let on_top = on_top.unwrap_or(false);
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_always_on_top(on_top);
        let _ = window.set_focus();
        if let Some(section) = section {
            let _ = tauri::Emitter::emit_to(&app, LABEL, "milo://settings-section", section);
        }
        return Ok(());
    }
    let url = match section {
        Some(s) if s.chars().all(|c| c.is_ascii_alphanumeric()) => format!("settings?section={s}"),
        _ => "settings".to_string(),
    };
    let mut builder = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App(url.into()))
        .title(tr!("Milo – Einstellungen", "Milo – Settings"))
        .min_inner_size(760.0, 520.0);
    // In der Tour links an den Rand, kleiner: die Insel (Mitte oben) mit dem hervorgehobenen
    // Menueknopf (rechts) und der Erklaerkasten darunter bleiben sichtbar.
    let tour_spot = if on_top {
        app.primary_monitor().ok().flatten().map(|m| {
            let scale = m.scale_factor();
            let area = m.work_area();
            let h = (area.size.height as f64 / scale - 120.0).clamp(520.0, 640.0);
            let x = area.position.x as f64 / scale + 24.0;
            let y = area.position.y as f64 / scale + (area.size.height as f64 / scale - h) / 2.0;
            (x, y, h)
        })
    } else {
        None
    };
    builder = match tour_spot {
        Some((x, y, h)) => builder.inner_size(760.0, h).position(x, y),
        None => builder.inner_size(980.0, 680.0).center(),
    };
    builder
        .transparent(true)
        .theme(Some(tauri::Theme::Dark))
        .effects(WindowEffectsConfig { effects: vec![Effect::Mica], ..Default::default() })
        .skip_taskbar(false)
        .always_on_top(on_top)
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Ist das Einstellungsfenster gerade offen (und sichtbar)?
#[tauri::command]
pub fn settings_is_open(app: AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some_and(|w| w.is_visible().unwrap_or(false))
}

/// Schliesst das Einstellungsfenster (die Tour zeigt es nur kurz).
#[tauri::command]
pub fn settings_close(app: AppHandle) {
    close(&app);
}

/// Spielemodus und Beenden: das Fenster schliessen.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}
