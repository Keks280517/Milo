//! Lokale KI auf dem eigenen PC: Ollama (`http://127.0.0.1:11434`) oder LM Studio
//! (`http://127.0.0.1:1234`). Beide sprechen das OpenAI-Format (`/v1/chat/completions`), deshalb
//! nutzt Milo dieselbe Uebersetzung wie fuer Groq. Nichts davon verlaesst den Rechner.
//!
//! Dazu: Erkennung, welcher Server laeuft und welche Modelle da sind, eine Hardware-Pruefung
//! (Prozessor, Arbeitsspeicher, Grafikkarte) und daraus eine Modellempfehlung. Die Empfehlungen
//! beruhen auf Messungen mit Ollama (Grafikspeicher, Tempo, Werkzeugaufrufe; siehe `bench/`).
//! Ein Modell laedt Milo nur auf Klick des Nutzers herunter (`ollama pull` ueber die API).

use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::gemini::{ErrorKind, Turn};
use crate::groq::{fail, to_messages, Collect};

pub const OLLAMA: &str = "http://127.0.0.1:11434";
pub const LMSTUDIO: &str = "http://127.0.0.1:1234";
pub const PULL_EVENT: &str = "milo://local-pull";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    pub name: String,
    /// Groesse auf der Platte in Bytes (nur Ollama).
    pub size: u64,
    /// z. B. "12B" (nur Ollama).
    pub params: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalStatus {
    /// "ollama", "lmstudio" oder "" (keiner laeuft).
    pub server: String,
    pub version: String,
    pub models: Vec<LocalModel>,
    /// Ollama ist installiert (auch wenn es gerade nicht laeuft).
    pub ollama_installed: bool,
}

/// Stand eines Modell-Downloads. Liegt im Kern, damit Insel, Pille und Einstellungen denselben
/// Fortschritt zeigen, auch wenn ein Fenster erst spaeter aufgeht.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    pub model: String,
    /// "start" (Ollama wird geweckt, Manifest), "download", "check" (pruefen und ablegen), "done", "error".
    pub phase: String,
    pub completed: u64,
    pub total: u64,
    /// 0..100
    pub percent: f32,
    pub bytes_per_sec: f64,
    /// Geschaetzte Restzeit in Sekunden (erst, wenn das Tempo bekannt ist).
    pub eta_secs: Option<u64>,
    pub error: String,
}

impl PullProgress {
    pub fn running(&self) -> bool {
        matches!(self.phase.as_str(), "start" | "download" | "check")
    }
}

pub struct LocalAi {
    http: reqwest::Client,
    progress: std::sync::Mutex<Option<PullProgress>>,
}

