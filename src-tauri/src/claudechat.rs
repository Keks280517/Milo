//! Mit Claude Code direkt aus Milos Chat schreiben ("/claude …").
//!
//! Milo startet dafuer den offiziellen Befehl `claude` im Druckmodus (`-p`) im Projektordner, auf
//! Wunsch mit `--resume <Sitzung>`, damit es dieselbe Unterhaltung wie in der Claude-App ist. Die
//! Nachricht geht ueber die Standardeingabe hinein (nie ueber die Kommandozeile), die Antwort kommt
//! als Datenstrom (`stream-json`) zurueck und erscheint Stueck fuer Stueck im Chat.
//!
//! Slash-Befehle, Skills und Plugins funktionieren wie in Claude selbst; `/plan` schaltet den
//! Planmodus ein. Freigaben laufen ueber dieselben Hooks wie sonst und erscheinen in der Insel.
//! Die Sitzungsliste stammt aus Claudes eigenem Verlauf (`~/.claude/projects`), nur lesend.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

pub const EVENT: &str = "milo://claude-chat";
/// So viele Projekte und Sitzungen je Projekt zeigt die Auswahl.
const PROJECTS: usize = 30;
const SESSIONS: usize = 8;

#[derive(Default)]
pub struct ClaudeChat {
    /// Der laufende Claude-Aufruf (zum Abbrechen).
    child: Mutex<Option<Child>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    /// Zuletzt geaendert, Millisekunden seit 1970.
    pub at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub cwd: String,
    pub name: String,
    pub sessions: Vec<Session>,
}

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default()
}

/// Wo liegt das Programm `claude`? (npm-Installation, eigener Installer, PATH)
pub fn find_claude() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(PathBuf::from(appdata).join(r"npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe"));
    }
    candidates.push(home().join(r".local\bin\claude.exe"));
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join("claude.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Ein Stueck einer Datei als Text (Anfang oder Ende).
fn read_part(path: &Path, from_end: bool, max: u64) -> String {
    let Ok(mut file) = std::fs::File::open(path) else { return String::new() };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let take = len.min(max);
    if from_end && file.seek(SeekFrom::Start(len - take)).is_err() {
        return String::new();
    }
    let mut buffer = vec![0u8; take as usize];
    if file.read_exact(&mut buffer).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

/// Ordner und Titel einer Sitzung aus ihrer Verlaufsdatei.
fn session_info(path: &Path) -> Option<(String, String)> {
    let head = read_part(path, false, 96 * 1024);
    let tail = read_part(path, true, 192 * 1024);
    let mut cwd = String::new();
    let mut first_prompt = String::new();
    for line in head.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        if cwd.is_empty() {
            if let Some(c) = entry["cwd"].as_str() {
                cwd = c.to_string();
            }
        }
        if first_prompt.is_empty() && entry["type"] == "user" {
            if let Some(text) = entry["message"]["content"].as_str() {
                let text = text.trim();
                if !text.starts_with('<') && !text.is_empty() {
                    first_prompt = text.chars().take(80).collect();
                }
            }
        }
        if !cwd.is_empty() && !first_prompt.is_empty() {
            break;
        }
    }
    // Ein vergebener Titel (Claude-App) steht meist am Ende, beim kurzen Verlauf auch am Anfang.
    let title = [tail.as_str(), head.as_str()]
        .iter()
        .flat_map(|part| part.lines().rev())
        .filter(|l| l.contains("\"customTitle\""))
        .find_map(|l| serde_json::from_str::<Value>(l).ok().and_then(|e| e["customTitle"].as_str().map(String::from)))
        .unwrap_or(first_prompt);
    if cwd.is_empty() {
        return None;
    }
    Some((cwd, title.lines().next().unwrap_or_default().to_string()))
}

fn millis(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Projekte mit ihren letzten Sitzungen, das zuletzt benutzte zuerst.
#[tauri::command]
pub async fn claude_chat_projects() -> Vec<Project> {
    tauri::async_runtime::spawn_blocking(list_projects).await.unwrap_or_default()
}

fn list_projects() -> Vec<Project> {
    let root = home().join(".claude").join("projects");
    let Ok(dirs) = std::fs::read_dir(&root) else { return Vec::new() };
    let mut projects: Vec<Project> = Vec::new();
    for dir in dirs.flatten().filter(|d| d.path().is_dir()) {
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        let mut sessions: Vec<(SystemTime, PathBuf)> = files
            .flatten()
            .filter(|f| f.path().extension().is_some_and(|e| e == "jsonl"))
            .filter_map(|f| Some((f.metadata().ok()?.modified().ok()?, f.path())))
            .collect();
        sessions.sort_by(|a, b| b.0.cmp(&a.0));
        let mut cwd = String::new();
        let mut list = Vec::new();
        for (at, path) in sessions.into_iter().take(SESSIONS) {
            let Some((session_cwd, title)) = session_info(&path) else { continue };
            if cwd.is_empty() {
                cwd = session_cwd;
            }
            let id = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            list.push(Session { id, title, at: millis(at) });
        }
        let lower = cwd.to_lowercase();
        // Temporaere Arbeitsordner der Claude-App und verschwundene Ordner nicht anbieten.
        if list.is_empty() || lower.contains(r"\scratch") || lower.contains(r"\appdata\local\temp") || !Path::new(&cwd).is_dir() {
            continue;
        }
        let name = Path::new(&cwd).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| cwd.clone());
        projects.push(Project { cwd, name, sessions: list });
    }
    projects.sort_by(|a, b| b.sessions[0].at.cmp(&a.sessions[0].at));
    projects.truncate(PROJECTS);
    projects
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// Woher: "eigene", "projekt", Name des Plugins oder "claude".
    pub source: String,
}

/// Feld aus dem Kopf einer Befehls- oder Skill-Datei (`description: …`).
fn front(text: &str, key: &str) -> String {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix(key).and_then(|rest| rest.strip_prefix(':')) {
            let value = value.trim().trim_matches(['"', '\'']);
            let first: String = value.chars().take(140).collect();
            return if value.chars().count() > 140 { format!("{first}…") } else { first };
        }
    }
    String::new()
}

