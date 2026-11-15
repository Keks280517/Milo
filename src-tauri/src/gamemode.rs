//! Spielemodus: Solange ein Spiel (oder ein anderes anspruchsvolles Programm) LAEUFT, zieht sich
//! Milo vollstaendig zurueck: Fenster geschlossen, Sprachmodell entladen, Mikrofon zu, keine
//! Abfragen mehr. Es bleibt nur dieser kleine Kern, der alle paar Sekunden nachsieht, ob das
//! Spiel noch laeuft. Ob das Spiel im Vordergrund ist, spielt keine Rolle (Heraustabben weckt nicht).
//!
//! Erkannt wird ueber die Liste der laufenden Programme, deren Dateipfad, Fenstertitel und
//! Speicherverbrauch (dieselben Angaben, die der Task-Manager zeigt). Drei Wege:
//! 1. bekannte Spiele und Spieleordner (Steam, Epic, ...),
//! 2. Minecraft Java mit und ohne Mods (laeuft als javaw.exe, erkannt an Ordner oder Fenstertitel),
//! 3. allgemein: ein Programm im Vordergrund, das den Bildschirm fuellt bzw. sehr viel Arbeitsspeicher
//!    belegt und kein bekanntes Arbeitsprogramm ist. So muss nicht jedes Spiel auf eine Liste.
//! In kein Programm wird eingegriffen: keine Injektion, kein Hooking, kein Lesen fremden Speichers.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::SettingsStore;
use crate::{media, overlay, voice};

pub const EVENT: &str = "milo://gamemode";

/// Ordner, in denen Spiele-Plattformen ihre Spiele ablegen.
const GAME_FOLDERS: [&str; 12] = [
    r"\steamapps\common\", r"\epic games\", r"\riot games\", r"\xboxgames\", r"\gog galaxy\games\", r"\gog games\",
    r"\ubisoft game launcher\games\", r"\ea games\", r"\origin games\", r"\battle.net\games\", r"\rockstar games\", r"\games\",
];

/// Programme, die in solchen Ordnern liegen, aber keine Spiele sind (Hintergrundbilder, Werkzeuge, Launcher).
const NEVER: [&str; 31] = [
    "wallpaper32.exe", "wallpaper64.exe", "webwallpaper32.exe", "ui32.exe", "wallpaperservice32.exe", "applicationwallpaperinject64.exe",
    "steam.exe", "steamwebhelper.exe", "steamservice.exe", "gameoverlayui.exe", "gameoverlayui64.exe", "epicgameslauncher.exe",
    "epicwebhelper.exe", "eosoverlayrenderer-win64-shipping.exe", "riotclientservices.exe", "riotclientux.exe", "riotclientuxrender.exe",
    "riotclientcrashhandler.exe", "riot client.exe", "vgtray.exe", "galaxyclient.exe", "galaxyclient helper.exe", "upc.exe", "ubisoftconnect.exe",
    "eadesktop.exe", "eabackgroundservice.exe", "battle.net.exe", "launcher.exe", "crashreportclient.exe", "unitycrashhandler64.exe",
    "soundpad.exe",
];

/// Spiele und VR-Dienste, die auch ausserhalb der ueblichen Ordner liegen koennen.
/// Nicht dabei: der Dienst der Meta-Quest-Software (OVRServer_x64.exe). Er laeuft dauernd im
/// Hintergrund, auch ohne VR, und hat im Live-Test sofort einen Fehlalarm ausgeloest.
const ALWAYS: [&str; 10] = [
    "vrserver.exe", "vrmonitor.exe", "vrcompositor.exe", "minecraft.windows.exe", "minecraftlauncher.exe", "robloxplayerbeta.exe", "cs2.exe",
    "valorant-win64-shipping.exe", "fortniteclient-win64-shipping.exe", "gta5.exe",
];

/// Minecraft (Java Edition) und alle Modpacks laufen als javaw.exe oder java.exe.
const JAVA: [&str; 2] = ["javaw.exe", "java.exe"];

/// Ordner der Minecraft-Launcher (offizieller Launcher, CurseForge, Modrinth, Prism, MultiMC, ATLauncher,
/// GDLauncher, TLauncher, Lunar, Feather, NoRisk). Liegt Java dort, laeuft fast sicher Minecraft.
const MINECRAFT_FOLDERS: [&str; 14] = [
    r"\norisk\",
    r"\.minecraft\", r"\minecraft launcher\", r"\curseforge\minecraft\", r"\com.modrinth.theseus\", r"\modrinthapp\", r"\prismlauncher\",
    r"\multimc\", r"\atlauncher\", r"\gdlauncher", r"\.tlauncher\", r"\.lunarclient\", r"\feather\", r"\minecraft\runtime\",
];