impl LocalAi {
    pub fn new() -> Self {
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(2)).no_proxy().build().expect("HTTP-Client");
        Self { http, progress: std::sync::Mutex::new(None) }
    }

    /// Letzter Stand des Downloads (oder keiner).
    pub fn progress(&self) -> Option<PullProgress> {
        self.progress.lock().ok().and_then(|p| p.clone())
    }

    fn report(&self, app: &AppHandle, progress: PullProgress) {
        let _ = app.emit(PULL_EVENT, &progress);
        if let Ok(mut slot) = self.progress.lock() {
            *slot = Some(progress);
        }
    }

    /// Nimmt einen Download an, wenn gerade keiner laeuft.
    pub fn claim(&self, model: &str) -> bool {
        let Ok(mut slot) = self.progress.lock() else { return false };
        if slot.as_ref().is_some_and(PullProgress::running) {
            return false;
        }
        *slot = Some(PullProgress { model: model.into(), phase: "start".into(), ..Default::default() });
        true
    }

    /// Startet Ollama, falls es installiert ist, aber nicht laeuft, und wartet bis zu 30 s darauf.
    pub async fn ensure_ollama(&self) -> bool {
        if self.get_json(&format!("{OLLAMA}/api/version")).await.is_ok() {
            return true;
        }
        if !start_ollama() {
            return false;
        }
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if self.get_json(&format!("{OLLAMA}/api/version")).await.is_ok() {
                log::info!("Ollama gestartet");
                return true;
            }
        }
        false
    }

    /// Welcher lokale Server laeuft, und welche Modelle hat er?
    pub async fn status(&self) -> LocalStatus {
        let mut status = LocalStatus { ollama_installed: ollama_installed(), ..Default::default() };
        if let Ok(tags) = self.get_json(&format!("{OLLAMA}/api/tags")).await {
            status.server = "ollama".into();
            status.version = self.get_json(&format!("{OLLAMA}/api/version")).await.ok().and_then(|v| v["version"].as_str().map(str::to_string)).unwrap_or_default();
            status.models = ollama_models(&tags);
            return status;
        }
        if let Ok(list) = self.get_json(&format!("{LMSTUDIO}/v1/models")).await {
            status.server = "lmstudio".into();
            status.models = crate::groq::chat_models(&list)
                .into_iter()
                .filter(|m| !m.contains("embed"))
                .map(|name| LocalModel { name, size: 0, params: String::new() })
                .collect();
        }
        status
    }

    async fn get_json(&self, url: &str) -> Result<Value> {
        let response = self.http.get(url).timeout(Duration::from_secs(3)).send().await?;
        anyhow::ensure!(response.status().is_success(), "HTTP {}", response.status());
        Ok(response.json().await?)
    }

    /// Adresse des laufenden Servers fuer Antworten (OpenAI-Format).
    pub async fn base(&self) -> Option<String> {
        if self.get_json(&format!("{OLLAMA}/api/version")).await.is_ok() {
            return Some(format!("{OLLAMA}/v1"));
        }
        if self.get_json(&format!("{LMSTUDIO}/v1/models")).await.is_ok() {
            return Some(format!("{LMSTUDIO}/v1"));
        }
        None
    }

    /// Ein Gespraechsschritt mit Strom, wie `Groq::turn`. Kann das Modell keine Werkzeuge,
    /// geht es einmal ohne Werkzeuge weiter.
    pub async fn turn(&self, model: &str, system: &str, contents: &[Value], tools: &[Value], first_wait: Duration, mut on_token: impl FnMut(&str) -> bool) -> Result<Turn> {
        // Laeuft nichts, aber Ollama ist installiert: einfach starten.
        let base = match self.base().await {
            Some(base) => Some(base),
            None if ollama_installed() && self.ensure_ollama().await => Some(format!("{OLLAMA}/v1")),
            None => None,
        };
        let base = base.ok_or_else(|| {
            fail(ErrorKind::Offline, tr!("Die lokale KI läuft gerade nicht. Starte Ollama oder LM Studio.", "The local AI isn't running right now. Start Ollama or LM Studio."))
        })?;
        let mut tools = tools.to_vec();
        loop {
            let mut body = json!({ "model": model, "messages": to_messages(system, contents), "stream": true });
            if !tools.is_empty() {
                body["tools"] = json!(tools);
            }
            let response = self.http.post(format!("{base}/chat/completions")).json(&body).send().await.map_err(|e| {
                fail(ErrorKind::Offline, tr!(format!("Die lokale KI ist nicht erreichbar ({}).", e.without_url()), format!("The local AI can't be reached ({}).", e.without_url())))
            })?;
            let status = response.status().as_u16();
            if status != 200 {
                let text = response.text().await.unwrap_or_default();
                let lower = text.to_lowercase();
                if !tools.is_empty() && lower.contains("tool") && (lower.contains("support") || lower.contains("not")) {
                    log::info!("Lokales Modell {model} kann keine Werkzeuge, weiter ohne");
                    tools.clear();
                    continue;
                }
                log::warn!("Lokale KI {model} meldet {status}: {}", text.chars().take(300).collect::<String>());
                let kind = if status == 404 { ErrorKind::Missing } else { ErrorKind::Other };
                let message = if status == 404 {
                    tr!(format!("Das lokale Modell „{model}“ ist nicht installiert."), format!("The local model \"{model}\" isn't installed."))
                } else {
                    tr!(format!("Die lokale KI meldet einen Fehler ({status})."), format!("The local AI reports an error ({status})."))
                };
                return Err(fail(kind, message));
            }
            let mut collect = Collect::default();
            let mut stream = response.bytes_stream();
            let mut buffer: Vec<u8> = Vec::new();
            let mut first = true;
            loop {
                // Das erste Wort kann dauern, wenn das Modell erst in den Speicher geladen wird.
                let wait = if first { first_wait.max(Duration::from_secs(120)) } else { Duration::from_secs(90) };
                let chunk = match tokio::time::timeout(wait, stream.next()).await {
                    Err(_) => return Err(fail(ErrorKind::Slow, tr!("Die lokale KI antwortet zu langsam.", "The local AI is answering too slowly."))),
                    Ok(None) => break,
                    Ok(Some(chunk)) => chunk.map_err(|_| fail(ErrorKind::Offline, tr!("Die Verbindung zur lokalen KI ist abgebrochen.", "The connection to the local AI dropped.")))?,
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
                return Err(fail(ErrorKind::Busy, tr!("Die lokale KI hat nichts geantwortet.", "The local AI didn't answer.")));
            }
            return Ok(turn);
        }
    }

    /// Laedt ein Modell mit Ollama herunter (vorher `claim`). Meldet Prozent und Restzeit als
    /// Ereignis; ein Fehler steht danach im Fortschritt.
    pub async fn pull(&self, app: &AppHandle, model: &str) -> Result<()> {
        let result = self.pull_inner(app, model).await;
        if let Err(err) = &result {
            let mut progress = self.progress().filter(|p| p.model == model).unwrap_or_default();
            progress.model = model.into();
            progress.phase = "error".into();
            progress.eta_secs = None;
            progress.error = err.to_string();
            self.report(app, progress);
        }
        result
    }

    async fn pull_inner(&self, app: &AppHandle, model: &str) -> Result<()> {
        let start = PullProgress { model: model.into(), phase: "start".into(), ..Default::default() };
        self.report(app, start);
        if !self.ensure_ollama().await {
            anyhow::bail!(tr!(
                "Ollama läuft nicht und ließ sich nicht starten. Ist es installiert?",
                "Ollama isn't running and couldn't be started. Is it installed?"
            ));
        }
        let mut last_emit = std::time::Instant::now() - Duration::from_secs(1);
        let mut last_phase = String::new();
        let progress = self
            .pull_stream(model, |progress| {
                // Hoechstens vier Meldungen pro Sekunde, Phasenwechsel sofort.
                if progress.phase != last_phase || last_emit.elapsed() >= Duration::from_millis(250) {
                    last_phase = progress.phase.clone();
                    last_emit = std::time::Instant::now();
                    self.report(app, progress.clone());
                }
                true
            })
            .await?;
        anyhow::ensure!(progress.phase == "done", tr!("Der Download wurde nicht fertig.", "The download didn't finish."));
        self.report(app, progress);
        Ok(())
    }

    /// Liest den Fortschritt von `/api/pull`. `on_progress` bekommt jeden Stand; gibt es `false`
    /// zurueck, endet das Lesen (die Verbindung wird getrennt, Ollama behaelt das Angefangene).
    async fn pull_stream(&self, model: &str, mut on_progress: impl FnMut(&PullProgress) -> bool) -> Result<PullProgress> {
        let response = reqwest::Client::new()
            .post(format!("{OLLAMA}/api/pull"))
            .json(&json!({ "model": model, "stream": true }))
            .send()
            .await
            .map_err(|_| anyhow::anyhow!(tr!("Ollama ist nicht erreichbar.", "Ollama can't be reached.")))?;
        anyhow::ensure!(response.status().is_success(), tr!("Ollama hat den Download abgelehnt.", "Ollama refused the download."));
        let mut meter = PullMeter::new(model);
        let mut stream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            buffer.extend_from_slice(&chunk.map_err(|_| anyhow::anyhow!(tr!("Die Verbindung zu Ollama ist abgebrochen.", "The connection to Ollama dropped.")))?);
            while let Some(end) = buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=end).collect();
                let Ok(value) = serde_json::from_slice::<Value>(&line) else { continue };
                if let Some(error) = value["error"].as_str() {
                    anyhow::bail!("{}", friendly_pull_error(error, model));
                }
                meter.update(&value, std::time::Instant::now());
                if !on_progress(&meter.progress) {
                    return Ok(meter.progress);
                }
            }
        }
        Ok(meter.progress)
    }
}

