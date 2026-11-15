//! Groq: sehr schnelle offene Modelle ueber eine OpenAI-kompatible Schnittstelle
//! (`https://api.groq.com/openai/v1`, laut console.groq.com/docs/openai).
//!
//! Milo nutzt Groq fuer leichte Anfragen samt Werkzeugen (schnelles erstes Wort) und bei schweren
//! Anfragen fuer eine Vorueberlegung in Sekunden, die Gemini dann gruendlich ausarbeitet. Faellt
//! einer der beiden aus (Kontingent, Ueberlastung), springt der andere ein.
//!
//! Der Verlauf bleibt im Gemini-Format; hier wird er beim Senden in OpenAI-Nachrichten uebersetzt
//! und die Antwort wieder zurueck. Der Schluessel liegt nur in der Windows-Anmeldeinformationsverwaltung.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::gemini::{vault, Call, ErrorKind, Turn};

const BASE: &str = "https://api.groq.com/openai/v1";
const CREDENTIAL: &str = "Milo/Groq-API-Schluessel";
/// Produktionsmodell laut console.groq.com/docs/models (Stand 04.11.2026); einstellbar.
pub const DEFAULT_MODEL: &str = "openai/gpt-oss-120b";

/// Adresse der API; nur Entwicklungsversionen lassen sich auf eine lokale Attrappe umbiegen.
fn base() -> String {
    if cfg!(debug_assertions) {
        if let Ok(other) = std::env::var("MILO_GROQ_BASE") {
            if other.starts_with("http://127.0.0.1:") {
                return other;
            }
        }
    }
    BASE.to_string()
}

pub fn has_key() -> bool {
    vault::read(CREDENTIAL).is_some()
}

fn key() -> Result<String> {
    vault::read(CREDENTIAL).ok_or_else(|| anyhow!(tr!("Es ist kein Groq-Schlüssel hinterlegt.", "No Groq key has been added.")))
}

pub fn store_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.len() < 20 || key.len() > 200 || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!(tr!("Das sieht nicht nach einem Groq-Schlüssel aus.", "That doesn't look like a Groq key."));
    }
    vault::write(CREDENTIAL, key)
}

pub fn delete_key() -> bool {
    vault::delete(CREDENTIAL)
}

#[derive(Debug)]
struct GroqError {
    kind: ErrorKind,
    message: String,
}

impl std::fmt::Display for GroqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GroqError {}

pub(crate) fn fail(kind: ErrorKind, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(GroqError { kind, message: message.into() })
}

/// Art eines Groq-Fehlers (auch in einer Kette); `None`, wenn es keiner ist.
pub fn kind_of(err: &anyhow::Error) -> Option<ErrorKind> {
    err.chain().find_map(|e| e.downcast_ref::<GroqError>().map(|g| g.kind))
}

fn classify(status: u16) -> ErrorKind {
    match status {
        // 413: Anfrage groesser als das Token-Kontingent pro Minute.
        429 | 413 => ErrorKind::Quota,
        401 | 403 => ErrorKind::Key,
        404 => ErrorKind::Missing,
        s if s >= 500 => ErrorKind::Busy,
        _ => ErrorKind::Other,
    }
}

fn api_message(status: u16, body: &str) -> String {
    let parsed: Value = serde_json::from_str(body).unwrap_or_default();
    let detail = parsed["error"]["message"].as_str().unwrap_or("").chars().take(200).collect::<String>();
    match classify(status) {
        ErrorKind::Quota => tr!("Groq hat sein Kontingent gerade aufgebraucht.", "Groq has used up its quota right now.").into(),
        ErrorKind::Key => tr!("Groq hat den Schlüssel nicht angenommen.", "Groq didn't accept the key.").into(),
        ErrorKind::Missing => tr!("Dieses Groq-Modell gibt es nicht (mehr).", "This Groq model doesn't exist (anymore).").into(),
        ErrorKind::Busy => tr!("Groq ist gerade überlastet.", "Groq is overloaded right now.").into(),
        _ => tr!(format!("Groq meldet einen Fehler ({status}): {detail}"), format!("Groq reports an error ({status}): {detail}")),
    }
}

pub struct Groq {
    http: reqwest::Client,
    /// Nach einem 429 bis hierhin nicht fragen.
    blocked_until: Mutex<Option<Instant>>,
}