/// Ist das ein Java-Prozess, der Minecraft spielt? `titles` sind die Fenstertitel dieses Prozesses.
pub fn is_minecraft(path: &str, name: &str, titles: &[String]) -> bool {
    if !JAVA.contains(&name) {
        return false;
    }
    MINECRAFT_FOLDERS.iter().any(|folder| path.contains(folder)) || titles.iter().any(|t| t.to_lowercase().contains("minecraft"))
}

/// Programme, die zwar viel Speicher brauchen oder Vollbild zeigen, aber keine Spiele sind:
/// Browser, Videoplayer, Praesentationen, Entwickler- und Kreativwerkzeuge, virtuelle Maschinen.
const HEAVY_NEVER: [&str; 53] = [
    "explorer.exe", "brave.exe", "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "opera_gx.exe", "vivaldi.exe", "arc.exe",
    "zen.exe", "vlc.exe", "mpc-hc64.exe", "mpc-be64.exe", "potplayermini64.exe", "mpv.exe", "wmplayer.exe", "microsoft.media.player.exe",
    "applicationframehost.exe", "netflix.exe", "spotify.exe", "discord.exe", "teams.exe", "ms-teams.exe", "zoom.exe", "slack.exe",
    "powerpnt.exe", "winword.exe", "excel.exe", "outlook.exe", "onenote.exe", "acrord32.exe", "acrobat.exe", "code.exe", "cursor.exe",
    "devenv.exe", "idea64.exe", "rider64.exe", "pycharm64.exe", "studio64.exe", "android studio.exe", "blender.exe", "photoshop.exe",
    "afterfx.exe", "adobe premiere pro.exe", "resolve.exe", "obs64.exe", "vmware-vmx.exe", "virtualboxvm.exe", "vmmem", "docker desktop.exe",
    "claude.exe", "ollama.exe", "msedgewebview2.exe",
];

/// Ab so viel Arbeitsspeicher gilt ein Vollbild-Programm im Vordergrund als Spiel.
const FULLSCREEN_MB: u64 = 600;
/// Ab so viel Arbeitsspeicher gilt auch ein normales Fenster im Vordergrund als Spiel.
const WINDOWED_MB: u64 = 2500;

/// Allgemeine Erkennung: Ist das Programm im Vordergrund vermutlich ein Spiel?
pub fn is_heavy_game(name: &str, fullscreen: bool, memory_mb: u64, user_never: &[String]) -> bool {
    if name.is_empty() || user_never.iter().any(|e| e.eq_ignore_ascii_case(name)) || HEAVY_NEVER.contains(&name) || NEVER.contains(&name) {
        return false;
    }
    if name.starts_with("milo") {
        return false;
    }
    (fullscreen && memory_mb >= FULLSCREEN_MB) || memory_mb >= WINDOWED_MB
}

