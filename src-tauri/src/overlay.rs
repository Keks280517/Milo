//! Overlay-Fenster: Positionierung am oberen Bildschirmrand, Klick-Durchlass und Fokus-Rueckgabe.
//!
//! Das Fenster ist transparent und groesser als die sichtbare Insel. Damit es
//! keine Klicks auf Fenster dahinter schluckt, laesst es Mauseingaben nur dort
//! zu, wo die Insel wirklich sichtbar ist. Die Oberflaeche meldet dafuer die
//! sichtbare Flaeche (`set_hit_rect`), ein Hintergrund-Thread vergleicht sie
//! mit der Mausposition.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewWindow};

pub const OVERLAY_LABEL: &str = "overlay";
/// Mausposition relativ zum Fenster, damit der Charakter dem Zeiger nachschauen kann.
pub const CURSOR_EVENT: &str = "milo://cursor";
/// Nur in diesem Umkreis um das Fenster (physische Pixel) wird die Mausposition gemeldet.
const CURSOR_RANGE: f64 = 700.0;
/// Auf dem Monitor der Insel laeuft etwas im Vollbild (Film, Spiel, Praesentation) oder nicht mehr.
pub const FULLSCREEN_EVENT: &str = "milo://fullscreen";

/// Sichtbare Inselflaeche in physischen Pixeln, relativ zur Fensterecke oben links.
#[derive(Default, Clone, Copy, Debug)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl HitRect {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && px >= self.x
            && px < self.x + self.w
            && py >= self.y
            && py < self.y + self.h
    }
}

#[derive(Default)]
pub struct OverlayState {
    pub hit: Mutex<HitRect>,
    /// Das Fenster, das zuletzt den Fokus hatte, bevor der Nutzer zur Insel ging.
    last_foreground: AtomicIsize,
    fullscreen: AtomicBool,
}

/// Laeuft auf dem Monitor der Insel gerade etwas im Vollbild?
#[tauri::command]
pub fn is_fullscreen(state: State<'_, OverlayState>) -> bool {
    state.fullscreen.load(Ordering::Relaxed)
}

/// Deckt ein Fenster mit diesem Rechteck den Monitor genau ab? Ein maximiertes Fenster ragt
/// ein paar Pixel ueber den Rand hinaus und laesst die Taskleiste frei; das zaehlt nicht.
pub fn covers_exactly(window: [i32; 4], monitor: [i32; 4]) -> bool {
    window.iter().zip(monitor.iter()).all(|(w, m)| (w - m).abs() <= 1)
}

#[tauri::command]
pub fn set_hit_rect(state: State<'_, OverlayState>, x: f64, y: f64, w: f64, h: f64) {
    *state.hit.lock().unwrap() = HitRect { x, y, w, h };
}

/// Gibt den Tastaturfokus an das Fenster zurueck, in dem der Nutzer vorher gearbeitet hat.
/// Ohne das blieben Tastendruecke nach dem Zuklappen bei der unsichtbaren Insel haengen.
#[tauri::command]
pub fn release_focus(state: State<'_, OverlayState>) {
    let previous = state.last_foreground.load(Ordering::Relaxed);
    if previous != 0 {
        native::focus(previous);
    }
}

/// Setzt das Fenster oben mittig auf den Hauptmonitor.
pub fn place_top_center(win: &WebviewWindow) -> tauri::Result<()> {
    let Some(mon) = win.primary_monitor()? else {
        return Ok(());
    };
    let size = win.outer_size()?;
    let x = mon.position().x + (mon.size().width as i32 - size.width as i32) / 2;
    win.set_position(PhysicalPosition::new(x, mon.position().y))
}

