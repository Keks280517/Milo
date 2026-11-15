//! Geplantes Herunterfahren, Neustarten und Energiesparen, mit Waechter.
//!
//! Milo zaehlt selbst. Frueher bekam Windows die ganze Wartezeit (`shutdown /s /t 1200`); dann zeigt
//! Windows einige Minuten vorher „Sie werden in Kuerze abgemeldet“, und solange man diese Meldung
//! nicht wegklickt, faehrt der PC oft gar nicht herunter. Jetzt merkt sich Milo den Termin (auch in
//! einer Datei, falls er neu startet), haelt den PC bis dahin wach, zeigt in den letzten Sekunden
//! einen grossen Countdown mit Ton in der Pille (abbrechen per Knopf, Esc oder Sprache) und gibt erst
//! am Ende `shutdown /s /t 0`. Laeuft der PC kurz danach noch (ein Programm haelt es auf), folgt
//! derselbe Befehl mit Zwang.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::system::{self, PowerAction};

pub const EVENT: &str = "milo://power";
/// So lange nach dem Befehl darf Windows sich Zeit lassen (Programme schliessen), bevor Milo mit
/// Zwang nachhilft.
pub const GRACE_MS: i64 = 40_000;
/// Letzter, unuebersehbarer Countdown in der Pille vor dem Herunterfahren (Sekunden).
pub const FINAL_SECONDS: i64 = 45;
/// Mindestvorlauf beim Energiesparen, damit die Antwort noch zu sehen ist.
pub const SLEEP_FLOOR_SECONDS: u64 = 10;
const FILE: &str = "geplant.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Shutdown,
    Restart,
    Sleep,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Shutdown => tr!("Herunterfahren", "Shutdown"),
            Action::Restart => tr!("Neustart", "Restart"),
            Action::Sleep => tr!("Energie sparen", "Sleep"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub action: Action,
    pub due_ms: i64,
    pub created_ms: i64,
}

struct Active {
    plan: Plan,
    task: tauri::async_runtime::JoinHandle<()>,
}

static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join(FILE))
}

fn save(app: &AppHandle, plan: Option<&Plan>) {
    let Some(path) = file(app) else { return };
    let result = match plan {
        Some(plan) => serde_json::to_string(plan).map_err(anyhow::Error::from).and_then(|text| Ok(std::fs::write(&path, text)?)),
        None if path.exists() => std::fs::remove_file(&path).map_err(anyhow::Error::from),
        None => Ok(()),
    };
    if let Err(err) = result {
        log::warn!("Geplanter Befehl ließ sich nicht speichern: {err}");
    }
}

/// Der aktuell geplante Befehl, falls einer ansteht.
pub fn current() -> Option<Plan> {
    ACTIVE.lock().unwrap().as_ref().map(|a| a.plan.clone())
}

/// Wann der Befehl ausgefuehrt wird: genau zum Zeitpunkt (das macht Milo selbst, nicht Windows).
pub fn watch_at(plan: &Plan) -> i64 {
    plan.due_ms
}

/// Ab wann der letzte Countdown laeuft (Ton, grosse Anzeige).
pub fn final_at(plan: &Plan) -> i64 {
    plan.due_ms - FINAL_SECONDS * 1000
}

/// Plant den Befehl in `seconds` Sekunden; ein frueherer Plan wird ersetzt. Liefert, ob einer ersetzt wurde.
pub fn schedule(app: &AppHandle, action: Action, seconds: u64) -> Result<bool> {
    let floor = if action == Action::Sleep { SLEEP_FLOOR_SECONDS } else { system::POWER_DELAY_SECONDS as u64 };
    let seconds = seconds.clamp(floor, 86_400);
    let previous = ACTIVE.lock().unwrap().take();
    let mut replaced = false;
    if let Some(old) = previous {
        old.task.abort();
        replaced = true;
    }
    // Ein bei Windows geplantes Herunterfahren (aeltere Milo-Fassung oder von Hand) zuruecknehmen:
    // sonst kaeme dessen Abmeldemeldung trotzdem. Gab es keins, meldet Windows nur einen Fehler.
    if system::power_in(PowerAction::Abort, 0).is_ok() {
        replaced = true;
    }
    let now = now_ms();
    let plan = Plan { action, due_ms: now + seconds as i64 * 1000, created_ms: now };
    arm(app, plan);
    Ok(replaced)
}

