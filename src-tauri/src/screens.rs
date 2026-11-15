//! Mehrere Bildschirme: erkennen, benennen („zweiter“, „linker“, „anderer“) und Fenster dorthin
//! schieben. Bildschirm 1 ist immer der Hauptbildschirm, die anderen folgen von links nach rechts.
//! Ausserdem: eine Webseite gezielt in einem neuen Browserfenster auf einem Bildschirm oeffnen.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::system::{self, AppWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Screen {
    /// 1 = Hauptbildschirm, danach von links nach rechts.
    pub number: usize,
    pub primary: bool,
    pub bounds: Rect,
    /// Ohne Taskleiste.
    pub work: Rect,
}

/// Bringt die Bildschirme in Milos Reihenfolge: Hauptbildschirm zuerst, dann von links nach rechts
/// (bei gleicher Hoehe von oben nach unten).
pub fn order(raw: Vec<(bool, Rect, Rect)>) -> Vec<Screen> {
    let mut raw = raw;
    raw.sort_by_key(|(primary, bounds, _)| (!*primary, bounds.left, bounds.top));
    raw.into_iter().enumerate().map(|(i, (primary, bounds, work))| Screen { number: i + 1, primary, bounds, work }).collect()
}

/// Wo ein Bildschirm relativ zum Hauptbildschirm steht, fuer Menschen.
fn position(screen: &Screen, main: &Screen) -> &'static str {
    let (b, m) = (screen.bounds, main.bounds);
    if b.right <= m.left {
        tr!("links vom Hauptbildschirm", "left of the main screen")
    } else if b.left >= m.right {
        tr!("rechts vom Hauptbildschirm", "right of the main screen")
    } else if b.bottom <= m.top {
        tr!("über dem Hauptbildschirm", "above the main screen")
    } else if b.top >= m.bottom {
        tr!("unter dem Hauptbildschirm", "below the main screen")
    } else {
        tr!("neben dem Hauptbildschirm", "next to the main screen")
    }
}

/// Kurzbeschreibung fuer den Systemprompt und das Werkzeug `list_screens`.
pub fn describe(screens: &[Screen]) -> String {
    match screens {
        [] => tr!("Milo kann die Bildschirme gerade nicht erkennen.", "Milo can't detect the screens right now.").into(),
        [only] => tr!(format!("Es ist nur ein Bildschirm angeschlossen ({}×{}).", only.bounds.width(), only.bounds.height()), format!("Only one screen is connected ({}×{}).", only.bounds.width(), only.bounds.height())),
        all => {
            let main = &all[0];
            let parts: Vec<String> = all
                .iter()
                .map(|s| {
                    let place = if s.primary { tr!("Hauptbildschirm", "main screen").to_string() } else { position(s, main).to_string() };
                    tr!(format!("Bildschirm {} = {place} ({}×{})", s.number, s.bounds.width(), s.bounds.height()), format!("Screen {} = {place} ({}×{})", s.number, s.bounds.width(), s.bounds.height()))
                })
                .collect();
            tr!(format!("Es sind {} Bildschirme angeschlossen: {}. „Zweiter“ oder „anderer Bildschirm“ meint Bildschirm 2.",
                all.len(),
                parts.join("; ")
            ), format!("{} screens are connected: {}. \"Second\" or \"other screen\" means screen 2.",
                all.len(),
                parts.join("; ")
            ))
        }
    }
}