/// Ist dieses Programm ein Spiel? `path` und `name` in Kleinbuchstaben.
/// Die Listen des Nutzers gehen vor, dann die eingebauten Ausnahmen, dann Namen und Ordner.
pub fn is_game(path: &str, name: &str, user_always: &[String], user_never: &[String]) -> bool {
    let listed = |list: &[String]| list.iter().any(|entry| entry.eq_ignore_ascii_case(name));
    if listed(user_never) {
        return false;
    }
    if listed(user_always) {
        return true;
    }
    if NEVER.contains(&name) {
        return false;
    }
    ALWAYS.contains(&name) || GAME_FOLDERS.iter().any(|folder| path.contains(folder))
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameStatus {
    /// Milo schlaeft gerade wegen eines laufenden Spiels.
    pub sleeping: bool,
    /// Dateiname des Programms, das den Schlaf ausgeloest hat.
    pub game: String,
}

#[derive(Default)]
pub struct GameMode {
    status: Mutex<GameStatus>,
    /// Der Nutzer hat Milo trotz laufendem Spiel geweckt; gilt, bis dieses Spiel endet.
    override_for: Mutex<String>,
    pub sleeping: AtomicBool,
}

impl GameMode {
    pub fn status(&self) -> GameStatus {
        self.status.lock().unwrap().clone()
    }
}

#[cfg(windows)]
mod native {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

    /// Laufende Programme als (Prozessnummer, Dateiname in Kleinbuchstaben).
    pub fn processes() -> Vec<(u32, String)> {
        let mut list = Vec::new();
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return list;
            };
            let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                    list.push((entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase()));
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        list
    }

    /// Dateipfad eines Programms in Kleinbuchstaben. Fragt nur die Auskunft ab, die auch der
    /// Task-Manager zeigt (kein Lese- oder Schreibzugriff auf das Programm selbst).
    pub fn path_of(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 1024];
            let mut len = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len);
            let _ = CloseHandle(handle);
            result.ok()?;
            Some(String::from_utf16_lossy(&buffer[..len as usize]).to_lowercase())
        }
    }

    /// Belegter Arbeitsspeicher eines Programms in MB (die Zahl aus dem Task-Manager).
    pub fn memory_mb(pid: u32) -> Option<u64> {
        use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        use windows::Win32::System::Threading::PROCESS_VM_READ;
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid)
                .or_else(|_| OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid))
                .ok()?;
            let mut counters = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
            let result = GetProcessMemoryInfo(handle, &mut counters, counters.cb);
            let _ = CloseHandle(handle);
            result.ok()?;
            Some(counters.WorkingSetSize as u64 / (1024 * 1024))
        }
    }

    /// Prozess des Vordergrundfensters und ob es seinen Bildschirm ohne Rahmen ganz ausfuellt.
    pub fn foreground() -> Option<(u32, bool)> {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
        use windows::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, GWL_STYLE, WS_CAPTION,
        };
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let mut class = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut class).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..len]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "XamlExplorerHostIslandWindow") {
                return Some((pid, false));
            }
            let captioned = GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION.0 == WS_CAPTION.0;
            let mut rect = RECT::default();
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if GetWindowRect(hwnd, &mut rect).is_err() || !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Some((pid, false));
            }
            let m = info.rcMonitor;
            let covers = rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom;
            Some((pid, covers && !captioned))
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub fn processes() -> Vec<(u32, String)> {
        Vec::new()
    }
    pub fn path_of(_pid: u32) -> Option<String> {
        None
    }
    pub fn memory_mb(_pid: u32) -> Option<u64> {
        None
    }
    pub fn foreground() -> Option<(u32, bool)> {
        None
    }
}

/// Vollstaendiger Pfad eines laufenden Programms in Kleinbuchstaben.
pub fn exe_path(pid: u32) -> Option<String> {
    native::path_of(pid)
}

/// Dateiname eines laufenden Programms in Kleinbuchstaben (z. B. "brave.exe").
pub fn process_name(pid: u32) -> Option<String> {
    native::path_of(pid).and_then(|path| path.rsplit('\\').next().map(String::from))
}

/// Was der Waechter sich zwischen zwei Durchlaeufen merkt.
#[derive(Default)]
struct Scan {
    /// Bereits abgefragte Dateipfade je Prozessnummer.
    paths: HashMap<u32, String>,
    /// Programme, die die allgemeine Erkennung als Spiel eingestuft hat. Gilt, bis das Programm
    /// endet (Heraustabben weckt Milo also nicht).
    heavy: HashSet<u32>,
}

/// Sucht unter den laufenden Programmen ein Spiel.
fn find_game(scan: &mut Scan, always: &[String], never: &[String]) -> Option<String> {
    let running = native::processes();
    scan.paths.retain(|pid, _| running.iter().any(|(p, _)| p == pid));
    scan.heavy.retain(|pid| running.iter().any(|(p, _)| p == pid));
    let own = std::process::id();
    let mut java = Vec::new();
    for (pid, name) in &running {
        if *pid <= 4 || *pid == own {
            continue;
        }
        let path = scan.paths.entry(*pid).or_insert_with(|| native::path_of(*pid).unwrap_or_default());
        if is_game(path, name, always, never) {
            return Some(name.clone());
        }
        if JAVA.contains(&name.as_str()) && !never.iter().any(|e| e.eq_ignore_ascii_case(name)) {
            java.push((*pid, name.clone(), path.clone()));
        }
    }
    // Minecraft Java (auch mit Mods): am Launcher-Ordner oder am Fenstertitel "Minecraft ...".
    if !java.is_empty() {
        let windows = crate::system::windows();
        for (pid, name, path) in &java {
            let titles: Vec<String> = windows.iter().filter(|w| w.pid == *pid).map(|w| w.title.clone()).collect();
            if is_minecraft(path, name, &titles) {
                return Some(name.clone());
            }
        }
    }
    // Schon frueher als Spiel erkannt und noch offen?
    if let Some((_, name)) = running.iter().find(|(pid, _)| scan.heavy.contains(pid)) {
        if !never.iter().any(|e| e.eq_ignore_ascii_case(name)) {
            return Some(name.clone());
        }
    }
    // Allgemein: das Programm im Vordergrund fuellt den Bildschirm oder braucht sehr viel Speicher.
    let (pid, fullscreen) = native::foreground()?;
    if pid <= 4 || pid == own {
        return None;
    }
    let name = running.iter().find(|(p, _)| *p == pid).map(|(_, n)| n.clone())?;
    let memory = native::memory_mb(pid).unwrap_or(0);
    if is_heavy_game(&name, fullscreen, memory, never) {
        log::info!("Spielemodus: {name} wirkt wie ein Spiel ({memory} MB, Vollbild: {fullscreen})");
        scan.heavy.insert(pid);
        return Some(name);
    }
    None
}

