//! Installierte Programme finden und starten.
//!
//! Die Liste kommt aus dem Startmenue (`Get-StartApps`), damit klassische Programme, Store-Apps
//! und Spiele-Verknuepfungen gleich behandelt werden. Gestartet wird ueber `shell:AppsFolder`,
//! also genau so, als haette der Nutzer das Programm im Startmenue angeklickt.

use std::process::Command;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Deserialize;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Ab diesem Wert gilt ein Treffer als sicher genug, um das Programm zu starten.
const MIN_SCORE: f32 = 55.0;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AppEntry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "AppID")]
    pub id: String,
}

/// Umgangssprachliche oder englische Bezeichnungen und der Name, unter dem Windows das Programm fuehrt.
const ALIASES: &[(&str, &str)] = &[
    ("taschenrechner", "rechner"), ("calculator", "rechner"), ("calc", "rechner"),
    ("dateien", "explorer"), ("dateimanager", "explorer"), ("datei explorer", "explorer"), ("dateiexplorer", "explorer"),
    ("notepad", "editor"), ("texteditor", "editor"),
    ("settings", "einstellungen"), ("systemeinstellungen", "einstellungen"),
    ("eingabeaufforderung", "terminal"), ("cmd", "terminal"), ("konsole", "terminal"), ("powershell", "terminal"),
    ("chrome", "google chrome"), ("vscode", "visual studio code"), ("vs code", "visual studio code"),
];

/// Eintraege, die fast nie gemeint sind, wenn jemand ein Programm starten will.
const NOISE: &[&str] = &["uninstall", "deinstall", "hilfe", "help", "readme", "support", "lizenz", "license", "website", "dokumentation"];

#[derive(Default)]
pub struct AppIndex {
    apps: RwLock<Vec<AppEntry>>,
    loaded: RwLock<Option<Instant>>,
}

impl AppIndex {
    /// Liest die Programmliste neu ein, wenn sie fehlt oder aelter als `max_age` ist.
    pub async fn ensure_fresh(&self, max_age: Duration) {
        let stale = self.loaded.read().unwrap().is_none_or(|at| at.elapsed() > max_age);
        if !stale {
            return;
        }
        match tokio::task::spawn_blocking(read_start_apps).await {
            Ok(Ok(apps)) => {
                log::info!("{} Programme im Startmenü gefunden", apps.len());
                *self.apps.write().unwrap() = apps;
                *self.loaded.write().unwrap() = Some(Instant::now());
            }
            Ok(Err(err)) => log::warn!("Programmliste nicht lesbar: {err}"),
            Err(err) => log::warn!("Programmliste nicht lesbar: {err}"),
        }
    }

    /// Bester Treffer fuer einen gesprochenen oder getippten Programmnamen.
    pub fn find(&self, query: &str) -> Option<AppEntry> {
        best_match(&self.apps.read().unwrap(), query).map(|(app, _)| app.clone())
    }

    /// Die aehnlichsten Namen, als Hilfe, wenn nichts sicher passt.
    pub fn suggestions(&self, query: &str, count: usize) -> Vec<String> {
        let apps = self.apps.read().unwrap();
        let wanted = resolve_alias(&normalize(query));
        let mut scored: Vec<(f32, &str)> = apps.iter().map(|a| (score(&wanted, a), a.name.as_str())).filter(|(s, _)| *s > 25.0).collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.into_iter().take(count).map(|(_, name)| name.to_string()).collect()
    }
}

