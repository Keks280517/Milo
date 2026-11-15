//! Bruecke zu Claude: zeigt in der Insel, ob Claude offen ist und was Claude Code gerade tut.
//!
//! Zwei Quellen, beide nur lesend und lokal:
//! - Laeuft ein Prozess `claude.exe`, ist Claude "da" (das orange Wuerfelchen erscheint).
//! - Claude Code kann ueber seine offiziellen Hooks bei bestimmten Ereignissen ein Programm
//!   starten. Das kleine Skript `scripts/claude-hook.mjs` schreibt dann pro Sitzung eine Datei
//!   in den Ordner `claude-bridge`; hier wird dieser Ordner regelmaessig gelesen.
//! Ohne eingerichtete Hooks ist nur zu sehen, dass Claude offen ist, nicht was es tut.
//! Normale Chats in der Claude-App melden nichts; das geht nur mit Claude Code.
//!
//! Dazu kommen die Live-Ansicht (welche Datei Claude gerade schreibt, mit Ausschnitt, dazu
//! Claudes letzte Nachricht, Aufgabenliste und Schritte aus dem Sitzungsverlauf), die
//! offiziellen Nutzungsgrenzen ueber die Statuszeile (`claude-statusline.mjs` schreibt sie nach
//! `limits.json`), Freigaben und Rueckfragen: das Hook-Skript legt eine Anfrage ab, Milo zeigt sie,
//! und die Antwort des Nutzers geht als Datei zurueck. Milo entscheidet dabei nie selbst.

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

pub const EVENT: &str = "milo://claude";
const POLL: Duration = Duration::from_millis(800);
/// "Fertig" wird so lange gezeigt, danach gilt Claude wieder als ruhend.
const DONE_SHOWN: Duration = Duration::from_secs(45);
/// So lange gilt Claude nach einem Stop mit laufendem Hintergrundbefehl noch als arbeitend.
const BACKGROUND_KEPT: Duration = Duration::from_secs(30 * 60);
/// Eine Sitzung, in der so lange nichts passiert ist, zaehlt nicht mehr als aktiv (Buddy verschwindet).
const ACTIVE: Duration = Duration::from_secs(10 * 60);
/// Wartet Claude auf den Nutzer, bleibt die Sitzung laenger sichtbar.
const WAITING_KEPT: Duration = Duration::from_secs(60 * 60);
/// So lange gilt Claude nach einer Hook-Meldung als laufend, auch ohne Prozess `claude.exe`
/// (Claude Code ueber npm laeuft als `node.exe`).
const RECENT_HOOK: Duration = Duration::from_secs(120);
/// Sitzungsdateien, die sich so lange nicht gemeldet haben, werden geloescht.
const STALE: Duration = Duration::from_secs(3 * 3600);
/// Liegengebliebene Antworten ohne Anfrage werden nach dieser Zeit geloescht.
const ORPHAN: Duration = Duration::from_secs(60);
/// Beim ersten Lesen eines Sitzungsverlaufs nur sein Ende ansehen.
const FIRST_READ: u64 = 512 * 1024;
const STEPS_KEPT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Activity {
    Idle,
    Done,
    Working,
    Question,
    Confirm,
}

/// Offizielle Nutzungsgrenze eines Zeitfensters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub used_percent: f64,
    /// Sekunden seit 1970, wann das Fenster zurueckgesetzt wird.
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub model: String,
    pub context: Option<f64>,
    /// Wann die Statuszeile das zuletzt gemeldet hat (ms).
    pub at: i64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { five_hour: None, seven_day: None, model: String::new(), context: None, at: 0 }
    }
}

/// Eine Anfrage, auf die Claude wartet: Freigabe (`permission`), Plan (`plan`) oder Rueckfrage (`ask`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    pub id: String,
    #[serde(default = "permission_kind")]
    pub kind: String,
    pub tool: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub cwd: String,
    /// Fragen samt Antwortmoeglichkeiten, so wie Claude sie gestellt hat.
    #[serde(default)]
    pub questions: Value,
    #[serde(default)]
    pub plan: String,
    /// Bis wann das Hook-Skript wartet (ms seit 1970); 0 bei Anfragen aelterer Skripte.
    #[serde(default)]
    pub until: i64,
}

fn permission_kind() -> String {
    "permission".into()
}

/// Ein Punkt aus Claudes Aufgabenliste.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Todo {
    pub text: String,
    /// pending, in_progress oder completed.
    pub status: String,
}

/// Ein Arbeitsschritt von Claude (Werkzeugaufruf) fuer die Zeitleiste.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub tool: String,
    pub what: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Eine Claude-Code-Sitzung ist aktiv (hat sich vor kurzem gemeldet und Claude laeuft).
    pub present: bool,
    /// Die Hooks sind eingerichtet; ohne sie bleibt `activity` immer `idle`.
    pub connected: bool,
    pub activity: Activity,
    /// Kurzer Text dazu, z. B. Claudes Rueckfrage.
    pub text: String,
    /// Name des Projektordners der Sitzung.
    pub project: String,
    /// Live-Ansicht: zuletzt benutztes Werkzeug, Datei, neuer und ersetzter Text.
    pub tool: String,
    pub file: String,
    pub snippet: String,
    pub old: String,
    /// Aus dem Sitzungsverlauf: Claudes letzte Nachricht, Aufgabenliste und letzte Schritte.
    pub said: String,
    pub todos: Vec<Todo>,
    pub steps: Vec<Step>,
    pub limits: Option<Limits>,
    pub permission: Option<Permission>,
}