/// Schickt Milo schlafen: alles freigeben, was Speicher oder Rechenzeit kostet.
async fn sleep(app: &AppHandle, game: &str) {
    log::info!("Spielemodus an: {game} läuft");
    let state = app.state::<GameMode>();
    state.sleeping.store(true, Ordering::SeqCst);
    *state.status.lock().unwrap() = GameStatus { sleeping: true, game: game.to_string() };
    app.state::<media::Media>().suspended.store(true, Ordering::Relaxed);
    app.state::<voice::Voice>().suspend(true);
    app.state::<crate::tts::Speaker>().stop();
    let _ = app.emit(EVENT, state.status());
    // Der Oberflaeche einen Augenblick fuer den Abschied lassen, dann das Fenster ganz schliessen.
    tokio::time::sleep(Duration::from_millis(900)).await;
    if let Some(win) = app.get_webview_window(overlay::OVERLAY_LABEL) {
        let _ = win.destroy();
    }
    crate::pet::close(app);
    crate::desk::close(app);
    crate::settings_window::close(app);
    crate::widgets::close_all(app);
    update_tray(app);
}

/// Weckt Milo wieder auf: Fenster neu aufbauen, Dienste fortsetzen.
pub fn wake(app: &AppHandle) {
    let state = app.state::<GameMode>();
    if !state.sleeping.swap(false, Ordering::SeqCst) {
        return;
    }
    log::info!("Spielemodus aus");
    *state.status.lock().unwrap() = GameStatus::default();
    app.state::<media::Media>().suspended.store(false, Ordering::Relaxed);
    app.state::<voice::Voice>().suspend(false);
    let handle = app.clone();
    // Fenster muessen im Hauptthread entstehen.
    let _ = app.run_on_main_thread(move || {
        if handle.get_webview_window(overlay::OVERLAY_LABEL).is_none() {
            let config = handle.config().app.windows.iter().find(|w| w.label == overlay::OVERLAY_LABEL).cloned();
            let built = config.map(|c| tauri::WebviewWindowBuilder::from_config(&handle, &c).and_then(|b| b.build()));
            match built {
                Some(Ok(win)) => {
                    let _ = overlay::place_top_center(&win);
                }
                Some(Err(err)) => log::error!("Die Insel ließ sich nicht wieder öffnen: {err}"),
                None => log::error!("Fensterbeschreibung fehlt"),
            }
        }
        crate::widgets::open_all(&handle);
        crate::desk::open_if_needed(&handle);
        update_tray(&handle);
    });
}

/// Der Nutzer will Milo trotz laufendem Spiel benutzen (Tray). Gilt, bis dieses Spiel endet.
pub fn wake_anyway(app: &AppHandle) {
    let state = app.state::<GameMode>();
    let game = state.status().game;
    if !game.is_empty() {
        *state.override_for.lock().unwrap() = game;
    }
    wake(app);
}

/// Das ausloesende Programm ist kein Spiel: dauerhaft merken und aufwachen.
pub fn not_a_game(app: &AppHandle) {
    let game = app.state::<GameMode>().status().game;
    if game.is_empty() {
        return;
    }
    let store = app.state::<SettingsStore>();
    let mut settings = store.get();
    if !settings.games_never.iter().any(|g| g.eq_ignore_ascii_case(&game)) {
        settings.games_never.push(game.clone());
        if let Err(err) = store.set(settings) {
            log::warn!("{err}");
        }
    }
    log::info!("{game} gilt ab jetzt nicht mehr als Spiel");
    wake(app);
}