/// Startet den Waechter fuer einen Plan und meldet ihn der Oberflaeche.
fn arm(app: &AppHandle, plan: Plan) {
    save(app, Some(&plan));
    let _ = app.emit(EVENT, Some(&plan));
    let handle = app.clone();
    let watched = plan.clone();
    let task = tauri::async_runtime::spawn(async move { watch(handle, watched).await });
    *ACTIVE.lock().unwrap() = Some(Active { plan, task });
}

async fn watch(app: AppHandle, plan: Plan) {
    let at = watch_at(&plan);
    let mut last_chime = 0i64;
    loop {
        keep_awake();
        let now = now_ms();
        let left = at - now;
        if left <= 0 {
            break;
        }
        // Letzter Countdown: anfangs alle 5 s ein Ton, in den letzten 10 s jede Sekunde.
        // Der Ton kommt aus dem Kern, damit er auch klingt, wenn die Oberflaeche gedrosselt ist.
        let wait = if now >= final_at(&plan) {
            // Laeuft gerade ein Spiel (Milo schlaeft, die Insel ist zu), fuer den Countdown aufwachen:
            // ein Herunterfahren mitten im Spiel darf nicht unbemerkt kommen.
            if last_chime == 0 && app.state::<crate::gamemode::GameMode>().status().sleeping {
                crate::gamemode::wake_anyway(&app);
            }
            let every = if left <= 10_000 { 1_000 } else { 5_000 };
            if now - last_chime >= every - 50 {
                last_chime = now;
                crate::sound::chime_once();
            }
            left.min(250)
        } else {
            // In kurzen Stuecken warten: so stimmt die Zeit auch, wenn die Uhr springt.
            (final_at(&plan) - now).clamp(50, 30_000).min(left)
        };
        tokio::time::sleep(Duration::from_millis(wait as u64)).await;
    }
    // Nur eingreifen, wenn genau dieser Plan noch gilt (nicht abgebrochen, nicht ersetzt).
    if current().as_ref() != Some(&plan) {
        return;
    }
    let memory = app.state::<crate::memory::Memory>();
    let result = match plan.action {
        Action::Sleep => {
            take(&app);
            crate::pc::sleep_now()
        }
        Action::Shutdown | Action::Restart => {
            let restart = plan.action == Action::Restart;
            memory.log_action("system_power", &tr!(format!("{} ausgeführt", plan.action.label()), format!("{} carried out", plan.action.label())), true);
            match system::power_now(restart) {
                Ok(()) => {
                    // Sicherheitsnetz: laeuft der PC nach der Frist noch (ein Programm fragt nach
                    // Speichern o. ae.), mit Zwang. Offene Programme schliessen dann ohne Speichern.
                    tokio::time::sleep(Duration::from_millis(GRACE_MS as u64)).await;
                    if current().as_ref() != Some(&plan) {
                        return;
                    }
                    log::warn!("{} wurde ausgelöst, der PC läuft aber noch: jetzt mit Zwang.", plan.action.label());
                    memory.log_action("system_power", &tr!(format!("{} erzwungen (ein Programm hielt es auf)", plan.action.label()), format!("{} forced (a program was holding it up)", plan.action.label())), true);
                    system::force_power(restart)
                }
                Err(err) => {
                    log::warn!("{} ohne Zwang abgelehnt ({err}), versuche es mit Zwang.", plan.action.label());
                    system::force_power(restart)
                }
            }
        }
    };
    if let Err(err) = result {
        log::error!("{} ist fehlgeschlagen: {err}", plan.action.label());
        memory.log_action("system_power", &tr!(format!("{} fehlgeschlagen: {err}", plan.action.label()), format!("{} failed: {err}", plan.action.label())), false);
        take(&app);
    }
}

/// Setzt den Leerlauf-Zaehler von Windows zurueck, damit der PC nicht vorher einschlaeft.
/// Der Bildschirm darf trotzdem ausgehen.
fn keep_awake() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Power::{SetThreadExecutionState, ES_SYSTEM_REQUIRED};
        SetThreadExecutionState(ES_SYSTEM_REQUIRED);
    }
}

/// Entfernt den Plan (ohne Windows zu fragen) und meldet das der Oberflaeche.
fn take(app: &AppHandle) -> Option<Plan> {
    let active = ACTIVE.lock().unwrap().take();
    save(app, None);
    let _ = app.emit(EVENT, None::<Plan>);
    active.map(|a| {
        a.task.abort();
        a.plan
    })
}