/// Findet den gemeinten Bildschirm: Nummer („2“, „zweiter“), Lage („links“, „rechts“), „Haupt“ oder „anderer“.
pub fn resolve<'a>(screens: &'a [Screen], spec: &str) -> Result<&'a Screen> {
    if screens.is_empty() {
        bail!(tr!("Ich kann die Bildschirme gerade nicht erkennen.", "I can't detect the screens right now."));
    }
    let spec = spec.trim().to_lowercase();
    let words: Vec<&str> = spec.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let any = |list: &[&str]| words.iter().any(|w| list.iter().any(|l| w.starts_with(l)));
    let number = if let Some(n) = words.iter().find_map(|w| w.parse::<usize>().ok()) {
        Some(n)
    } else if any(&["haupt", "primär", "primaer", "main", "primary", "erst", "eins", "first"]) {
        Some(1)
    } else if any(&["zweit", "zwei", "second", "neben", "ander", "other", "extern"]) {
        Some(2)
    } else if any(&["dritt", "drei", "third"]) {
        Some(3)
    } else if any(&["viert", "vier", "fourth"]) {
        Some(4)
    } else {
        None
    };
    if let Some(n) = number {
        return screens.iter().find(|s| s.number == n).ok_or_else(|| {
            if screens.len() == 1 {
                anyhow!(tr!("Ich sehe nur einen Bildschirm. Ist der zweite eingeschaltet und angeschlossen?", "I only see one screen. Is the second one on and connected?"))
            } else {
                anyhow!("{}", tr!(format!("Es gibt nur {} Bildschirme.", screens.len()), format!("There are only {} screens.", screens.len())))
            }
        });
    }
    if any(&["link", "left"]) {
        return Ok(screens.iter().min_by_key(|s| s.bounds.left).unwrap_or(&screens[0]));
    }
    if any(&["recht", "right"]) {
        return Ok(screens.iter().max_by_key(|s| s.bounds.left).unwrap_or(&screens[0]));
    }
    if any(&["ob", "top", "über"]) {
        return Ok(screens.iter().min_by_key(|s| s.bounds.top).unwrap_or(&screens[0]));
    }
    if any(&["unt", "bottom"]) {
        return Ok(screens.iter().max_by_key(|s| s.bounds.top).unwrap_or(&screens[0]));
    }
    bail!("{}", tr!(format!("Welchen Bildschirm meinst du? {}", describe(screens)), format!("Which screen do you mean? {}", describe(screens))))
}

/// Rechteck fuer ein Fenster auf dem Bildschirm: mittig, hoechstens so gross wie die Arbeitsflaeche.
pub fn centered(work: Rect, width: i32, height: i32) -> Rect {
    let w = width.clamp(320, work.width().max(320));
    let h = height.clamp(240, work.height().max(240));
    let left = work.left + (work.width() - w) / 2;
    let top = work.top + (work.height() - h) / 2;
    Rect { left, top, right: left + w, bottom: top + h }
}

/// Linke oder rechte Haelfte der Arbeitsflaeche.
pub fn half(work: Rect, left: bool) -> Rect {
    let middle = work.left + work.width() / 2;
    if left {
        Rect { right: middle, ..work }
    } else {
        Rect { left: middle, ..work }
    }
}

