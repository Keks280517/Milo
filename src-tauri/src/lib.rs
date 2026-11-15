#[macro_use]
mod i18n;
mod actionlog;
mod agent;
mod agenda;
mod apps;
mod audio;
mod browse;
mod browser;
mod claude;
mod claudechat;
mod browser_bridge;
mod confirm;
mod daily;
mod desk;
mod docs;
mod files;
mod gamemode;
mod gemini;
mod groq;
mod guard;
mod lifecycle;
mod limits;
mod local;
mod media;
mod memory;
mod migrate;
mod monitor;
mod overlay;
mod pc;
mod pulse;
mod alarms;
mod pet;
mod power;
mod quick;
mod quick_en;
mod richtext;
mod router;
mod screens;
mod scripts;
mod settings;
mod settings_window;
mod sound;
mod system;
mod textwork;
mod timers;
mod tools;
mod tts;
mod twitch;
mod voice;
mod web;
mod widgets;
mod workspaces;
mod youtube;

use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};

use memory::{Action, Memory, Note};
use settings::{Settings, SettingsStore};

#[tauri::command]
fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

#[tauri::command]
fn save_settings(app: AppHandle, store: State<'_, SettingsStore>, settings: Settings) -> Result<Settings, String> {
    let before = store.get();
    let saved = store.set(settings)?;
    // Alle Fenster (Insel, Einstellungen, Desktop-Ebene) uebernehmen den neuen Stand.
    let _ = app.emit("milo://settings", &saved);
    if saved.hotkey != before.hotkey || saved.text_hotkey != before.text_hotkey || saved.note_hotkey != before.note_hotkey {
        register_hotkeys(&app, &saved)?;
    }
    if saved.wake_word != before.wake_word || saved.wake_level != before.wake_level || saved.stt_model != before.stt_model {
        app.state::<voice::Voice>().refresh();
    }
    Ok(saved)
}

/// Meldet die Tastenkombinationen fuers Zuhoeren und fuer die Text-Werkzeuge bei Windows an
/// (und die alten ab).
fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let mut problems = Vec::new();
    for hotkey in [&settings.hotkey, &settings.text_hotkey, &settings.note_hotkey] {
        if !hotkey.is_empty() {
            if let Err(e) = shortcuts.register(hotkey.as_str()) {
                problems.push(tr!(format!("Die Tastenkombination „{hotkey}“ lässt sich nicht verwenden ({e}). Vielleicht belegt sie schon ein anderes Programm."), format!("The shortcut \"{hotkey}\" can't be used ({e}). Another app may already be using it.")));
            }
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join(" ")) }
}

// ---------- Gedaechtnis: ansehen, bearbeiten, loeschen ----------