/// Befehle aus einem `commands`-Ordner (Unterordner werden zu "ordner:befehl").
fn scan_commands(dir: &Path, prefix: &str, source: &str, out: &mut Vec<SlashCommand>) {
    fn walk(dir: &Path, base: &Path, prefix: &str, source: &str, out: &mut Vec<SlashCommand>, depth: u32) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 3 {
                walk(&path, base, prefix, source, out, depth + 1);
            } else if path.extension().is_some_and(|e| e == "md") {
                let relative = path.strip_prefix(base).unwrap_or(&path).with_extension("");
                let name = relative.to_string_lossy().replace(['\\', '/'], ":");
                let text = read_part(&path, false, 8 * 1024);
                out.push(SlashCommand { name: format!("{prefix}{name}"), description: front(&text, "description"), source: source.into() });
            }
        }
    }
    walk(dir, dir, prefix, source, out, 0);
}

/// Skills aus einem `skills`-Ordner (je Unterordner eine SKILL.md).
fn scan_skills(dir: &Path, prefix: &str, source: &str, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let file = entry.path().join("SKILL.md");
        if !file.is_file() {
            continue;
        }
        let text = read_part(&file, false, 8 * 1024);
        let name = Some(front(&text, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
        out.push(SlashCommand { name: format!("{prefix}{name}"), description: front(&text, "description"), source: source.into() });
    }
}

/// Alle Slash-Befehle fuer ein Projekt: eigene Befehle und Skills, die des Projekts und die aller
/// installierten Plugins, jeweils mit Beschreibung. Claudes eingebaute kommen beim Start dazu.
#[tauri::command]
pub async fn claude_chat_commands(cwd: String) -> Vec<SlashCommand> {
    tauri::async_runtime::spawn_blocking(move || list_commands(&cwd)).await.unwrap_or_default()
}

fn list_commands(cwd: &str) -> Vec<SlashCommand> {
    let claude = home().join(".claude");
    let mut out = Vec::new();
    scan_commands(&claude.join("commands"), "", "eigene", &mut out);
    scan_skills(&claude.join("skills"), "", "eigene", &mut out);
    if !cwd.is_empty() {
        let project = Path::new(cwd).join(".claude");
        scan_commands(&project.join("commands"), "", "projekt", &mut out);
        scan_skills(&project.join("skills"), "", "projekt", &mut out);
    }
    let installed = std::fs::read_to_string(claude.join("plugins").join("installed_plugins.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    if let Some(plugins) = installed.as_ref().and_then(|v| v["plugins"].as_object()) {
        for (key, installs) in plugins {
            let plugin = key.split('@').next().unwrap_or(key);
            let Some(path) = installs.as_array().and_then(|list| list.last()).and_then(|i| i["installPath"].as_str()) else { continue };
            let root = Path::new(path);
            scan_commands(&root.join("commands"), &format!("{plugin}:"), plugin, &mut out);
            scan_skills(&root.join("skills"), &format!("{plugin}:"), plugin, &mut out);
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.name.clone()));
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Arbeitet Claude gerade in dieser Sitzung (z. B. in der Claude-App)? Dann nichts hineinschreiben.
fn session_busy(session: &str) -> bool {
    let file = home().join(".claude").join("milo").join("bridge").join(format!("{session}.json"));
    let Ok(meta) = std::fs::metadata(&file) else { return false };
    let fresh = meta.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok()).is_some_and(|age| age < Duration::from_secs(90));
    let Ok(text) = std::fs::read_to_string(&file) else { return false };
    let event = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["event"].as_str().map(String::from)).unwrap_or_default();
    fresh && matches!(event.as_str(), "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PermissionRequest")
}

/// Bisheriger Verlauf einer Sitzung (die letzten Nachrichten), damit man in Milo sieht, was zuvor
/// geschrieben wurde, auch in der Claude-App. Nur lesend.
#[tauri::command]
pub async fn claude_chat_history(session: String) -> Vec<Value> {
    if session.is_empty() || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Vec::new();
    }
    tauri::async_runtime::spawn_blocking(move || history(&session)).await.unwrap_or_default()
}