/// Rechnet die Meldungen von `ollama pull` in einen Gesamtfortschritt um. Ollama meldet jede
/// Schicht (Gewichte, Vorlage, Lizenz …) einzeln; gezaehlt wird die Summe aller Schichten.
struct PullMeter {
    progress: PullProgress,
    layers: std::collections::HashMap<String, (u64, u64)>,
    sample: Option<(std::time::Instant, u64, u64)>,
    measured: u32,
}

impl PullMeter {
    fn new(model: &str) -> Self {
        Self { progress: PullProgress { model: model.into(), phase: "start".into(), ..Default::default() }, layers: Default::default(), sample: None, measured: 0 }
    }

    fn update(&mut self, value: &Value, now: std::time::Instant) {
        let status = value["status"].as_str().unwrap_or("");
        let p = &mut self.progress;
        if status == "success" {
            p.phase = "done".into();
            p.percent = 100.0;
            p.eta_secs = Some(0);
            return;
        }
        if status.starts_with("verifying") || status.starts_with("writing") || status.starts_with("removing") {
            p.phase = "check".into();
            p.percent = p.percent.max(99.0);
            p.eta_secs = None;
            return;
        }
        let (Some(digest), Some(total)) = (value["digest"].as_str(), value["total"].as_u64()) else { return };
        let completed = value["completed"].as_u64().unwrap_or(0).min(total);
        self.layers.insert(digest.to_string(), (completed, total));
        p.phase = "download".into();
        p.completed = self.layers.values().map(|l| l.0).sum();
        p.total = self.layers.values().map(|l| l.1).sum();
        p.percent = if p.total > 0 { (p.completed as f64 / p.total as f64 * 100.0) as f32 } else { 0.0 };
        // Tempo: jede Sekunde messen und glaetten, damit die Restzeit nicht springt. Kommt eine neue
        // Schicht dazu (die Gesamtgroesse aendert sich), bringt sie oft schon Geladenes von einem
        // frueheren Versuch mit; das ist kein Tempo, also neu ansetzen.
        match self.sample {
            Some((at, bytes, total)) if total == p.total => {
                let secs = now.duration_since(at).as_secs_f64();
                if secs >= 1.0 {
                    let speed = p.completed.saturating_sub(bytes) as f64 / secs;
                    p.bytes_per_sec = if p.bytes_per_sec > 0.0 { p.bytes_per_sec * 0.7 + speed * 0.3 } else { speed };
                    self.sample = Some((now, p.completed, p.total));
                    self.measured += 1;
                }
            }
            _ => self.sample = Some((now, p.completed, p.total)),
        }
        // Erst nach zwei Messungen eine Restzeit zeigen; die erste Sekunde ist zu unruhig.
        p.eta_secs = (self.measured >= 2 && p.bytes_per_sec > 1024.0).then(|| (p.total.saturating_sub(p.completed) as f64 / p.bytes_per_sec).round() as u64);
    }
}

