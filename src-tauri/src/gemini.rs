//! Milos Gehirn: Google Gemini ueber den eigenen API-Schluessel des Nutzers.
//!
//! Der Schluessel liegt ausschliesslich in der Windows-Anmeldeinformationsverwaltung, nie in
//! einer Datei, nie im Protokoll.

use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};

const BASE: &str = "https://generativelanguage.googleapis.com";
/// Name des Eintrags in der Windows-Anmeldeinformationsverwaltung.
const CREDENTIAL: &str = "Milo/Gemini-API-Schluessel";

/// Adresse der API. Nur in Entwicklungsversionen laesst sie sich fuer Tests auf eine
/// lokale Attrappe umbiegen; die fertige App spricht immer mit Google.
fn base() -> String {
    if cfg!(debug_assertions) {
        if let Ok(other) = std::env::var("MILO_GEMINI_BASE") {
            if other.starts_with("http://127.0.0.1:") {
                return other;
            }
        }
    }
    BASE.to_string()
}

// ---------- Schluessel ----------

/// Schluessel in der Windows-Anmeldeinformationsverwaltung (auch fuer Groq).
#[cfg(windows)]
pub(crate) mod vault {
    use anyhow::{bail, Result};
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    pub fn write(name: &str, secret: &str) -> Result<()> {
        let target = HSTRING::from(name);
        let user = HSTRING::from("Milo");
        let mut blob = secret.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_ptr() as *mut u16),
            UserName: PWSTR(user.as_ptr() as *mut u16),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        if unsafe { CredWriteW(&credential, 0) }.is_err() {
            bail!(tr!("Der Schlüssel ließ sich nicht in der Windows-Anmeldeinformationsverwaltung speichern.", "The key couldn't be saved in Windows Credential Manager."));
        }
        Ok(())
    }

    pub fn read(name: &str) -> Option<String> {
        let target = HSTRING::from(name);
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut found).ok()?;
            let credential = &*found;
            let bytes = std::slice::from_raw_parts(credential.CredentialBlob, credential.CredentialBlobSize as usize).to_vec();
            CredFree(found as *const _);
            String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
        }
    }

    pub fn delete(name: &str) -> bool {
        let target = HSTRING::from(name);
        unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) }.is_ok()
    }
}

#[cfg(not(windows))]
pub(crate) mod vault {
    pub fn write(_: &str, _: &str) -> anyhow::Result<()> {
        anyhow::bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
    pub fn read(_: &str) -> Option<String> {
        None
    }
    pub fn delete(_: &str) -> bool {
        false
    }
}

pub fn has_key() -> bool {
    vault::read(CREDENTIAL).is_some()
}

fn key() -> Result<String> {
    vault::read(CREDENTIAL).ok_or_else(|| anyhow!(tr!("Es ist kein Gemini-Schlüssel hinterlegt.", "No Gemini key has been added.")))
}

pub fn store_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.len() < 20 || key.len() > 200 || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!(tr!("Das sieht nicht nach einem API-Schlüssel aus.", "That doesn't look like an API key."));
    }
    vault::write(CREDENTIAL, key)
}

pub fn delete_key() -> bool {
    vault::delete(CREDENTIAL)
}

// ---------- Aufrufe ----------

/// Wie gruendlich das Modell vor der Antwort nachdenkt. Weniger ist schneller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thinking {
    Low,
    Medium,
    High,
}

/// Fehlerarten, nach denen Milo entscheidet, ob es mit einem anderen Modell weitergeht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Kontingent aufgebraucht (429).
    Quota,
    /// Das Modell gibt es fuer diesen Schluessel nicht (404).
    Missing,
    /// Google ist ueberlastet (5xx) oder hat nichts geliefert.
    Busy,
    /// Das erste Wort kam zu spaet oder der Strom riss ab.
    Slow,
    /// Schluessel fehlt oder wird abgelehnt.
    Key,
    /// Keine Internetverbindung.
    Offline,
    Other,
}

/// Ein Fehler der Gemini-Schnittstelle mit verstaendlicher deutscher Meldung.
#[derive(Debug)]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