/// Was mit einem Fenster passieren soll.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Auf den Bildschirm schieben, Groesse behalten (passt sie nicht, wird verkleinert).
    Move,
    Maximize,
    LeftHalf,
    RightHalf,
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LWIN, VK_MENU};
    use windows::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, IsIconic, IsZoomed, SetForegroundWindow, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_MINIMIZE,
        SW_RESTORE,
    };

    const MONITORINFOF_PRIMARY: u32 = 1;

    fn rect(r: RECT) -> Rect {
        Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }

    pub fn list() -> Vec<Screen> {
        unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            let list = unsafe { &mut *(data.0 as *mut Vec<(bool, Rect, Rect)>) };
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                list.push((info.dwFlags & MONITORINFOF_PRIMARY != 0, rect(info.rcMonitor), rect(info.rcWork)));
            }
            BOOL(1)
        }
        let mut raw: Vec<(bool, Rect, Rect)> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut raw as *mut _ as isize));
        }
        order(raw)
    }

    fn hwnd(handle: isize) -> HWND {
        HWND(handle as *mut core::ffi::c_void)
    }

    pub fn window_rect(handle: isize) -> Option<Rect> {
        let mut r = RECT::default();
        unsafe { GetWindowRect(hwnd(handle), &mut r) }.ok().map(|_| rect(r))
    }

    pub fn place(handle: isize, screen: &Screen, how: Placement) -> Result<()> {
        let h = hwnd(handle);
        unsafe {
            // Maximierte oder minimierte Fenster lassen sich nicht verschieben: erst wiederherstellen.
            if IsIconic(h).as_bool() || IsZoomed(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
                std::thread::sleep(Duration::from_millis(120));
            }
            let current = window_rect(handle).unwrap_or(screen.work);
            let target = match how {
                Placement::LeftHalf => half(screen.work, true),
                Placement::RightHalf => half(screen.work, false),
                Placement::Move | Placement::Maximize => centered(screen.work, current.width(), current.height()),
            };
            SetWindowPos(h, None, target.left, target.top, target.width(), target.height(), SWP_NOZORDER | SWP_NOACTIVATE)
                .map_err(|e| anyhow!("{}", tr!(format!("Das Fenster ließ sich nicht verschieben: {e}"), format!("The window couldn't be moved: {e}"))))?;
            if how == Placement::Maximize {
                let _ = ShowWindow(h, SW_MAXIMIZE);
            }
        }
        Ok(())
    }

    pub fn minimize(handle: isize) {
        unsafe {
            let _ = ShowWindow(hwnd(handle), SW_MINIMIZE);
        }
    }

    pub fn is_maximized(handle: isize) -> bool {
        unsafe { IsZoomed(hwnd(handle)).as_bool() }
    }

    /// Legt ein Fenster genau auf `target` (fuer Arbeitsplaetze); `maximized` maximiert es danach dort.
    pub fn set_rect(handle: isize, target: Rect, maximized: bool) -> Result<()> {
        let h = hwnd(handle);
        unsafe {
            if IsIconic(h).as_bool() || IsZoomed(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
                std::thread::sleep(Duration::from_millis(120));
            }
            SetWindowPos(h, None, target.left, target.top, target.width(), target.height(), SWP_NOZORDER | SWP_NOACTIVATE)
                .map_err(|e| anyhow!("{}", tr!(format!("Das Fenster ließ sich nicht verschieben: {e}"), format!("The window couldn't be moved: {e}"))))?;
            if maximized {
                let _ = ShowWindow(h, SW_MAXIMIZE);
            }
        }
        Ok(())
    }

    pub fn restore(handle: isize) {
        unsafe {
            let _ = ShowWindow(hwnd(handle), SW_RESTORE);
        }
    }

    fn keys(combo: &[VIRTUAL_KEY]) {
        let event = |vk: VIRTUAL_KEY, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } },
        };
        let mut inputs: Vec<INPUT> = combo.iter().map(|vk| event(*vk, false)).collect();
        inputs.extend(combo.iter().rev().map(|vk| event(*vk, true)));
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    }

    /// Holt ein Fenster nach vorne. Windows erlaubt das nur dem vorderen Programm; ein kurzer
    /// Alt-Tastendruck vorher ist der uebliche, harmlose Weg dazu.
    pub fn activate(handle: isize) {
        let h = hwnd(handle);
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            keys(&[VK_MENU]);
            let _ = SetForegroundWindow(h);
        }
    }

    /// Windows-Taste + D: alle Fenster minimieren (noch einmal: zurueck).
    pub fn show_desktop() {
        keys(&[VK_LWIN, VIRTUAL_KEY(b'D' as u16)]);
    }

    /// Programmdatei des Standardbrowsers (der Handler fuer https).
    pub fn default_browser() -> Option<String> {
        unsafe {
            let mut buffer = [0u16; 1024];
            let mut len = buffer.len() as u32;
            AssocQueryStringW(ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE, &HSTRING::from("https"), PCWSTR::null(), Some(PWSTR(buffer.as_mut_ptr())), &mut len)
                .ok()
                .ok()?;
            let text = String::from_utf16_lossy(&buffer[..len.saturating_sub(1) as usize]);
            let text = text.trim_end_matches('\0').trim().to_string();
            (!text.is_empty()).then_some(text)
        }
    }
}