/// Was das Hook-Skript pro Sitzung ablegt.
#[derive(Debug, Deserialize)]
struct Report {
    event: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    old: String,
}

/// Milos Ordner fuer die Claude-Verbindung: Skripte und Meldungen. Er liegt bewusst unter
/// `%USERPROFILE%\.claude` und nicht in AppData: Die Claude-Desktop-App ist ein Windows-Paket und
/// leitet alles, was ihre Hooks nach AppData schreiben, in einen eigenen Ordner um, den Milo nie saehe.
pub fn milo_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(".claude").join("milo")
}

pub fn bridge_dir(_app: &AppHandle) -> PathBuf {
    milo_dir().join("bridge")
}

/// Uebersetzt ein Hook-Ereignis in das, was der Nutzer wissen will.
fn activity_of(report: &Report, age: Duration) -> Option<Activity> {
    Some(match report.event.as_str() {
        "SessionEnd" => return None,
        "PermissionRequest" => Activity::Confirm,
        // Nur echte Rueckfragen melden. `idle_prompt` kommt schon, wenn Claude eine Minute nach
        // seiner Antwort einfach wartet, und ist keine Frage.
        "Notification" => match report.kind.as_str() {
            "permission_prompt" => Activity::Confirm,
            "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input" => Activity::Question,
            // Aeltere Versionen von Claude Code nennen keine Art; dann nur Freigaben erkennen.
            "" if report.message.to_lowercase().contains("permission") => Activity::Confirm,
            _ => Activity::Idle,
        },
        "PreToolUse" if report.tool == "AskUserQuestion" || report.tool == "ExitPlanMode" => Activity::Question,
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "SubagentStop" => Activity::Working,
        // Das Hook-Skript liest beim Stop das Ende des Verlaufs: endet Claudes Zug mit einer Frage an
        // dich, wartet es (nicht fertig); laeuft noch ein Hintergrundbefehl, arbeitet es gleich weiter.
        "Stop" if report.kind == "question" => Activity::Question,
        "Stop" if report.kind == "background" && age < BACKGROUND_KEPT => Activity::Working,
        "Stop" if report.kind == "background" => Activity::Idle,
        "Stop" if age < DONE_SHOWN => Activity::Done,
        _ => Activity::Idle,
    })
}

/// Wie lange eine Sitzung mit dieser Taetigkeit als aktiv gilt.
fn kept_for(activity: Activity) -> Duration {
    if matches!(activity, Activity::Question | Activity::Confirm) { WAITING_KEPT } else { ACTIVE }
}

/// Dateien im Bruecken-Ordner, die keine Sitzungsmeldungen sind.
fn is_session_file(name: &str) -> bool {
    name.ends_with(".json") && !name.starts_with("frage-") && !name.starts_with("antwort-") && name != "limits.json" && name != "milo-approvals.json"
}

fn age_of(entry: &std::fs::DirEntry) -> Option<Duration> {
    entry.metadata().ok()?.modified().ok().map(|m| SystemTime::now().duration_since(m).unwrap_or_default())
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn read_reports(dir: &Path) -> Vec<(Report, Duration)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| is_session_file(&e.file_name().to_string_lossy()))
        .filter_map(|entry| {
            let age = age_of(&entry)?;
            if age > STALE {
                // Liegengebliebene Meldung einer laengst beendeten Sitzung: aufraeumen.
                let _ = std::fs::remove_file(entry.path());
                return None;
            }
            let report: Report = serde_json::from_str(&std::fs::read_to_string(entry.path()).ok()?).ok()?;
            Some((report, age))
        })
        .collect()
}

fn read_limits(dir: &Path) -> Option<Limits> {
    serde_json::from_str(&std::fs::read_to_string(dir.join("limits.json")).ok()?).ok()
}

/// Ist diese Anfrage abgelaufen? Das Hook-Skript wartet nur bis `until`; danach (oder wenn es
/// vorher beendet wurde) bleibt die Datei liegen und darf keinen Buddy mehr herbeirufen.
fn expired(permission: &Permission, age: Duration, now: i64) -> bool {
    if permission.until > 0 { now > permission.until + 10_000 } else { age > Duration::from_secs(40) }
}

/// Raeumt abgelaufene Anfragen und Antworten auf, die niemand mehr liest.
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = now_ms();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(age) = age_of(&entry) else { continue };
        let stale = if name.starts_with("frage-") && name.ends_with(".json") {
            std::fs::read_to_string(entry.path()).ok().and_then(|t| serde_json::from_str::<Permission>(&t).ok()).is_none_or(|p| expired(&p, age, now))
        } else if name.starts_with("antwort-") && name.ends_with(".json") {
            let id = &name["antwort-".len()..name.len() - ".json".len()];
            age > ORPHAN && !dir.join(format!("frage-{id}.json")).exists()
        } else {
            false
        };
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Aelteste offene Anfrage, die noch nicht abgelaufen ist.
fn read_permission(dir: &Path) -> Option<Permission> {
    let now = now_ms();
    let mut found: Vec<(SystemTime, Permission)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("frage-") && e.file_name().to_string_lossy().ends_with(".json"))
        .filter_map(|e| {
            let at = e.metadata().ok()?.modified().ok()?;
            let permission: Permission = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            let age = SystemTime::now().duration_since(at).unwrap_or_default();
            (!expired(&permission, age, now)).then_some((at, permission))
        })
        .collect();
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().next().map(|(_, p)| p)
}