fn fail(kind: ErrorKind, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ApiError { kind, message: message.into() })
}

/// Art eines Fehlers, auch wenn er in einer Kette steckt.
pub fn kind_of(err: &anyhow::Error) -> ErrorKind {
    err.chain().find_map(|e| e.downcast_ref::<ApiError>().map(|a| a.kind)).unwrap_or(ErrorKind::Other)
}

/// Werkzeugaufruf, den das Modell verlangt.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: String,
    pub args: Value,
}

/// Ergebnis eines Gespraechsschritts.
#[derive(Debug, Default)]
pub struct Turn {
    /// Der Beitrag des Modells (`{role: "model", parts}`), unveraendert fuer den naechsten Schritt
    /// aufzuheben, damit die Denk-Signaturen erhalten bleiben.
    pub content: Value,
    /// Sichtbarer Text der Antwort.
    pub text: String,
    pub calls: Vec<Call>,
    /// Verbrauchte Token laut Google.
    pub tokens: u64,
    /// False, wenn der Aufrufer die Ausgabe vorzeitig abgebrochen hat.
    pub completed: bool,
}

/// Sammelt die Teile eines gestreamten Beitrags.
#[derive(Debug, Default)]
struct Collect {
    parts: Vec<Value>,
    tokens: u64,
}

impl Collect {
    /// Verarbeitet eine Zeile des Antwortstroms ("data: {...}") und liefert neuen sichtbaren Text.
    fn line(&mut self, line: &[u8]) -> Result<Option<String>> {
        let line = std::str::from_utf8(line).unwrap_or_default().trim();
        let Some(data) = line.strip_prefix("data:") else {
            return Ok(None);
        };
        let Ok(chunk) = serde_json::from_str::<Value>(data.trim()) else {
            return Ok(None);
        };
        if let Some(reason) = chunk["promptFeedback"]["blockReason"].as_str() {
            return Err(fail(ErrorKind::Other, tr!(format!("Gemini hat die Anfrage abgelehnt ({reason})."), format!("Gemini rejected the request ({reason})."))));
        }
        if let Some(error) = chunk.get("error") {
            let status = error["code"].as_u64().unwrap_or(0) as u16;
            let body = json!({ "error": error }).to_string();
            return Err(fail(classify(status, &body), api_error(status, &body)));
        }
        if let Some(total) = chunk["usageMetadata"]["totalTokenCount"].as_u64() {
            self.tokens = total;
        }
        let mut shown = String::new();
        for part in chunk["candidates"][0]["content"]["parts"].as_array().cloned().unwrap_or_default() {
            let thought = part["thought"].as_bool() == Some(true);
            if let (Some(text), false) = (part["text"].as_str(), thought) {
                shown.push_str(text);
            }
            self.push(part, thought);
        }
        Ok((!shown.is_empty()).then_some(shown))
    }

    /// Haengt einen Teil an; aufeinanderfolgende Textstuecke werden zu einem Teil zusammengefuegt.
    fn push(&mut self, part: Value, thought: bool) {
        let plain = |p: &Value| p.get("text").is_some() && p.get("functionCall").is_none() && p["thought"].as_bool() != Some(true);
        if !thought && plain(&part) {
            if let Some(last) = self.parts.last_mut().filter(|l| plain(l)) {
                let joined = format!("{}{}", last["text"].as_str().unwrap_or_default(), part["text"].as_str().unwrap_or_default());
                last["text"] = json!(joined);
                if let Some(signature) = part.get("thoughtSignature") {
                    last["thoughtSignature"] = signature.clone();
                }
                return;
            }
        }
        self.parts.push(part);
    }

    fn finish(self, completed: bool) -> Turn {
        let text: String = self
            .parts
            .iter()
            .filter(|p| p["thought"].as_bool() != Some(true))
            .filter_map(|p| p["text"].as_str())
            .collect();
        let calls = self
            .parts
            .iter()
            .filter_map(|p| p.get("functionCall"))
            .map(|c| Call { name: c["name"].as_str().unwrap_or_default().to_string(), args: c.get("args").cloned().unwrap_or(json!({})) })
            .collect();
        Turn { content: json!({ "role": "model", "parts": self.parts }), text, calls, tokens: self.tokens, completed }
    }
}