fn history(session: &str) -> Vec<Value> {
    let root = home().join(".claude").join("projects");
    let Some(file) = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|d| d.path().join(format!("{session}.jsonl")))
        .find(|p| p.is_file())
    else {
        return Vec::new();
    };
    let tail = read_part(&file, true, 3 * 1024 * 1024);
    let mut lines: Vec<Value> = Vec::new();
    // Die erste Zeile kann abgeschnitten sein; unlesbare Zeilen werden einfach uebersprungen.
    for line in tail.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        if entry["isSidechain"] == true || entry["isMeta"] == true {
            continue;
        }
        let content = &entry["message"]["content"];
        match entry["type"].as_str().unwrap_or_default() {
            "user" => {
                let text = match content {
                    Value::String(s) => s.clone(),
                    Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
                    _ => String::new(),
                };
                let text = text.trim();
                // Systemhinweise, Befehlsausgaben und Werkzeugergebnisse sind keine Nachrichten des Nutzers.
                if text.is_empty() || text.starts_with('<') || text.starts_with("[Request interrupted") {
                    continue;
                }
                lines.push(json!({ "role": "user", "text": text.chars().take(4000).collect::<String>(), "tools": [] }));
            }
            "assistant" => {
                let Some(blocks) = content.as_array() else { continue };
                let text: Vec<&str> = blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
                let tools: Vec<Value> = blocks
                    .iter()
                    .filter(|b| b["type"] == "tool_use")
                    .map(|b| json!({ "name": b["name"], "detail": tool_detail(&b["input"]) }))
                    .collect();
                // Aufeinanderfolgende Schritte von Claude zu einer Antwort zusammenfassen.
                match lines.last_mut() {
                    Some(last) if last["role"] == "claude" => {
                        if !text.is_empty() {
                            let before = last["text"].as_str().unwrap_or_default().to_string();
                            last["text"] = json!(if before.is_empty() { text.join("\n\n") } else { format!("{before}\n\n{}", text.join("\n\n")) });
                        }
                        if let Some(list) = last["tools"].as_array_mut() {
                            list.extend(tools);
                        }
                    }
                    _ => lines.push(json!({ "role": "claude", "text": text.join("\n\n"), "tools": tools })),
                }
            }
            _ => {}
        }
    }
    // Nur das Ende: die letzten 30 Beitraege, Werkzeugschritte je Antwort gekuerzt.
    let start = lines.len().saturating_sub(30);
    let mut recent = lines.split_off(start);
    for line in &mut recent {
        if let Some(list) = line["tools"].as_array_mut() {
            if list.len() > 6 {
                let skipped = list.len() - 6;
                list.drain(..skipped);
            }
        }
    }
    recent
}