#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn list() -> Vec<Screen> {
        Vec::new()
    }
    pub fn window_rect(_handle: isize) -> Option<Rect> {
        None
    }
    pub fn place(_handle: isize, _screen: &Screen, _how: Placement) -> Result<()> {
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
    pub fn minimize(_handle: isize) {}
    pub fn is_maximized(_handle: isize) -> bool {
        false
    }
    pub fn set_rect(_handle: isize, _target: Rect, _maximized: bool) -> Result<()> {
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
    pub fn restore(_handle: isize) {}
    pub fn activate(_handle: isize) {}
    pub fn show_desktop() {}
    pub fn default_browser() -> Option<String> {
        None
    }
}

pub use native::{activate, default_browser, is_maximized, list, minimize, place, restore, set_rect, show_desktop};

/// Fensterrahmen eines Fensters (fuer Arbeitsplaetze).
pub fn rect_of(handle: isize) -> Option<Rect> {
    native::window_rect(handle)
}

/// Auf welchem Bildschirm ein Fenster gerade liegt (nach der Mitte des Fensters).
pub fn screen_of<'a>(screens: &'a [Screen], handle: isize) -> Option<&'a Screen> {
    let r = native::window_rect(handle)?;
    let (x, y) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    screens.iter().find(|s| x >= s.bounds.left && x < s.bounds.right && y >= s.bounds.top && y < s.bounds.bottom)
}

/// Programmdateien von Browsern, die ein neues Fenster mit `--new-window` oeffnen.
const CHROMIUM: [&str; 5] = ["brave", "chrome", "msedge", "vivaldi", "opera"];

/// Dateiname ohne Endung, klein geschrieben ("C:\...\brave.exe" -> "brave").
pub fn stem(path: &str) -> String {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")).unwrap_or(name).to_lowercase()
}

/// Meint der Nutzer mit diesem Wort einfach „seinen Browser“ (egal welcher)?
pub fn is_browser_word(name: &str) -> bool {
    let n = crate::apps::normalize(name);
    let n = n.trim_start_matches("mein ").trim_start_matches("meinen ").trim_start_matches("den ").trim();
    ["browser", "webbrowser", "internetbrowser", "standardbrowser", "internet", "browserfenster", "web browser", "internet browser"].contains(&n)
}

/// Ist das Programm (Dateiname ohne Endung) ein Browser? Bekannte Browser und der Standardbrowser,
/// auch wenn er nicht in der Liste steht.
pub fn is_browser_process(process: &str, default_stem: Option<&str>) -> bool {
    let process = process.to_lowercase();
    crate::browser::BROWSERS.iter().any(|b| stem(b) == process) || default_stem.is_some_and(|d| d == process)
}

/// Lesbarer Name eines Browsers ("brave" -> "Brave", "msedge" -> "Edge").
pub fn browser_name(stem: &str) -> String {
    match stem {
        "msedge" => "Edge".into(),
        "chrome" => "Google Chrome".into(),
        other => {
            let mut chars = other.chars();
            chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        }
    }
}

/// Der Standardbrowser als lesbarer Name, z. B. "Brave".
pub fn default_browser_name() -> Option<String> {
    default_browser().map(|exe| browser_name(&stem(&exe)))
}

/// Argumente fuer ein neues Browserfenster, je nach Browser.
pub fn new_window_args(browser_stem: &str, url: &str) -> Option<Vec<String>> {
    if CHROMIUM.contains(&browser_stem) {
        Some(vec!["--new-window".into(), url.into()])
    } else if browser_stem == "firefox" {
        Some(vec!["-new-window".into(), url.into()])
    } else {
        None
    }
}