/// Fasst alle Sitzungen zusammen; die dringendste gewinnt (Bestaetigung vor Frage vor Arbeit).
/// Claude gilt nur als da, wenn eine Sitzung vor kurzem etwas gemeldet hat und Claude laeuft
/// (Prozess oder Hook-Meldung in den letzten zwei Minuten). Die Desktop-App im Infobereich allein
/// reicht nicht.
fn summarize(reports: &[(Report, Duration)], process_running: bool, connected: bool) -> Status {
    let alive = process_running || reports.iter().any(|(_, age)| *age < RECENT_HOOK);
    let mut best: Option<(Activity, &Report)> = None;
    let mut sessions = 0;
    for (report, age) in reports {
        let Some(activity) = activity_of(report, *age) else { continue };
        if !alive || *age > kept_for(activity) {
            continue;
        }
        sessions += 1;
        if best.as_ref().is_none_or(|(current, _)| activity > *current) {
            best = Some((activity, report));
        }
    }
    let (activity, text, project) = match best {
        Some((activity, report)) => {
            let project = Path::new(&report.cwd).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let text = match activity {
                Activity::Question if report.message.is_empty() && report.tool == "ExitPlanMode" => tr!("Claude hat einen Plan und wartet auf deine Freigabe.", "Claude has a plan and is waiting for your approval.").to_string(),
                Activity::Question | Activity::Confirm => report.message.chars().take(160).collect(),
                _ => String::new(),
            };
            (activity, text, project)
        }
        None => (Activity::Idle, String::new(), String::new()),
    };
    let present = sessions > 0;
    // Live-Ansicht: die juengste Meldung mit Werkzeug.
    let live = reports.iter().filter(|(r, _)| !r.tool.is_empty() && r.event != "SessionEnd").min_by_key(|(_, age)| *age).map(|(r, _)| r);
    Status {
        present,
        connected,
        activity,
        text,
        project,
        tool: live.map(|r| r.tool.clone()).unwrap_or_default(),
        file: live.map(|r| r.file.clone()).unwrap_or_default(),
        snippet: live.map(|r| r.snippet.chars().take(4000).collect()).unwrap_or_default(),
        old: live.map(|r| r.old.chars().take(4000).collect()).unwrap_or_default(),
        said: String::new(),
        todos: Vec::new(),
        steps: Vec::new(),
        limits: None,
        permission: None,
    }
}

// ---------- Sitzungsverlauf mitlesen ----------

/// Liest den Verlauf (JSONL) der aktiven Sitzung mit: immer nur die neu hinzugekommenen Zeilen,
/// nie die ganze Datei. Daraus kommen Claudes letzte Nachricht, die Aufgabenliste und die Schritte.
#[derive(Default)]
struct Follow {
    path: PathBuf,
    offset: u64,
    said: String,
    todos: Vec<Todo>,
    steps: VecDeque<Step>,
}

static FOLLOW: LazyLock<Mutex<Follow>> = LazyLock::new(|| Mutex::new(Follow::default()));

impl Follow {
    fn update(&mut self, path: &Path) {
        if self.path != path {
            *self = Follow { path: path.to_path_buf(), ..Default::default() };
        }
        let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { return };
        if len < self.offset {
            // Neu geschrieben (z. B. nach /clear): von vorn.
            *self = Follow { path: path.to_path_buf(), ..Default::default() };
        }
        if len == self.offset {
            return;
        }
        let start = if self.offset == 0 && len > FIRST_READ { len - FIRST_READ } else { self.offset };
        let Ok(mut file) = std::fs::File::open(path) else { return };
        if file.seek(SeekFrom::Start(start)).is_err() {
            return;
        }
        let mut bytes = Vec::with_capacity((len - start) as usize);
        if file.take(len - start).read_to_end(&mut bytes).is_err() {
            return;
        }
        // Nur vollstaendige Zeilen; den Rest gibt es beim naechsten Mal.
        let Some(last) = bytes.iter().rposition(|b| *b == b'\n') else { return };
        let mut body = &bytes[..last];
        if start != self.offset {
            // Mitten in einer Zeile eingestiegen: bis zum naechsten Zeilenende ueberspringen.
            body = body.iter().position(|b| *b == b'\n').map(|i| &body[i + 1..]).unwrap_or(&[]);
        }
        self.offset = start + last as u64 + 1;
        for line in String::from_utf8_lossy(body).lines() {
            self.line(line);
        }
    }

    fn line(&mut self, line: &str) {
        // Nur Claudes eigene Nachrichten; Teilagenten (Sidechain) zaehlen nicht.
        if !line.contains("\"assistant\"") {
            return;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else { return };
        if entry["type"] != "assistant" || entry["isSidechain"] == true {
            return;
        }
        let at = entry["timestamp"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
            .unwrap_or_default();
        for block in entry["message"]["content"].as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("text") => {
                    let text = block["text"].as_str().unwrap_or("").trim();
                    if !text.is_empty() {
                        self.said = text.chars().take(1200).collect();
                    }
                }
                Some("tool_use") => {
                    let name = block["name"].as_str().unwrap_or("");
                    let input = &block["input"];
                    self.plan(name, input);
                    self.steps.push_back(Step { tool: short_tool(name), what: step_detail(input), at: at.clone() });
                    while self.steps.len() > STEPS_KEPT {
                        self.steps.pop_front();
                    }
                }
                _ => {}
            }
        }
    }