/// Passt Tooltip und Menue des Symbols in der Taskleiste an den Schlafzustand an.
fn update_tray(app: &AppHandle) {
    let status = app.state::<GameMode>().status();
    if let Some(tray) = app.tray_by_id("milo") {
        let tip = if status.sleeping { tr!(format!("Milo schläft, weil {} läuft", status.game), format!("Milo is asleep because {} is running", status.game)) } else { "Milo".to_string() };
        let _ = tray.set_tooltip(Some(tip));
    }
    crate::refresh_tray_menu(app, &status);
}

/// Beobachtet die laufenden Programme. Wach: alle 4 Sekunden. Schlafend: alle 6 Sekunden.
pub fn spawn_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut scan = Scan::default();
        // Erst beim zweiten Fund hintereinander schlafen gehen (kurzlebige Hilfsprogramme zaehlen nicht).
        let mut seen: Option<String> = None;
        loop {
            let state = app.state::<GameMode>();
            let sleeping = state.sleeping.load(Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(if sleeping { 6 } else { 4 })).await;
            let settings = app.state::<SettingsStore>().get();
            if !settings.game_mode {
                if sleeping {
                    wake(&app);
                }
                seen = None;
                continue;
            }
            let (always, never) = (settings.games_always.clone(), settings.games_never.clone());
            let found = tokio::task::spawn_blocking({
                let mut taken = std::mem::take(&mut scan);
                move || {
                    let game = find_game(&mut taken, &always, &never);
                    (taken, game)
                }
            })
            .await;
            let Ok((kept, game)) = found else { continue };
            scan = kept;

            // Die Ausnahme "trotzdem wach" endet mit dem Spiel, fuer das sie galt.
            let overridden = state.override_for.lock().unwrap().clone();
            if !overridden.is_empty() && game.as_deref() != Some(overridden.as_str()) {
                state.override_for.lock().unwrap().clear();
            }
            match game {
                Some(game) if !sleeping => {
                    if overridden == game {
                        continue;
                    }
                    if seen.as_deref() == Some(game.as_str()) {
                        sleep(&app, &game).await;
                        seen = None;
                    } else {
                        seen = Some(game);
                    }
                }
                Some(game) => {
                    // Schlaeft schon; nur den Namen aktuell halten, falls ein anderes Spiel uebernommen hat.
                    if state.status().game != game {
                        *state.status.lock().unwrap() = GameStatus { sleeping: true, game };
                        update_tray(&app);
                    }
                }
                None => {
                    seen = None;
                    if sleeping {
                        wake(&app);
                    }
                }
            }
        }
    });
}