/// Bricht den geplanten Befehl ab, bei Milo und (falls dort noch einer steht, etwa von Hand oder
/// aus einer aelteren Fassung) bei Windows. Liefert, was abgebrochen wurde (`Some(None)`: nur bei
/// Windows war etwas geplant), oder `None`, wenn nichts anstand.
pub fn cancel(app: &AppHandle) -> Option<Option<Action>> {
    let plan = take(app);
    let windows = system::power_in(PowerAction::Abort, 0).is_ok();
    match plan {
        Some(plan) => Some(Some(plan.action)),
        None if windows => Some(None),
        None => None,
    }
}

/// Darf ein gespeicherter Plan nach einem Start von Milo wieder aufgenommen werden?
/// Nur aus derselben Windows-Sitzung und nur, solange sein Zeitpunkt nicht lange vorbei ist:
/// nach einem Neustart oder einer neuen Anmeldung faehrt Milo nie ueberraschend herunter.
pub fn resumable(plan: &Plan, now_ms: i64, session_start_ms: i64) -> bool {
    plan.created_ms >= session_start_ms && plan.due_ms + 60_000 > now_ms && plan.created_ms <= now_ms
}

/// Beginn der Windows-Sitzung: der spaetere Wert aus Systemstart und Start von explorer.exe
/// (beim Schnellstart von Windows zaehlt der Systemstart sonst ueber das Herunterfahren hinweg).
fn session_start_ms() -> i64 {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let boot = System::boot_time() as i64 * 1000;
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let explorer = system
        .processes()
        .values()
        .filter(|p| p.name().eq_ignore_ascii_case("explorer.exe"))
        .map(|p| p.start_time() as i64 * 1000)
        .min()
        .unwrap_or(0);
    boot.max(explorer)
}

/// Nimmt nach einem Start von Milo einen gespeicherten Plan wieder auf oder verwirft ihn.
pub fn restore(app: &AppHandle) {
    let Some(path) = file(app) else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(plan) = serde_json::from_str::<Plan>(&text) else {
        save(app, None);
        return;
    };
    let now = now_ms();
    if !resumable(&plan, now, session_start_ms()) {
        log::info!("Alter Plan „{}“ verworfen (andere Sitzung oder längst vorbei).", plan.action.label());
        save(app, None);
        return;
    }
    // Verpasster oder sehr naher Zeitpunkt: neu planen, damit der letzte Countdown sichtbar ist
    // und man noch abbrechen kann.
    if plan.due_ms - now < system::POWER_DELAY_SECONDS as i64 * 1000 {
        let _ = schedule(app, plan.action, system::POWER_DELAY_SECONDS as u64);
        return;
    }
    log::info!("Plan „{}“ wieder aufgenommen.", plan.action.label());
    arm(app, plan);
}

#[tauri::command]
pub fn power_plan() -> Option<Plan> {
    current()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(action: Action, created_ms: i64, due_ms: i64) -> Plan {
        Plan { action, due_ms, created_ms }
    }

    #[test]
    fn milo_acts_on_time_after_a_final_countdown() {
        // Kein Warten auf Windows mehr: Milo fuehrt zum Zeitpunkt selbst aus.
        assert_eq!(watch_at(&plan(Action::Shutdown, 0, 600_000)), 600_000);
        assert_eq!(watch_at(&plan(Action::Restart, 0, 600_000)), 600_000);
        assert_eq!(watch_at(&plan(Action::Sleep, 0, 600_000)), 600_000);
        assert_eq!(final_at(&plan(Action::Shutdown, 0, 600_000)), 600_000 - FINAL_SECONDS * 1000);
    }

    #[test]
    fn plans_from_an_earlier_session_are_dropped() {
        let session = 1_000_000;
        // Vor dieser Windows-Sitzung angelegt: nie wieder aufnehmen.
        assert!(!resumable(&plan(Action::Shutdown, session - 1, session + 900_000), session + 10_000, session));
        // Laengst vorbei (der PC ist damals ausgegangen): verwerfen.
        assert!(!resumable(&plan(Action::Shutdown, session + 10, session + 20_000), session + 200_000, session));
        // Gleiche Sitzung, Zeitpunkt noch offen: aufnehmen.
        assert!(resumable(&plan(Action::Shutdown, session + 10, session + 900_000), session + 60_000, session));
        // Knapp verpasst (Milo lief gerade nicht): aufnehmen, Milo faehrt dann herunter.
        assert!(resumable(&plan(Action::Shutdown, session + 10, session + 50_000), session + 80_000, session));
    }

    #[test]
    fn plan_round_trips_as_json() {
        let original = plan(Action::Restart, 5, 10);
        let text = serde_json::to_string(&original).unwrap();
        assert!(text.contains("\"action\":\"restart\""));
        assert_eq!(serde_json::from_str::<Plan>(&text).unwrap(), original);
    }
}