    /// Aufgabenliste nachfuehren: TodoWrite (Terminal) und die Schrittliste der Desktop-App.
    fn plan(&mut self, name: &str, input: &Value) {
        let todo = |item: &Value| Todo {
            text: item["content"].as_str().or(item["subject"].as_str()).unwrap_or("").chars().take(120).collect(),
            status: item["status"].as_str().unwrap_or("pending").to_string(),
        };
        if name == "TodoWrite" {
            if let Some(list) = input["todos"].as_array() {
                self.todos = list.iter().map(todo).collect();
            }
        } else if name.ends_with("update_plan") {
            if let Some(items) = input["items"].as_array() {
                self.todos = items.iter().map(todo).collect();
            }
            if input["advance"] == true {
                if let Some(t) = self.todos.iter_mut().find(|t| t.status == "in_progress") {
                    t.status = "completed".into();
                }
                if let Some(t) = self.todos.iter_mut().find(|t| t.status == "pending") {
                    t.status = "in_progress".into();
                }
            }
            for n in input["done"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
                if let Some(t) = self.todos.get_mut((n as usize).saturating_sub(1)) {
                    t.status = "completed".into();
                }
            }
            if let Some(t) = input["start"].as_u64().and_then(|n| self.todos.get_mut((n as usize).saturating_sub(1))) {
                t.status = "in_progress".into();
            }
        } else if name.ends_with("wrap_up") {
            // Abschluss der Runde in der Claude-App: dort wird die ganze Liste abgehakt.
            for t in &mut self.todos {
                t.status = "completed".into();
            }
        } else if name == "TaskCreate" {
            let item = todo(input);
            if !item.text.is_empty() {
                self.todos.push(Todo { status: "pending".into(), ..item });
            }
        }
    }
}

/// `mcp__server__werkzeug` → `werkzeug`.
fn short_tool(name: &str) -> String {
    name.rsplit("__").next().unwrap_or(name).to_string()
}

/// Kurzbeschreibung eines Werkzeugaufrufs: Datei, Befehl, Suche oder Adresse.
fn step_detail(input: &Value) -> String {
    let text = ["file_path", "notebook_path", "path"]
        .iter()
        .find_map(|k| input[*k].as_str())
        .map(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string()))
        .or_else(|| ["description", "command", "pattern", "query", "url", "prompt", "content"].iter().find_map(|k| input[*k].as_str().map(str::to_string)))
        .unwrap_or_default();
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() > 80 { format!("{}…", line.chars().take(80).collect::<String>()) } else { line.to_string() }
}

/// Vollstaendiger Zustand samt Grenzen, offener Anfrage und Verlauf der aktiven Sitzung.
fn status(dir: &Path, running: bool, connected: bool) -> Status {
    let reports = read_reports(dir);
    let mut status = summarize(&reports, running, connected);
    status.limits = read_limits(dir);
    let alive = running || reports.iter().any(|(_, age)| *age < RECENT_HOOK);
    status.permission = if alive { read_permission(dir) } else { None };
    if let Some(permission) = &status.permission {
        status.present = true;
        status.activity = if permission.kind == "ask" { Activity::Question } else { Activity::Confirm };
        if permission.kind == "ask" {
            status.text = permission.detail.chars().take(160).collect();
        }
    }
    if status.present {
        let newest = reports.iter().filter(|(r, _)| !r.transcript.is_empty()).min_by_key(|(_, age)| *age).map(|(r, _)| PathBuf::from(&r.transcript));
        if let (Some(path), Ok(mut follow)) = (newest, FOLLOW.lock()) {
            follow.update(&path);
            status.said = follow.said.clone();
            status.todos = follow.todos.clone();
            status.steps = follow.steps.iter().cloned().collect();
        }
    }
    status
}

// ---------- Hooks einrichten ----------

const HOOK_EVENTS: [&str; 8] =
    ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "Notification", "PermissionRequest", "Stop", "SessionEnd"];
/// Daran erkennt Milo seine eigenen Eintraege wieder.
const MARKER: &str = "claude-hook.mjs";
const STATUS_MARKER: &str = "claude-statusline.mjs";
/// Die Skripte liegen in der App und werden in Milos Datenordner geschrieben.
const HOOK_SCRIPT: &str = include_str!("../../scripts/claude-hook.mjs");
const STATUS_SCRIPT: &str = include_str!("../../scripts/claude-statusline.mjs");

/// Schreibt die aktuellen Skripte in Milos Datenordner (nur wenn sie sich geaendert haben)
/// und liefert den Pfad des Hook-Skripts.
pub fn install_scripts(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let _ = app;
    let dir = milo_dir();
    std::fs::create_dir_all(&dir)?;
    for (name, content) in [("claude-hook.mjs", HOOK_SCRIPT), ("claude-statusline.mjs", STATUS_SCRIPT)] {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content) {
            std::fs::write(&path, content)?;
        }
    }
    Ok(dir.join("claude-hook.mjs"))
}

/// Befehl fuer ein Skript, mit Schraegstrichen wie Claude ihn auf Windows gut verarbeitet.
fn node_command(script: &Path) -> String {
    format!("node \"{}\"", script.display().to_string().replace('\\', "/"))
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".claude").join("settings.json"))
}