pub struct Gemini {
    http: reqwest::Client,
    /// Modelle, die keine Denk-Einstellung annehmen.
    no_thinking: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl Gemini {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            // Verbindung offen halten, damit die naechste Anfrage ohne neuen Verbindungsaufbau startet.
            .pool_idle_timeout(Duration::from_secs(300))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .expect("HTTP-Client");
        Self { http, no_thinking: Default::default() }
    }

    /// Namen der Modelle, die mit diesem Schluessel Text erzeugen koennen (ohne "models/").
    /// Dient zugleich als Pruefung, ob der Schluessel funktioniert, und waermt die Verbindung vor.
    pub async fn models(&self) -> Result<Vec<String>> {
        let response = self
            .http
            .get(format!("{}/v1beta/models?pageSize=200", base()))
            .header("x-goog-api-key", key()?)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(offline)?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        if status != 200 {
            return Err(fail(classify(status, &text), api_error(status, &text)));
        }
        Ok(text_models(&serde_json::from_str(&text).unwrap_or_default()))
    }

    /// Ein Gespraechsschritt mit Strom: `contents` ist der bisherige Verlauf im Gemini-Format,
    /// `tools` die Werkzeugbeschreibungen (`functionDeclarations`). `on_token` bekommt jedes
    /// sichtbare Textstueck und gibt `false` zurueck, um abzubrechen. Kommt innerhalb von
    /// `first_wait` gar nichts, gilt das Modell als zu langsam.
    pub async fn turn(
        &self,
        model: &str,
        system: &str,
        contents: &[Value],
        tools: &[Value],
        thinking: Thinking,
        first_wait: Duration,
        mut on_token: impl FnMut(&str) -> bool,
    ) -> Result<Turn> {
        let mut body = json!({
            "systemInstruction": { "parts": [{ "text": system }] },
            "contents": contents,
        });
        if !tools.is_empty() {
            body["tools"] = json!([{ "functionDeclarations": tools }]);
        }
        let with_thinking = !self.no_thinking.lock().unwrap().contains(model);
        if with_thinking {
            body["generationConfig"] = json!({ "thinkingConfig": thinking_config(model, thinking) });
        }
        let mut response = self.post_stream(model, &body).await?;
        if response.status().as_u16() == 400 && with_thinking {
            let text = response.text().await.unwrap_or_default();
            if !text.to_lowercase().contains("thinking") {
                return Err(fail(classify(400, &text), api_error(400, &text)));
            }
            // Dieses Modell kennt die Denk-Einstellung nicht: ohne sie noch einmal.
            log::info!("{model} nimmt keine Denk-Einstellung an, frage ohne");
            self.no_thinking.lock().unwrap().insert(model.to_string());
            body.as_object_mut().unwrap().remove("generationConfig");
            response = self.post_stream(model, &body).await?;
        }
        let status = response.status().as_u16();
        if status != 200 {
            let text = response.text().await.unwrap_or_default();
            // Googles eigene Meldung ins Protokoll (enthaelt nie den Schluessel), damit Fehler nachvollziehbar bleiben.
            let detail: Value = serde_json::from_str(&text).unwrap_or_default();
            log::warn!("{model} meldet {status}: {}", detail["error"]["message"].as_str().unwrap_or(&text).chars().take(300).collect::<String>());
            return Err(fail(classify(status, &text), api_error(status, &text)));
        }

        let mut collect = Collect::default();
        let mut stream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut first = true;
        loop {
            let wait = if first { first_wait } else { Duration::from_secs(60) };
            let chunk = match tokio::time::timeout(wait, stream.next()).await {
                Err(_) if first => return Err(fail(ErrorKind::Slow, tr!(format!("{model} antwortet zu langsam."), format!("{model} is answering too slowly.")))),
                Err(_) => return Err(fail(ErrorKind::Slow, tr!("Gemini antwortet nicht mehr.", "Gemini stopped answering."))),
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
            return Err(fail(ErrorKind::Busy, tr!("Gemini hat nichts geantwortet.", "Gemini didn't answer.")));
        }
        Ok(turn)
    }

    async fn post_stream(&self, model: &str, body: &Value) -> Result<reqwest::Response> {
        self.http
            .post(format!("{}/v1beta/models/{model}:streamGenerateContent?alt=sse", base()))
            .header("x-goog-api-key", key()?)
            .json(body)
            .send()
            .await
            .map_err(offline)
    }
}

/// Denk-Einstellung: Gemini 3 kennt Stufen, aeltere Modelle ein Token-Budget.
fn thinking_config(model: &str, thinking: Thinking) -> Value {
    if version(model) >= 3.0 {
        let level = match thinking {
            Thinking::Low => "low",
            Thinking::Medium => "medium",
            Thinking::High => "high",
        };
        json!({ "thinkingLevel": level })
    } else {
        let budget = match thinking {
            Thinking::Low => 0,
            Thinking::Medium => 2048,
            Thinking::High => -1,
        };
        json!({ "thinkingBudget": budget })
    }
}

fn offline(err: reqwest::Error) -> anyhow::Error {
    if err.is_connect() || err.is_timeout() {
        fail(ErrorKind::Offline, tr!("Ich erreiche Gemini nicht. Besteht eine Internetverbindung?", "I can't reach Gemini. Are you connected to the internet?"))
    } else {
        // Ohne Adresse, damit nie etwas aus der Anfrage im Protokoll landet.
        fail(ErrorKind::Offline, tr!(format!("Die Verbindung zu Gemini ist abgebrochen ({}).", err.without_url()), format!("The connection to Gemini dropped ({}).", err.without_url())))
    }
}

/// Wandelt eine Werkzeugbeschreibung (`{type: "function", function: {...}}`) in eine
/// Gemini-`functionDeclaration` um. Gemini erwartet die Typnamen gross geschrieben ("STRING", "OBJECT" ...).
pub fn declaration(tool: &Value) -> Value {
    fn upper_types(schema: &Value) -> Value {
        match schema {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        let v = if k == "type" { v.as_str().map(|t| json!(t.to_uppercase())).unwrap_or(v.clone()) } else { upper_types(v) };
                        (k.clone(), v)
                    })
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(upper_types).collect()),
            other => other.clone(),
        }
    }
    let function = &tool["function"];
    let mut out = json!({ "name": function["name"], "description": function["description"] });
    let params = &function["parameters"];
    if params["properties"].as_object().is_some_and(|p| !p.is_empty()) {
        out["parameters"] = upper_types(params);
    }
    out
}