impl Groq {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .pool_idle_timeout(Duration::from_secs(300))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .expect("HTTP-Client");
        Self { http, blocked_until: Mutex::new(None) }
    }

    /// Schluessel da und gerade nicht wegen Kontingent gesperrt.
    pub fn available(&self) -> bool {
        has_key() && self.blocked_until.lock().unwrap().is_none_or(|until| Instant::now() >= until)
    }

    fn block_for(&self, seconds: u64) {
        *self.blocked_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(seconds.clamp(5, 3600)));
    }

    /// Textmodelle fuer diesen Schluessel (ohne Sprach-, Vorlese- und Waechtermodelle).
    pub async fn models(&self) -> Result<Vec<String>> {
        let response = self.http.get(format!("{}/models", base())).bearer_auth(key()?).timeout(Duration::from_secs(15)).send().await.map_err(offline)?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        if status != 200 {
            return Err(fail(classify(status), api_message(status, &text)));
        }
        Ok(chat_models(&serde_json::from_str(&text).unwrap_or_default()))
    }

    /// Ein Gespraechsschritt mit Strom, wie `Gemini::turn`: Verlauf im Gemini-Format, Werkzeuge im
    /// OpenAI-Format (so wie `tools::definitions` sie liefert). Das Ergebnis ist wieder Gemini-Format.
    pub async fn turn(&self, model: &str, system: &str, contents: &[Value], tools: &[Value], first_wait: Duration, mut on_token: impl FnMut(&str) -> bool) -> Result<Turn> {
        let mut body = json!({ "model": model, "messages": to_messages(system, contents), "stream": true });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }
        let response = self.http.post(format!("{}/chat/completions", base())).bearer_auth(key()?).json(&body).send().await.map_err(offline)?;
        let status = response.status().as_u16();
        if status != 200 {
            let retry = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<f64>().ok()).map(|s| s.ceil() as u64);
            let text = response.text().await.unwrap_or_default();
            if classify(status) == ErrorKind::Quota {
                self.block_for(retry.unwrap_or(60));
            }
            log::warn!("Groq {model} meldet {status}: {}", text.chars().take(300).collect::<String>());
            return Err(fail(classify(status), api_message(status, &text)));
        }

        let mut collect = Collect::default();
        let mut stream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut first = true;
        loop {
            let wait = if first { first_wait } else { Duration::from_secs(60) };
            let chunk = match tokio::time::timeout(wait, stream.next()).await {
                Err(_) => return Err(fail(ErrorKind::Slow, tr!(format!("Groq ({model}) antwortet zu langsam."), format!("Groq ({model}) is answering too slowly.")))),
                Ok(None) => break,
                Ok(Some(chunk)) => chunk.map_err(offline)?,
            };
            first = false;
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=end).collect();
                if let Some(text) = collect.line(&line)? {
                    if !on_token(&text) {
                        return Ok(collect.finish(false));
                    }
                }
            }
        }
        if let Some(text) = collect.line(&buffer)? {
            on_token(&text);
        }
        let turn = collect.finish(true);
        if turn.text.trim().is_empty() && turn.calls.is_empty() {
            return Err(fail(ErrorKind::Busy, tr!("Groq hat nichts geantwortet.", "Groq didn't answer.")));
        }
        Ok(turn)
    }
}

fn offline(err: reqwest::Error) -> anyhow::Error {
    if err.is_connect() || err.is_timeout() {
        fail(ErrorKind::Offline, tr!("Ich erreiche Groq nicht. Besteht eine Internetverbindung?", "I can't reach Groq. Are you connected to the internet?"))
    } else {
        fail(ErrorKind::Offline, tr!(format!("Die Verbindung zu Groq ist abgebrochen ({}).", err.without_url()), format!("The connection to Groq dropped ({}).", err.without_url())))
    }
}

/// Werkzeugliste in Kurzform: nur der erste Satz jeder Beschreibung, ohne Erklaerungen zu den
/// einzelnen Angaben (Aufzaehlungen bleiben). Groqs Gratis-Kontingent zaehlt Tokens pro Minute;
/// die volle Liste allein waere schon fast eine Minute Kontingent.
pub fn compact_tools(tools: &[Value]) -> Vec<Value> {
    fn first_sentence(text: &str) -> String {
        let end = text.find(". ").map(|i| i + 1).unwrap_or(text.len());
        text[..end].chars().take(140).collect()
    }
    fn strip(schema: &Value) -> Value {
        match schema {
            Value::Object(map) => Value::Object(map.iter().filter(|(k, _)| k.as_str() != "description").map(|(k, v)| (k.clone(), strip(v))).collect()),
            Value::Array(items) => Value::Array(items.iter().map(strip).collect()),
            other => other.clone(),
        }
    }
    tools
        .iter()
        .map(|tool| {
            let mut tool = tool.clone();
            let description = first_sentence(tool["function"]["description"].as_str().unwrap_or(""));
            tool["function"]["description"] = json!(description);
            tool["function"]["parameters"] = strip(&tool["function"]["parameters"]);
            tool
        })
        .collect()
}