fn is_ours(entry: &Value) -> bool {
    entry["hooks"].as_array().is_some_and(|hooks| hooks.iter().any(|h| h["command"].as_str().is_some_and(|c| c.contains(MARKER))))
}

/// Sind Milos Hooks in Claudes Einstellungen eingetragen?
pub fn hooks_installed() -> bool {
    let Some(path) = settings_path() else { return false };
    let Ok(text) = std::fs::read_to_string(path) else { return false };
    let Ok(settings) = serde_json::from_str::<Value>(&text) else { return false };
    settings["hooks"]["Stop"].as_array().is_some_and(|list| list.iter().any(is_ours))
}

/// Traegt Milos Hooks in `settings` ein (oder entfernt sie), ohne fremde Eintraege anzufassen.
pub fn merge_hooks(settings: &mut Value, command: Option<&str>) {
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    for event in HOOK_EVENTS {
        let mut list: Vec<Value> = settings["hooks"][event].as_array().cloned().unwrap_or_default();
        list.retain(|entry| !is_ours(entry));
        if let Some(command) = command {
            // Bei Freigaben und Rueckfragen (AskUserQuestion kommt als PreToolUse) wartet das Skript
            // auf die Antwort des Nutzers in Milo, sonst endet es sofort.
            let timeout = match event {
                "PermissionRequest" => 30,
                "PreToolUse" => 300,
                // Fragt Claude am Ende seines Zugs etwas, ist die Frage 2 Minuten lang in Milo beantwortbar.
                "Stop" => 150,
                _ => 5,
            };
            list.push(json!({ "hooks": [{ "type": "command", "command": format!("{command} {event}"), "timeout": timeout }] }));
        }
        if list.is_empty() {
            if let Some(map) = settings["hooks"].as_object_mut() {
                map.remove(event);
            }
        } else {
            settings["hooks"][event] = Value::Array(list);
        }
    }
    if settings["hooks"].as_object().is_some_and(|m| m.is_empty()) {
        if let Some(map) = settings.as_object_mut() {
            map.remove("hooks");
        }
    }
}

/// Traegt Milos Statuszeile ein (oder entfernt sie). Eine fremde Statuszeile bleibt unangetastet.
pub fn merge_statusline(settings: &mut Value, command: Option<&str>) {
    let ours = settings["statusLine"]["command"].as_str().is_some_and(|c| c.contains(STATUS_MARKER));
    let free = settings.get("statusLine").is_none_or(Value::is_null);
    match command {
        Some(command) if ours || free => settings["statusLine"] = json!({ "type": "command", "command": command, "padding": 0 }),
        None if ours => {
            if let Some(map) = settings.as_object_mut() {
                map.remove("statusLine");
            }
        }
        _ => {}
    }
}

/// Richtet die Verbindung ein oder entfernt sie. Vorher wird eine Sicherungskopie von Claudes
/// Einstellungen angelegt. `script` ist der Pfad zu `claude-hook.mjs`; die Statuszeile liegt daneben.
pub fn set_hooks(script: Option<&Path>) -> anyhow::Result<()> {
    let path = settings_path().ok_or_else(|| anyhow::anyhow!(tr!("Benutzerordner nicht gefunden", "User folder not found")))?;
    let mut settings: Value = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{}", tr!(format!("Claudes settings.json ist nicht lesbar ({e}); ich ändere sie lieber nicht."), format!("Claude's settings.json can't be read ({e}); I'd rather not change it."))))?,
        Err(_) => json!({}),
    };
    let before = settings.clone();
    let command = script.map(node_command);
    merge_hooks(&mut settings, command.as_deref());
    let status_command = script.map(|s| node_command(&s.with_file_name("claude-statusline.mjs")));
    merge_statusline(&mut settings, status_command.as_deref());
    if settings != before {
        if path.exists() {
            std::fs::copy(&path, path.with_extension("json.milo-sicherung"))?;
        } else if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
    }
    Ok(())
}

// ---------- Beobachten ----------

#[cfg(windows)]
fn claude_running() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    // SAFETY: Die Prozessliste wird nur gelesen; die Struktur hat die verlangte Groesse.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("claude.exe") {
                    found = true;
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
fn claude_running() -> bool {
    false
}

/// Meldet der Oberflaeche jede Aenderung von Claudes Zustand. Haelt ausserdem die Skripte aktuell
/// und zeigt dem Hook-Skript mit einem Herzschlag, dass Milo Freigaben entgegennimmt.
pub fn spawn_watcher(app: AppHandle) {
    match install_scripts(&app) {
        // Bestehende Verbindung auf den neuesten Stand bringen (Zeitlimits, Statuszeile).
        Ok(script) if hooks_installed() => {
            if let Err(err) = set_hooks(Some(&script)) {
                log::warn!("Claude-Verbindung nicht aktualisiert: {err}");
            }
        }
        Ok(_) => {}
        Err(err) => log::warn!("Claude-Skripte nicht geschrieben: {err}"),
    }
    std::thread::spawn(move || {
        let dir = bridge_dir(&app);
        let _ = std::fs::create_dir_all(&dir);
        let mut last: Option<Status> = None;
        let mut running = false;
        let mut connected = false;
        let mut tick = 0u32;
        loop {
            // Die Prozessliste und Claudes Einstellungen aendern sich selten: nur alle paar Sekunden pruefen.
            if tick % 4 == 0 {
                sweep(&dir);
                running = claude_running();
                connected = hooks_installed();
                if !running {
                    remove_old_dir();
                }
                let beat = dir.join("milo-approvals.json");
                if connected && app.state::<crate::settings::SettingsStore>().get().claude_approvals {
                    let _ = std::fs::write(&beat, format!("{{\"pid\":{}}}", std::process::id()));
                } else {
                    let _ = std::fs::remove_file(&beat);
                }
            }
            tick = tick.wrapping_add(1);
            let status = status(&dir, running, connected);
            if last.as_ref() != Some(&status) {
                let _ = app.emit(EVENT, &status);
                last = Some(status);
            }
            std::thread::sleep(POLL);
        }
    });
}