fn friendly_pull_error(error: &str, model: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("file does not exist") || lower.contains("not found") {
        return tr!(format!("Ollama kennt kein Modell namens „{model}“. Vielleicht braucht es eine neuere Ollama-Version."), format!("Ollama doesn't know a model called \"{model}\". It may need a newer Ollama version."));
    }
    if lower.contains("no space") || lower.contains("disk") {
        return tr!("Auf der Festplatte ist nicht genug Platz für das Modell.", "There isn't enough disk space for the model.").into();
    }
    tr!(format!("Ollama meldet: {error}"), format!("Ollama says: {error}"))
}

/// Startet die installierte Ollama-App (Symbol im Infobereich) oder, falls nur die
/// Kommandozeile da ist, `ollama serve` unsichtbar.
fn start_ollama() -> bool {
    let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    if let Some(app) = local.as_ref().map(|p| p.join("Programs").join("Ollama").join("ollama app.exe")).filter(|p| p.exists()) {
        return std::process::Command::new(app).spawn().is_ok();
    }
    let exe = local
        .map(|p| p.join("Programs").join("Ollama").join("ollama.exe"))
        .filter(|p| p.exists())
        .or_else(|| std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|p| p.join("ollama.exe")).find(|p| p.exists())));
    match exe {
        Some(exe) => hidden(&exe.to_string_lossy()).arg("serve").spawn().is_ok(),
        None => false,
    }
}