/// Modell-IDs aus `GET /models`, nur solche fuer Text.
pub fn chat_models(list: &Value) -> Vec<String> {
    let mut models: Vec<String> = list["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| !["whisper", "orpheus", "guard", "tts", "playai"].iter().any(|skip| id.contains(skip)))
        .map(str::to_string)
        .collect();
    models.sort();
    models
}

/// Uebersetzt Systemtext und Gemini-Verlauf in OpenAI-Nachrichten. Werkzeugaufrufe bekommen
/// feste IDs (Nachricht und Position), die Ergebnisse in der folgenden Nachricht dieselben.
pub fn to_messages(system: &str, contents: &[Value]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
    let mut open_calls: Vec<String> = Vec::new();
    for (i, message) in contents.iter().enumerate() {
        let parts = message["parts"].as_array().cloned().unwrap_or_default();
        if message["role"] == "model" {
            let text: String = parts.iter().filter(|p| p["thought"].as_bool() != Some(true)).filter_map(|p| p["text"].as_str()).collect();
            let calls: Vec<Value> = parts
                .iter()
                .filter_map(|p| p.get("functionCall"))
                .enumerate()
                .map(|(j, call)| {
                    json!({
                        "id": format!("call_{i}_{j}"),
                        "type": "function",
                        "function": { "name": call["name"], "arguments": call.get("args").cloned().unwrap_or(json!({})).to_string() },
                    })
                })
                .collect();
            open_calls = calls.iter().filter_map(|c| c["id"].as_str().map(str::to_string)).collect();
            let mut assistant = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
            if !calls.is_empty() {
                assistant["tool_calls"] = json!(calls);
            } else if text.is_empty() {
                continue;
            }
            out.push(assistant);
        } else {
            let mut texts: Vec<String> = Vec::new();
            let mut answered = 0;
            for part in &parts {
                if let Some(response) = part.get("functionResponse") {
                    let id = open_calls.get(answered).cloned();
                    answered += 1;
                    let result = &response["response"]["result"];
                    let content = result.as_str().map(str::to_string).unwrap_or_else(|| result.to_string());
                    match id {
                        Some(id) => out.push(json!({ "role": "tool", "tool_call_id": id, "content": content })),
                        None => texts.push(format!("[Ergebnis von {}: {content}]", response["name"].as_str().unwrap_or("Werkzeug"))),
                    }
                } else if let Some(text) = part["text"].as_str() {
                    texts.push(text.to_string());
                } else if part.get("inlineData").is_some() {
                    texts.push("[Bild – dieses Modell kann keine Bilder sehen]".into());
                }
            }
            open_calls.clear();
            if !texts.is_empty() {
                out.push(json!({ "role": "user", "content": texts.join("\n") }));
            }
        }
    }
    out
}

/// Sammelt einen gestreamten Beitrag (Text und Werkzeugaufrufe, die stueckweise kommen).
#[derive(Default)]
pub(crate) struct Collect {
    text: String,
    /// Je Index: Name und bisher gekommene Argumente (als Text).
    calls: Vec<(String, String)>,
    tokens: u64,
}

impl Collect {
    pub(crate) fn line(&mut self, line: &[u8]) -> Result<Option<String>> {
        let line = std::str::from_utf8(line).unwrap_or_default().trim();
        let Some(data) = line.strip_prefix("data:").map(str::trim) else { return Ok(None) };
        if data == "[DONE]" {
            return Ok(None);
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else { return Ok(None) };
        if let Some(error) = chunk.get("error") {
            return Err(fail(ErrorKind::Other, tr!(format!("Das Modell meldet: {}", error["message"].as_str().unwrap_or("Fehler")), format!("The model says: {}", error["message"].as_str().unwrap_or("error")))));
        }
        for usage in [&chunk["usage"], &chunk["x_groq"]["usage"]] {
            if let Some(total) = usage["total_tokens"].as_u64() {
                self.tokens = total;
            }
        }
        let delta = &chunk["choices"][0]["delta"];
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = call["index"].as_u64().unwrap_or(self.calls.len() as u64) as usize;
            if self.calls.len() <= index {
                self.calls.resize(index + 1, (String::new(), String::new()));
            }
            if let Some(name) = call["function"]["name"].as_str() {
                self.calls[index].0.push_str(name);
            }
            if let Some(args) = call["function"]["arguments"].as_str() {
                self.calls[index].1.push_str(args);
            }
        }
        let text = delta["content"].as_str().unwrap_or("");
        if text.is_empty() {
            return Ok(None);
        }
        self.text.push_str(text);
        Ok(Some(text.to_string()))
    }

    pub(crate) fn finish(self, completed: bool) -> Turn {
        let calls: Vec<Call> = self
            .calls
            .into_iter()
            .filter(|(name, _)| !name.is_empty())
            .map(|(name, args)| Call { name, args: serde_json::from_str(&args).unwrap_or(json!({})) })
            .collect();
        let mut parts: Vec<Value> = Vec::new();
        if !self.text.is_empty() {
            parts.push(json!({ "text": self.text }));
        }
        parts.extend(calls.iter().map(|c| json!({ "functionCall": { "name": c.name, "args": c.args } })));
        Turn { content: json!({ "role": "model", "parts": parts }), text: self.text, calls, tokens: self.tokens, completed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_becomes_openai_messages() {
        let contents = vec![
            json!({ "role": "user", "parts": [{ "text": "Wie spät ist es und mach lauter" }] }),
            json!({ "role": "model", "parts": [{ "text": "denke", "thought": true }, { "functionCall": { "name": "set_volume", "args": { "percent": 40 } }, "thoughtSignature": "abc" }] }),
            json!({ "role": "user", "parts": [{ "functionResponse": { "name": "set_volume", "response": { "result": "ok" } } }] }),
            json!({ "role": "model", "parts": [{ "text": "Erledigt." }] }),
            json!({ "role": "user", "parts": [{ "text": "Was siehst du?" }, { "inlineData": { "mimeType": "image/png", "data": "xx" } }] }),
        ];
        let messages = to_messages("System", &contents);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[2]["role"], "assistant");
        assert!(messages[2]["content"].is_null());
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call_1_0");
        assert_eq!(messages[2]["tool_calls"][0]["function"]["arguments"], "{\"percent\":40}");
        assert_eq!((messages[3]["role"].as_str(), messages[3]["tool_call_id"].as_str()), (Some("tool"), Some("call_1_0")));
        assert_eq!(messages[4]["content"], "Erledigt.");
        assert!(messages[5]["content"].as_str().unwrap().contains("keine Bilder"));
    }

    #[test]
    fn streamed_tool_calls_are_joined() {
        let mut c = Collect::default();
        let lines = [
            r#"data: {"choices":[{"delta":{"content":"Moment"}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"x","function":{"name":"set_volume","arguments":"{\"perc"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ent\":30}"}}]}}],"x_groq":{"usage":{"total_tokens":321}}}"#,
            "data: [DONE]",
        ];
        let shown: Vec<String> = lines.iter().filter_map(|l| c.line(l.as_bytes()).unwrap()).collect();
        assert_eq!(shown, vec!["Moment".to_string()]);
        let turn = c.finish(true);
        assert_eq!(turn.calls, vec![Call { name: "set_volume".into(), args: json!({ "percent": 30 }) }]);
        assert_eq!(turn.tokens, 321);
        assert_eq!(turn.content["parts"][1]["functionCall"]["name"], "set_volume");
    }

    #[test]
    fn only_chat_models_are_offered() {
        let list = json!({ "data": [{ "id": "whisper-large-v3" }, { "id": "openai/gpt-oss-120b" }, { "id": "meta-llama/llama-prompt-guard-2-22m" }, { "id": "llama-3.1-8b-instant" }] });
        assert_eq!(chat_models(&list), vec!["llama-3.1-8b-instant".to_string(), "openai/gpt-oss-120b".to_string()]);
    }
}

#[cfg(test)]
mod size_probe {
    #[test]
    #[ignore]
    fn print_request_size() {
        let settings = crate::settings::Settings::default();
        let tools = serde_json::to_string(&crate::tools::definitions(&settings)).unwrap();
        let compact = serde_json::to_string(&super::compact_tools(&crate::tools::definitions(&settings))).unwrap();
        println!("KOMPAKT {} Zeichen", compact.chars().count());
        let system = crate::agent::system_prompt(&settings);
        println!("TOOLS {} Zeichen, {} Werkzeuge; SYSTEM {} Zeichen", tools.chars().count(), crate::tools::definitions(&settings).len(), system.chars().count());
    }
}