#[tauri::command]
pub fn game_status(state: tauri::State<'_, GameMode>) -> GameStatus {
    state.status()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(path: &str, name: &str) -> bool {
        is_game(path, name, &[], &[])
    }

    /// Nur von Hand: welches laufende Programm haelt der Spielemodus gerade fuer ein Spiel?
    #[test]
    #[ignore]
    fn live_detection() {
        let mut scan = super::Scan::default();
        for (pid, name) in super::native::processes() {
            let path = super::native::path_of(pid).unwrap_or_default();
            if super::is_game(&path, &name, &[], &[]) {
                println!("SPIEL: {name} | {path}");
            }
            scan.paths.insert(pid, path);
        }
        println!("Vordergrund: {:?}", super::native::foreground());
        println!("find_game: {:?}", super::find_game(&mut scan, &[], &[]));
    }

    #[test]
    fn games_are_found_by_folder_or_name() {
        assert!(game(r"d:\steamlibrary\steamapps\common\counter-strike global offensive\game\bin\win64\cs2.exe", "cs2.exe"));
        assert!(game(r"c:\program files\epic games\fortnite\fortnitegame\binaries\win64\fortniteclient-win64-shipping.exe", "fortniteclient-win64-shipping.exe"));
        assert!(game(r"c:\riot games\valorant\live\shootergame\binaries\win64\valorant-win64-shipping.exe", "valorant-win64-shipping.exe"));
        assert!(game(r"c:\program files (x86)\steam\steamapps\common\steamvr\bin\win64\vrserver.exe", "vrserver.exe"));
        assert!(game(r"c:\irgendwo\cs2.exe", "cs2.exe"));
        assert!(!game(r"c:\program files\meta horizon\support\oculus-runtime\ovrserver_x64.exe", "ovrserver_x64.exe"));
        assert!(!game(r"c:\riot games\riot client\riotclientservices.exe", "riotclientservices.exe"));
        // Der Launcher von Riot (Electron) liegt im Spieleordner, ist aber kein Spiel.
        assert!(!game(r"c:\riot games\riot client\riotclientelectron\riot client.exe", "riot client.exe"));
    }

    #[test]
    fn everyday_programs_and_launchers_are_not_games() {
        assert!(!game(r"c:\program files\bravesoftware\brave-browser\application\brave.exe", "brave.exe"));
        assert!(!game(r"c:\users\x\appdata\roaming\spotify\spotify.exe", "spotify.exe"));
        assert!(!game(r"c:\program files (x86)\steam\steam.exe", "steam.exe"));
        assert!(!game(r"c:\program files (x86)\steam\bin\cef\cef.win7x64\steamwebhelper.exe", "steamwebhelper.exe"));
        // Wallpaper Engine liegt im Spieleordner von Steam, ist aber kein Spiel.
        assert!(!game(r"c:\program files (x86)\steam\steamapps\common\wallpaper_engine\wallpaper64.exe", "wallpaper64.exe"));
        assert!(!game(r"c:\program files\epic games\launcher\portal\binaries\win64\epicgameslauncher.exe", "epicgameslauncher.exe"));
        assert!(!game("", "system"));
    }

    #[test]
    fn minecraft_java_and_modpacks_are_found() {
        let none: Vec<String> = vec![];
        // Offizieller Launcher, CurseForge-Modpack, Modrinth
        assert!(is_minecraft(r"c:\users\x\appdata\roaming\.minecraft\runtime\java-runtime-gamma\bin\javaw.exe", "javaw.exe", &none));
        assert!(is_minecraft(r"c:\users\x\curseforge\minecraft\install\runtime\java-runtime-gamma\bin\javaw.exe", "javaw.exe", &none));
        assert!(is_minecraft(r"c:\users\x\appdata\roaming\com.modrinth.theseus\meta\java_versions\zulu17\bin\javaw.exe", "javaw.exe", &none));
        // NoRisk Client (eigener Launcher mit Modpack-Profilen)
        assert!(is_minecraft(r"c:\users\x\appdata\roaming\norisk\noriskclientv3\data\runtime\java\bin\javaw.exe", "javaw.exe", &none));
        // Java aus einem beliebigen Ordner (z. B. Prism mit eigenem Java): am Fenstertitel
        assert!(is_minecraft(r"c:\program files\eclipse adoptium\jdk-21\bin\javaw.exe", "javaw.exe", &["Minecraft* 1.20.1 - Singleplayer".to_string()]));
        // Anderes Java-Programm ist kein Minecraft
        assert!(!is_minecraft(r"c:\program files\eclipse adoptium\jdk-21\bin\javaw.exe", "javaw.exe", &["JDownloader 2".to_string()]));
        assert!(!is_minecraft(r"c:\users\x\.minecraft\x.exe", "x.exe", &none));
    }

    #[test]
    fn heavy_programs_count_as_games_unless_they_are_work_tools() {
        assert!(is_heavy_game("eldenring.exe", true, 4000, &[]));
        assert!(is_heavy_game("indiegame.exe", true, 800, &[]));
        assert!(is_heavy_game("windowedgame.exe", false, 3500, &[]));
        assert!(!is_heavy_game("smallapp.exe", false, 900, &[]));
        assert!(!is_heavy_game("smallfullscreen.exe", true, 200, &[]));
        // Browser im Vollbild (Video) und grosse Arbeitsprogramme nicht
        assert!(!is_heavy_game("chrome.exe", true, 3000, &[]));
        assert!(!is_heavy_game("blender.exe", false, 6000, &[]));
        assert!(!is_heavy_game("milo.exe", true, 3000, &[]));
        // Was der Nutzer als "kein Spiel" markiert hat, bleibt draussen
        assert!(!is_heavy_game("eldenring.exe", true, 4000, &["EldenRing.exe".to_string()]));
    }

    #[test]
    fn user_lists_win_over_built_in_rules() {
        let always = vec!["Blender.exe".to_string()];
        let never = vec!["cs2.exe".to_string()];
        assert!(is_game(r"c:\program files\blender\blender.exe", "blender.exe", &always, &[]));
        assert!(!is_game(r"d:\steamapps\common\cs\cs2.exe", "cs2.exe", &[], &never));
        // Steht etwas in beiden Listen, gewinnt "kein Spiel".
        assert!(!is_game("", "blender.exe", &always, &always));
    }
}
