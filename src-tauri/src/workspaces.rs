//! Arbeitsplaetze: die offenen Programme samt Fensterlage unter einem Namen merken und spaeter mit
//! einem Befehl wiederherstellen ("Arbeitsplatz Schule"). Fehlt ein Programm, startet Milo es und
//! legt das neue Fenster an die gemerkte Stelle. Gespeichert wird nur Programmpfad, Titel und Lage,
//! keine Inhalte.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::screens::{self, Rect};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Spot {
    /// Programmname ohne Endung, z. B. "spotify".
    pub process: String,
    /// Vollstaendiger Pfad zum Starten, falls das Programm nicht laeuft.
    pub exe: String,
    pub title: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub maximized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub spots: Vec<Spot>,
}

/// Programme, die nie zu einem Arbeitsplatz gehoeren (Systemoberflaeche, Milo selbst).
const SKIP: &[&str] = &["explorer", "textinputhost", "applicationframehost", "shellexperiencehost", "searchhost", "startmenuexperiencehost", "systemsettings", "milo"];

fn file(app: &AppHandle) -> PathBuf {
    app.path().app_config_dir().unwrap_or_default().join("arbeitsplaetze.json")
}

pub fn load(app: &AppHandle) -> Vec<Workspace> {
    std::fs::read_to_string(file(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn store(app: &AppHandle, all: &[Workspace]) -> Result<()> {
    let path = file(app);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(all)?)?;
    Ok(())
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Merkt sich die jetzt offenen Fenster unter `name` (ersetzt einen gleichnamigen Arbeitsplatz).
pub fn save(app: &AppHandle, name: &str) -> Result<Workspace> {
    let name = name.trim();
    if name.is_empty() {
        bail!(tr!("Wie soll der Arbeitsplatz heißen?", "What should the workspace be called?"));
    }
    let spots: Vec<Spot> = screens::other_windows()
        .into_iter()
        .filter(|w| !SKIP.contains(&w.process.to_lowercase().as_str()) && !w.title.trim().is_empty())
        .filter_map(|w| {
            let r = screens::rect_of(w.handle)?;
            // Minimierte Fenster liegen weit ausserhalb des Bildschirms; die Lage waere nutzlos.
            if r.left <= -30000 || r.width() < 80 || r.height() < 60 {
                return None;
            }
            Some(Spot {
                process: w.process.to_lowercase(),
                exe: crate::gamemode::exe_path(w.pid).unwrap_or_default(),
                title: w.title.chars().take(120).collect(),
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
                maximized: screens::is_maximized(w.handle),
            })
        })
        .take(20)
        .collect();
    if spots.is_empty() {
        bail!(tr!("Es sind keine Programmfenster offen, die ich mir merken könnte.", "There are no app windows open that I could remember."));
    }
    let workspace = Workspace { name: name.to_string(), spots };
    let mut all = load(app);
    all.retain(|w| !same_name(&w.name, name));
    all.push(workspace.clone());
    store(app, &all)?;
    Ok(workspace)
}

pub fn remove(app: &AppHandle, name: &str) -> Result<bool> {
    let mut all = load(app);
    let before = all.len();
    all.retain(|w| !same_name(&w.name, name));
    store(app, &all)?;
    Ok(all.len() < before)
}

/// Stellt einen Arbeitsplatz wieder her. Laeuft blockierend (Programme starten), also in
/// `spawn_blocking` aufrufen. Liefert, was geklappt hat und was nicht.
pub fn open(app: &AppHandle, name: &str) -> Result<(Vec<String>, Vec<String>)> {
    let all = load(app);
    let Some(workspace) = all.iter().find(|w| same_name(&w.name, name)) else {
        let known: Vec<&str> = all.iter().map(|w| w.name.as_str()).collect();
        if known.is_empty() {
            bail!(tr!("Ich habe noch keinen Arbeitsplatz gespeichert. Sag zum Beispiel: „Arbeitsplatz speichern als Schule“.", "I haven't saved a workspace yet. Say for example: \"save workspace as School\"."));
        }
        bail!("{}", tr!(format!("Einen Arbeitsplatz „{name}“ kenne ich nicht. Gespeichert sind: {}.", known.join(", ")), format!("I don't know a workspace called \"{name}\". Saved: {}.", known.join(", "))));
    };
    let mut used: HashSet<isize> = HashSet::new();
    let (mut placed, mut missing) = (Vec::new(), Vec::new());
    for spot in &workspace.spots {
        let label = spot.title.split(" - ").last().unwrap_or(&spot.process).trim().to_string();
        let open_window = screens::other_windows().into_iter().find(|w| !used.contains(&w.handle) && w.process.to_lowercase() == spot.process);
        let window = match open_window {
            Some(window) => Some(window),
            None if !spot.exe.is_empty() && std::path::Path::new(&spot.exe).exists() => {
                let before: HashSet<isize> = screens::other_windows().iter().map(|w| w.handle).collect();
                match std::process::Command::new(&spot.exe).spawn() {
                    Ok(_) => screens::wait_for_new_window(&before, Duration::from_secs(12), |w| w.process.to_lowercase() == spot.process),
                    Err(_) => None,
                }
            }
            None => None,
        };
        match window {
            Some(window) => {
                used.insert(window.handle);
                let target = Rect { left: spot.left, top: spot.top, right: spot.right, bottom: spot.bottom };
                match screens::set_rect(window.handle, target, spot.maximized) {
                    Ok(()) => placed.push(label),
                    Err(_) => missing.push(label),
                }
            }
            None => missing.push(label),
        }
    }
    Ok((placed, missing))
}

#[tauri::command]
pub fn list_workspaces(app: AppHandle) -> Vec<Workspace> {
    load(&app)
}

#[tauri::command]
pub fn delete_workspace(app: AppHandle, name: String) -> Result<bool, String> {
    remove(&app, &name).map_err(|e| e.to_string())
}