/// Schaltet Mauseingaben nur ein, solange der Zeiger ueber der Insel steht.
pub fn spawn_click_through(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ignoring: Option<bool> = None;
        // Zuletzt gemeldete Mausposition; None = Zeiger ist ausser Reichweite.
        let mut reported: Option<(f64, f64)> = None;
        let mut tick = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(25));
            tick = tick.wrapping_add(1);
            let Some(win) = app.get_webview_window(OVERLAY_LABEL) else {
                // Kein Fenster (Spielemodus): nur noch einmal pro Sekunde nachsehen.
                ignoring = None;
                reported = None;
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            };
            let (Ok(cursor), Ok(origin)) = (app.cursor_position(), win.outer_position()) else {
                continue;
            };
            let state = app.state::<OverlayState>();
            let rect = *state.hit.lock().unwrap();
            let inside = rect.contains(cursor.x - origin.x as f64, cursor.y - origin.y as f64);
            if ignoring != Some(!inside) && win.set_ignore_cursor_events(!inside).is_ok() {
                ignoring = Some(!inside);
            }
            // Mausposition hoechstens 20-mal pro Sekunde und nur bei Bewegung in der Naehe melden.
            if tick % 2 == 0 {
                let (x, y) = (cursor.x - origin.x as f64, cursor.y - origin.y as f64);
                let size = win.outer_size().map(|s| (s.width as f64, s.height as f64)).unwrap_or_default();
                let near = x > -CURSOR_RANGE && x < size.0 + CURSOR_RANGE && y > -CURSOR_RANGE && y < size.1 + CURSOR_RANGE;
                let next = near.then_some((x, y));
                let moved = match (reported, next) {
                    (Some(a), Some(b)) => (a.0 - b.0).abs() + (a.1 - b.1).abs() >= 3.0,
                    (None, None) => false,
                    _ => true,
                };
                if moved {
                    reported = next;
                    let _ = win.emit(CURSOR_EVENT, next.map(|(x, y)| [x, y]));
                }
            }
            let own = win.hwnd().map(|h| h.0 as isize).unwrap_or(0);
            let foreground = native::foreground();
            if !inside && foreground != 0 && foreground != own {
                state.last_foreground.store(foreground, Ordering::Relaxed);
            }
            // Zweimal pro Sekunde: Vollbild auf dem Monitor der Insel? Hat die Insel selbst den
            // Fokus, bleibt der letzte Stand, sonst wuerde sie sich beim Benutzen verstecken.
            if tick % 20 == 0 && foreground != 0 && foreground != own {
                let full = native::fullscreen(foreground, own);
                if state.fullscreen.swap(full, Ordering::Relaxed) != full {
                    let _ = win.emit(FULLSCREEN_EVENT, full);
                    // Widgets verschwinden wie die Insel und kommen danach an derselben Stelle wieder.
                    let hide = full && app.state::<crate::settings::SettingsStore>().get().hide_in_fullscreen;
                    crate::widgets::set_hidden(&app, hide);
                    crate::desk::set_hidden(&app, hide);
                }
            }
        }
    });
}

#[cfg(windows)]
mod native {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, IsWindow, SetForegroundWindow, GWL_STYLE, WS_CAPTION,
    };

    /// Fuellt das Fenster `window` den Monitor, auf dem auch die Insel `own` sitzt, vollstaendig aus?
    /// Liest nur Fenstergroessen; greift in kein fremdes Programm ein.
    pub fn fullscreen(window: isize, own: isize) -> bool {
        let hwnd = HWND(window as *mut core::ffi::c_void);
        let own = HWND(own as *mut core::ffi::c_void);
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor != MonitorFromWindow(own, MONITOR_DEFAULTTONEAREST) {
                return false;
            }
            // Der Desktop selbst ist auch "bildschirmfuellend", aber kein Vollbild-Programm.
            let mut class = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut class).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..len]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "XamlExplorerHostIslandWindow") {
                return false;
            }
            // Echtes Vollbild hat keine Titelleiste. Das schliesst maximierte Fenster aus, die bei
            // automatisch ausgeblendeter Taskleiste ebenfalls den ganzen Monitor bedecken.
            if GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION.0 == WS_CAPTION.0 {
                return false;
            }
            let mut rect = RECT::default();
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetWindowRect(hwnd, &mut rect).is_err() || !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return false;
            }
            let m = info.rcMonitor;
            super::covers_exactly([rect.left, rect.top, rect.right, rect.bottom], [m.left, m.top, m.right, m.bottom])
        }
    }

    pub fn foreground() -> isize {
        unsafe { GetForegroundWindow().0 as isize }
    }

    pub fn focus(handle: isize) {
        let hwnd = HWND(handle as *mut core::ffi::c_void);
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub fn foreground() -> isize {
        0
    }
    pub fn focus(_handle: isize) {}
    pub fn fullscreen(_window: isize, _own: isize) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{covers_exactly, HitRect};

    #[test]
    fn only_true_fullscreen_counts() {
        let monitor = [0, 0, 2560, 1440];
        assert!(covers_exactly([0, 0, 2560, 1440], monitor));
        // Maximiert: ragt ueber den Rand und endet ueber der Taskleiste.
        assert!(!covers_exactly([-8, -8, 2568, 1400], monitor));
        // Normales Fenster.
        assert!(!covers_exactly([200, 100, 1400, 900], monitor));
        // Zweiter Monitor links daneben.
        assert!(covers_exactly([-1920, 0, 0, 1080], [-1920, 0, 0, 1080]));
    }

    #[test]
    fn empty_rect_never_hits() {
        assert!(!HitRect::default().contains(0.0, 0.0));
    }

    #[test]
    fn hit_is_inclusive_left_top_exclusive_right_bottom() {
        let r = HitRect { x: 10.0, y: 0.0, w: 100.0, h: 50.0 };
        assert!(r.contains(10.0, 0.0));
        assert!(r.contains(109.9, 49.9));
        assert!(!r.contains(110.0, 10.0));
        assert!(!r.contains(50.0, 50.0));
        assert!(!r.contains(9.9, 10.0));
    }
}