/// Kleinbuchstaben, Umlaute aufgeloest, alles ausser Buchstaben und Ziffern wird zum Leerzeichen.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.to_lowercase().chars() {
        match ch {
            'ä' => out.push('a'),
            'ö' => out.push('o'),
            'ü' => out.push('u'),
            'ß' => out.push_str("ss"),
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn resolve_alias(normalized: &str) -> String {
    ALIASES.iter().find(|(alias, _)| *alias == normalized).map(|(_, real)| real.to_string()).unwrap_or_else(|| normalized.to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut diagonal = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let above = row[j];
            row[j] = (diagonal + usize::from(a[i - 1] != b[j - 1])).min(above + 1).min(row[j - 1] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Aehnlichkeit 0..1 (1 = gleich), gemessen an der Zahl noetiger Aenderungen.
fn similarity(a: &str, b: &str) -> f32 {
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 { 1.0 } else { 1.0 - levenshtein(a, b) as f32 / longest as f32 }
}

/// Bewertet, wie gut `wanted` (bereits normalisiert) zu einem Eintrag passt. 0 = gar nicht, 100 = exakt.
fn score(wanted: &str, app: &AppEntry) -> f32 {
    let name = normalize(&app.name);
    if wanted.is_empty() || name.is_empty() {
        return 0.0;
    }
    let tokens: Vec<&str> = name.split(' ').collect();
    let mut value = if name == wanted {
        100.0
    } else if name.starts_with(wanted) {
        88.0
    } else if tokens.contains(&wanted) {
        84.0
    } else if name.contains(wanted) {
        74.0
    } else if wanted.split(' ').all(|w| tokens.contains(&w)) {
        70.0
    } else {
        // Vertipper oder Hoerfehler: mit dem ganzen Namen und mit jedem einzelnen Wort vergleichen.
        let best = tokens.iter().map(|t| similarity(wanted, t)).fold(similarity(wanted, &name), f32::max);
        if best >= 0.7 { 40.0 + best * 30.0 } else { best * 30.0 }
    };
    // Kuerzere Namen gewinnen bei Gleichstand ("Steam" vor "Steam Support Center").
    value -= (name.chars().count() as f32 - wanted.chars().count() as f32).max(0.0) * 0.15;
    let id = app.id.to_lowercase();
    if id.starts_with("http://") || id.starts_with("https://") || NOISE.iter().any(|n| name.contains(n)) {
        value -= 30.0;
    }
    value.max(0.0)
}

fn best_match<'a>(apps: &'a [AppEntry], query: &str) -> Option<(&'a AppEntry, f32)> {
    let wanted = resolve_alias(&normalize(query));
    apps.iter()
        .map(|app| (app, score(&wanted, app)))
        .filter(|(_, s)| *s >= MIN_SCORE)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn read_start_apps() -> Result<Vec<AppEntry>> {
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile", "-NonInteractive", "-Command",
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; @(Get-StartApps | Select-Object Name, AppID) | ConvertTo-Json -Compress",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        return Err(anyhow!("{}", tr!(format!("PowerShell meldet: {}", String::from_utf8_lossy(&output.stderr).trim()), format!("PowerShell says: {}", String::from_utf8_lossy(&output.stderr).trim()))));
    }
    parse_start_apps(&String::from_utf8_lossy(&output.stdout))
}

fn parse_start_apps(json: &str) -> Result<Vec<AppEntry>> {
    let json = json.trim().trim_start_matches('\u{feff}');
    if json.is_empty() {
        return Ok(Vec::new());
    }
    let mut apps: Vec<AppEntry> = serde_json::from_str(json)?;
    apps.retain(|a| !a.name.trim().is_empty() && !a.id.trim().is_empty());
    Ok(apps)
}

/// Startet einen Eintrag so, als waere er im Startmenue angeklickt worden.
pub fn launch(app: &AppEntry) -> Result<()> {
    crate::system::shell_open(&format!("shell:AppsFolder\\{}", app.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<AppEntry> {
        [
            ("Rechner", "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"),
            ("Spotify", "C:\\Users\\x\\AppData\\Roaming\\Spotify\\Spotify.exe"),
            ("Discord", "com.squirrel.Discord.Discord"),
            ("Steam", "{7C5A40EF}\\Steam\\Steam.exe"),
            ("Steam Support Center", "http://support.steampowered.com/"),
            ("Explorer", "Microsoft.Windows.Explorer"),
            ("Editor", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"),
            ("Registrierungs-Editor", "{F38BF404}\\regedit.exe"),
            ("Word", "Microsoft.Office.WINWORD.EXE.15"),
            ("Excel", "Microsoft.Office.EXCEL.EXE.15"),
            ("Google Chrome", "Chrome"),
            ("Visual Studio Code", "Microsoft.VisualStudioCode"),
            ("Uninstall Spotify", "C:\\x\\uninstall.exe"),
            ("Einstellungen", "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel"),
        ]
        .into_iter()
        .map(|(name, id)| AppEntry { name: name.into(), id: id.into() })
        .collect()
    }

    fn found(query: &str) -> Option<String> {
        let apps = sample();
        best_match(&apps, query).map(|(a, _)| a.name.clone())
    }

    #[test]
    fn exact_and_case_insensitive() {
        assert_eq!(found("Spotify").as_deref(), Some("Spotify"));
        assert_eq!(found("discord").as_deref(), Some("Discord"));
        assert_eq!(found("EXCEL").as_deref(), Some("Excel"));
    }

    #[test]
    fn aliases_resolve_german_and_english_names() {
        assert_eq!(found("Taschenrechner").as_deref(), Some("Rechner"));
        assert_eq!(found("calculator").as_deref(), Some("Rechner"));
        assert_eq!(found("Notepad").as_deref(), Some("Editor"));
        assert_eq!(found("Chrome").as_deref(), Some("Google Chrome"));
        assert_eq!(found("VS Code").as_deref(), Some("Visual Studio Code"));
        assert_eq!(found("Datei-Explorer").as_deref(), Some("Explorer"));
    }

    #[test]
    fn typos_and_mishearings_still_match() {
        assert_eq!(found("spotifei").as_deref(), Some("Spotify"));
        assert_eq!(found("Spotfy").as_deref(), Some("Spotify"));
        assert_eq!(found("Diskord").as_deref(), Some("Discord"));
    }

    #[test]
    fn prefers_the_program_over_helpers_and_links() {
        assert_eq!(found("Steam").as_deref(), Some("Steam"));
        assert_eq!(found("Editor").as_deref(), Some("Editor"));
        assert_ne!(found("Spotify").as_deref(), Some("Uninstall Spotify"));
    }

    #[test]
    fn unrelated_names_do_not_match() {
        assert_eq!(found("Photoshop"), None);
        assert_eq!(found(""), None);
        assert_eq!(found("xyz"), None);
    }

    #[test]
    fn normalize_folds_umlauts_and_punctuation() {
        assert_eq!(normalize("Registrierungs-Editor"), "registrierungs editor");
        assert_eq!(normalize("  Größe:  Ä/Ö "), "grosse a o");
    }

    #[test]
    fn parses_powershell_json() {
        let apps = parse_start_apps("\u{feff}[{\"Name\":\"Rechner\",\"AppID\":\"x!App\"},{\"Name\":\"\",\"AppID\":\"y\"}]").unwrap();
        assert_eq!(apps, vec![AppEntry { name: "Rechner".into(), id: "x!App".into() }]);
        assert!(parse_start_apps("  ").unwrap().is_empty());
        assert!(parse_start_apps("kein json").is_err());
    }

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("spotify", "spotify"), 0);
        assert_eq!(levenshtein("spotfy", "spotify"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
    }
}