/// Raeumt den Ordner unter dem alten App-Namen weg, sobald Claudes Einstellungen nicht mehr darauf
/// zeigen. Laufende Claude-Sitzungen lesen ihre Hooks nur beim Start, deshalb erst ohne Claude.
fn remove_old_dir() {
    let Some(old) = crate::migrate::old_claude_dir() else { return };
    let needle = format!("/.claude/{}/", crate::migrate::OLD_CLAUDE_DIR);
    let still_used = settings_path().and_then(|p| std::fs::read_to_string(p).ok()).is_some_and(|t| t.replace('\\', "/").replace("//", "/").contains(&needle));
    if !still_used && std::fs::remove_dir_all(&old).is_ok() {
        log::info!("Alter Claude-Ordner entfernt: {}", old.display());
    }
}

#[tauri::command]
pub fn claude_status(app: AppHandle) -> Status {
    status(&bridge_dir(&app), claude_running(), hooks_installed())
}

/// Verbindet Milo mit Claude Code (Hooks und Statuszeile) oder trennt die Verbindung wieder.
#[tauri::command]
pub fn connect_claude(app: AppHandle, enable: bool) -> Result<bool, String> {
    let script = install_scripts(&app).map_err(|e| e.to_string())?;
    set_hooks(enable.then_some(script.as_path())).map_err(|e| e.to_string())?;
    Ok(hooks_installed())
}

/// Antwort des Nutzers auf eine Anfrage von Claude. Nur auf Klick oder ausdrueckliches Wort des
/// Nutzers. `reply` ist `{allow, message?}` (Freigabe, Plan), `{answers: {Frage: Antwort}}`
/// (Rueckfrage) oder `{passthrough: true}` (lieber in Claude selbst antworten).
#[tauri::command]
pub fn claude_reply(app: AppHandle, id: String, reply: Value) -> Result<(), String> {
    if id.is_empty() || id.len() > 100 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(tr!("Ungültige Anfrage.", "Invalid request.").into());
    }
    let reply = clean_reply(&reply).ok_or(tr!("Ungültige Antwort.", "Invalid answer."))?;
    let dir = bridge_dir(&app);
    if !dir.join(format!("frage-{id}.json")).exists() {
        return Err(tr!("Claude wartet nicht mehr darauf.", "Claude isn't waiting for that anymore.").into());
    }
    let file = dir.join(format!("antwort-{id}.json"));
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, reply.to_string()).and_then(|_| std::fs::rename(&tmp, &file)).map_err(|e| e.to_string())?;
    let what = if reply.get("answers").is_some() {
        tr!("Claude-Rückfrage beantwortet", "Answered a Claude question")
    } else if reply["passthrough"] == true {
        tr!("Claude-Rückfrage an Claude zurückgegeben", "Handed a Claude question back to Claude")
    } else if reply["allow"] == true {
        tr!("Claude-Freigabe erlaubt", "Allowed a Claude approval")
    } else {
        tr!("Claude-Freigabe abgelehnt", "Denied a Claude approval")
    };
    app.state::<crate::memory::Memory>().log_action("claude", what, true);
    Ok(())
}

/// Laesst nur die erwarteten Felder durch, mit begrenzter Laenge.
fn clean_reply(reply: &Value) -> Option<Value> {
    let cut = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    if reply["passthrough"] == true {
        return Some(json!({ "passthrough": true }));
    }
    if let Some(answers) = reply["answers"].as_object() {
        if answers.is_empty() || answers.len() > 8 {
            return None;
        }
        let clean: serde_json::Map<String, Value> = answers.iter().map(|(q, a)| (cut(q, 500), Value::String(cut(a.as_str().unwrap_or(""), 2000)))).collect();
        return Some(json!({ "answers": clean }));
    }
    let allow = reply["allow"].as_bool()?;
    let message = cut(reply["message"].as_str().unwrap_or(""), 2000);
    Some(if message.is_empty() { json!({ "allow": allow }) } else { json!({ "allow": allow, "message": message }) })
}