/// Kurze Beschreibung eines Werkzeugaufrufs fuer den Chat.
fn tool_detail(input: &Value) -> String {
    let text = ["file_path", "command", "pattern", "url", "query", "description", "skill", "path"]
        .iter()
        .find_map(|key| input[key].as_str())
        .unwrap_or_default();
    let text = text.lines().next().unwrap_or_default();
    let short: String = text.chars().take(90).collect();
    if text.chars().count() > 90 { format!("{short}…") } else { short }
}

/// Uebersetzt eine Zeile aus Claudes Datenstrom in ein schlankes Ereignis fuer den Chat.
fn translate(line: &str) -> Vec<Value> {
    let Ok(entry) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    match entry["type"].as_str().unwrap_or_default() {
        "system" if entry["subtype"] == "init" => vec![json!({
            "kind": "init",
            "session": entry["session_id"],
            "commands": entry["slash_commands"],
            "model": entry["model"],
        })],
        "assistant" => entry["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| match block["type"].as_str()? {
                "text" => Some(json!({ "kind": "text", "text": block["text"] })),
                "tool_use" => Some(json!({ "kind": "tool", "name": block["name"], "detail": tool_detail(&block["input"]) })),
                _ => None,
            })
            .collect(),
        "result" => {
            let text = entry["result"].as_str().unwrap_or_default();
            let login = text.contains("authenticate") || text.contains("/login") || text.contains("OAuth");
            vec![json!({
                "kind": "done",
                "error": entry["is_error"].as_bool().unwrap_or(false) || entry["subtype"] != "success" || login,
                "login": login,
                "text": text,
                "session": entry["session_id"],
            })]
        }
        _ => Vec::new(),
    }
}

