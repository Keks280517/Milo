//! Systemmonitor fuer den System-Tab: Prozessor, Arbeitsspeicher, Laufwerke, Netzwerk und die
//! Programme, die gerade am meisten brauchen.
//!
//! Gemessen wird nur, solange der Tab offen ist: die Oberflaeche meldet sich alle paar Sekunden
//! (`monitor_watch`). Bleibt die Meldung aus, hoert der Mess-Thread von selbst auf.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{DiskRefreshKind, Disks, Networks, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT: &str = "milo://monitor";
/// Ohne neue Meldung der Oberflaeche nach dieser Zeit aufhoeren.
const IDLE_STOP: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub free: u64,
    /// Lesen und Schreiben gerade, Bytes pro Sekunde.
    pub read: u64,
    pub write: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub name: String,
    /// Anteil am ganzen Prozessor in Prozent (0 bis 100).
    pub cpu: f32,
    pub memory: u64,
    /// Wie viele Prozesse dieses Programms laufen (Browser haben viele).
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub cpu_name: String,
    pub ram_used: u64,
    pub ram_total: u64,
    pub disks: Vec<DiskInfo>,
    /// Bytes pro Sekunde.
    pub net_down: u64,
    pub net_up: u64,
    pub top: Vec<ProcessInfo>,
    pub uptime: u64,
}

#[derive(Default)]
pub struct Monitor {
    running: AtomicBool,
    last_ping: Mutex<Option<Instant>>,
}

/// Fasst Prozesse gleichen Namens zusammen und sortiert nach Prozessorlast, dann Speicher.
pub fn group_processes(list: impl Iterator<Item = (String, f32, u64)>, cpus: usize, limit: usize) -> Vec<ProcessInfo> {
    let mut groups: HashMap<String, ProcessInfo> = HashMap::new();
    for (name, cpu, memory) in list {
        let name = name.trim_end_matches(".exe").trim_end_matches(".EXE").to_string();
        if name.is_empty() || name == "System Idle Process" || name == "Idle" {
            continue;
        }
        let entry = groups.entry(name.to_lowercase()).or_insert(ProcessInfo { name, cpu: 0.0, memory: 0, count: 0 });
        entry.cpu += cpu / cpus.max(1) as f32;
        entry.memory += memory;
        entry.count += 1;
    }
    let mut out: Vec<ProcessInfo> = groups.into_values().collect();
    out.sort_by(|a, b| {
        let (ca, cb) = ((a.cpu * 10.0).round(), (b.cpu * 10.0).round());
        cb.total_cmp(&ca).then(b.memory.cmp(&a.memory))
    });
    out.truncate(limit);
    for p in &mut out {
        p.cpu = (p.cpu * 10.0).round() / 10.0;
    }
    out
}

fn measure(app: AppHandle) {
    let monitor = app.state::<Monitor>();
    let mut sys = System::new();
    let mut networks = Networks::new_with_refreshed_list();
    let mut disks = Disks::new_with_refreshed_list();
    let mut tick: u64 = 0;
    let mut last = Instant::now();
    let mut top: Vec<ProcessInfo> = Vec::new();
    sys.refresh_cpu_usage();
    while monitor.last_ping.lock().unwrap().is_some_and(|t| t.elapsed() < IDLE_STOP) {
        std::thread::sleep(Duration::from_millis(1000));
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        networks.refresh(true);
        let elapsed = last.elapsed().as_secs_f64().max(0.2);
        last = Instant::now();
        let (down, up) = networks.iter().fold((0u64, 0u64), |(d, u), (_, n)| (d + n.received(), u + n.transmitted()));
        // Prozesse sind teurer: nur alle drei Sekunden.
        if tick % 3 == 0 {
            sys.refresh_processes(ProcessesToUpdate::All, true);
            let cpus = sys.cpus().len();
            top = group_processes(sys.processes().values().map(|p| (p.name().to_string_lossy().to_string(), p.cpu_usage(), p.memory())), cpus, 8);
        }
        if tick % 30 == 0 {
            disks.refresh(true);
        } else {
            // Jede Sekunde nur die Lese- und Schreibmengen, nicht den freien Platz.
            disks.refresh_specifics(false, DiskRefreshKind::nothing().with_io_usage());
        }
        tick += 1;
        let snapshot = Snapshot {
            cpu: (sys.global_cpu_usage() * 10.0).round() / 10.0,
            cores: sys.cpus().iter().map(|c| c.cpu_usage().round()).collect(),
            cpu_name: sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
            ram_used: sys.used_memory(),
            ram_total: sys.total_memory(),
            disks: disks
                .iter()
                .filter(|d| d.total_space() > 0)
                .map(|d| DiskInfo {
                    name: d.name().to_string_lossy().to_string(),
                    mount: d.mount_point().display().to_string(),
                    total: d.total_space(),
                    free: d.available_space(),
                    read: (d.usage().read_bytes as f64 / elapsed) as u64,
                    write: (d.usage().written_bytes as f64 / elapsed) as u64,
                })
                .collect(),
            net_down: (down as f64 / elapsed) as u64,
            net_up: (up as f64 / elapsed) as u64,
            top: top.clone(),
            uptime: System::uptime(),
        };
        if app.emit(EVENT, snapshot).is_err() {
            break;
        }
    }
    monitor.running.store(false, Ordering::SeqCst);
}

/// Die Oberflaeche zeigt den System-Tab (`on`) oder hat ihn verlassen.
#[tauri::command]
pub fn monitor_watch(app: AppHandle, monitor: State<'_, Monitor>, on: bool) {
    *monitor.last_ping.lock().unwrap() = on.then(Instant::now);
    if on && !monitor.running.swap(true, Ordering::SeqCst) {
        std::thread::spawn(move || measure(app));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processes_are_grouped_and_sorted() {
        let list = vec![
            ("chrome.exe".to_string(), 40.0, 300),
            ("chrome.exe".to_string(), 40.0, 200),
            ("Code.exe".to_string(), 10.0, 900),
            ("System Idle Process".to_string(), 700.0, 0),
            ("notepad.exe".to_string(), 0.0, 50),
        ];
        let top = group_processes(list.into_iter(), 8, 5);
        assert_eq!(top[0].name, "chrome");
        assert_eq!((top[0].cpu, top[0].memory, top[0].count), (10.0, 500, 2));
        assert_eq!(top[1].name, "Code");
        assert!(top.iter().all(|p| p.name != "System Idle Process"));
        assert_eq!(top.last().unwrap().name, "notepad");
    }
}