/// Ordnet eine Fehlerantwort einer Fehlerart zu.
pub fn classify(status: u16, body: &str) -> ErrorKind {
    let parsed: Value = serde_json::from_str(body).unwrap_or_default();
    let code = parsed["error"]["status"].as_str().unwrap_or_default();
    let lower = parsed["error"]["message"].as_str().unwrap_or_default().to_lowercase();
    if status == 429 || code == "RESOURCE_EXHAUSTED" {
        ErrorKind::Quota
    } else if lower.contains("api key") || status == 401 || status == 403 {
        ErrorKind::Key
    } else if status == 404 {
        ErrorKind::Missing
    } else if status >= 500 {
        ErrorKind::Busy
    } else {
        ErrorKind::Other
    }
}

/// Macht aus einer Fehlerantwort der API eine verstaendliche deutsche Meldung.
pub fn api_error(status: u16, body: &str) -> String {
    let parsed: Value = serde_json::from_str(body).unwrap_or_default();
    let message = parsed["error"]["message"].as_str().unwrap_or_default();
    let code = parsed["error"]["status"].as_str().unwrap_or_default();
    let lower = message.to_lowercase();
    if status == 429 || code == "RESOURCE_EXHAUSTED" {
        tr!("Das kostenlose Gemini-Kontingent ist gerade aufgebraucht.", "The free Gemini quota is used up right now.").into()
    } else if lower.contains("api key") || status == 401 || status == 403 {
        tr!("Der Gemini-Schlüssel wird nicht angenommen. Bitte in den Einstellungen prüfen.", "The Gemini key isn't accepted. Please check it in Settings.").into()
    } else if status == 404 {
        tr!("Dieses Gemini-Modell gibt es nicht (mehr). Bitte in den Einstellungen ein anderes wählen.", "This Gemini model doesn't exist (anymore). Please pick another one in Settings.").into()
    } else if status >= 500 {
        tr!("Gemini ist gerade überlastet. Später noch einmal versuchen.", "Gemini is overloaded right now. Try again later.").into()
    } else if message.is_empty() {
        tr!(format!("Gemini meldet einen Fehler ({status})."), format!("Gemini reports an error ({status})."))
    } else {
        tr!(format!("Gemini meldet: {message}"), format!("Gemini says: {message}"))
    }
}