/// Schickt eine Nachricht an Claude. `session`: weiterschreiben in dieser Sitzung, sonst neue Sitzung
/// im Ordner `cwd`. `plan`: im Planmodus (Claude plant erst und aendert nichts).
#[tauri::command]
pub async fn claude_chat_send(app: AppHandle, text: String, session: Option<String>, cwd: String, plan: bool) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(tr!("Die Nachricht ist leer.", "The message is empty.").into());
    }
    let exe = find_claude().ok_or(tr!(
        "Claude Code ist nicht installiert. Installiere es mit „npm install -g @anthropic-ai/claude-code“.",
        "Claude Code isn't installed. Install it with \"npm install -g @anthropic-ai/claude-code\"."
    ))?;
    if !Path::new(&cwd).is_dir() {
        return Err(tr!("Den Projektordner gibt es nicht mehr.", "The project folder no longer exists.").into());
    }
    let session = session.filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    if let Some(id) = &session {
        if session_busy(id) {
            return Err(tr!(
                "Claude arbeitet gerade in dieser Sitzung. Warte kurz, bis es fertig ist, oder starte einen neuen Chat.",
                "Claude is working in this session right now. Wait until it's done, or start a new chat."
            )
            .into());
        }
    }
    let state = app.state::<ClaudeChat>();
    if state.child.lock().unwrap().is_some() {
        return Err(tr!("Claude antwortet noch auf die letzte Nachricht.", "Claude is still answering the last message.").into());
    }

    let mut command = Command::new(&exe);
    command.args(["-p", "--output-format", "stream-json", "--verbose"]);
    if let Some(id) = &session {
        command.args(["--resume", id]);
    }
    if plan {
        command.args(["--permission-mode", "plan"]);
    }
    // Das Hook-Skript erkennt so, dass Milo selbst fragt: Rueckfragen am Ende kommen in den Chat.
    command.env("MILO_CHAT", "1").current_dir(&cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| tr!(format!("Claude ließ sich nicht starten: {e}"), format!("Couldn't start Claude: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    let stdout = child.stdout.take().ok_or("stdout")?;
    let stderr = child.stderr.take();
    *state.child.lock().unwrap() = Some(child);

    let handle = app.clone();
    std::thread::spawn(move || {
        let mut finished = false;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            for event in translate(&line) {
                finished |= event["kind"] == "done";
                let _ = handle.emit(EVENT, event);
            }
        }
        let mut problem = String::new();
        if let Some(mut err) = stderr {
            let _ = err.read_to_string(&mut problem);
        }
        let child = handle.state::<ClaudeChat>().child.lock().unwrap().take();
        let stopped = child.is_none();
        if let Some(mut child) = child {
            let _ = child.wait();
        }
        if !finished {
            let text = if stopped {
                tr!("Abgebrochen.", "Stopped.").to_string()
            } else {
                problem.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(tr!("Claude hat ohne Antwort aufgehört.", "Claude stopped without an answer.")).chars().take(300).collect()
            };
            let login = text.contains("authenticate") || text.contains("login");
            let _ = handle.emit(EVENT, json!({ "kind": "done", "error": !stopped, "login": login, "text": text }));
        }
    });
    Ok(())
}

/// Bricht die laufende Antwort ab.
#[tauri::command]
pub fn claude_chat_stop(app: AppHandle) {
    if let Some(mut child) = app.state::<ClaudeChat>().child.lock().unwrap().take() {
        let _ = child.kill();
    }
}

/// Oeffnet ein Fenster mit Claude Code, damit man sich einmal anmeldet (/login).
#[tauri::command]
pub fn claude_chat_login() -> Result<(), String> {
    let exe = find_claude().ok_or(tr!("Claude Code ist nicht installiert.", "Claude Code isn't installed."))?;
    let mut command = Command::new(&exe);
    command.arg("/login").current_dir(home());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        command.creation_flags(CREATE_NEW_CONSOLE);
    }
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_lines_become_chat_events() {
        let init = r#"{"type":"system","subtype":"init","session_id":"abc","slash_commands":["plan","code-review"],"model":"opus"}"#;
        assert_eq!(translate(init)[0]["kind"], "init");
        assert_eq!(translate(init)[0]["commands"][1], "code-review");
        let said = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hallo"},{"type":"tool_use","name":"Read","input":{"file_path":"D:\\x\\a.rs"}}]}}"#;
        let events = translate(said);
        assert_eq!(events[0]["text"], "Hallo");
        assert_eq!(events[1]["name"], "Read");
        assert_eq!(events[1]["detail"], "D:\\x\\a.rs");
        let login = r#"{"type":"result","subtype":"success","is_error":true,"result":"Failed to authenticate: OAuth session expired"}"#;
        assert_eq!(translate(login)[0]["login"], true);
        assert!(translate("kein json").is_empty());
    }

    #[test]
    fn frontmatter_is_read() {
        let text = "---\nname: code-review\ndescription: \"Prüft den Diff\"\n---\nInhalt";
        assert_eq!(front(text, "name"), "code-review");
        assert_eq!(front(text, "description"), "Prüft den Diff");
        assert_eq!(front("kein kopf", "name"), "");
    }

    /// Nur von Hand: Verlauf der neuesten Sitzung, wie Milo ihn zeigt.
    #[test]
    #[ignore]
    fn live_history() {
        let newest = &list_projects()[0];
        let lines = history(&newest.sessions[0].id);
        println!("{} Beiträge aus „{}“", lines.len(), newest.sessions[0].title);
        for line in lines.iter().rev().take(6).rev() {
            let text: String = line["text"].as_str().unwrap_or_default().chars().take(90).collect();
            println!("[{}] {} Werkzeuge | {}", line["role"].as_str().unwrap_or_default(), line["tools"].as_array().map(|t| t.len()).unwrap_or(0), text.replace('\n', " "));
        }
    }

    /// Nur von Hand: welche Befehle findet Milo auf diesem PC?
    #[test]
    #[ignore]
    fn live_commands() {
        // Der Projektordner (eine Ebene ueber src-tauri).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_string_lossy().to_string();
        for c in list_commands(&root) {
            println!("/{} [{}] {}", c.name, c.source, c.description);
        }
    }
}
