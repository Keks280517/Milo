//! Der Haupt-Agent: nimmt eine Nutzereingabe, erledigt Alltagsbefehle sofort selbst und spricht
//! sonst mit Gemini. Werkzeuge fuehrt er aus und meldet jeden Schritt als Ereignis an die Oberflaeche.
//!
//! Welches Gemini-Modell eine Anfrage bekommt, entscheidet die Bewertung (leicht, schwer, Bericht)
//! zusammen mit dem Kontingent-Zaehler: Milo fragt nie ein Modell, von dem er weiss, dass es voll ist.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use chrono::{Datelike, Local, Timelike};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::apps::AppIndex;
use crate::gemini::{self, Call, ErrorKind, Gemini, Thinking};
use crate::groq::Groq;
use crate::limits::{Limits, Tier, MAX_WAIT_MS};
use crate::memory::Memory;
use crate::quick::{self, Quick};
use crate::router::{self, Rating};
use crate::settings::{Settings, SettingsStore};
use crate::{confirm, tools};

pub const EVENT: &str = "milo://agent";
/// So viele Gespraechsrunden (Nutzereingabe bis Antwort) bleiben im Verlauf.
const KEEP_TURNS: usize = 8;
/// Der Helfer-Wuerfel kommt bei schweren Anfragen dazu. Aus, solange Gemini das einzige Gehirn ist:
/// dann denkt Milo selbst sichtbar nach. Gedacht fuer spaeter, wenn ein zweites Modell (z. B. GLM)
/// als eigener Helfer mitarbeitet.
const SHOW_HELPER: bool = false;
/// Hoechstens so oft wechselt Milo innerhalb einer Anfrage das Modell, wenn eins versagt.
const MAX_SWITCHES: usize = 3;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentEvent {
    /// Zustand fuer die Animation: thinking, answering, working, collab, idle.
    State { state: &'static str },
    Token { text: String },
    /// status: start, ok, error oder declined. `card` fuellt eine fertige Ansicht in der Oberflaeche.
    Tool { name: String, summary: String, status: &'static str, links: Vec<tools::Link>, card: Option<Value> },
    /// Wertung der Anfrage (Bewertungssystem) samt gewaehltem Modell, fuer die Debug-Ansicht.
    Route { rating: Rating },
    /// Der Helfer-Wuerfel fuer das staerkere Modell: arrive, working, talking, done oder leave.
    Helper { phase: &'static str, model: String },
    Done { text: String },
    Error { message: String },
}

#[derive(Default)]
pub struct Agent {
    /// Verlauf im Gemini-Format (`{role: "user"|"model", parts: [...]}`).
    history: tokio::sync::Mutex<Vec<Value>>,
    busy: AtomicBool,
    /// Wird bei jedem Abbruch erhoeht; eine laufende Antwort mit aelterem Stand hoert auf.
    epoch: AtomicU64,
    /// Wann die Verbindung zu Google zuletzt vorgewaermt wurde.
    warmed: Mutex<Option<Instant>>,
    /// Milo hat nach dem Tag gefragt; die naechste Eingabe ist der Rueckblick.
    pub awaiting_review: AtomicBool,
}

fn user_text(text: &str) -> Value {
    json!({ "role": "user", "parts": [{ "text": text }] })
}

fn model_text(text: &str) -> Value {
    json!({ "role": "model", "parts": [{ "text": text }] })
}

impl Agent {
    /// Traegt etwas in den Gespraechsverlauf ein, das ausserhalb des Chats passiert ist
    /// (z. B. eine hineingezogene Datei), damit das Modell bei der naechsten Frage davon weiss.
    pub async fn remember(&self, happened: &str, reply: &str) {
        let mut history = self.history.lock().await;
        history.push(user_text(&tr!(format!("[Hinweis der App, nicht vom Nutzer getippt: {happened}]"), format!("[Note from the app, not typed by the user: {happened}]"))));
        history.push(model_text(reply));
        trim_history(&mut history, KEEP_TURNS);
    }
}

fn emit(app: &AppHandle, event: AgentEvent) {
    if let Err(err) = app.emit(EVENT, event) {
        log::warn!("Ereignis konnte nicht gesendet werden: {err}");
    }
}

#[tauri::command]
pub fn send_message(app: AppHandle, agent: State<'_, Agent>, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(tr!("Leere Eingabe.", "Empty input.").into());
    }
    if agent.busy.swap(true, Ordering::SeqCst) {
        return Err(tr!("Ich bin noch mit der letzten Aufgabe beschäftigt.", "I'm still busy with the last task.").into());
    }
    let epoch = agent.epoch.load(Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        let agent = app.state::<Agent>();
        // Stop wirkt sofort, auch waehrend Gemini noch nachdenkt oder ein Werkzeug laeuft: die ganze
        // Anfrage wird fallen gelassen, damit bricht auch die offene Verbindung ab.
        let outcome = tokio::select! {
            result = run_turn(&app, text, epoch) => Some(result),
            _ = stopped(&agent, epoch) => None,
        };
        match outcome {
            Some(Ok(text)) => emit(&app, AgentEvent::Done { text }),
            Some(Err(err)) => {
                log::error!("Anfrage fehlgeschlagen: {err:#}");
                emit(&app, AgentEvent::Error { message: friendly(&err) });
            }
            None => {
                // Eine halbe Runde darf nicht im Verlauf bleiben, sonst lehnt Gemini die naechste ab.
                drop_unfinished_turn(&mut *agent.history.lock().await);
                emit(&app, AgentEvent::Done { text: tr!("Abgebrochen.", "Stopped.").into() });
            }
        }
        agent.busy.store(false, Ordering::SeqCst);
        emit(&app, AgentEvent::State { state: "idle" });
    });
    Ok(())
}

/// Wartet, bis der Nutzer die Anfrage mit diesem Stand abbricht.
async fn stopped(agent: &Agent, epoch: u64) {
    while agent.epoch.load(Ordering::SeqCst) == epoch {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Bricht die laufende Antwort ab.
#[tauri::command]
pub fn cancel(app: AppHandle, agent: State<'_, Agent>) {
    agent.epoch.fetch_add(1, Ordering::SeqCst);
    // Wartet gerade eine Rueckfrage auf Antwort, gilt sie als abgelehnt.
    crate::confirm::cancel_all(&app);
}

#[tauri::command]
pub async fn clear_chat(app: AppHandle, agent: State<'_, Agent>) -> Result<(), String> {
    agent.epoch.fetch_add(1, Ordering::SeqCst);
    crate::confirm::cancel_all(&app);
    agent.history.lock().await.clear();
    Ok(())
}

/// Waermt die Verbindung zu Google vor, sobald sich Nutzung abzeichnet (Insel geoeffnet,
/// Zuhoeren gestartet). Die erste Anfrage spart sich dann den Verbindungsaufbau.
#[tauri::command]
pub fn warm_up(app: AppHandle, agent: State<'_, Agent>) {
    if !gemini::has_key() {
        return;
    }
    {
        let mut warmed = agent.warmed.lock().unwrap();
        if warmed.is_some_and(|t| t.elapsed() < Duration::from_secs(240)) {
            return;
        }
        *warmed = Some(Instant::now());
    }
    tauri::async_runtime::spawn(async move {
        if let Err(err) = app.state::<Gemini>().models().await {
            log::info!("Vorwaermen: {err}");
        }
    });
}

/// Fragt beim allerersten Mal, ob Anfragen an Google gehen duerfen.
async fn ensure_consent(app: &AppHandle) -> bool {
    let store = app.state::<SettingsStore>();
    if store.get().consent_given {
        return true;
    }
    let detail = tr!("Milo beantwortet Fragen mit Google Gemini. Dafür gehen an Google: deine Frage, der bisherige Gesprächsverlauf \
und alles, was Milo für die Antwort liest (z. B. eine Datei, die du ihm gibst, oder ein Bildschirmfoto, wenn du ihn darum bittest).\n\n\
In der EU gilt dabei auch für das kostenlose Kontingent: Google nutzt die Daten nicht zum Trainieren und veröffentlicht nichts. \
Google speichert Anfragen nur kurz, um Missbrauch zu erkennen.\n\n\
Gib Milo trotzdem keine Passwörter, Kontonummern oder Gesundheitsdaten. Schnellbefehle (Timer, Lautstärke, Programme öffnen) \
laufen weiter ganz ohne Internet-KI.", "Milo answers questions with Google Gemini. Google receives your question, the conversation so far \
and everything Milo reads to answer (for example a file you give it, or a screenshot if you ask for one).\n\n\
In the EU this applies even on the free tier: Google doesn't use the data for training and publishes nothing. \
Google keeps requests only briefly to detect abuse.\n\n\
Still, don't give Milo passwords, account numbers or health data. Quick commands (timers, volume, opening apps) \
keep working without any online AI.");
    if !confirm::ask(app, tr!("Darf Milo Google Gemini benutzen?", "May Milo use Google Gemini?"), detail, tr!("Einverstanden", "Agree"), false).await {
        return false;
    }
    let mut settings = store.get();
    settings.consent_given = true;
    // Die einmalige Zustimmung ersetzt das Nachfragen bei jeder Anfrage (wer es will, schaltet es wieder ein).
    settings.confirm_cloud = false;
    if let Err(err) = store.set(settings) {
        log::warn!("Zustimmung nicht speicherbar: {err}");
    }
    app.state::<Memory>().log_action("cloud", tr!("Zustimmung zur Nutzung von Google Gemini gegeben", "Consent given to use Google Gemini"), true);
    true
}

async fn run_turn(app: &AppHandle, text: String, epoch: u64) -> Result<String> {
    let settings = app.state::<SettingsStore>().get();
    let agent = app.state::<Agent>();
    let cancelled = || agent.epoch.load(Ordering::SeqCst) != epoch;

    // Alltagsbefehle aus der festen Liste: sofort ausfuehren, ganz ohne KI.
    if let Some(found) = quick::parse(&text) {
        if let Some(answer) = run_quick(app, &settings, found).await {
            let mut history = agent.history.lock().await;
            history.push(user_text(&text));
            history.push(model_text(&answer));
            trim_history(&mut history, KEEP_TURNS);
            return Ok(answer);
        }
    }
    // Abendfrage: die Antwort wird als Tagesrueckblick gespeichert, ohne KI.
    let lower = text.to_lowercase();
    if agent.awaiting_review.swap(false, Ordering::SeqCst) {
        let answer = crate::daily::save_review(app, &text);
        remember_plain(&agent, &text, &answer).await;
        return Ok(answer);
    }
    if lower.contains("feierabend") || lower.contains("tagesrückblick") || lower.contains("done for today") || lower.contains("end of day") || lower.contains("day review") {
        let answer = crate::daily::start_review(app).text;
        remember_plain(&agent, &text, &answer).await;
        return Ok(answer);
    }
    let groq_only = !gemini::has_key() && settings.groq_enabled && crate::groq::has_key();
    let local_only = local_only(&settings);
    if !gemini::has_key() && !groq_only && !local_only {
        bail!(tr!("Dafür brauche ich deinen Gemini-Schlüssel oder eine lokale KI. Beides richtest du in den Einstellungen unter KI ein. Schnellbefehle wie Timer oder Lautstärke gehen auch ohne.", "For that I need your Gemini key or a local AI. You can set up either in Settings under AI. Quick commands like timers or volume work without them."));
    }
    if !groq_only && !local_only && !ensure_consent(app).await {
        return Ok(tr!("Okay, dann schicke ich nichts an Gemini. Schnellbefehle wie Timer, Lautstärke oder Programme öffnen funktionieren trotzdem.", "Okay, then I won't send anything to Gemini. Quick commands like timers, volume or opening apps still work.").into());
    }

    let mut rating = if quick::is_browser_task(&text) {
        Rating {
            score: 0,
            threshold: settings.cloud_threshold,
            kind: "browser".into(),
            model: String::new(),
            parts: vec![router::Part { label: tr!("Mehrere Schritte im Browser: schnelles Modell mit den Klick-Werkzeugen", "Several steps in the browser: fast model with the click tools").into(), points: 0 }],
            reason: tr!("Browser-Aufgabe mit mehreren Schritten.", "Browser task with several steps.").into(),
        }
    } else {
        router::decide(router::draft(&text), settings.cloud_threshold)
    };
    // Bleibt die Anfrage auf diesem PC, gibt es nichts zu bestaetigen.
    let stays_local = local_only || (settings.ai_mode == "mixed" && !settings.local_model.is_empty() && rating.tier() == Tier::Easy);
    if settings.confirm_cloud && !stays_local {
        let detail = tr!(format!("Das geht an Google Gemini, zusammen mit dem bisherigen Gesprächsverlauf.\n\n„{text}“"), format!("This goes to Google Gemini, together with the conversation so far.\n\n\"{text}\""));
        if !confirm::ask(app, tr!("An Gemini senden?", "Send to Gemini?"), &detail, tr!("Senden", "Send"), false).await {
            app.state::<Memory>().log_action("cloud", tr!("Senden an Gemini abgelehnt, nichts gesendet", "Sending to Gemini declined, nothing sent"), false);
            return Ok(tr!("Okay, ich habe nichts gesendet.", "Okay, I didn't send anything.").into());
        }
    }
    if cancelled() {
        return Ok(String::new());
    }

    let mut history = agent.history.lock().await;
    history.push(user_text(&text));
    trim_history(&mut history, KEEP_TURNS);
    let answer = converse(app, &settings, &mut history, &mut rating, &cancelled).await;
    // Halbe Werkzeugrunden (Fehler, Abbruch) nicht im Verlauf lassen, sonst lehnt Gemini die naechste Anfrage ab.
    drop_unfinished_turn(&mut history);
    answer
}

/// Eingabe und Antwort ohne KI in den Verlauf schreiben.
async fn remember_plain(agent: &Agent, text: &str, answer: &str) {
    let mut history = agent.history.lock().await;
    history.push(user_text(text));
    history.push(model_text(answer));
    trim_history(&mut history, KEEP_TURNS);
}

/// Einstellungen je Art der Anfrage: Runden, voraussichtliche Aufrufe, Denktiefe, Geduld fuers erste Wort.
fn plan_for(kind: &str) -> (usize, u32, Thinking, Duration) {
    match kind {
        "browser" => (12, 1, Thinking::Low, Duration::from_secs(20)),
        "hard" => (10, 1, Thinking::Medium, Duration::from_secs(90)),
        "report" => (18, 10, Thinking::High, Duration::from_secs(150)),
        _ => (8, 1, Thinking::Low, Duration::from_secs(20)),
    }
}

/// Fuehrt die verlangten Werkzeuge aus und liefert die Antwortnachricht fuer das Modell.
async fn run_tools(app: &AppHandle, calls: &[Call]) -> Value {
    emit(app, AgentEvent::State { state: "working" });
    let mut responses = Vec::new();
    for call in calls {
        emit(app, AgentEvent::Tool { name: call.name.clone(), summary: tools::describe(call), status: "start", links: Vec::new(), card: None });
        let outcome = tools::execute(app, call).await;
        emit(app, AgentEvent::Tool {
            name: call.name.clone(),
            summary: outcome.summary.clone(),
            status: outcome.status(),
            links: outcome.links.clone(),
            card: outcome.card.clone(),
        });
        responses.push(json!({ "functionResponse": { "name": call.name, "response": { "result": outcome.result } } }));
    }
    json!({ "role": "user", "parts": responses })
}

/// Kann Groq jetzt mitarbeiten? (eingeschaltet, Schluessel da, nicht gesperrt, Zustimmung)
async fn groq_ready(app: &AppHandle, settings: &Settings) -> bool {
    if !settings.groq_enabled || !app.state::<Groq>().available() {
        return false;
    }
    if settings.groq_consent {
        return true;
    }
    let detail = tr!("Milo kann zusätzlich Groq benutzen: sehr schnelle Modelle für einfache Fragen und eine Vorüberlegung bei schweren Aufgaben, \
die Gemini dann ausarbeitet. Dafür gehen an Groq (USA): deine Frage, der bisherige Gesprächsverlauf und Werkzeugergebnisse dieser Anfrage.\n\n\
Gib Milo trotzdem keine Passwörter, Kontonummern oder Gesundheitsdaten. Ablehnen schaltet Groq aus; Gemini und Schnellbefehle bleiben.", "Milo can also use Groq: very fast models for simple questions and a first draft for hard tasks, \
which Gemini then works out. Groq (USA) receives your question, the conversation so far and the tool results of this request.\n\n\
Still, don't give Milo passwords, account numbers or health data. Declining turns Groq off; Gemini and quick commands stay.");
    let store = app.state::<SettingsStore>();
    let allowed = confirm::ask(app, tr!("Darf Milo Groq benutzen?", "May Milo use Groq?"), detail, tr!("Einverstanden", "Agree"), false).await;
    let mut next = store.get();
    next.groq_consent = allowed;
    next.groq_enabled = allowed;
    if let Err(err) = store.set(next) {
        log::warn!("Groq-Zustimmung nicht speicherbar: {err}");
    }
    app.state::<Memory>().log_action("cloud", if allowed { tr!("Zustimmung zur Nutzung von Groq gegeben", "Consent given to use Groq") } else { tr!("Groq abgelehnt und ausgeschaltet", "Groq declined and turned off") }, allowed);
    allowed
}

/// Lohnt ein Wechsel zum anderen Anbieter nach diesem Fehler? Nicht bei Fehlern, die der andere
/// genauso haette (z. B. eine abgelehnte Anfrage).
fn worth_switching(err: &anyhow::Error) -> bool {
    let kind = crate::groq::kind_of(err).unwrap_or_else(|| gemini::kind_of(err));
    !matches!(kind, ErrorKind::Other) || { let text = err.to_string(); text.contains("Kontingent") || text.contains("quota") }
}

/// Nur die lokale KI: ausdruecklich so eingestellt, oder "gemischt" ohne Cloud-Schluessel.
fn local_only(settings: &Settings) -> bool {
    let cloud = gemini::has_key() || (settings.groq_enabled && crate::groq::has_key());
    !settings.local_model.is_empty() && (settings.ai_mode == "local" || (settings.ai_mode == "mixed" && !cloud))
}

/// Verteilt die Arbeit: leichte Anfragen beantwortet Groq (schnell), schwere arbeitet Gemini aus,
/// nachdem Groq in Sekunden vorgedacht hat. Faellt einer aus, uebernimmt der andere.
/// Mit lokaler KI: "local" fragt nur sie; "mixed" gibt ihr die leichten Anfragen und springt bei
/// Problemen in beide Richtungen ein.
async fn converse(app: &AppHandle, settings: &Settings, history: &mut Vec<Value>, rating: &mut Rating, cancelled: &impl Fn() -> bool) -> Result<String> {
    let easy = rating.tier() == Tier::Easy;
    if local_only(settings) {
        return converse_local(app, settings, history, rating, cancelled).await;
    }
    let mixed = settings.ai_mode == "mixed" && !settings.local_model.is_empty();
    let mut local_tried = false;
    if mixed && easy {
        local_tried = true;
        match converse_local(app, settings, history, rating, cancelled).await {
            Err(err) if !cancelled() => {
                log::warn!("Lokale KI: {err}");
                note(app, Some(tr!(format!("{err} Ich frage die Cloud."), format!("{err} Asking the cloud."))));
                flatten_current_turn(history);
            }
            other => return other,
        }
    }
    let answer = converse_cloud(app, settings, history, rating, cancelled).await;
    match answer {
        Err(err) if mixed && !local_tried && !cancelled() && worth_switching(&err) => {
            log::warn!("Cloud: {err}");
            note(app, Some(tr!("Die Cloud kommt gerade nicht durch, ich frage die lokale KI.", "The cloud isn't getting through right now, asking the local AI.").into()));
            flatten_current_turn(history);
            converse_local(app, settings, history, rating, cancelled).await
        }
        other => other,
    }
}

/// Gemini und Groq (wie vor der lokalen KI).
async fn converse_cloud(app: &AppHandle, settings: &Settings, history: &mut Vec<Value>, rating: &mut Rating, cancelled: &impl Fn() -> bool) -> Result<String> {
    let groq = groq_ready(app, settings).await;
    let gemini_key = gemini::has_key();
    let easy = rating.tier() == Tier::Easy;
    let mut groq_tried = false;
    if groq && (easy || !gemini_key) {
        groq_tried = true;
        match converse_groq(app, settings, history, rating, cancelled).await {
            Err(err) if gemini_key && !cancelled() && worth_switching(&err) => {
                log::warn!("Groq: {err}");
                note(app, Some(tr!(format!("{err} Ich frage Gemini."), format!("{err} Asking Gemini."))));
                flatten_current_turn(history);
            }
            other => return other,
        }
    }
    let mut extra = String::new();
    if groq && !groq_tried && !cancelled() {
        if let Some(sketch) = sketch(app, settings, history).await {
            extra = tr!(
                format!("\n\nVorüberlegung eines schnellen Helfermodells (Groq) zu dieser Anfrage. Sie kann Fehler enthalten; prüfe sie, nutze, was stimmt, und erwähne sie nicht:\n{sketch}"),
                format!("\n\nFirst draft by a fast helper model (Groq) for this request. It may contain mistakes; check it, use what's right and don't mention it:\n{sketch}")
            );
        }
    }
    match converse_gemini(app, settings, history, rating, cancelled, &extra).await {
        Err(err) if groq && !groq_tried && !cancelled() && worth_switching(&err) => {
            log::warn!("Gemini: {err}");
            note(app, Some(tr!("Gemini kommt gerade nicht durch, ich frage Groq.", "Gemini isn't getting through right now, asking Groq.").into()));
            flatten_current_turn(history);
            converse_groq(app, settings, history, rating, cancelled).await
        }
        other => other,
    }
}

/// Fuehrt das Gespraech mit Groq (samt Werkzeugen), bis eine Antwort ohne Werkzeugaufruf kommt.
async fn converse_groq(app: &AppHandle, settings: &Settings, history: &mut Vec<Value>, rating: &mut Rating, cancelled: &impl Fn() -> bool) -> Result<String> {
    let groq = app.state::<Groq>();
    let model = settings.groq_model.clone();
    let (rounds, _, _, first_wait) = plan_for(&rating.kind);
    let system = format!("{}{}", system_prompt(settings), hint_for(&rating.kind));
    let tools = crate::groq::compact_tools(&tools::definitions(settings));
    rating.model = format!("Groq · {model}");
    emit(app, AgentEvent::Route { rating: rating.clone() });
    emit(app, AgentEvent::State { state: "thinking" });
    let sent: usize = history.iter().map(|m| m.to_string().len()).sum();
    app.state::<Memory>().log_action("cloud", &tr!(format!("Anfrage an Groq {model} ({}, Verlauf mit ca. {sent} Zeichen)", rating.kind), format!("Request to Groq {model} ({}, history of about {sent} characters)", rating.kind)), true);
    for _ in 0..rounds {
        let mut speaking = false;
        let turn = groq
            .turn(&model, &system, history, &tools, first_wait.min(Duration::from_secs(20)), |token| {
                if cancelled() {
                    return false;
                }
                if !speaking {
                    speaking = true;
                    emit(app, AgentEvent::State { state: "answering" });
                }
                emit(app, AgentEvent::Token { text: token.to_string() });
                true
            })
            .await?;
        history.push(turn.content.clone());
        if cancelled() || !turn.completed || turn.calls.is_empty() {
            return Ok(turn.text);
        }
        history.push(run_tools(app, &turn.calls).await);
        if cancelled() {
            return Ok(String::new());
        }
        emit(app, AgentEvent::State { state: "thinking" });
    }
    bail!(tr!("Das hat zu viele Schritte gebraucht, ich habe aufgehört.", "That took too many steps, so I stopped."))
}

/// Fuehrt das Gespraech mit der lokalen KI (samt Werkzeugen, soweit das Modell sie kann).
/// Nichts davon verlaesst den Rechner.
async fn converse_local(app: &AppHandle, settings: &Settings, history: &mut Vec<Value>, rating: &mut Rating, cancelled: &impl Fn() -> bool) -> Result<String> {
    let model = settings.local_model.clone();
    if model.is_empty() {
        bail!(tr!("Es ist noch kein lokales Modell gewählt. Das geht in den Einstellungen unter KI.", "No local model has been chosen yet. You can pick one in Settings under AI."));
    }
    let local = app.state::<crate::local::LocalAi>();
    let (rounds, _, _, first_wait) = plan_for(&rating.kind);
    let system = format!("{}{}", system_prompt(settings), hint_for(&rating.kind));
    let tools = crate::groq::compact_tools(&tools::definitions(settings));
    rating.model = tr!(format!("Lokal · {model}"), format!("Local · {model}"));
    emit(app, AgentEvent::Route { rating: rating.clone() });
    emit(app, AgentEvent::State { state: "thinking" });
    app.state::<Memory>().log_action("local", &tr!(format!("Anfrage an die lokale KI {model} ({})", rating.kind), format!("Request to the local AI {model} ({})", rating.kind)), true);
    for _ in 0..rounds {
        let mut speaking = false;
        let turn = local
            .turn(&model, &system, history, &tools, first_wait, |token| {
                if cancelled() {
                    return false;
                }
                if !speaking {
                    speaking = true;
                    emit(app, AgentEvent::State { state: "answering" });
                }
                emit(app, AgentEvent::Token { text: token.to_string() });
                true
            })
            .await?;
        history.push(turn.content.clone());
        if cancelled() || !turn.completed || turn.calls.is_empty() {
            return Ok(turn.text);
        }
        history.push(run_tools(app, &turn.calls).await);
        if cancelled() {
            return Ok(String::new());
        }
        emit(app, AgentEvent::State { state: "thinking" });
    }
    bail!(tr!("Das hat zu viele Schritte gebraucht, ich habe aufgehört.", "That took too many steps, so I stopped."))
}

/// Zusammenarbeit bei schweren Anfragen: Groq zerlegt die Aufgabe in wenigen Sekunden in
/// Teilschritte. Gemini bekommt das als Vorueberlegung; der Nutzer sieht sie als Karte.
async fn sketch(app: &AppHandle, settings: &Settings, history: &[Value]) -> Option<String> {
    let start = last_user_input(history)?;
    let question = history[start]["parts"].as_array()?.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n");
    let helper = |phase: &'static str| emit(app, AgentEvent::Helper { phase, model: "Groq".into() });
    helper("arrive");
    helper("working");
    let started = Instant::now();
    let system = "Du bist ein schneller Helfer eines anderen KI-Assistenten auf einem Windows-PC. Zerlege die Aufgabe in höchstens sechs knappe Stichpunkte: \
was zu klären, nachzuschlagen oder zu prüfen ist und ein möglicher Lösungsweg mit den wichtigsten Fakten, die du sicher weißt. \
Keine endgültige Antwort, keine Einleitung, keine Werkzeuge. Schreib Deutsch. Wenn du etwas nicht sicher weißt, kennzeichne es.";
    let system = tr!(system, "You are a fast helper for another AI assistant on a Windows PC. Break the task down into at most six short bullet points: \
what needs to be clarified, looked up or checked, and a possible approach with the most important facts you know for sure. \
No final answer, no introduction, no tools. Write in English. Mark anything you're not sure about.");
    let contents = vec![user_text(&question)];
    let groq = app.state::<Groq>();
    let call = groq.turn(&settings.groq_model, system, &contents, &[], Duration::from_secs(8), |_| true);
    match tokio::time::timeout(Duration::from_secs(12), call).await {
        Ok(Ok(turn)) if !turn.text.trim().is_empty() => {
            helper("done");
            let seconds = format!("{:.1}", started.elapsed().as_secs_f32()).replace('.', ",");
            let text: String = turn.text.trim().chars().take(2500).collect();
            emit(app, AgentEvent::Tool {
                name: "groq".into(),
                summary: tr!(format!("Groq hat in {seconds} s vorgedacht, Gemini arbeitet es aus"), format!("Groq drafted it in {seconds} s, Gemini is working it out")),
                status: "ok",
                links: Vec::new(),
                card: Some(json!({ "type": "sketch", "text": text })),
            });
            Some(text)
        }
        other => {
            if let Ok(Err(err)) = other {
                log::info!("Groq-Vorüberlegung entfällt: {err}");
            }
            helper("leave");
            None
        }
    }
}

/// Fuehrt das Gespraech mit Gemini, bis eine Antwort ohne Werkzeugaufruf kommt. `extra` haengt
/// an den Systemtext an (z. B. die Vorueberlegung von Groq).
async fn converse_gemini(app: &AppHandle, settings: &Settings, history: &mut Vec<Value>, rating: &mut Rating, cancelled: &impl Fn() -> bool, extra: &str) -> Result<String> {
    let limits = app.state::<Limits>();
    let gemini = app.state::<Gemini>();
    let memory = app.state::<Memory>();
    let tier = rating.tier();
    let (rounds, calls, thinking, first_wait) = plan_for(&rating.kind);
    let system = format!("{}{}{extra}", system_prompt(settings), hint_for(&rating.kind));
    let decls: Vec<Value> = tools::definitions(settings).iter().map(gemini::declaration).collect();
    let strong = SHOW_HELPER && tier != Tier::Easy;
    let helper = |phase: &'static str, model: &str| {
        if strong {
            emit(app, AgentEvent::Helper { phase, model: model.to_string() });
        }
    };

    let mut skip: Vec<String> = Vec::new();
    let Some(mut pick) = limits.pick(settings, tier, calls, &skip) else {
        bail!(quota_gone(&limits.usage(settings)));
    };
    rating.model = pick.model.clone();
    emit(app, AgentEvent::Route { rating: rating.clone() });
    note(app, pick.note.take());
    emit(app, AgentEvent::State { state: if strong { "collab" } else { "thinking" } });
    helper("arrive", &pick.model);

    let mut switches = 0;
    // Bei Ueberlastung (503) einmal kurz dasselbe Modell erneut fragen, bevor gewechselt wird.
    let mut retried = false;
    // Sind alle Modelle gleichzeitig ueberlastet, wartet Milo kurz und versucht es erneut.
    let mut waits = 0;
    let mut round = 0;
    let mut logged = false;
    while round < rounds {
        if pick.wait_ms > 0 {
            if !wait(pick.wait_ms, cancelled).await {
                helper("leave", &pick.model);
                return Ok(String::new());
            }
            pick.wait_ms = 0;
        }
        let model = pick.model.clone();
        helper("working", &model);
        limits.begin(&model);
        if !logged {
            let sent: usize = history.iter().map(|m| m.to_string().len()).sum();
            memory.log_action("cloud", &tr!(format!("Anfrage an {model} ({}, Verlauf mit ca. {} Zeichen)", rating.kind, sent), format!("Request to {model} ({}, history of about {} characters)", rating.kind, sent)), true);
            logged = true;
        }
        let mut speaking = false;
        let result = gemini
            .turn(&model, &system, history, &decls, thinking, first_wait, |token| {
                if cancelled() {
                    return false;
                }
                if !speaking {
                    speaking = true;
                    emit(app, AgentEvent::State { state: "answering" });
                    helper("talking", &model);
                }
                emit(app, AgentEvent::Token { text: token.to_string() });
                true
            })
            .await;

        let turn = match result {
            Ok(turn) => turn,
            Err(err) => {
                let kind = gemini::kind_of(&err);
                log::warn!("{model}: {err}");
                match kind {
                    ErrorKind::Quota | ErrorKind::Missing => limits.mark_exhausted(&model),
                    ErrorKind::Busy if !retried && !cancelled() => {
                        retried = true;
                        tokio::time::sleep(Duration::from_millis(900)).await;
                        continue;
                    }
                    ErrorKind::Busy | ErrorKind::Slow => {}
                    _ => {
                        helper("leave", &model);
                        return Err(err);
                    }
                }
                retried = false;
                skip.push(model.clone());
                switches += 1;
                let next = if switches <= MAX_SWITCHES { limits.pick(settings, tier, 1, &skip) } else { None };
                let next = match next {
                    Some(next) => next,
                    None if matches!(kind, ErrorKind::Busy | ErrorKind::Slow) && waits < 3 => {
                        waits += 1;
                        note(app, Some(tr!(format!("Gemini ist gerade stark ausgelastet, ich versuche es in {} Sekunden noch einmal.", waits * 5), format!("Gemini is very busy right now, I'll try again in {} seconds.", waits * 5))));
                        if !wait(waits as i64 * 5000, cancelled).await {
                            helper("leave", &model);
                            return Ok(String::new());
                        }
                        skip.clear();
                        switches = 0;
                        match limits.pick(settings, tier, 1, &skip) {
                            Some(again) => again,
                            None => bail!(quota_gone(&limits.usage(settings))),
                        }
                    }
                    None => {
                        helper("leave", &model);
                        if matches!(kind, ErrorKind::Busy | ErrorKind::Slow) {
                            return Err(err);
                        }
                        bail!(quota_gone(&limits.usage(settings)));
                    }
                };
                // Ein anderes Modell kennt die Denk-Signaturen des ersten nicht.
                flatten_current_turn(history);
                rating.model = next.model.clone();
                emit(app, AgentEvent::Route { rating: rating.clone() });
                let why = match kind {
                    ErrorKind::Busy => tr!("ist gerade überlastet", "is overloaded right now"),
                    ErrorKind::Slow => tr!("antwortet gerade zu langsam", "is answering too slowly right now"),
                    ErrorKind::Missing => tr!("gibt es für deinen Schlüssel nicht", "isn't available for your key"),
                    _ => tr!("hat sein Kontingent für heute aufgebraucht", "has used up its quota for today"),
                };
                let quota_note = if kind == ErrorKind::Quota { next.note.clone() } else { None };
                note(app, quota_note.or_else(|| Some(tr!(format!("{model} {why}, ich frage {}.", next.model), format!("{model} {why}, asking {}.", next.model)))));
                pick = next;
                continue;
            }
        };
        round += 1;
        limits.add_tokens(&model, turn.tokens);
        history.push(turn.content.clone());
        if cancelled() || !turn.completed || turn.calls.is_empty() {
            helper(if turn.text.trim().is_empty() { "leave" } else { "done" }, &model);
            return Ok(turn.text);
        }

        history.push(run_tools(app, &turn.calls).await);
        if cancelled() {
            helper("leave", &model);
            return Ok(String::new());
        }
        emit(app, AgentEvent::State { state: if strong { "collab" } else { "thinking" } });
        // Innerhalb einer Anfrage beim selben Modell bleiben; notfalls kurz auf die Minutengrenze warten.
        pick.wait_ms = match limits.room(settings, &model, 1) {
            crate::limits::Room::WaitMs(ms) => ms,
            _ => 0,
        };
    }
    helper("leave", &pick.model);
    bail!(tr!("Das hat zu viele Schritte gebraucht, ich habe aufgehört.", "That took too many steps, so I stopped."))
}

/// Kurzer Hinweis in der Oberflaeche (z. B. Modellwechsel).
fn note(app: &AppHandle, text: Option<String>) {
    if let Some(summary) = text {
        emit(app, AgentEvent::Tool { name: "cloud".into(), summary, status: "ok", links: Vec::new(), card: None });
    }
}

/// Wartet, bis ein Modell in der Minute wieder frei ist. `false`, wenn der Nutzer abgebrochen hat.
async fn wait(ms: i64, cancelled: &impl Fn() -> bool) -> bool {
    let until = Instant::now() + Duration::from_millis(ms.clamp(0, 65_000) as u64);
    while Instant::now() < until {
        if cancelled() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    !cancelled()
}

/// Meldung, wenn fuer heute kein Modell mehr frei ist.
fn quota_gone(usage: &crate::limits::Usage) -> String {
    let at = chrono::DateTime::from_timestamp_millis(usage.reset_ms)
        .map(|t| t.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| tr!("morgen", "tomorrow").into());
    tr!(format!("Das kostenlose Gemini-Kontingent ist für heute aufgebraucht. Um {at} Uhr gibt es neues. Schnellbefehle wie Timer, Lautstärke oder Programme öffnen gehen weiterhin."), format!("The free Gemini quota is used up for today. New quota arrives at {at}. Quick commands like timers, volume or opening apps still work."))
}

/// Index der letzten echten Nutzereingabe (nicht der Werkzeugergebnisse).
fn last_user_input(history: &[Value]) -> Option<usize> {
    history.iter().rposition(is_user_input)
}

fn is_user_input(message: &Value) -> bool {
    message["role"] == "user" && message["parts"].as_array().is_some_and(|parts| parts.iter().any(|p| p.get("text").is_some()))
}

/// Macht aus den Werkzeugschritten der laufenden Anfrage normalen Text. Noetig, wenn mitten in
/// der Anfrage das Modell wechselt: Denk-Signaturen gelten nur fuer das Modell, das sie erzeugt hat.
fn flatten_current_turn(history: &mut [Value]) {
    let Some(start) = last_user_input(history) else { return };
    for message in history[start + 1..].iter_mut() {
        let Some(parts) = message["parts"].as_array() else { continue };
        let lines: Vec<String> = parts
            .iter()
            .filter_map(|p| {
                if let Some(text) = p["text"].as_str().filter(|_| p["thought"].as_bool() != Some(true)) {
                    Some(text.to_string())
                } else if let Some(call) = p.get("functionCall") {
                    Some(format!("[Werkzeug {} aufgerufen mit {}]", call["name"].as_str().unwrap_or_default(), call["args"]))
                } else {
                    p.get("functionResponse").map(|r| format!("[Ergebnis von {}: {}]", r["name"].as_str().unwrap_or_default(), r["response"]["result"]))
                }
            })
            .collect();
        *message = json!({ "role": message["role"], "parts": [{ "text": lines.join("\n") }] });
    }
}

/// Entfernt eine abgebrochene Anfrage samt halber Werkzeugrunden aus dem Verlauf.
fn drop_unfinished_turn(history: &mut Vec<Value>) {
    if let Some(start) = last_user_input(history) {
        let finished = history.last().is_some_and(|m| m["role"] == "model" && !m["parts"].as_array().is_some_and(|p| p.iter().any(|x| x.get("functionCall").is_some())));
        if !finished {
            history.truncate(start);
        }
    }
}

/// Eine einzelne Frage ohne Werkzeuge und ohne Verlauf (Bildschirmblick, Text umschreiben,
/// Seiten zusammenfassen, Briefing). Waehlt das Modell ueber den Kontingent-Zaehler und weicht
/// bei vollen Modellen aus.
pub async fn ask_once(app: &AppHandle, tier: Tier, system: &str, parts: Vec<Value>, thinking: Thinking) -> Result<String> {
    if !gemini::has_key() {
        bail!(tr!("Es ist kein Gemini-Schlüssel hinterlegt.", "No Gemini key has been added."));
    }
    let settings = app.state::<SettingsStore>().get();
    let limits = app.state::<Limits>();
    let gemini = app.state::<Gemini>();
    let contents = vec![json!({ "role": "user", "parts": parts })];
    let mut skip: Vec<String> = Vec::new();
    for _ in 0..=MAX_SWITCHES {
        let Some(pick) = limits.pick(&settings, tier, 1, &skip) else {
            bail!(quota_gone(&limits.usage(&settings)));
        };
        if pick.wait_ms > 0 {
            tokio::time::sleep(Duration::from_millis(pick.wait_ms.min(MAX_WAIT_MS) as u64)).await;
        }
        limits.begin(&pick.model);
        match gemini.turn(&pick.model, system, &contents, &[], thinking, Duration::from_secs(60), |_| true).await {
            Ok(turn) => {
                limits.add_tokens(&pick.model, turn.tokens);
                return Ok(turn.text.trim().to_string());
            }
            Err(err) => match gemini::kind_of(&err) {
                ErrorKind::Quota | ErrorKind::Missing => {
                    limits.mark_exhausted(&pick.model);
                    skip.push(pick.model);
                }
                ErrorKind::Busy | ErrorKind::Slow => skip.push(pick.model),
                _ => return Err(err),
            },
        }
    }
    Err(anyhow!(tr!("Gemini ist gerade nicht erreichbar.", "Gemini can't be reached right now.")))
}

const BROWSER_HINT: &str = "\nDas ist eine Aufgabe im Webbrowser. Geh planvoll vor: Was genau soll am Ende auf dem Bildschirm laufen? \
Kennst du die Adresse (oder findest sie mit twitch_live bzw. find_links heraus), öffne sie direkt mit open_website, bei genanntem Bildschirm mit screen. \
Suchst du etwas auf einer bekannten Seite (Netflix, YouTube, Prime Video, Disney+, Spotify, Twitch …), nimm browser_search_site: \
das öffnet direkt die Trefferliste; dann klickst du mit browser_click auf den Titel und auf Abspielen. \
Sonst öffne zuerst die passende Seite mit open_website \
und klicke dann mit browser_click Schritt für Schritt auf die sichtbaren Namen (Profil, Serie, Folge, Abspielen). Nach jedem Klick lädt die Seite kurz; das Ergebnis von browser_click listet, was danach anklickbar ist, \
ein gerade geöffnetes Fenster (z. B. die Vorschau einer Serie) steht dabei vorn: Abspielen/Wiedergabe klickst du dort, nie im großen Banner oben auf der Startseite. \
Weißt du nicht, was auf der Seite steht, schau mit browser_list oder browser_read nach, statt zu raten. \
Gib nicht beim ersten Fehlschlag auf. Klicke nie auf Kaufen, Bezahlen, Abonnieren, Löschen, Senden oder Konto-Einstellungen und tippe nie Passwörter. \
Wenn alles erledigt ist, antworte in einem kurzen Satz, was du getan hast.";

const REPORT_HINT: &str = "\nDas ist ein Bericht-Auftrag. Gehe so vor:\n\
1. Teile das Thema in 3 bis 6 sinnvolle Unterthemen und recherchiere jedes mit deep_research (ein Aufruf pro Unterthema).\n\
2. Schreibe daraus einen gut gegliederten, sachlichen Bericht auf Deutsch: Einleitung, ein Abschnitt pro Unterthema, Fazit. \
Schreibe ausführlich: pro Abschnitt mehrere Absätze mit allen wichtigen Fakten, Zahlen und Namen, insgesamt etwa 1500 bis 3000 Wörter. \
Nur Fakten aus der Recherche, nichts erfinden.\n\
3. Speichere ihn mit create_word_document: Titel, Abschnitte mit Überschrift und Absätzen, und alle verwendeten Quellen.\n\
4. Antworte danach nur in zwei, drei Sätzen, worum es im Bericht geht und dass die Datei geöffnet wurde.";

const HARD_HINT: &str = "\nDiese Anfrage ist anspruchsvoll: denke sorgfältig nach und antworte gründlich, aber ohne Füllsätze.";

const BROWSER_HINT_EN: &str = "\nThis is a task in the web browser. Plan it: what exactly should be running on screen at the end? \
If you know the address (or find it with twitch_live or find_links), open it directly with open_website, with screen if a screen was named. \
If you're looking for something on a known site (Netflix, YouTube, Prime Video, Disney+, Spotify, Twitch …), use browser_search_site: \
it opens the results list directly; then click the title and Play with browser_click. \
Otherwise first open the right site with open_website \
and then click the visible names step by step with browser_click (profile, show, episode, play). The page loads briefly after each click; the result of browser_click lists what can be clicked next, \
with a window that just opened (such as a show preview) first: click Play there, never in the big banner at the top of the home page. \
If you don't know what's on the page, check with browser_list or browser_read instead of guessing. \
Don't give up after the first failure. Never click buy, pay, subscribe, delete, send or account settings, and never type passwords. \
When everything is done, say in one short sentence what you did.";

const REPORT_HINT_EN: &str = "\nThis is a report request. Proceed like this:\n\
1. Split the topic into 3 to 6 sensible subtopics and research each with deep_research (one call per subtopic).\n\
2. Write a well-structured, factual report in English from it: introduction, one section per subtopic, conclusion. \
Be thorough: several paragraphs per section with all the important facts, numbers and names, about 1500 to 3000 words in total. \
Only facts from the research, nothing made up.\n\
3. Save it with create_word_document: title, sections with heading and paragraphs, and all sources used.\n\
4. Afterwards answer in just two or three sentences what the report is about and that the file was opened.";

const HARD_HINT_EN: &str = "\nThis request is demanding: think carefully and answer thoroughly, but without filler.";

fn hint_for(kind: &str) -> &'static str {
    match kind {
        "browser" if crate::i18n::en() => BROWSER_HINT_EN,
        "report" if crate::i18n::en() => REPORT_HINT_EN,
        "hard" if crate::i18n::en() => HARD_HINT_EN,
        "browser" => BROWSER_HINT,
        "report" => REPORT_HINT,
        "hard" => HARD_HINT,
        _ => "",
    }
}

// ---------- Schnellbefehle ----------

/// Werkzeuge, bei denen ein Fehlschlag oft an der Schreibweise liegt („Lübek“ statt „Lübeck“):
/// dann versucht es das Sprachmodell mit seinen Korrekturregeln noch einmal.
const RETRY_WITH_MODEL: &[&str] = &["open_app", "open_website", "close_app", "get_weather", "play_youtube"];

fn quick_rating(threshold: u8) -> Rating {
    Rating {
        score: 0,
        threshold,
        kind: "quick".into(),
        model: String::new(),
        parts: vec![router::Part { label: tr!("Schnellbefehl aus der festen Liste, ganz ohne KI", "Quick command from the fixed list, no AI at all").into(), points: 0 }],
        reason: tr!("Erkannter Alltagsbefehl: sofort ausgeführt.", "Recognized everyday command: done right away.").into(),
    }
}

/// Fuehrt einen Schnellbefehl aus und liefert die Antwort. `None`: passt doch nicht, Gemini uebernimmt.
async fn run_quick(app: &AppHandle, settings: &Settings, found: Quick) -> Option<String> {
    let (name, mut args, times) = match found {
        Quick::Say(answer) => {
            emit(app, AgentEvent::Route { rating: quick_rating(settings.cloud_threshold) });
            return Some(answer);
        }
        Quick::Open(target) => resolve_open(app, &target).await?,
        Quick::OpenOn(target, screen) => {
            let (name, mut args, times) = resolve_open(app, &target).await?;
            args["screen"] = json!(screen);
            (name, args, times)
        }
        Quick::Tool { name, args, times } => (name, args, times),
    };
    if name == "get_weather" && args["location"].as_str().unwrap_or_default().is_empty() {
        if settings.weather_place.is_empty() {
            return None; // ohne bekannten Ort fragt das Modell nach
        }
        args["location"] = json!(settings.weather_place);
    }

    emit(app, AgentEvent::Route { rating: quick_rating(settings.cloud_threshold) });
    emit(app, AgentEvent::State { state: "working" });
    let call = Call { name: name.to_string(), args: args.clone() };
    emit(app, AgentEvent::Tool { name: name.into(), summary: tools::describe(&call), status: "start", links: Vec::new(), card: None });
    let mut outcome = tools::execute(app, &call).await;
    for _ in 1..times {
        if !outcome.ok {
            break;
        }
        outcome = tools::execute(app, &call).await;
    }
    emit(app, AgentEvent::Tool {
        name: name.into(),
        summary: outcome.summary.clone(),
        status: outcome.status(),
        links: outcome.links.clone(),
        card: outcome.card.clone(),
    });
    if !outcome.ok && !outcome.declined && RETRY_WITH_MODEL.contains(&name) {
        return None;
    }
    Some(quick_reply(name, &args, &outcome))
}

/// Entscheidet, ob ein Name ein Programm oder eine Webseite meint. `None`: unklar, Gemini soll es versuchen.
async fn resolve_open(app: &AppHandle, target: &str) -> Option<(&'static str, Value, u8)> {
    let site = quick::site(target);
    let website = |(display, url): (&str, &str)| ("open_website", json!({ "url": url, "display": display }), 1);
    if let Some((display, url, false)) = site {
        return Some(website((display, url)));
    }
    if crate::screens::is_browser_word(target) {
        return Some(("open_app", json!({ "name": target }), 1));
    }
    let index = app.state::<AppIndex>();
    index.ensure_fresh(std::time::Duration::from_secs(600)).await;
    if index.find(target).is_some() {
        return Some(("open_app", json!({ "name": target }), 1));
    }
    if let Some((display, url, _)) = site {
        return Some(website((display, url)));
    }
    quick::looks_like_domain(target).then(|| website((target, target)))
}

/// Kurze Bestaetigung in normalem Deutsch; Fehlermeldungen der Werkzeuge sind schon fuer den Nutzer formuliert.
fn quick_reply(name: &str, args: &Value, outcome: &tools::Outcome) -> String {
    if !outcome.ok {
        return outcome.summary.clone();
    }
    let arg = |key: &str| args[key].as_str().unwrap_or_default().to_string();
    match name {
        "open_app" => tr!(format!("Ich habe {}.", outcome.summary), format!("{}.", outcome.summary)),
        "open_website" => tr!(format!("{} ist geöffnet.", arg("display")), format!("{} is open.", arg("display"))),
        "close_app" => tr!(format!("Ich habe {} geschlossen.", capitalize(&arg("name"))), format!("I closed {}.", capitalize(&arg("name")))),
        "save_note" => tr!("Okay, hab ich mir gemerkt.", "Okay, got it.").into(),
        "browser_search" => tr!(format!("Ich habe die {}.", outcome.summary), format!("{}.", outcome.summary)),
        "media_control" => format!("{}.", outcome.summary),
        "get_weather" => {
            let place = outcome.card.as_ref().and_then(|c| c["place"].as_str()).unwrap_or(tr!("deinem Ort", "your area")).to_string();
            tr!(format!("So sieht das Wetter in {place} aus."), format!("Here's the weather in {place}."))
        }
        // „Timer“ ist nur die Standardbezeichnung, die muss nicht noch einmal genannt werden.
        "set_timer" => outcome.result.replace("Timer „Timer“ gestellt", "Timer gestellt").replace("Timer \"Timer\" set", "Timer set"),
        "cancel_timer" => outcome.result.replace("Der Timer „Timer“ wurde", "Der Timer wurde").replace("The timer \"Timer\" was", "The timer was"),
        "system_power" => {
            let minutes = args["in_minutes"].as_f64().unwrap_or(0.0);
            let action = arg("action");
            let floor = if action == "sleep" { crate::power::SLEEP_FLOOR_SECONDS } else { crate::system::POWER_DELAY_SECONDS as u64 };
            let when = tools::wait_text(((minutes * 60.0).round() as u64).max(floor));
            let replaced = if outcome.summary.contains("ersetzt") || outcome.summary.contains("replaced") { tr!(" Den vorher geplanten Zeitpunkt habe ich ersetzt.", " I replaced the time planned before.") } else { "" };
            match action.as_str() {
                "shutdown" => tr!(format!("Alles klar, der PC fährt {when} herunter. Bis dahin kannst du es jederzeit abbrechen.{replaced}"), format!("All right, the PC shuts down {when}. Until then you can cancel it any time.{replaced}")),
                "restart" => tr!(format!("Alles klar, der PC startet {when} neu. Bis dahin kannst du es jederzeit abbrechen.{replaced}"), format!("All right, the PC restarts {when}. Until then you can cancel it any time.{replaced}")),
                "sleep" => tr!(format!("Alles klar, der PC geht {when} in den Energiesparmodus."), format!("All right, the PC goes to sleep {when}.")),
                _ => outcome.result.clone(),
            }
        }
        _ => outcome.result.clone(),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

/// Behaelt nur die letzten `keep` Gespraechsrunden. Geschnitten wird immer vor einer
/// Nutzereingabe, damit Werkzeugaufruf und Werkzeugergebnis nie getrennt werden.
pub fn trim_history(history: &mut Vec<Value>, keep: usize) {
    let starts: Vec<usize> = history.iter().enumerate().filter(|(_, m)| is_user_input(m)).map(|(i, _)| i).collect();
    if starts.len() > keep {
        history.drain(..starts[starts.len() - keep]);
    }
}

const WEEKDAYS: [&str; 7] = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];
const MONTHS: [&str; 12] = [
    "Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember",
];

pub fn system_prompt(settings: &Settings) -> String {
    let now = Local::now();
    if crate::i18n::en() {
        return format!(
        "You are {name}, a personal assistant on the user's Windows PC. \
You always answer in English: short, friendly and casual. No Markdown headings, at most short lists.\n\
When the user asks for an action, call the matching tool instead of just describing what you would do. \
Handle several requests in one sentence one after another with several tool calls. \
For small talk or questions you can answer with confidence, answer directly.\n\
\"Calculator\" means the Calculator app, not the PC. \
For timers with a clock time (\"at 6 pm\") pass at_time and don't do the math yourself.\n\
If the user asks about something they may have told you before, look it up in memory with search_notes.\n\
Read files the user gave you with read_file before saying anything about their content; \
without a name they mean the most recent file. A summary covers the whole file: \
every section and, for worksheets, every task, not just the first.\n\
If you need current information (news, prices, results, dates) or aren't sure about a fact, \
research it with web_research instead of guessing, and briefly name the source afterwards. For weather always use get_weather, never web_research; \
if get_weather doesn't know the place, fix the spelling (speech recognition makes mistakes) and try again. \
If no suitable tool is available or you find nothing, say so honestly.\n\
For YouTube videos use play_youtube. To find something on Netflix, Prime Video, Disney+, Spotify or Twitch use browser_search_site \
(opens the results directly), then click the title and Play with browser_click. On other sites open the site with open_website \
and then click the visible names step by step with browser_click; the page loads briefly after each click. \
browser_list and browser_read show what's on the page; browser_scroll and browser_key help with scrolling and navigating, \
browser_tabs switches tabs, browser_navigate opens an address in the current tab. \
If you don't know the exact address, find it with find_links and open the best match directly instead of clicking your way there. \
For Twitch: check with twitch_live which channel is live right now, then open https://www.twitch.tv/<channel>. \
Official esports channels are usually named after the game (valorant, riotgames, lolesports, esl_csgo, rocketleague) or with a region (valorant_emea, valorant_americas, valorant_pacific); \
if several are live, pick the best fit (main channel or big event) and briefly mention the stream title. \
To save something as a Word file, use create_word_document. \
Pass delayed commands (\"shut down in five minutes\") with in_minutes.\n\
{browser}{screens} If the user names a screen (\"on the second screen\"), pass screen to open_website, play_youtube, browser_search or open_app; \
arrange windows that are already open with window_control.\n\
If there's no dedicated tool for a request on the PC, you may suggest a short PowerShell script with run_script; \
the user sees it first and decides. Only look at the screen with look_at_screen when the user explicitly asks for it.\n\
Shut down, restart and sleep without asking back when the user requests it. \
Before closing an app and before a script, the app itself shows the user a safety prompt; don't ask again in the chat.\n\
After finishing an action, confirm it in one short sentence and state numbers and names as you understood them.\n\
Names and titles from speech recognition are often misspelled. \
Don't give up after the first failure: if a tool can't find something, think about what might be meant \
(different spelling, pronunciation, original title), and try again with your best guess \
before telling the user it doesn't work. If a tool offers you alternatives, pick the best fit yourself.\n\
{user}Today is {today}.",
        screens = crate::screens::describe(&crate::screens::list()),
        browser = match crate::screens::default_browser_name() {
            Some(name) => format!("The user's default browser is {name}; \"browser\" or \"internet\" means it. "),
            None => String::new(),
        },
        user = if settings.user_name.is_empty() { String::new() } else { format!("The user's name is {}.\n", settings.user_name) },
        name = settings.name,
        today = now.format("%A, %B %-d, %Y, %H:%M"),
    );
    }
    format!(
        "Du bist {name}, ein persönlicher Assistent auf dem Windows-PC des Nutzers. \
Du antwortest immer auf Deutsch, kurz, freundlich und in der Du-Form. Kein Markdown mit Überschriften, höchstens kurze Listen.\n\
Wenn der Nutzer eine Aktion verlangt, rufe das passende Werkzeug auf, statt nur zu beschreiben, was du tun würdest. \
Mehrere Aufträge in einem Satz erledigst du nacheinander mit mehreren Werkzeugaufrufen. \
Bei Smalltalk oder Fragen, die du sicher beantworten kannst, antworte direkt.\n\
„Rechner“ oder „Taschenrechner“ meint das Rechner-Programm, nicht den PC. \
Bei Timern mit Uhrzeit („um 18 Uhr“) gibst du at_time an und rechnest nicht selbst.\n\
Fragt der Nutzer nach etwas, das er dir früher gesagt haben könnte, schau mit search_notes im Gedächtnis nach.\n\
Dateien, die der Nutzer dir gegeben hat, liest du mit read_file, bevor du etwas über ihren Inhalt sagst; \
ohne Namen meint er die zuletzt gegebene Datei. Eine Zusammenfassung deckt die ganze Datei ab: \
alle Abschnitte und bei Arbeitsblättern jede Aufgabe, nicht nur die erste.\n\
Brauchst du aktuelle Informationen (Nachrichten, Preise, Ergebnisse, Termine) oder bist du dir bei einer Tatsache nicht sicher, \
recherchiere mit web_research, statt zu raten, und nenne danach kurz die Quelle. Für Wetter nimmst du immer get_weather, nie web_research; \
kennt get_weather den Ort nicht, korrigiere die Schreibweise (Spracherkennung macht Fehler, z. B. Lübek statt Lübeck) und versuche es noch einmal. \
Steht dir kein passendes Werkzeug zur Verfügung oder findest du nichts, sag das ehrlich.\n\
Für YouTube-Videos nimmst du play_youtube. Suchst du etwas auf Netflix, Prime Video, Disney+, Spotify oder Twitch, nimm browser_search_site \
(öffnet direkt die Treffer), dann klickst du mit browser_click auf den Titel und auf Abspielen. Auf anderen Seiten öffnest du die Seite mit open_website \
und klickst dann Schritt für Schritt mit browser_click auf die sichtbaren Namen; nach jedem Klick lädt die Seite kurz. \
Mit browser_list und browser_read siehst du, was auf der Seite steht; browser_scroll und browser_key helfen beim Blättern und Navigieren, \
browser_tabs wechselt Tabs, browser_navigate öffnet eine Adresse im aktuellen Tab. \
Kennst du die genaue Adresse nicht, finde sie mit find_links und öffne dann direkt den passenden Treffer, statt dich durchzuklicken. \
Für Twitch: Prüfe mit twitch_live, welcher Kanal gerade live ist, und öffne dann https://www.twitch.tv/<kanal>. \
Offizielle E-Sport-Kanäle heißen meist wie das Spiel (valorant, riotgames, lolesports, esl_csgo, rocketleague) oder mit Region (valorant_emea, valorant_americas, valorant_pacific); \
sind mehrere live, nimm den passendsten (Hauptkanal oder große Veranstaltung) und nenne kurz den Streamtitel. \
Soll etwas als Word-Datei gespeichert werden, nimm create_word_document. \
Zeitversetzte Befehle („in fünf Minuten herunterfahren“) gibst du mit in_minutes an.\n\
{browser}{screens} Nennt der Nutzer einen Bildschirm („auf dem zweiten Bildschirm“), gib bei open_website, play_youtube, browser_search oder open_app screen an; \
schon offene Fenster ordnest du mit window_control an.\n\
Gibt es für einen Wunsch am PC kein eigenes Werkzeug, darfst du mit run_script ein kurzes PowerShell-Skript vorschlagen; \
der Nutzer sieht es vorher und entscheidet. Auf den Bildschirm schaust du mit look_at_screen nur, wenn er das ausdrücklich will.\n\
Herunterfahren, Neustart und Energie sparen führst du ohne Rückfrage aus, wenn der Nutzer es verlangt. \
Vor dem Schließen eines Programms und vor einem Skript zeigt die App dem Nutzer selbst eine Sicherheitsabfrage; frag dort nicht zusätzlich im Chat nach.\n\
Nach einer erledigten Aktion bestätigst du sie in einem kurzen Satz und nennst dabei Zahlen und Namen so, wie du sie verstanden hast.\n\
Namen und Titel aus der Spracherkennung sind oft falsch geschrieben (Filip statt Philipp, „der Mittel“ statt „The Middle“). \
Gib nicht beim ersten Fehlschlag auf: Findet ein Werkzeug etwas nicht, überlege, was gemeint sein könnte \
(andere Schreibweise, Aussprache, englischer Originaltitel), und versuche es mit der besten Vermutung erneut, \
bevor du dem Nutzer sagst, dass es nicht geht. Nennt dir ein Werkzeug Alternativen, wähle die passendste selbst aus.\n\
{user}Heute ist {weekday}, der {day}. {month} {year}, {hour:02}:{minute:02} Uhr.",
        screens = crate::screens::describe(&crate::screens::list()),
        browser = match crate::screens::default_browser_name() {
            Some(name) => format!("Der Standardbrowser des Nutzers ist {name}; „Browser“ oder „Internet“ meint ihn. "),
            None => String::new(),
        },
        user = if settings.user_name.is_empty() { String::new() } else { format!("Der Nutzer heißt {}.\n", settings.user_name) },
        name = settings.name,
        weekday = WEEKDAYS[now.weekday().num_days_from_monday() as usize],
        day = now.day(),
        month = MONTHS[now.month0() as usize],
        year = now.year(),
        hour = now.hour(),
        minute = now.minute(),
    )
}

/// Uebersetzt technische Fehler in eine verstaendliche deutsche Meldung. Gemini-Fehler sind
/// schon fuer den Nutzer formuliert.
pub fn friendly(err: &anyhow::Error) -> String {
    if let Some(api) = err.chain().find_map(|e| e.downcast_ref::<gemini::ApiError>()) {
        return api.message.clone();
    }
    if err.chain().any(|e| e.downcast_ref::<reqwest::Error>().is_some_and(|r| r.is_connect() || r.is_timeout())) {
        return tr!("Ich komme gerade nicht ins Internet.", "I can't reach the internet right now.").into();
    }
    format!("{err:#}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str) -> Value {
        json!({ "role": "model", "parts": [{ "functionCall": { "name": name, "args": { "a": 1 } }, "thoughtSignature": "sig" }] })
    }

    fn result(name: &str) -> Value {
        json!({ "role": "user", "parts": [{ "functionResponse": { "name": name, "response": { "result": "ok" } } }] })
    }

    #[test]
    fn trim_keeps_recent_turns_and_never_splits_tool_pairs() {
        let mut h = vec![
            user_text("a"), model_text("b"),
            user_text("c"), call("t"), result("t"), model_text("d"),
            user_text("e"), model_text("f"),
        ];
        trim_history(&mut h, 2);
        assert_eq!(h.len(), 6);
        assert_eq!(h[0], user_text("c"));
        assert_eq!(h[2], result("t"));
    }

    #[test]
    fn flatten_turns_tool_steps_into_text() {
        let mut h = vec![user_text("alt"), call("x"), result("x"), model_text("fertig"), user_text("neu"), call("open_app"), result("open_app")];
        flatten_current_turn(&mut h);
        assert_eq!(h[1], call("x"), "fruehere Anfragen bleiben unveraendert");
        assert_eq!(h[5]["role"], "model");
        assert!(h[5]["parts"][0]["text"].as_str().unwrap().contains("open_app"));
        assert!(h[6]["parts"][0]["text"].as_str().unwrap().contains("Ergebnis von open_app"));
        assert!(h[5]["parts"][0].get("thoughtSignature").is_none());
    }

    #[test]
    fn unfinished_turns_are_dropped() {
        let mut h = vec![user_text("a"), model_text("b"), user_text("c"), call("x"), result("x")];
        drop_unfinished_turn(&mut h);
        assert_eq!(h.len(), 2);
        let mut h = vec![user_text("a"), model_text("b")];
        drop_unfinished_turn(&mut h);
        assert_eq!(h.len(), 2);
        let mut h = vec![user_text("a"), model_text("b"), user_text("c")];
        drop_unfinished_turn(&mut h);
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn trim_is_noop_when_short() {
        let mut h = vec![user_text("a"), model_text("b")];
        trim_history(&mut h, 8);
        assert_eq!(h.len(), 2);
        let mut empty: Vec<Value> = vec![];
        trim_history(&mut empty, 8);
        assert!(empty.is_empty());
    }

    #[test]
    fn prompt_contains_name_and_date() {
        let prompt = system_prompt(&Settings { name: "Bixo".into(), ..Settings::default() });
        assert!(prompt.starts_with("Du bist Bixo,"));
        assert!(prompt.contains("Heute ist "));
        assert!(prompt.contains(" Uhr."));
    }

    #[test]
    fn friendly_keeps_gemini_messages() {
        let err = anyhow::Error::new(gemini::ApiError { kind: ErrorKind::Quota, message: "Kontingent weg.".into() }).context("Schritt 2");
        assert_eq!(friendly(&err), "Kontingent weg.");
        assert_eq!(friendly(&anyhow::anyhow!("etwas anderes")), "etwas anderes");
    }

    #[test]
    fn plans_differ_by_kind() {
        assert_eq!(plan_for("report").1, 10);
        assert_eq!(plan_for("easy").2, Thinking::Low);
        assert_eq!(plan_for("hard").2, Thinking::Medium);
    }
}