#[tauri::command]
fn list_notes(memory: State<'_, Memory>, query: String) -> Result<Vec<Note>, String> {
    memory.search_notes(&query, 200).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_note(memory: State<'_, Memory>, text: String) -> Result<i64, String> {
    if text.trim().is_empty() {
        return Err(tr!("Die Notiz ist leer.", "The note is empty.").into());
    }
    memory.add_note(&text).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_note(memory: State<'_, Memory>, id: i64, text: String) -> Result<bool, String> {
    if text.trim().is_empty() {
        return Err(tr!("Die Notiz ist leer.", "The note is empty.").into());
    }
    memory.update_note(id, &text).map_err(|e| e.to_string())
}

/// Formatierte Notiz anlegen (id leer) oder aendern. Das HTML wird hier bereinigt, egal was die
/// Oberflaeche schickt.
#[tauri::command]
fn save_rich_note(memory: State<'_, Memory>, id: Option<i64>, text: String, rich: String) -> Result<i64, String> {
    if text.trim().is_empty() {
        return Err(tr!("Die Notiz ist leer.", "The note is empty.").into());
    }
    memory.save_rich_note(id, &text, &richtext::sanitize(&rich)).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_note(memory: State<'_, Memory>, id: i64) -> Result<bool, String> {
    memory.delete_note(id).map_err(|e| e.to_string())
}

/// Protokoll: was der Assistent zuletzt getan hat.
#[tauri::command]
fn list_actions(memory: State<'_, Memory>) -> Result<Vec<Action>, String> {
    memory.recent_actions(200).map_err(|e| e.to_string())
}

// ---------- Dateien ----------

/// Ordner, in dem Milo seine Kopien der erhaltenen Dateien aufbewahrt.
pub fn files_dir(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap_or_default().join("Dateien")
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReport {
    files: Vec<files::StoredFile>,
    /// Was nicht angenommen werden konnte, als lesbare Meldungen.
    errors: Vec<String>,
}

/// Nimmt hineingezogene Dateien an: kopieren, Text auslesen, merken.
#[tauri::command]
async fn import_files(app: AppHandle, paths: Vec<String>) -> Result<ImportReport, String> {
    // Kopieren und Auslesen kann dauern, deshalb nicht im Oberflaechen-Thread.
    let handle = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let memory = app.state::<Memory>();
        let dir = files_dir(&app);
        let mut report = ImportReport { files: Vec::new(), errors: Vec::new() };
        for path in paths.iter().take(20) {
            match files::import(&memory, &dir, std::path::Path::new(path)) {
                Ok(file) => {
                    memory.log_action("file_in", &tr!(format!("Datei „{}“ angenommen", file.name), format!("Received the file \"{}\"", file.name)), true);
                    report.files.push(file);
                }
                Err(err) => report.errors.push(format!("{err:#}")),
            }
        }
        report
    })
    .await
    .map_err(|e| e.to_string())?;

    // Das Modell muss wissen, dass gerade Dateien kamen; sonst bezieht es "fass es zusammen"
    // auf das bisherige Gespraech statt auf die neue Datei.
    if !report.files.is_empty() {
        let names: Vec<String> = report.files.iter().map(|f| format!("„{}“", f.name)).collect();
        handle
            .state::<agent::Agent>()
            .remember(
                &format!(
                    "Der Nutzer hat dir soeben diese Datei(en) gegeben: {}. Sagt er jetzt „es“, „das“ oder „die Datei“, meint er genau diese. \
Lies sie mit read_file (bei langen Dateien alle Abschnitte), bevor du antwortest oder zusammenfasst.",
                    names.join(", ")
                ),
                tr!("Ich habe die Datei(en) bekommen.", "Got the file(s)."),
            )
            .await;
    }
    Ok(report)
}

#[tauri::command]
fn list_files(memory: State<'_, Memory>) -> Result<Vec<files::StoredFile>, String> {
    files::list(&memory, 200).map_err(|e| e.to_string())
}

/// Loescht Milos Kopie einer Datei (nie das Original).
#[tauri::command]
fn delete_file(app: AppHandle, memory: State<'_, Memory>, id: i64) -> Result<bool, String> {
    let result = files::remove(&memory, &files_dir(&app), id).map_err(|e| format!("{e:#}"));
    // Auch das Loeschen steht im Protokoll, damit nachvollziehbar bleibt, was wann verschwand.
    memory.log_action("file_delete", &tr!(format!("Kopie Nr. {id} aus der Ablage gelöscht"), format!("Deleted copy no. {id} from the shelf")), result.is_ok());
    result
}

/// Wetter fuer die Wetter-Ansicht, ohne Umweg ueber das Modell.
#[tauri::command]
async fn fetch_weather(place: String) -> Result<web::Weather, String> {
    web::weather(&place).await.map_err(|e| e.to_string())
}

/// Bricht ein angekuendigtes Herunterfahren, einen Neustart oder ein geplantes Energiesparen ab (Knopf in der Insel).
#[tauri::command]
fn abort_power(app: AppHandle, memory: State<'_, Memory>) -> Result<(), String> {
    match power::cancel(&app) {
        Some(what) => {
            memory.log_action("system_power", &tr!(format!("{} abgebrochen (Knopf)", what.map(|a| a.label()).unwrap_or(tr!("Geplanter Befehl", "Scheduled command"))), format!("{} cancelled (button)", what.map(|a| a.label()).unwrap_or(tr!("Geplanter Befehl", "Scheduled command")))), true);
            Ok(())
        }
        None => Err(tr!("Es war nichts geplant.", "Nothing was scheduled.").into()),
    }
}

// ---------- Gemini ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudStatus {
    has_key: bool,
    /// Der Schluessel wurde gerade erfolgreich bei Google geprueft.
    verified: bool,
    message: String,
}

#[tauri::command]
fn cloud_status() -> CloudStatus {
    let has_key = gemini::has_key();
    CloudStatus { has_key, verified: false, message: String::new() }
}

/// Speichert den Gemini-Schluessel in der Windows-Anmeldeinformationsverwaltung und prueft ihn.
/// Ein abgelehnter Schluessel wird nicht behalten.
#[tauri::command]
async fn set_gemini_key(app: AppHandle, key: String) -> Result<CloudStatus, String> {
    gemini::store_key(&key).map_err(|e| e.to_string())?;
    drop(key);
    let store = app.state::<SettingsStore>();
    match app.state::<gemini::Gemini>().models().await {
        Ok(models) => {
            // Eingestellte Modelle, die es fuer diesen Schluessel nicht gibt, durch die neuesten passenden ersetzen.
            let mut settings = store.get();
            let before = settings.clone();
            if let Some(easy) = gemini::choose_model(&models, &settings.model_easy, true) {
                settings.model_easy = easy;
            }
            if let Some(hard) = gemini::choose_model(&models, &settings.model_hard, false) {
                settings.model_hard = hard;
            }
            if !models.contains(&settings.model_hard_2) {
                // Das zweite Flash-Modell ist nur ein Zusatzkontingent; fehlt es, gilt das erste doppelt.
                settings.model_hard_2 = settings.model_hard.clone();
            }
            if settings != before {
                store.set(settings)?;
            }
            let settings = store.get();
            app.state::<Memory>().log_action("cloud", tr!("Gemini-Schlüssel hinterlegt und geprüft", "Gemini key added and verified"), true);
            Ok(CloudStatus {
                has_key: true,
                verified: true,
                message: tr!(format!("Schlüssel funktioniert. Leicht: {}, schwer: {} und {}.", settings.model_easy, settings.model_hard, settings.model_hard_2), format!("The key works. Simple: {}, hard: {} and {}.", settings.model_easy, settings.model_hard, settings.model_hard_2)),
            })
        }
        Err(err) => {
            let message = err.to_string();
            if message.contains("nicht angenommen") {
                gemini::delete_key();
                return Err(message);
            }
            // Kein Netz o. Ae.: der Schluessel bleibt, ist aber ungeprueft.
            Ok(CloudStatus { has_key: true, verified: false, message: tr!(format!("Gespeichert, aber noch nicht geprüft: {message}"), format!("Saved, but not verified yet: {message}")) })
        }
    }
}

/// Groq-Schluessel speichern und pruefen (Modellliste abrufen). Abgelehnte Schluessel fliegen raus.
#[tauri::command]
async fn set_groq_key(app: AppHandle, key: String) -> Result<CloudStatus, String> {
    groq::store_key(&key).map_err(|e| e.to_string())?;
    drop(key);
    match app.state::<groq::Groq>().models().await {
        Ok(models) => {
            let store = app.state::<SettingsStore>();
            let mut settings = store.get();
            if !models.contains(&settings.groq_model) {
                if let Some(first) = models.iter().find(|m| m.as_str() == groq::DEFAULT_MODEL).or(models.first()) {
                    settings.groq_model = first.clone();
                    store.set(settings)?;
                }
            }
            app.state::<Memory>().log_action("cloud", tr!("Groq-Schlüssel hinterlegt und geprüft", "Groq key added and verified"), true);
            Ok(CloudStatus { has_key: true, verified: true, message: tr!(format!("Schlüssel funktioniert. Modell: {}.", store.get().groq_model), format!("The key works. Model: {}.", store.get().groq_model)) })
        }
        Err(err) => {
            let message = err.to_string();
            if message.contains("nicht angenommen") {
                groq::delete_key();
                return Err(message);
            }
            Ok(CloudStatus { has_key: true, verified: false, message: tr!(format!("Gespeichert, aber noch nicht geprüft: {message}"), format!("Saved, but not verified yet: {message}")) })
        }
    }
}

/// Regen in den naechsten zwei Stunden am Wetter-Ort: Beginn (ms) und Menge, sonst nichts.
#[tauri::command]
async fn rain_soon(store: State<'_, SettingsStore>) -> Result<Option<(i64, f64)>, String> {
    let place = store.get().weather_place;
    web::rain_soon(&place).await.map_err(|e| e.to_string())
}

/// Wecker per Oberflaeche anlegen.
#[tauri::command]
fn alarm_add(app: AppHandle, time: String, days: Vec<u8>, label: String) -> Result<settings::Alarm, String> {
    alarms::add(&app, &time, days, &label)
}

/// Mausrad ueber der Pille: Lautstaerke in kleinen Schritten.
#[tauri::command]
fn volume_step(delta: i32) -> Result<u32, String> {
    pc::change_volume(delta.clamp(-20, 20)).map_err(|e| e.to_string())
}

/// Ein Termin beginnt: einmal kurz klingeln, wenn Timer-Toene an sind.
#[tauri::command]
fn play_chime(store: State<'_, SettingsStore>) {
    if store.get().timer_sound {
        sound::chime_once();
    }
}

#[tauri::command]
fn groq_status() -> CloudStatus {
    CloudStatus { has_key: groq::has_key(), verified: false, message: String::new() }
}

#[tauri::command]
fn delete_groq_key(memory: State<'_, Memory>) -> CloudStatus {
    let removed = groq::delete_key();
    memory.log_action("cloud", tr!("Groq-Schlüssel gelöscht", "Groq key removed"), removed);
    CloudStatus { has_key: groq::has_key(), verified: false, message: if removed { tr!("Schlüssel gelöscht.", "Key removed.").into() } else { String::new() } }
}

/// Laeuft eine lokale KI (Ollama, LM Studio), und welche Modelle hat sie?
#[tauri::command]
async fn local_ai_status(app: AppHandle) -> local::LocalStatus {
    app.state::<local::LocalAi>().status().await
}

#[derive(serde::Serialize)]
struct HardwareCheck {
    hardware: local::Hardware,
    recommendation: local::Recommendation,
}

/// Prueft Prozessor, Arbeitsspeicher und Grafikkarte (nur lokal) und empfiehlt ein Modell.
/// Die Oberflaeche ruft das erst, nachdem der Nutzer es erlaubt hat.
#[tauri::command]
async fn local_ai_hardware() -> HardwareCheck {
    let hardware = tokio::task::spawn_blocking(local::hardware).await.unwrap_or_default();
    let recommendation = local::recommend(&hardware);
    HardwareCheck { hardware, recommendation }
}

/// Laedt ein Modell mit Ollama herunter (nur auf Klick). Laeuft im Hintergrund weiter, auch wenn
/// das Fenster zugeht; Fortschritt kommt als Ereignis. Danach nutzt Milo das Modell sofort.
#[tauri::command]
fn local_ai_pull(app: AppHandle, model: String) -> Result<(), String> {
    let model: String = model.trim().chars().filter(|c| c.is_ascii_alphanumeric() || "-._:/".contains(*c)).take(120).collect();
    if model.is_empty() {
        return Err(tr!("Kein Modell angegeben.", "No model given.").into());
    }
    if !app.state::<local::LocalAi>().claim(&model) {
        return Err(tr!("Es läuft schon ein Download.", "A download is already running.").into());
    }
    tauri::async_runtime::spawn(async move {
        let result = app.state::<local::LocalAi>().pull(&app, &model).await;
        app.state::<Memory>().log_action("local", &tr!(format!("Lokales Modell {model} heruntergeladen"), format!("Downloaded local model {model}")), result.is_ok());
        if result.is_ok() {
            use_local_model(&app, &model);
        }
    });
    Ok(())
}

/// Stellt Milo auf das lokale Modell um. War nur die Cloud eingestellt, wird gemischt (mit
/// Gemini-Schluessel) oder ganz lokal (ohne) gearbeitet.
fn use_local_model(app: &AppHandle, model: &str) {
    let store = app.state::<SettingsStore>();
    let mut settings = store.get();
    settings.local_model = model.to_string();
    if settings.ai_mode == "cloud" {
        settings.ai_mode = if gemini::has_key() { "mixed" } else { "local" }.into();
    }
    if let Ok(saved) = store.set(settings) {
        let _ = app.emit("milo://settings", &saved);
        log::info!("Lokales Modell {model} verbunden ({})", saved.ai_mode);
    }
}

/// Letzter Stand eines Modell-Downloads, fuer Fenster, die erst spaeter aufgehen.
#[tauri::command]
fn local_ai_pull_state(app: AppHandle) -> Option<local::PullProgress> {
    app.state::<local::LocalAi>().progress()
}

/// Startet die installierte Ollama-App.
#[tauri::command]
async fn local_ai_start(app: AppHandle) -> bool {
    app.state::<local::LocalAi>().ensure_ollama().await
}

#[tauri::command]
async fn groq_models(app: AppHandle) -> Result<Vec<String>, String> {
    app.state::<groq::Groq>().models().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_gemini_key(memory: State<'_, Memory>) -> CloudStatus {
    let removed = gemini::delete_key();
    memory.log_action("cloud", tr!("Gemini-Schlüssel gelöscht", "Gemini key removed"), removed);
    CloudStatus { has_key: gemini::has_key(), verified: false, message: if removed { tr!("Schlüssel gelöscht.", "Key removed.").into() } else { String::new() } }
}

/// Bewertet einen Text, ohne ihn auszufuehren (Debug-Ansicht).
#[tauri::command]
fn rate_text(store: State<'_, SettingsStore>, text: String) -> router::Rating {
    router::decide(router::draft(&text), store.get().cloud_threshold)
}

/// Verbrauch der Gemini-Kontingente fuer die Verbrauchsanzeige.
#[tauri::command]
fn gemini_usage(store: State<'_, SettingsStore>, limits: State<'_, limits::Limits>) -> limits::Usage {
    limits.usage(&store.get())
}

/// Modelle, die mit dem hinterlegten Schluessel verfuegbar sind (Auswahl in den Einstellungen).
#[tauri::command]
async fn gemini_models(app: AppHandle) -> Result<Vec<String>, String> {
    app.state::<gemini::Gemini>().models().await.map_err(|e| e.to_string())
}

/// Zeigt oder versteckt die Insel (Tray-Symbol). Schlaeft Milo wegen eines Spiels, weckt der Klick es auf.
fn toggle_overlay(app: &AppHandle) {
    let Some(win) = app.get_webview_window(overlay::OVERLAY_LABEL) else {
        gamemode::wake_anyway(app);
        return;
    };
    if win.is_visible().unwrap_or(true) {
        let _ = win.hide();
    } else {
        let _ = win.show();
    }
}

/// Milo vollstaendig entfernen. Die Oberflaeche ruft das erst nach zwei Bestaetigungen und
/// der Eingabe eines Codes auf.
#[tauri::command]
fn wipe_everything(app: AppHandle) -> Result<(), String> {
    lifecycle::wipe(&app).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn get_autostart() -> bool {
    lifecycle::autostart_enabled()
}

#[tauri::command]
fn set_autostart(enable: bool) -> Result<bool, String> {
    lifecycle::set_autostart(enable).map_err(|e| e.to_string())?;
    Ok(lifecycle::autostart_enabled())
}

/// Baut das Menue des Symbols in der Taskleiste; im Spielemodus mit den Eintraegen zum Aufwecken.
fn tray_menu(app: &AppHandle, status: &gamemode::GameStatus) -> tauri::Result<Menu<tauri::Wry>> {
    use tauri::menu::{CheckMenuItem, PredefinedMenuItem};
    let menu = Menu::new(app)?;
    if status.sleeping {
        menu.append(&MenuItem::with_id(app, "sleeping", tr!(format!("Milo schläft, weil {} läuft", status.game), format!("Milo is asleep because {} is running", status.game)), false, None::<&str>)?)?;
        menu.append(&MenuItem::with_id(app, "wake", tr!("Trotzdem aufwecken (bis das Spiel endet)", "Wake up anyway (until the game ends)"), true, None::<&str>)?)?;
        menu.append(&MenuItem::with_id(app, "notgame", tr!(format!("„{}“ ist kein Spiel: nie mehr deswegen schlafen", status.game), format!("\"{}\" isn't a game: never sleep for it again", status.game)), true, None::<&str>)?)?;
    } else {
        menu.append(&MenuItem::with_id(app, "toggle", tr!("Milo ein-/ausblenden", "Show/hide Milo"), true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&CheckMenuItem::with_id(app, "autostart", tr!("Mit Windows starten", "Start with Windows"), true, lifecycle::autostart_enabled(), None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", tr!("Für jetzt beenden (bis zum nächsten Start)", "Quit for now (until next start)"), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "disable", tr!("Dauerhaft deaktivieren (kein Autostart, beenden)", "Disable permanently (no autostart, quit)"), true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "wipe", tr!("Milo löschen …", "Delete Milo …"), !status.sleeping, None::<&str>)?)?;
    Ok(menu)
}

/// Setzt das Menue neu (anderer Schlafzustand, anderer Autostart-Haken).
pub fn refresh_tray_menu(app: &AppHandle, status: &gamemode::GameStatus) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("milo"), tray_menu(app, status)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app, &gamemode::GameStatus::default())?;
    let mut tray = TrayIconBuilder::with_id("milo")
        .tooltip("Milo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "toggle" => toggle_overlay(app),
            "wake" => gamemode::wake_anyway(app),
            "notgame" => gamemode::not_a_game(app),
            "quit" => app.exit(0),
            "autostart" => {
                let wanted = !lifecycle::autostart_enabled();
                if let Err(err) = lifecycle::set_autostart(wanted) {
                    log::warn!("Autostart: {err}");
                }
                // Das Haekchen zeigt danach den wirklichen Zustand, auch wenn das Umstellen scheitert.
                refresh_tray_menu(app, &app.state::<gamemode::GameMode>().status());
            }
            "disable" => {
                if let Err(err) = lifecycle::set_autostart(false) {
                    log::warn!("Autostart: {err}");
                }
                app.exit(0);
            }
            "wipe" => {
                // Nur die Rueckfrage anstossen; geloescht wird erst nach doppelter Bestaetigung mit Code.
                if let Some(win) = app.get_webview_window(overlay::OVERLAY_LABEL) {
                    let _ = win.show();
                }
                let _ = tauri::Emitter::emit(app, "milo://wipe-request", ());
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_overlay(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Stand der Browser-Erweiterung fuer die Einstellungen.
#[tauri::command]
fn browser_extension_status() -> serde_json::Value {
    serde_json::json!({
        "connected": browser_bridge::connected(),
        "browser": browser_bridge::browser_name(),
        "folder": browser_bridge::extension_dir(),
    })
}

/// Oeffnet den Ordner der Erweiterung im Explorer (zum Laden im Browser).
#[tauri::command]
fn browser_extension_folder() -> Result<(), String> {
    system::shell_open(&browser_bridge::extension_dir().display().to_string()).map_err(|e| e.to_string())
}

pub fn run() {
    // Vom Browser als Verbindung zur Erweiterung gestartet: nur weiterreichen, keine Oberflaeche.
    if browser_bridge::is_host_launch() {
        browser_bridge::run_host();
        return;
    }
    // Einmalig: Daten vom alten App-Namen uebernehmen, bevor irgendetwas sie oeffnet.
    migrate::run();
    let app = tauri::Builder::default()
        // Ein zweiter Start holt die Insel wieder hervor. Schlaeft Milo wegen eines Spiels (dann gibt
        // es kein Fenster), weckt er es auf, statt still nichts zu tun.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window(overlay::OVERLAY_LABEL) {
                let _ = win.show();
            } else {
                gamemode::wake_anyway(app);
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let text_key = app.state::<SettingsStore>().get().text_hotkey;
                    let is_text = text_key.parse::<tauri_plugin_global_shortcut::Shortcut>().is_ok_and(|s| &s == shortcut);
                    let note_key = app.state::<SettingsStore>().get().note_hotkey;
                    let is_note = !note_key.is_empty() && note_key.parse::<tauri_plugin_global_shortcut::Shortcut>().is_ok_and(|s| &s == shortcut);
                    if is_note {
                        // Schnellnotiz: die Insel bekommt den Fokus, die Pille zeigt ein Eingabefeld.
                        if let Some(win) = app.get_webview_window(overlay::OVERLAY_LABEL) {
                            let _ = win.set_focus();
                        }
                        let _ = app.emit_to(overlay::OVERLAY_LABEL, "milo://quicknote", ());
                    } else if is_text {
                        textwork::start(app);
                    } else {
                        app.state::<voice::Voice>().toggle();
                    }
                })
                .build(),
        )
        .manage(voice::Voice::default())
        .manage(tts::Speaker::default())
        .manage(media::Media::default())
        .manage(gamemode::GameMode::default())
        .manage(overlay::OverlayState::default())
        .manage(agent::Agent::default())
        .manage(apps::AppIndex::default())
        .manage(timers::Timers::default())
        .manage(pulse::Pulse::default())
        .manage(confirm::Confirmations::default())
        .manage(monitor::Monitor::default())
        .manage(textwork::TextWork::default())
        .manage(pet::PetState::default())
        .manage(claudechat::ClaudeChat::default())
        .manage(desk::DeskState::default())
        .manage(gemini::Gemini::new())
        .manage(groq::Groq::new())
        .manage(local::LocalAi::new())
        .invoke_handler(tauri::generate_handler![
            overlay::set_hit_rect,
            audio::audio_outputs,
            audio::audio_set_output,
            agenda::agenda_list,
            agenda::agenda_add,
            agenda::agenda_delete,
            desk::desk_carry,
            desk::desk_drop_home,
            desk::desk_hits,
            desk::desk_ready,
            desk::desk_take,
            desk::desk_item,
            desk::desk_game,
            desk::desk_place,
            settings_window::settings_open,
            settings_window::settings_is_open,
            settings_window::settings_close,
            overlay::release_focus,
            overlay::is_fullscreen,
            get_settings,
            save_settings,
            agent::send_message,
            agent::cancel,
            agent::clear_chat,
            agent::warm_up,
            confirm::answer_confirmation,
            timers::list_timers,
            timers::cancel_timer,
            timers::start_timer,
            timers::extend_timer,
            fetch_weather,
            abort_power,
            power::power_plan,
            wipe_everything,
            get_autostart,
            set_autostart,
            claude::claude_status,
            claude::connect_claude,
            claude::claude_answer,
            claude::claude_reply,
            actionlog::action_log,
            actionlog::undo_action,
            browse::browse_places,
            browse::browse_add_place,
            browse::browse_remove_place,
            browse::browse_list,
            browse::browse_transfer,
            browse::browse_open,
            browse::browse_text,
            browse::browse_thumb,
            browse::browse_watch,
            workspaces::list_workspaces,
            widgets::widgets_list,
            widgets::widget_get,
            widgets::widget_save,
            widgets::widget_delete,
            widgets::widget_drag,
            widgets::widget_drop,
            widgets::widget_lock,
            widgets::widget_edit,
            widgets::widget_design,
            widgets::battery_status,
            set_groq_key,
            groq_status,
            volume_step,
            play_chime,
            rain_soon,
            alarm_add,
            pulse::clipboard_history,
            pulse::clipboard_use,
            pulse::clipboard_clear,
            pulse::screenshot_save,
            pulse::screenshot_ocr,
            pulse::reveal_path,
            pulse::privacy_status,
            delete_groq_key,
            groq_models,
            local_ai_status,
            local_ai_hardware,
            local_ai_pull,
            local_ai_pull_state,
            local_ai_start,
            workspaces::delete_workspace,
            monitor::monitor_watch,
            textwork::text_transform,
            textwork::text_cancel,
            daily::daily_check,
            pet::pet_out,
            pet::pet_ready,
            pet::pet_recall,
            pet::pet_home,
            pet::pet_adopt,
            claudechat::claude_chat_projects,
            claudechat::claude_chat_send,
            claudechat::claude_chat_stop,
            claudechat::claude_chat_login,
            claudechat::claude_chat_commands,
            claudechat::claude_chat_history,
            browser_extension_status,
            browser_extension_folder,
            cloud_status,
            set_gemini_key,
            delete_gemini_key,
            rate_text,
            gemini_usage,
            gemini_models,
            voice::voice_toggle,
            voice::voice_cancel,
            voice::voice_info,
            voice::voice_inject,
            tts::speak,
            tts::stop_speaking,
            media::now_playing,
            media::media_command,
            gamemode::game_status,
            import_files,
            list_files,
            delete_file,
            list_notes,
            save_rich_note,
            add_note,
            update_note,
            delete_note,
            list_actions,
        ])
        .setup(|app| {
            app.manage(SettingsStore::load(app.path().app_config_dir()?));
            app.manage(Memory::open(&app.path().app_data_dir()?.join("milo.db"))?);
            app.manage(limits::Limits::load(app.path().app_data_dir()?.join("kontingent.json")));
            app.manage(daily::Daily::load(app.path().app_data_dir()?.join("tag.json")));

            if let Some(win) = app.get_webview_window(overlay::OVERLAY_LABEL) {
                overlay::place_top_center(&win)?;
            }
            overlay::spawn_click_through(app.handle().clone());
            desk::spawn_pointer(app.handle().clone());
            desk::open_if_needed(app.handle());
            widgets::open_all(app.handle());
            build_tray(app.handle())?;
            timers::restore(app.handle());
            power::restore(app.handle());
            claude::spawn_watcher(app.handle().clone());
            std::thread::spawn(browser_bridge::start);
            voice::spawn(app.handle().clone());
            media::spawn_watcher(app.handle().clone());
            gamemode::spawn_watcher(app.handle().clone());
            pulse::spawn(app.handle().clone());
            std::thread::spawn(lifecycle::refresh_autostart);
            alarms::spawn(app.handle().clone());
            if let Err(err) = register_hotkeys(app.handle(), &app.state::<SettingsStore>().get()) {
                log::warn!("{err}");
            }

            // Im Hintergrund die Programmliste einlesen.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                handle.state::<apps::AppIndex>().ensure_fresh(Duration::ZERO).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Die Insel wird nie geschlossen, nur versteckt; beendet wird ueber das Tray-Menue.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("Milo konnte nicht gestartet werden");

    app.run(|app, event| {
        // Im Spielemodus gibt es kein Fenster mehr; die App soll trotzdem weiterlaufen.
        // Beendet wird nur ausdruecklich ueber das Menue (dann mit Code).
        if let RunEvent::ExitRequested { api, code, .. } = &event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        if let RunEvent::Exit = event {
            app.state::<tts::Speaker>().stop();
        }
    });
}