fn text_models(body: &Value) -> Vec<String> {
    body["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter(|m| m["supportedGenerationMethods"].as_array().is_some_and(|methods| methods.iter().any(|x| x == "generateContent")))
                .filter_map(|m| m["name"].as_str().map(|n| n.trim_start_matches("models/").to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Versionsnummer aus einem Modellnamen, z. B. "gemini-2.5-flash" -> 2.5.
fn version(name: &str) -> f32 {
    name.split('-').find_map(|part| part.parse::<f32>().ok()).unwrap_or(0.0)
}

/// Waehlt aus der Liste des Schluessels das passende Modell: das gewuenschte, falls es existiert,
/// sonst das neueste stabile aus derselben Familie ("flash-lite" oder "flash").
pub fn choose_model(available: &[String], wanted: &str, lite: bool) -> Option<String> {
    if available.iter().any(|m| m == wanted) {
        return Some(wanted.to_string());
    }
    let fits = |name: &&String| {
        let stable = !["preview", "exp", "tts", "image", "audio", "live", "embedding"].iter().any(|bad| name.contains(bad));
        let family = if lite { name.contains("flash-lite") } else { name.contains("flash") && !name.contains("lite") };
        name.starts_with("gemini-") && stable && family
    };
    let mut candidates: Vec<&String> = available.iter().filter(fits).collect();
    candidates.sort_by(|a, b| version(b).partial_cmp(&version(a)).unwrap_or(std::cmp::Ordering::Equal).then(a.len().cmp(&b.len())));
    candidates.first().map(|name| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_lines_collect_text_calls_and_tokens() {
        let mut c = Collect::default();
        assert_eq!(c.line(br#"data: {"candidates":[{"content":{"parts":[{"text":"Hal"}],"role":"model"}}]}"#).unwrap().as_deref(), Some("Hal"));
        assert_eq!(c.line(br#"data: {"candidates":[{"content":{"parts":[{"text":"lo","thoughtSignature":"sig1"}]}}]}"#).unwrap().as_deref(), Some("lo"));
        assert_eq!(c.line(b"\r\n").unwrap(), None);
        assert_eq!(c.line(b": keepalive").unwrap(), None);
        // Gedankentext gehoert nicht in die Antwort.
        assert_eq!(c.line(br#"data: {"candidates":[{"content":{"parts":[{"text":"hm","thought":true}]}}]}"#).unwrap(), None);
        let call = br#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"set_timer","args":{"duration_seconds":600}},"thoughtSignature":"sig2"}]}}],"usageMetadata":{"totalTokenCount":321}}"#;
        assert_eq!(c.line(call).unwrap(), None);
        let turn = c.finish(true);
        assert_eq!(turn.text, "Hallo");
        assert_eq!(turn.tokens, 321);
        assert_eq!(turn.calls, vec![Call { name: "set_timer".into(), args: json!({ "duration_seconds": 600 }) }]);
        // Textstuecke werden zu einem Teil, Signaturen bleiben erhalten.
        let parts = turn.content["parts"].as_array().unwrap();
        assert_eq!(turn.content["role"], "model");
        assert_eq!(parts[0], json!({ "text": "Hallo", "thoughtSignature": "sig1" }));
        assert_eq!(parts[2]["thoughtSignature"], "sig2");
    }

    #[test]
    fn blocked_prompt_and_stream_errors_are_errors() {
        let err = Collect::default().line(br#"data: {"promptFeedback":{"blockReason":"SAFETY"}}"#).unwrap_err();
        assert!(err.to_string().contains("abgelehnt"));
        let quota = Collect::default().line(br#"data: {"error":{"code":429,"message":"Quota","status":"RESOURCE_EXHAUSTED"}}"#).unwrap_err();
        assert_eq!(kind_of(&quota), ErrorKind::Quota);
    }

    #[test]
    fn thinking_levels_depend_on_the_model_generation() {
        assert_eq!(thinking_config("gemini-3.8-flash", Thinking::Low), json!({ "thinkingLevel": "low" }));
        assert_eq!(thinking_config("gemini-3.5-flash-lite", Thinking::High), json!({ "thinkingLevel": "high" }));
        assert_eq!(thinking_config("gemini-2.5-flash", Thinking::Low), json!({ "thinkingBudget": 0 }));
    }

    #[test]
    fn errors_are_classified() {
        assert_eq!(classify(429, "{}"), ErrorKind::Quota);
        assert_eq!(classify(400, r#"{"error":{"message":"API key not valid"}}"#), ErrorKind::Key);
        assert_eq!(classify(404, ""), ErrorKind::Missing);
        assert_eq!(classify(503, ""), ErrorKind::Busy);
        assert_eq!(classify(400, ""), ErrorKind::Other);
    }

    #[test]
    fn errors_become_german_messages_without_details() {
        assert!(api_error(429, r#"{"error":{"code":429,"message":"Quota exceeded","status":"RESOURCE_EXHAUSTED"}}"#).contains("Kontingent"));
        assert!(api_error(400, r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT"}}"#).contains("Schlüssel"));
        assert!(api_error(404, "{}").contains("Modell"));
        assert!(api_error(503, "").contains("überlastet"));
        assert_eq!(api_error(400, "kaputt"), "Gemini meldet einen Fehler (400).");
    }

    #[test]
    fn picks_wanted_model_or_newest_of_the_family() {
        let list: Vec<String> = ["gemini-2.0-flash", "gemini-2.5-flash", "gemini-2.5-flash-lite", "gemini-3.0-flash-preview", "gemini-2.5-flash-image", "gemini-2.5-pro"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(choose_model(&list, "gemini-2.0-flash", false).as_deref(), Some("gemini-2.0-flash"));
        assert_eq!(choose_model(&list, "gibt-es-nicht", false).as_deref(), Some("gemini-2.5-flash"));
        assert_eq!(choose_model(&list, "gibt-es-nicht", true).as_deref(), Some("gemini-2.5-flash-lite"));
        assert_eq!(choose_model(&[], "x", true), None);
    }

    #[test]
    fn model_list_keeps_only_text_models() {
        let body = json!({ "models": [
            { "name": "models/gemini-2.5-flash", "supportedGenerationMethods": ["generateContent", "countTokens"] },
            { "name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"] },
        ]});
        assert_eq!(text_models(&body), vec!["gemini-2.5-flash"]);
    }

    #[test]
    fn implausible_keys_are_rejected_before_storing() {
        assert!(store_key("kurz").is_err());
        assert!(store_key("mit leerzeichen und noch viel mehr text dahinter").is_err());
    }

    #[test]
    fn declaration_uppercases_types_and_drops_empty_parameters() {
        let tool = json!({ "type": "function", "function": { "name": "browser_click", "description": "Klickt",
            "parameters": { "type": "object", "properties": { "target": { "type": "string", "enum": ["a"] } }, "required": ["target"] } } });
        let d = declaration(&tool);
        assert_eq!(d["name"], "browser_click");
        assert_eq!(d["parameters"]["type"], "OBJECT");
        assert_eq!(d["parameters"]["properties"]["target"]["type"], "STRING");
        assert_eq!(d["parameters"]["properties"]["target"]["enum"][0], "a");
        let empty = json!({ "function": { "name": "list_timers", "description": "x", "parameters": { "type": "object", "properties": {} } } });
        assert!(declaration(&empty).get("parameters").is_none());
    }
}