/// Aeltere Oberflaechen: nur Erlauben/Ablehnen.
#[tauri::command]
pub fn claude_answer(app: AppHandle, id: String, allow: bool) -> Result<(), String> {
    claude_reply(app, id, json!({ "allow": allow }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn wrap_up_completes_the_whole_task_list() {
        let mut follow = super::Follow::default();
        let items = serde_json::json!({ "items": [
            { "content": "Bauen", "status": "completed" },
            { "content": "Prüfen und berichten", "status": "in_progress" }
        ] });
        follow.plan("mcp__ccd_turn__update_plan", &items);
        assert_eq!(follow.todos[1].status, "in_progress");
        follow.plan("mcp__ccd_turn__wrap_up", &serde_json::json!({ "recap": "Fertig." }));
        assert!(follow.todos.iter().all(|t| t.status == "completed"));
    }

    use super::*;

    fn report(event: &str, kind: &str, tool: &str, message: &str) -> Report {
        Report { event: event.into(), kind: kind.into(), tool: tool.into(), message: message.into(), cwd: "D:\\Projekte\\Milo".into(), transcript: String::new(), file: String::new(), snippet: String::new(), old: String::new() }
    }

    #[test]
    fn events_map_to_what_the_user_needs_to_know() {
        let now = Duration::ZERO;
        assert_eq!(activity_of(&report("UserPromptSubmit", "", "", ""), now), Some(Activity::Working));
        assert_eq!(activity_of(&report("PreToolUse", "", "Bash", ""), now), Some(Activity::Working));
        assert_eq!(activity_of(&report("PreToolUse", "", "AskUserQuestion", ""), now), Some(Activity::Question));
        assert_eq!(activity_of(&report("Notification", "permission_prompt", "", "Claude needs your permission to use Bash"), now), Some(Activity::Confirm));
        // Leerlauf ist keine Frage.
        assert_eq!(activity_of(&report("Notification", "idle_prompt", "", "Claude is waiting for your input"), now), Some(Activity::Idle));
        assert_eq!(activity_of(&report("Notification", "auth_success", "", "Signed in"), now), Some(Activity::Idle));
        assert_eq!(activity_of(&report("Notification", "", "", "Claude is waiting for your input"), now), Some(Activity::Idle));
        assert_eq!(activity_of(&report("Notification", "elicitation_dialog", "", "Server fragt"), now), Some(Activity::Question));
        assert_eq!(activity_of(&report("Stop", "", "", ""), now), Some(Activity::Done));
        // Frage am Ende des Zugs bzw. noch laufender Hintergrundbefehl: nicht "fertig".
        assert_eq!(activity_of(&report("Stop", "question", "", "Welchen Ansatz soll ich bauen?"), now), Some(Activity::Question));
        assert_eq!(activity_of(&report("Stop", "background", "", ""), now), Some(Activity::Working));
        assert_eq!(activity_of(&report("Stop", "background", "", ""), Duration::from_secs(3600)), Some(Activity::Idle));
        assert_eq!(activity_of(&report("Stop", "", "", ""), Duration::from_secs(300)), Some(Activity::Idle));
        assert_eq!(activity_of(&report("SessionEnd", "", "", ""), now), None);
    }

    #[test]
    fn the_most_urgent_session_wins() {
        let reports = vec![
            (report("PreToolUse", "", "Edit", ""), Duration::ZERO),
            (report("Notification", "permission_prompt", "", "Claude needs your permission to use Bash"), Duration::ZERO),
            (report("Stop", "", "", ""), Duration::ZERO),
        ];
        let status = summarize(&reports, true, true);
        assert_eq!(status.activity, Activity::Confirm);
        assert_eq!(status.project, "Milo");
        assert!(status.text.contains("permission"));
        // Ohne aktive Sitzung ist Claude nicht da, auch wenn die App im Infobereich laeuft.
        assert!(!summarize(&[], false, true).present);
        assert!(!summarize(&[], true, false).present);
    }

    #[test]
    fn only_recent_sessions_of_a_running_claude_count() {
        let working = || report("PreToolUse", "", "Edit", "");
        // Vor kurzem gemeldet: da, auch ohne Prozess claude.exe (npm-Installation).
        assert!(summarize(&[(working(), Duration::from_secs(5))], false, true).present);
        // Laenger still und kein Prozess: weg.
        assert!(!summarize(&[(working(), Duration::from_secs(300))], false, true).present);
        // Prozess laeuft, aber seit ueber 10 Minuten nichts passiert: weg.
        assert!(!summarize(&[(working(), Duration::from_secs(11 * 60))], true, true).present);
        assert!(summarize(&[(report("Stop", "", "", ""), Duration::from_secs(5 * 60))], true, true).present);
        // Wartet Claude auf den Nutzer, bleibt die Sitzung laenger.
        let asking = report("PreToolUse", "", "AskUserQuestion", "Welche Farbe?");
        let status = summarize(&[(asking, Duration::from_secs(20 * 60))], true, true);
        assert_eq!((status.present, status.activity, status.text.as_str()), (true, Activity::Question, "Welche Farbe?"));
    }

    #[test]
    fn old_requests_expire() {
        let permission = |until| Permission { id: "a".into(), kind: "ask".into(), tool: "AskUserQuestion".into(), detail: String::new(), cwd: String::new(), questions: Value::Null, plan: String::new(), until };
        let now = 1_000_000;
        assert!(!expired(&permission(now + 5_000), Duration::from_secs(1), now));
        assert!(expired(&permission(now - 11_000), Duration::from_secs(1), now));
        // Aelteres Skript ohne Frist: nach 40 s weg (es wartete hoechstens 25 s).
        assert!(expired(&permission(0), Duration::from_secs(41), now));
        assert!(!expired(&permission(0), Duration::from_secs(10), now));
    }

    #[test]
    fn replies_are_cleaned() {
        assert_eq!(clean_reply(&json!({ "passthrough": true, "x": 1 })), Some(json!({ "passthrough": true })));
        assert_eq!(clean_reply(&json!({ "allow": false, "message": "Bitte nicht" })), Some(json!({ "allow": false, "message": "Bitte nicht" })));
        assert_eq!(clean_reply(&json!({ "answers": { "Welche Farbe?": "Blau" } })), Some(json!({ "answers": { "Welche Farbe?": "Blau" } })));
        assert_eq!(clean_reply(&json!({ "answers": {} })), None);
        assert_eq!(clean_reply(&json!({ "foo": 1 })), None);
    }

    #[test]
    fn transcript_gives_text_plan_and_steps() {
        let dir = std::env::temp_dir().join(format!("milo-follow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let line = |content: Value| json!({ "type": "assistant", "timestamp": "2026-11-04T10:00:00Z", "message": { "content": content } }).to_string();
        let mut text = String::new();
        text += &line(json!([{ "type": "text", "text": "Ich lege los." }, { "type": "tool_use", "name": "mcp__ccd_turn__update_plan", "input": { "items": [{ "content": "Lesen", "status": "in_progress" }, { "content": "Bauen", "status": "pending" }] } }]));
        text += "
";
        text += &line(json!([{ "type": "tool_use", "name": "Edit", "input": { "file_path": "D:\\x\\main.rs", "old_string": "a", "new_string": "b" } }]));
        text += "
";
        text += &json!({ "type": "user", "message": { "content": "egal" } }).to_string();
        text += "
";
        std::fs::write(&path, &text).unwrap();
        let mut follow = Follow::default();
        follow.update(&path);
        assert_eq!(follow.said, "Ich lege los.");
        assert_eq!(follow.todos.len(), 2);
        assert_eq!(follow.steps.back().map(|s| (s.tool.as_str(), s.what.as_str())), Some(("Edit", "main.rs")));
        // Nur Neues wird gelesen; eine halbe Zeile wartet aufs naechste Mal.
        let more = line(json!([{ "type": "tool_use", "name": "mcp__ccd_turn__update_plan", "input": { "advance": true } }]));
        let (head, tail) = more.split_at(20);
        std::fs::write(&path, format!("{text}{head}")).unwrap();
        follow.update(&path);
        assert_eq!(follow.todos[0].status, "in_progress");
        std::fs::write(&path, format!("{text}{head}{tail}
")).unwrap();
        follow.update(&path);
        assert_eq!((follow.todos[0].status.as_str(), follow.todos[1].status.as_str()), ("completed", "in_progress"));
        assert_eq!(follow.steps.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hooks_merge_keeps_foreign_entries_and_is_reversible() {
        let mut settings = json!({
            "model": "opus",
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "mein-eigenes-skript" }] }] }
        });
        merge_hooks(&mut settings, Some("node \"C:\\milo\\claude-hook.mjs\""));
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(settings["hooks"]["Notification"][0]["hooks"][0]["command"].as_str().unwrap().ends_with("claude-hook.mjs\" Notification"));
        // Zweimal einrichten verdoppelt nichts.
        merge_hooks(&mut settings, Some("node \"C:\\milo\\claude-hook.mjs\""));
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
        // Freigaben brauchen mehr Zeit als die anderen Ereignisse.
        assert_eq!(settings["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"], 30);
        assert_eq!(settings["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"], 300);
        assert_eq!(settings["hooks"]["Stop"][1]["hooks"][0]["timeout"], 150);
        // Entfernen laesst nur das Fremde uebrig.
        merge_hooks(&mut settings, None);
        assert_eq!(settings, json!({ "model": "opus", "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "mein-eigenes-skript" }] }] } }));
    }

    #[test]
    fn statusline_is_added_only_when_free_or_ours() {
        let mut free = json!({});
        merge_statusline(&mut free, Some("node \"C:/milo/claude-statusline.mjs\""));
        assert_eq!(free["statusLine"]["type"], "command");
        merge_statusline(&mut free, None);
        assert!(free.get("statusLine").is_none());
        let mut foreign = json!({ "statusLine": { "type": "command", "command": "mein-status.sh" } });
        merge_statusline(&mut foreign, Some("node \"C:/milo/claude-statusline.mjs\""));
        assert_eq!(foreign["statusLine"]["command"], "mein-status.sh");
        merge_statusline(&mut foreign, None);
        assert_eq!(foreign["statusLine"]["command"], "mein-status.sh");
    }

    #[test]
    fn live_view_uses_the_newest_tool_report() {
        let mut old = report("PostToolUse", "", "Edit", "");
        old.file = "alt.rs".into();
        let mut new = report("PreToolUse", "", "Write", "");
        new.file = "neu.rs".into();
        new.snippet = "fn main() {}".into();
        let status = summarize(&[(old, Duration::from_secs(9)), (new, Duration::from_secs(1))], true, true);
        assert_eq!((status.tool.as_str(), status.file.as_str(), status.snippet.as_str()), ("Write", "neu.rs", "fn main() {}"));
    }

    #[test]
    fn bridge_files_are_told_apart() {
        assert!(is_session_file("abc-123.json"));
        assert!(!is_session_file("limits.json"));
        assert!(!is_session_file("frage-x.json"));
        assert!(!is_session_file("antwort-x.json"));
        assert!(!is_session_file("milo-approvals.json"));
        assert!(!is_session_file("abc.json.tmp"));
    }
}