fn ollama_models(tags: &Value) -> Vec<LocalModel> {
    let mut models: Vec<LocalModel> = tags["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| !m["details"]["family"].as_str().unwrap_or("").contains("bert") && !m["name"].as_str().unwrap_or("").contains("embed"))
        .filter_map(|m| {
            Some(LocalModel {
                name: m["name"].as_str()?.to_string(),
                size: m["size"].as_u64().unwrap_or(0),
                params: m["details"]["parameter_size"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();
    models.sort_by(|a, b| a.name.cmp(&b.name));
    models
}

/// Ist Ollama installiert? (Standardpfad des Installers oder im Suchpfad.)
fn ollama_installed() -> bool {
    let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    if local.is_some_and(|p| p.join("Programs").join("Ollama").join("ollama.exe").exists()) {
        return true;
    }
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join("ollama.exe").exists()))
}

// ---------- Hardware und Empfehlung ----------

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub cpu: String,
    pub cores: usize,
    pub ram_gb: f32,
    pub gpu: String,
    /// Grafikspeicher in GB (0 = keine eigene Grafikkarte erkannt).
    pub vram_gb: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub model: String,
    /// Etwa so viel Speicher braucht das Modell (GB).
    pub needs_gb: f32,
    /// Laeuft es auf der Grafikkarte (schnell) oder nur auf dem Prozessor (langsam)?
    pub on_gpu: bool,
    pub reason: String,
    /// Eine Alternative (andere Staerken), falls vorhanden.
    pub alternative: String,
}

/// Prozessor, Arbeitsspeicher und Grafikkarte. Liest nur Systemangaben, nichts wird gesendet.
pub fn hardware() -> Hardware {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let cpu = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    let (gpu, vram_gb) = gpu_info();
    Hardware { cpu, cores: sys.cpus().len(), ram_gb: round1(sys.total_memory() as f32 / 1024f32.powi(3)), gpu, vram_gb: round1(vram_gb) }
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

fn hidden(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Groesste Grafikkarte: zuerst `nvidia-smi`, sonst die Treiberangaben in der Registrierung
/// (dort steht der Grafikspeicher auch ueber 4 GB richtig, anders als in WMI).
fn gpu_info() -> (String, f32) {
    if let Ok(out) = hidden("nvidia-smi").args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]).output() {
        if out.status.success() {
            let best = String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| {
                    let (name, mib) = l.rsplit_once(',')?;
                    Some((name.trim().to_string(), mib.trim().parse::<f32>().ok()? / 1024.0))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some(best) = best {
                return best;
            }
        }
    }
    let script = r"Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -ErrorAction SilentlyContinue | ForEach-Object { '{0}|{1}' -f $_.DriverDesc, $_.'HardwareInformation.qwMemorySize' }";
    if let Ok(out) = hidden("powershell").args(["-NoProfile", "-NonInteractive", "-Command", script]).output() {
        let best = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let (name, bytes) = l.split_once('|')?;
                let gb = bytes.trim().parse::<u64>().unwrap_or(0) as f32 / 1024f32.powi(3);
                Some((name.trim().to_string(), gb))
            })
            .filter(|(name, _)| !name.is_empty())
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some(best) = best {
            return best;
        }
    }
    (String::new(), 0.0)
}

/// Passendes Modell fuer diesen Rechner. Grundlage: gemessener Grafikspeicher je Modell (Ollama,
/// 8k Kontext) plus etwa 1,5 GB Reserve fuer Windows und andere Programme.
pub fn recommend(hw: &Hardware) -> Recommendation {
    // (Modell, gemessener Speicher in GB, Grund)
    let pick = |model: &str, needs: f32, on_gpu: bool, reason: String, alternative: &str| Recommendation { model: model.into(), needs_gb: needs, on_gpu, reason, alternative: alternative.into() };
    let gpu_free = hw.vram_gb - 1.5;
    if hw.vram_gb >= 4.0 {
        if gpu_free >= 9.0 {
            return pick("gemma4:12b", 9.0, true, tr!("Deine Grafikkarte hat genug Speicher für das gründlichere Modell: gute Antworten und zuverlässige Werkzeuge (24 von 25 im Test).", "Your graphics card has enough memory for the more thorough model: good answers and reliable tools (24 of 25 in testing).").into(), "qwen3:14b");
        }
        if gpu_free >= 4.8 {
            return pick("gemma4:e4b", 4.9, true, tr!("Schnell und sparsam, passt gut auf deine Grafikkarte; im Test hat es alle 25 Werkzeugaufgaben gelöst.", "Fast and lean, fits your graphics card well; it solved all 25 tool tasks in testing.").into(), "qwen3:8b");
        }
    }
    // Ohne passende Grafikkarte rechnet der Prozessor: deutlich langsamer.
    if hw.ram_gb >= 15.0 {
        return pick("gemma4:e4b", 4.9, false, tr!("Ohne passende Grafikkarte rechnet der Prozessor. Das klappt mit diesem Modell, ist aber deutlich langsamer.", "Without a suitable graphics card, the processor does the work. This model handles that, but noticeably slower.").into(), "qwen3:4b");
    }
    if hw.ram_gb >= 7.5 {
        return pick("qwen3:4b", 3.0, false, tr!("Kleines Modell für deinen Arbeitsspeicher. Für einfache Fragen und Befehle gut, für lange Texte eher nicht.", "Small model for your memory. Good for simple questions and commands, less so for long texts.").into(), "");
    }
    pick("qwen3:1.7b", 1.6, false, tr!("Dein PC hat wenig Arbeitsspeicher. Dieses sehr kleine Modell läuft, kann aber nur einfache Dinge gut.", "Your PC has little memory. This very small model runs, but only does simple things well.").into(), "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(ram: f32, vram: f32) -> Hardware {
        Hardware { ram_gb: ram, vram_gb: vram, ..Default::default() }
    }

    #[test]
    fn recommendation_fits_the_machine() {
        assert_eq!(recommend(&hw(32.0, 16.0)).model, "gemma4:12b");
        assert!(recommend(&hw(32.0, 16.0)).on_gpu);
        assert_eq!(recommend(&hw(16.0, 8.0)).model, "gemma4:e4b");
        assert!(recommend(&hw(16.0, 8.0)).on_gpu);
        // Grafikkarte zu klein: Prozessor
        let cpu = recommend(&hw(16.0, 2.0));
        assert_eq!(cpu.model, "gemma4:e4b");
        assert!(!cpu.on_gpu);
        assert_eq!(recommend(&hw(8.0, 0.0)).model, "qwen3:4b");
        assert_eq!(recommend(&hw(4.0, 0.0)).model, "qwen3:1.7b");
    }

    /// Live-Pruefung auf diesem PC (manuell: cargo test -- --ignored live_ --nocapture)
    #[test]
    #[ignore]
    fn live_hardware() {
        let hw = hardware();
        println!("{hw:?}");
        println!("{:?}", recommend(&hw));
    }

    /// Live mit dem echten Ollama (manuell: cargo test -- --ignored live_ollama --nocapture):
    /// startet Ollama selbst, laedt das empfohlene Modell 40 s lang an und bricht dann ab.
    #[test]
    #[ignore]
    fn live_ollama_start_and_pull() {
        tauri::async_runtime::block_on(async {
            let ai = LocalAi::new();
            assert!(ollama_installed(), "Ollama ist nicht installiert");
            let t = std::time::Instant::now();
            assert!(ai.ensure_ollama().await, "Ollama ließ sich nicht starten");
            println!("Ollama bereit nach {:.1} s", t.elapsed().as_secs_f32());
            let status = ai.status().await;
            println!("Status: server={} version={} modelle={}", status.server, status.version, status.models.len());
            assert_eq!(status.server, "ollama");
            let model = std::env::var("MILO_PULL_MODEL").unwrap_or_else(|_| recommend(&hardware()).model);
            println!("Modell: {model}");
            let t = std::time::Instant::now();
            let mut last_print = t;
            let mut seen_download = false;
            let mut seen_eta = false;
            let last = ai
                .pull_stream(&model, |p| {
                    seen_download |= p.phase == "download" && p.total > 0;
                    seen_eta |= p.eta_secs.is_some();
                    if last_print.elapsed() >= Duration::from_secs(2) {
                        last_print = std::time::Instant::now();
                        println!(
                            "{:>5.1} s  {:<8} {:>5.1} %  {:.2}/{:.2} GB  {:.1} MB/s  Rest {:?} s",
                            t.elapsed().as_secs_f32(),
                            p.phase,
                            p.percent,
                            p.completed as f64 / 1e9,
                            p.total as f64 / 1e9,
                            p.bytes_per_sec / 1e6,
                            p.eta_secs
                        );
                    }
                    p.phase == "done" || t.elapsed() < Duration::from_secs(40)
                })
                .await
                .expect("Download-Strom");
            println!("Ende: {last:?}");
            assert!(seen_download, "kein Download-Fortschritt gesehen");
            assert!(seen_eta, "keine Restzeit berechnet");
        });
    }

    #[test]
    fn pull_meter_sums_layers_and_estimates_time() {
        let t0 = std::time::Instant::now();
        let mut meter = PullMeter::new("gemma4:12b");
        meter.update(&json!({ "status": "pulling manifest" }), t0);
        assert_eq!(meter.progress.phase, "start");
        meter.update(&json!({ "status": "pulling aaa", "digest": "aaa", "total": 1000u64, "completed": 0u64 }), t0);
        meter.update(&json!({ "status": "pulling bbb", "digest": "bbb", "total": 1000u64, "completed": 0u64 }), t0);
        assert_eq!(meter.progress.phase, "download");
        assert_eq!(meter.progress.total, 2000);
        assert_eq!(meter.progress.eta_secs, None);
        // 1 s spaeter: 500 kB/s gemessen (Groessen hochgerechnet)
        let mut meter = PullMeter::new("m");
        meter.update(&json!({ "digest": "a", "total": 10_000_000u64, "completed": 0u64 }), t0);
        meter.update(&json!({ "digest": "a", "total": 10_000_000u64, "completed": 500_000u64 }), t0 + Duration::from_secs(1));
        assert_eq!(meter.progress.percent.round(), 5.0);
        // Eine Messung reicht noch nicht fuer eine Restzeit.
        assert_eq!(meter.progress.eta_secs, None);
        meter.update(&json!({ "digest": "a", "total": 10_000_000u64, "completed": 1_000_000u64 }), t0 + Duration::from_secs(2));
        assert_eq!(meter.progress.eta_secs, Some(18));
        // Neue Schicht mit 3 MB von frueher: kein Tempo-Sprung, die Restzeit bleibt ruhig.
        meter.update(&json!({ "digest": "b", "total": 4_000_000u64, "completed": 3_000_000u64 }), t0 + Duration::from_millis(2500));
        meter.update(&json!({ "digest": "b", "total": 4_000_000u64, "completed": 3_500_000u64 }), t0 + Duration::from_millis(3500));
        assert!((meter.progress.bytes_per_sec - 500_000.0).abs() < 1.0, "{}", meter.progress.bytes_per_sec);
        meter.update(&json!({ "status": "verifying sha256 digest" }), t0 + Duration::from_secs(2));
        assert_eq!(meter.progress.phase, "check");
        meter.update(&json!({ "status": "success" }), t0 + Duration::from_secs(3));
        assert_eq!(meter.progress.phase, "done");
        assert_eq!(meter.progress.percent, 100.0);
    }

    #[test]
    fn only_one_download_at_a_time() {
        let ai = LocalAi::new();
        assert!(ai.claim("a"));
        assert!(!ai.claim("b"));
        *ai.progress.lock().unwrap() = Some(PullProgress { phase: "done".into(), ..Default::default() });
        assert!(ai.claim("b"));
    }

    #[test]
    fn ollama_tags_become_models() {
        let tags = json!({ "models": [
            { "name": "qwen3:8b", "size": 5_200_000_000u64, "details": { "parameter_size": "8.2B", "family": "qwen3" } },
            { "name": "nomic-embed-text:latest", "size": 1u64, "details": { "family": "nomic-bert" } },
            { "name": "gemma4:e4b", "size": 4_000_000_000u64, "details": { "parameter_size": "4B", "family": "gemma4" } },
        ] });
        let models = ollama_models(&tags);
        assert_eq!(models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["gemma4:e4b", "qwen3:8b"]);
        assert_eq!(models[1].params, "8.2B");
    }
}