fn own_pid() -> u32 {
    std::process::id()
}

/// Sichtbare Fenster anderer Programme (ohne Milo selbst), von vorne nach hinten.
pub fn other_windows() -> Vec<AppWindow> {
    let own = own_pid();
    system::windows().into_iter().filter(|w| w.pid != own).collect()
}

/// Wartet auf ein neues Fenster (nicht in `before`). `matches` waehlt passende Fenster aus.
pub fn wait_for_new_window(before: &HashSet<isize>, timeout: Duration, matches: impl Fn(&AppWindow) -> bool) -> Option<AppWindow> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(found) = other_windows().into_iter().find(|w| !before.contains(&w.handle) && matches(w)) {
            return Some(found);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    None
}

/// Ergebnis von `open_url_on`.
pub struct Opened {
    pub screen: usize,
    /// Milo musste ein schon offenes Browserfenster verschieben, weil kein neues auftauchte.
    pub moved_existing: bool,
}

/// Oeffnet eine Adresse in einem neuen Fenster des Standardbrowsers und legt es maximiert auf den Bildschirm.
/// Laeuft blockierend (bis zu einige Sekunden), also in `spawn_blocking` aufrufen.
pub fn open_url_on(url: &str, spec: &str) -> Result<Opened> {
    let screens = list();
    let screen = resolve(&screens, spec)?.clone();
    let exe = default_browser();
    let browser = exe.as_deref().map(stem).unwrap_or_default();
    let is_browser = |w: &AppWindow| {
        let process = w.process.to_lowercase();
        if browser.is_empty() { crate::browser::BROWSERS.iter().any(|b| stem(b) == process) } else { process == browser }
    };
    let before: HashSet<isize> = other_windows().iter().map(|w| w.handle).collect();
    match (exe.as_deref(), new_window_args(&browser, url)) {
        (Some(exe), Some(args)) => {
            let mut command = std::process::Command::new(exe);
            command.args(&args);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            command.spawn().map_err(|e| anyhow!("{}", tr!(format!("Der Browser ließ sich nicht starten: {e}"), format!("The browser couldn't be started: {e}"))))?;
        }
        _ => system::shell_open(url)?,
    }
    let fresh = wait_for_new_window(&before, Duration::from_secs(8), |w| is_browser(w) && !w.title.trim().is_empty());
    let (window, moved_existing) = match fresh {
        Some(window) => (window, false),
        // Kein neues Fenster (der Browser hat einen Tab im vorhandenen geoeffnet): das vordere Browserfenster nehmen.
        None => (other_windows().into_iter().find(|w| is_browser(w)).ok_or_else(|| anyhow!(tr!("Ich finde das Browserfenster nicht.", "I can't find the browser window.")))?, true),
    };
    // Chromium baut das Fenster noch auf; zu fruehes Verschieben wird sonst wieder ueberschrieben.
    std::thread::sleep(Duration::from_millis(350));
    place(window.handle, &screen, Placement::Maximize)?;
    activate(window.handle);
    crate::browser::remember_opened(window.handle);
    Ok(Opened { screen: screen.number, moved_existing })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(left: i32, top: i32, w: i32, h: i32) -> Rect {
        Rect { left, top, right: left + w, bottom: top + h }
    }

    fn two_screens() -> Vec<Screen> {
        // Zweiter Bildschirm links vom Hauptbildschirm, dritter rechts.
        order(vec![(false, r(2560, 0, 1920, 1080), r(2560, 0, 1920, 1040)), (true, r(0, 0, 2560, 1440), r(0, 0, 2560, 1392)), (false, r(-1920, 200, 1920, 1080), r(-1920, 200, 1920, 1040))])
    }

    #[test]
    fn primary_is_number_one_then_left_to_right() {
        let screens = two_screens();
        assert!(screens[0].primary);
        assert_eq!(screens[0].number, 1);
        assert_eq!(screens[1].bounds.left, -1920);
        assert_eq!(screens[2].bounds.left, 2560);
    }

    #[test]
    fn spoken_names_find_the_right_screen() {
        let screens = two_screens();
        let n = |spec: &str| resolve(&screens, spec).map(|s| s.number).unwrap_or(0);
        assert_eq!(n("2"), 2);
        assert_eq!(n("zweiten Bildschirm"), 2);
        assert_eq!(n("auf dem zweiten Monitor"), 2);
        assert_eq!(n("anderen"), 2);
        assert_eq!(n("second"), 2);
        assert_eq!(n(tr!("Hauptbildschirm", "main screen")), 1);
        assert_eq!(n("ersten"), 1);
        assert_eq!(n("dritten"), 3);
        assert_eq!(n("linken Bildschirm"), 2);
        assert_eq!(n("rechts"), 3);
        assert_eq!(n("4"), 0);
        assert!(resolve(&screens, "irgendwo").is_err());
    }

    #[test]
    fn single_screen_explains_itself() {
        let one = order(vec![(true, r(0, 0, 2560, 1440), r(0, 0, 2560, 1392))]);
        let err = resolve(&one, "zweiter").unwrap_err().to_string();
        assert!(err.contains("nur einen Bildschirm"), "{err}");
        assert_eq!(resolve(&one, "haupt").unwrap().number, 1);
        assert!(describe(&one).contains("nur ein Bildschirm"));
    }

    #[test]
    fn description_names_positions() {
        let text = describe(&two_screens());
        assert!(text.contains("3 Bildschirme"), "{text}");
        assert!(text.contains("Bildschirm 2 = links vom Hauptbildschirm (1920×1080)"), "{text}");
        assert!(text.contains("Bildschirm 3 = rechts vom Hauptbildschirm"), "{text}");
    }

    #[test]
    fn windows_fit_on_the_target_screen() {
        let work = r(2560, 0, 1920, 1040);
        let big = centered(work, 2500, 1400);
        assert_eq!((big.width(), big.height()), (1920, 1040));
        let small = centered(work, 800, 600);
        assert_eq!((small.left, small.top, small.width()), (2560 + 560, 220, 800));
        assert_eq!(half(work, true), r(2560, 0, 960, 1040));
        assert_eq!(half(work, false), r(3520, 0, 960, 1040));
    }

    #[test]
    #[ignore = "liest die echten Bildschirme"]
    fn real_screens() {
        let screens = list();
        println!("{}", describe(&screens));
        println!("Standardbrowser: {:?}", default_browser());
        assert!(!screens.is_empty());
        assert!(screens[0].primary);
    }

    #[test]
    #[ignore = "oeffnet ein Browserfenster"]
    fn real_focus_timeline() {
        let before: HashSet<isize> = other_windows().iter().map(|w| w.handle).collect();
        std::process::Command::new(default_browser().unwrap()).args(["--new-window", "https://example.com"]).spawn().unwrap();
        let started = Instant::now();
        let mut handle = 0;
        while started.elapsed() < Duration::from_secs(5) {
            let front = other_windows().into_iter().next().unwrap();
            let new = other_windows().into_iter().find(|w| !before.contains(&w.handle) && w.process.eq_ignore_ascii_case("brave"));
            if let Some(n) = &new {
                handle = n.handle;
            }
            println!("{:>5} ms  vorne: {}  neu: {:?}", started.elapsed().as_millis(), front.title, new.map(|n| n.title));
            std::thread::sleep(Duration::from_millis(250));
        }
        if handle != 0 {
            let _ = crate::system::close_window(handle);
        }
    }

    #[test]
    #[ignore = "oeffnet und bewegt ein Browserfenster"]
    fn real_window_moves() {
        let opened = open_url_on("https://example.com", "1").unwrap();
        assert!(!opened.moved_existing);
        std::thread::sleep(Duration::from_millis(1500));
        let window = other_windows().into_iter().find(|w| w.title.starts_with("Example Domain")).expect("Testfenster");
        let screen = list()[0].clone();
        for (how, name) in [(Placement::LeftHalf, "links"), (Placement::RightHalf, "rechts"), (Placement::Move, "mittig"), (Placement::Maximize, "maximiert")] {
            place(window.handle, &screen, how).unwrap();
            std::thread::sleep(Duration::from_millis(700));
            println!("{name}: {:?}", native::window_rect(window.handle));
        }
        minimize(window.handle);
        std::thread::sleep(Duration::from_millis(700));
        restore(window.handle);
        activate(window.handle);
        std::thread::sleep(Duration::from_millis(700));
        let front = other_windows().into_iter().next().unwrap();
        println!("nach Wiederherstellen vorne: {}", front.title);
        let (page, tabs) = crate::browser::tabs().unwrap();
        println!("Browser-Werkzeuge nehmen: {page}; Tabs: {tabs:?}");
        assert!(page.starts_with("Example Domain"), "{page}");
        crate::system::close_window(window.handle).unwrap();
    }

    #[test]
    #[ignore = "schliesst Testfenster"]
    fn real_cleanup_test_windows() {
        for w in other_windows().into_iter().filter(|w| w.title.starts_with("Example Domain")) {
            println!("schliesse {}", w.title);
            let _ = crate::system::close_window(w.handle);
        }
    }

    #[test]
    #[ignore = "liest die echten Fenster"]
    fn real_windows() {
        for w in other_windows() {
            println!("{:>10} {:<12} {:?} {}", w.handle, w.process, screen_of(&list(), w.handle).map(|s| s.number), w.title);
        }
    }

    #[test]
    #[ignore = "oeffnet ein Browserfenster"]
    fn real_open_on_screen() {
        // Reste frueherer Laeufe wegraeumen.
        for w in other_windows().into_iter().filter(|w| w.title.starts_with("Example Domain")) {
            let _ = crate::system::close_window(w.handle);
        }
        // Brave holt nach dem Schliessen verzoegert ein anderes Fenster nach vorne; das abwarten.
        std::thread::sleep(Duration::from_millis(2500));
        let opened = open_url_on("https://example.com", "haupt").unwrap();
        println!("Bildschirm {}, vorhandenes Fenster: {}", opened.screen, opened.moved_existing);
        std::thread::sleep(Duration::from_secs(2));
        let (page, tabs) = crate::browser::tabs().unwrap();
        println!("Vorne: {page}; Tabs: {tabs:?}");
        let front = other_windows().into_iter().next().unwrap();
        println!("Vorderstes Fenster: {} ({}) auf {:?}", front.title, front.process, screen_of(&list(), front.handle).map(|s| s.number));
        assert!(front.title.contains("Example"));
        crate::system::close_window(front.handle).unwrap();
    }

    #[test]
    fn any_browser_counts_as_the_browser() {
        assert!(is_browser_word("browser"));
        assert!(is_browser_word("meinen Browser"));
        assert!(is_browser_word("Internet"));
        assert!(!is_browser_word("brave"));
        assert!(is_browser_process("brave", None));
        assert!(is_browser_process("Brave", None));
        assert!(is_browser_process("arc", Some("arc")));
        assert!(!is_browser_process("discord", Some("brave")));
        assert_eq!(browser_name("msedge"), "Edge");
        assert_eq!(browser_name("brave"), "Brave");
    }

    #[test]
    fn browser_arguments() {
        assert_eq!(stem("C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe"), "brave");
        assert_eq!(new_window_args("brave", "https://x.de").unwrap(), vec!["--new-window", "https://x.de"]);
        assert_eq!(new_window_args("firefox", "https://x.de").unwrap()[0], "-new-window");
        assert!(new_window_args("unbekannt", "https://x.de").is_none());
    }
}
