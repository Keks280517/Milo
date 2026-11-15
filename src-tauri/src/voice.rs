//! Sprache: Mikrofon, Sprechpausen-Erkennung, lokale Spracherkennung und das Aktivierungswort.
//!
//! Grundsaetze:
//! - Das Mikrofon ist nur offen, wenn die Oberflaeche es anzeigt (jeder Zustand ausser "off").
//! - Audio verlaesst den Rechner nie; es wird auch nichts gespeichert.
//! - Der Erkenner wird erst beim Zuhoeren geladen und nach einer Minute ohne Nutzung entladen.
//!
//! Ein eigener Thread besitzt Mikrofon und Modelle. Befehle und Audio kommen ueber einen Kanal.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use sherpa_onnx::{
    KeywordSpotter, KeywordSpotterConfig, LinearResampler, OfflineRecognizer, OfflineRecognizerConfig, OnlineStream,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::SettingsStore;

pub const EVENT: &str = "milo://voice";
/// Abtastrate, mit der Erkenner und Aktivierungswort arbeiten.
const RATE: usize = 16_000;
/// Laenge eines Auswertungsfensters (30 ms).
const FRAME: usize = RATE * 30 / 1000;
/// So viel Ton vor dem Sprechbeginn wird mitgenommen, damit Satzanfaenge nicht fehlen.
const PREROLL: usize = RATE * 6 / 10;
/// Stille nach dem Sprechen, die den Satz beendet (Einstellung "normal"). Mit 1,1 s wurden laengere
/// Saetze mit kurzem Nachdenken mittendrin abgeschnitten.
const END_SILENCE: Duration = Duration::from_millis(1800);

/// Pause bis zum Ende je nach Einstellung: 1 = kurz, 2 = normal, 3 = lang.
pub fn end_silence(level: u8) -> Duration {
    match level {
        1 => Duration::from_millis(1200),
        3 => Duration::from_millis(2600),
        _ => END_SILENCE,
    }
}
/// Sagt der Nutzer so lange gar nichts, wird abgebrochen.
const START_TIMEOUT: Duration = Duration::from_secs(7);
const MAX_UTTERANCE: Duration = Duration::from_secs(30);
/// Ohne Nutzung wird der Erkenner nach dieser Zeit entladen.
const UNLOAD_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VoiceEvent {
    /// off = Mikrofon zu, wake = wartet auf das Aktivierungswort, listening = nimmt auf,
    /// transcribing = wertet aus.
    State { state: &'static str },
    /// Lautstaerke 0..1 waehrend der Aufnahme.
    Level { level: f32 },
    /// Erkannter Satz.
    Heard { text: String },
    /// Hinweis oder Fehler als lesbarer Text.
    Notice { message: String },
}

enum Msg {
    Audio(Vec<f32>),
    /// Zuhoeren beginnen (Hotkey, Mikrofon-Knopf); laeuft schon eine Aufnahme, wird sie beendet.
    Toggle,
    Cancel,
    /// Einstellungen haben sich geaendert (Aktivierungswort an/aus, Modellwahl).
    Refresh,
    /// Alles freigeben: Mikrofon zu, Modelle entladen (Spielemodus). `false` nimmt den Betrieb wieder auf.
    Suspend(bool),
    Recognizer(Result<Loaded, String>),
    Transcript(Loaded, Result<String, String>),
    MicError(String),
    /// Nur zum Pruefen in Entwicklungsversionen: eine Aufnahme durchlaufen lassen, als kaeme sie vom Mikrofon.
    Inject(Vec<f32>),
}

struct Loaded {
    kind: String,
    recognizer: OfflineRecognizer,
}

/// Zugang zum Sprach-Thread fuer den Rest der App.
pub struct Voice {
    tx: Mutex<Option<Sender<Msg>>>,
}

impl Default for Voice {
    fn default() -> Self {
        Self { tx: Mutex::new(None) }
    }
}

impl Voice {
    fn send(&self, msg: Msg) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(msg);
        }
    }

    pub fn toggle(&self) {
        self.send(Msg::Toggle);
    }

    pub fn refresh(&self) {
        self.send(Msg::Refresh);
    }

    pub fn suspend(&self, on: bool) {
        self.send(Msg::Suspend(on));
    }
}

#[tauri::command]
pub fn voice_toggle(voice: tauri::State<'_, Voice>) {
    voice.toggle();
}

#[tauri::command]
pub fn voice_cancel(voice: tauri::State<'_, Voice>) {
    voice.send(Msg::Cancel);
}

/// Spielt eine WAV-Datei durch denselben Weg wie Mikrofonton (nur Entwicklungsversionen).
#[tauri::command]
pub fn voice_inject(voice: tauri::State<'_, Voice>, path: String) -> Result<f32, String> {
    if !cfg!(debug_assertions) {
        return Err(tr!("Nur in Entwicklungsversionen.", "Only in development builds.").into());
    }
    let wave = sherpa_onnx::Wave::read(&path).ok_or(tr!("Die Datei lässt sich nicht lesen.", "The file can't be read."))?;
    let resampler = LinearResampler::create(wave.sample_rate(), RATE as i32).ok_or(tr!("Abtastrate nicht unterstützt.", "Sample rate not supported."))?;
    let mut samples = resampler.resample(wave.samples(), true);
    // Stille anhaengen, damit die Sprechpause erkannt wird.
    samples.extend(std::iter::repeat_n(0.0, RATE * 2));
    let seconds = samples.len() as f32 / RATE as f32;
    voice.send(Msg::Inject(samples));
    Ok(seconds)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    /// Welche Erkenner-Modelle vorhanden sind.
    pub parakeet: bool,
    pub whisper: bool,
    /// Modell fuer das Aktivierungswort vorhanden.
    pub wake: bool,
    pub folder: String,
}

#[tauri::command]
pub fn voice_info(app: AppHandle) -> VoiceInfo {
    let dirs = model_dirs(&app);
    VoiceInfo {
        parakeet: find_model(&dirs, PARAKEET).is_some(),
        whisper: find_model(&dirs, WHISPER).is_some(),
        wake: find_model(&dirs, KWS).is_some(),
        folder: dirs[0].display().to_string(),
    }
}

// ---------- Modelle ----------

const PARAKEET: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const WHISPER: &str = "sherpa-onnx-whisper-turbo";
const KWS: &str = "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01";

/// Ordner, in dem die Sprachmodelle liegen (wird beim Loeschen von Milo mit entfernt).
fn model_dirs(app: &AppHandle) -> Vec<PathBuf> {
    vec![app.path().app_local_data_dir().unwrap_or_default().join("Sprachmodelle")]
}

fn find_model(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_dir())
}

/// Erste Datei im Ordner, deren Name alle Teile enthaelt.
fn file_with(dir: &Path, parts: &[&str]) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names.into_iter().find(|n| parts.iter().all(|p| n.contains(p))).map(|n| dir.join(n).to_string_lossy().into_owned())
}

fn load_recognizer(dirs: &[PathBuf], kind: &str) -> Result<Loaded, String> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = 4;
    if kind == "whisper" {
        let dir = find_model(dirs, WHISPER).ok_or(tr!("Das Whisper-Modell ist nicht vorhanden.", "The Whisper model isn't installed."))?;
        config.model_config.whisper.encoder = file_with(&dir, &["encoder", ".onnx"]);
        config.model_config.whisper.decoder = file_with(&dir, &["decoder", ".onnx"]);
        config.model_config.whisper.language = Some(tr!("de", "en").into());
        config.model_config.whisper.task = Some("transcribe".into());
        config.model_config.tokens = file_with(&dir, &["tokens.txt"]);
    } else {
        let dir = find_model(dirs, PARAKEET).ok_or(tr!("Das Spracherkennungs-Modell (Parakeet) ist nicht vorhanden.", "The speech recognition model (Parakeet) isn't installed."))?;
        config.model_config.transducer.encoder = file_with(&dir, &["encoder", ".onnx"]);
        config.model_config.transducer.decoder = file_with(&dir, &["decoder", ".onnx"]);
        config.model_config.transducer.joiner = file_with(&dir, &["joiner", ".onnx"]);
        config.model_config.tokens = file_with(&dir, &["tokens.txt"]);
        config.model_config.model_type = Some("nemo_transducer".into());
    }
    let recognizer = OfflineRecognizer::create(&config).ok_or(tr!("Das Spracherkennungs-Modell ließ sich nicht laden.", "The speech recognition model couldn't be loaded."))?;
    Ok(Loaded { kind: kind.to_string(), recognizer })
}

fn transcribe(recognizer: &OfflineRecognizer, samples: &[f32]) -> Result<String, String> {
    let stream = recognizer.create_stream();
    stream.accept_waveform(RATE as i32, samples);
    recognizer.decode(&stream);
    let text = stream.get_result().map(|r| r.text).unwrap_or_default();
    Ok(clean_transcript(&text))
}

/// Raeumt den erkannten Text auf: Leerraum und ein vorangestelltes Aktivierungswort entfernen.
pub fn clean_transcript(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = text.to_lowercase();
    for wake in [
        "hey milo", "hey, milo", "hey mylo", "hey, mylo", "hey meelo", "hey mailo", "hey milow", "hey mylow", "hi milo", "hi, milo", "he milo",
        "hey milo", "hey, milo",
    ] {
        if lower.starts_with(wake) {
            // Gleich viele Zeichen wie Bytes: alle Varianten sind reines ASCII.
            let rest = text[wake.len()..].trim_start_matches(|c: char| c == ',' || c == '.' || c == '!' || c == ':' || c.is_whitespace());
            let mut chars = rest.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            };
        }
    }
    text
}

/// Nach dem Aktivierungswort bleibt manchmal dessen Ende am Satzanfang haengen ("Bo, wie wird ...").
pub fn strip_wake_rest(text: &str) -> String {
    let Some((first, rest)) = text.split_once(' ') else {
        return text.to_string();
    };
    let word = first.trim_end_matches([',', '.', '!']).to_lowercase();
    if first.len() != word.len() && ["oh", "o", "lo", "low", "ilo", "milo", "mylo", "meelo", "hey", "bo", "milo"].contains(&word.as_str()) {
        let mut chars = rest.trim_start().chars();
        return match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        };
    }
    text.to_string()
}

// ---------- Aktivierungswort ----------

/// Schreibweisen von "Hey Milo" in den Lauteinheiten des (englischen) Schluesselwort-Modells:
/// englisch "Mai-lo" (MY LO, MI LO) und deutsch "Mii-lo" (ME LO). Im Test (bench/stt/milo-test.mjs,
/// Windows-Stimmen deutsch und englisch, dazu die 42 alten Aufnahmen des Nutzers als Gegenprobe):
/// bei normaler Empfindlichkeit 9 von 9 erkannt, 1 Fehlausloesung von 47 ("Hey Mila").
const WAKE_SPELLINGS: [&str; 3] = ["▁HE Y ▁MI LO", "▁HE Y ▁MY LO", "▁HE Y ▁ME LO"];

/// Schluesselwortliste mit Gewicht und Schwelle je nach eingestellter Empfindlichkeit (1 bis 3).
pub fn wake_keywords(level: u8) -> String {
    let (score, threshold) = match level {
        1 => (1.5, 0.35),
        3 => (2.5, 0.1),
        _ => (2.0, 0.2),
    };
    WAKE_SPELLINGS.iter().map(|k| format!("{k} :{score} #{threshold} @hey_milo
")).collect()
}

/// Das Schluesselwort-Modell ist empfindlich dafuer, wie viel Stille vor dem Wort liegt. Deshalb
/// startet die Suche erst beim Sprechbeginn, und zwar mehrfach mit leicht versetztem Vorlauf;
/// es genuegt, wenn eine der Suchen anschlaegt. Im Test mit durchlaufendem Ton: 24 von 24 erkannt,
/// 0 von 504 Fehlausloesungen (eine einzelne Suche kam je nach Vorlauf nur auf 12 bis 23 von 24).
const WAKE_PREROLLS_MS: [usize; 4] = [60, 180, 300, 420];
/// Ab diesem Pegel gilt ein Fenster als Sprechbeginn.
const WAKE_GATE: f32 = 0.008;

struct Wake {
    spotter: KeywordSpotter,
    /// Laufende Suchen; leer, solange niemand spricht (dann kostet das Warten fast nichts).
    streams: Vec<OnlineStream>,
    /// Die letzten Fenster vor dem Sprechbeginn.
    history: std::collections::VecDeque<Vec<f32>>,
    quiet_frames: u32,
}

fn load_wake(dirs: &[PathBuf], level: u8) -> Result<Wake, String> {
    let dir = find_model(dirs, KWS).ok_or(tr!("Das Modell für das Aktivierungswort ist nicht vorhanden.", "The wake word model isn't installed."))?;
    let mut config = KeywordSpotterConfig::default();
    config.model_config.transducer.encoder = file_with(&dir, &["encoder", "chunk-16-left-64", "int8"]).or_else(|| file_with(&dir, &["encoder", ".onnx"]));
    config.model_config.transducer.decoder = file_with(&dir, &["decoder", "chunk-16-left-64", ".onnx"]).or_else(|| file_with(&dir, &["decoder", ".onnx"]));
    config.model_config.transducer.joiner = file_with(&dir, &["joiner", "chunk-16-left-64", "int8"]).or_else(|| file_with(&dir, &["joiner", ".onnx"]));
    config.model_config.tokens = file_with(&dir, &["tokens.txt"]);
    config.model_config.num_threads = 1;
    config.keywords_buf = Some(wake_keywords(level));
    let spotter = KeywordSpotter::create(&config).ok_or(tr!("Das Modell für das Aktivierungswort ließ sich nicht laden.", "The wake word model couldn't be loaded."))?;
    Ok(Wake { spotter, streams: Vec::new(), history: Default::default(), quiet_frames: 0 })
}

impl Wake {
    /// Vergisst alles Gehoerte (nach einer Aufnahme oder einer Pause).
    fn reset(&mut self) {
        self.streams.clear();
        self.history.clear();
        self.quiet_frames = 0;
    }

    /// Fuettert ein Fenster Ton nach und meldet, ob das Aktivierungswort gefallen ist.
    fn heard(&mut self, frame: &[f32]) -> bool {
        if rms(frame) > WAKE_GATE {
            if self.streams.is_empty() {
                for ms in WAKE_PREROLLS_MS {
                    let stream = self.spotter.create_stream();
                    let frames = ms / 30;
                    for old in self.history.iter().skip(self.history.len().saturating_sub(frames)) {
                        stream.accept_waveform(RATE as i32, old);
                    }
                    self.streams.push(stream);
                }
            }
            self.quiet_frames = 0;
        } else {
            self.quiet_frames += 1;
        }
        let mut hit = false;
        for stream in &self.streams {
            stream.accept_waveform(RATE as i32, frame);
            while self.spotter.is_ready(stream) {
                self.spotter.decode(stream);
                if self.spotter.get_result(stream).is_some_and(|r| !r.keyword.is_empty()) {
                    hit = true;
                    self.spotter.reset(stream);
                }
            }
        }
        // Eine Dreiviertelsekunde Stille beendet die Suche bis zum naechsten Sprechbeginn.
        if hit || self.quiet_frames >= 25 {
            self.streams.clear();
        }
        self.history.push_back(frame.to_vec());
        while self.history.len() > WAKE_PREROLLS_MS[3] / 30 + 1 {
            self.history.pop_front();
        }
        hit
    }
}

// ---------- Sprechpausen ----------

/// Erkennt Sprechen und Stille an der Lautstaerke, mit mitlaufendem Grundrauschen.
pub struct Endpointer {
    /// Geschaetztes Grundrauschen (Effektivwert).
    floor: f32,
    /// Hintereinander laute Fenster; ein einzelnes Knacken zaehlt nicht als Sprechen.
    loud_run: u32,
    pub spoke: bool,
    /// Fenster seit dem letzten lauten Fenster.
    quiet_frames: u32,
}

impl Default for Endpointer {
    fn default() -> Self {
        Self { floor: 0.004, loud_run: 0, spoke: false, quiet_frames: 0 }
    }
}

pub fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

impl Endpointer {
    /// Wertet ein Fenster aus; liefert, ob darin gesprochen wurde.
    pub fn feed(&mut self, level: f32) -> bool {
        let threshold = (self.floor * 3.2).max(0.011);
        let loud = level > threshold;
        if loud {
            self.loud_run += 1;
            self.quiet_frames = 0;
            if self.loud_run >= 4 {
                self.spoke = true;
            }
        } else {
            self.loud_run = 0;
            self.quiet_frames += 1;
            // Vor dem Sprechen folgt das Grundrauschen leisen Fenstern zuegig (Luefter, Musik im Hintergrund).
            // Waehrend des Sprechens darf es nur sinken oder sehr langsam steigen: leise Silben und
            // Atempausen sollen die Schwelle nicht hochziehen, sonst gilt der Rest des Satzes als Stille.
            let rate = if !self.spoke || level < self.floor { 0.05 } else { 0.003 };
            self.floor += (level - self.floor) * rate;
        }
        // Dauerhaft lauter Hintergrund: die Schwelle zieht ganz langsam nach, damit ein Ende moeglich bleibt.
        if loud && self.spoke {
            self.floor += (level - self.floor) * 0.0003;
        }
        loud
    }

    /// Dauer der Stille seit dem letzten Sprechen.
    pub fn silence(&self) -> Duration {
        Duration::from_millis(self.quiet_frames as u64 * 30)
    }
}

// ---------- Mikrofon ----------

/// Oeffnet das Standard-Mikrofon und liefert Mono-Ton in seiner Abtastrate ueber den Kanal.
fn open_mic(tx: Sender<Msg>) -> Result<(cpal::Stream, u32), String> {
    let device = cpal::default_host().default_input_device().ok_or(tr!("Ich finde kein Mikrofon.", "I can't find a microphone."))?;
    let supported = device.default_input_config().map_err(|e| tr!(format!("Das Mikrofon lässt sich nicht öffnen ({e})."), format!("The microphone can't be opened ({e}).")))?;
    let rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    log::info!("Mikrofon: {rate} Hz, {channels} Kanal/Kanäle, {format:?}");
    let err_tx = tx.clone();
    let on_error = move |err: cpal::Error| {
        // Nur ein wirklich verlorenes Geraet beendet die Aufnahme; kurze Aussetzer im Tonpuffer nicht.
        if matches!(err.kind(), cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated | cpal::ErrorKind::PermissionDenied) {
            let _ = err_tx.send(Msg::MicError(tr!(format!("Das Mikrofon ist ausgefallen ({err})."), format!("The microphone stopped working ({err})."))));
        } else {
            log::debug!("Mikrofon: {err}");
        }
    };
    fn mono<T: Copy>(data: &[T], channels: usize, to_f32: impl Fn(T) -> f32) -> Vec<f32> {
        data.chunks(channels.max(1)).map(|frame| frame.iter().map(|&s| to_f32(s)).sum::<f32>() / frame.len() as f32).collect()
    }
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(Msg::Audio(mono(data, channels, |s| s)));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(Msg::Audio(mono(data, channels, |s| s as f32 / 32768.0)));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream(
            config,
            move |data: &[i32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(Msg::Audio(mono(data, channels, |s| s as f32 / 2_147_483_648.0)));
            },
            on_error,
            None,
        ),
        other => return Err(tr!(format!("Das Mikrofon liefert ein Tonformat, das ich nicht kenne ({other:?})."), format!("The microphone delivers an audio format I don't know ({other:?})."))),
    }
    .map_err(|e| tr!(format!("Das Mikrofon lässt sich nicht öffnen ({e}). Ist der Zugriff in den Windows-Datenschutzeinstellungen erlaubt?"), format!("The microphone can't be opened ({e}). Is access allowed in the Windows privacy settings?")))?;
    stream.play().map_err(|e| tr!(format!("Das Mikrofon startet nicht ({e})."), format!("The microphone won't start ({e}).")))?;
    Ok((stream, rate))
}

// ---------- Der Sprach-Thread ----------

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    /// Mikrofon zu.
    Off,
    /// Mikrofon offen, nur das Aktivierungswort wird gesucht.
    Wake,
    Listening,
    Transcribing,
}

struct Worker {
    app: AppHandle,
    tx: Sender<Msg>,
    dirs: Vec<PathBuf>,
    mode: Mode,
    suspended: bool,
    mic: Option<(cpal::Stream, LinearResampler)>,
    wake: Option<Wake>,
    recognizer: Option<Loaded>,
    loading: bool,
    last_use: Instant,
    /// Ton in 16 kHz, der noch nicht in ganze Fenster zerlegt ist.
    pending: Vec<f32>,
    /// Die letzten Augenblicke vor dem Sprechbeginn.
    preroll: Vec<f32>,
    utterance: Vec<f32>,
    endpointer: Endpointer,
    started: Instant,
    last_level: Instant,
    /// Lautester Pegel der laufenden Aufnahme (nur fuers Protokoll).
    peak: f32,
    /// Die laufende Aufnahme wurde durch das Aktivierungswort gestartet.
    via_wake: bool,
}

impl Worker {
    fn emit(&self, event: VoiceEvent) {
        let _ = self.app.emit(EVENT, event);
    }

    fn notice(&self, message: impl Into<String>) {
        self.emit(VoiceEvent::Notice { message: message.into() });
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            let state = match mode {
                Mode::Off => "off",
                Mode::Wake => "wake",
                Mode::Listening => "listening",
                Mode::Transcribing => "transcribing",
            };
            self.emit(VoiceEvent::State { state });
        }
    }

    fn wake_wanted(&self) -> bool {
        !self.suspended && self.app.state::<SettingsStore>().get().wake_word
    }

    fn ensure_mic(&mut self) -> bool {
        if self.mic.is_some() {
            return true;
        }
        match open_mic(self.tx.clone()).and_then(|(stream, rate)| {
            LinearResampler::create(rate as i32, RATE as i32).map(|r| (stream, r)).ok_or_else(|| tr!("Die Abtastrate des Mikrofons wird nicht unterstützt.", "The microphone's sample rate isn't supported.").to_string())
        }) {
            Ok(mic) => {
                self.mic = Some(mic);
                self.pending.clear();
                true
            }
            Err(message) => {
                self.notice(message);
                false
            }
        }
    }

    /// Geht in den Ruhezustand: mit Aktivierungswort bleibt das Mikrofon offen, sonst schliesst es.
    fn rest(&mut self) {
        self.utterance = Vec::new();
        self.via_wake = false;
        if self.wake_wanted() && self.ensure_wake() && self.ensure_mic() {
            // Frisch anfangen: ein halb gehoerter Rest von vorhin darf das naechste "Hey Milo" nicht stoeren.
            if let Some(wake) = self.wake.as_mut() {
                wake.reset();
            }
            self.set_mode(Mode::Wake);
        } else {
            self.mic = None;
            self.wake = None;
            self.preroll.clear();
            self.set_mode(Mode::Off);
        }
    }

    fn ensure_wake(&mut self) -> bool {
        if self.wake.is_some() {
            return true;
        }
        let level = self.app.state::<SettingsStore>().get().wake_level;
        match load_wake(&self.dirs, level) {
            Ok(wake) => {
                self.wake = Some(wake);
                true
            }
            Err(message) => {
                self.notice(message);
                false
            }
        }
    }

    fn start_listening(&mut self) {
        if self.suspended || !self.ensure_mic() {
            return;
        }
        // Mit Vorlauf starten, falls das Mikrofon schon offen war.
        self.utterance = std::mem::take(&mut self.preroll);
        self.endpointer = Endpointer::default();
        self.peak = 0.0;
        self.started = Instant::now();
        self.last_use = Instant::now();
        self.set_mode(Mode::Listening);
        self.want_recognizer();
    }

    /// Laedt den Erkenner im Hintergrund, waehrend der Nutzer schon spricht.
    fn want_recognizer(&mut self) {
        let kind = self.app.state::<SettingsStore>().get().stt_model;
        if self.recognizer.as_ref().is_some_and(|r| r.kind != kind) {
            self.recognizer = None;
        }
        if self.recognizer.is_some() || self.loading {
            return;
        }
        self.loading = true;
        let (tx, dirs) = (self.tx.clone(), self.dirs.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Recognizer(load_recognizer(&dirs, &kind)));
        });
    }

    fn finish(&mut self) {
        log::info!(
            "Aufnahme beendet: {:.1} s Ton, gesprochen: {}, lautester Pegel {:.4}",
            self.utterance.len() as f32 / RATE as f32,
            self.endpointer.spoke,
            self.peak
        );
        if !self.endpointer.spoke || self.utterance.len() < RATE / 3 {
            self.notice(tr!("Ich habe nichts gehört.", "I didn't hear anything."));
            self.rest();
            return;
        }
        self.set_mode(Mode::Transcribing);
        self.try_transcribe();
    }

    fn try_transcribe(&mut self) {
        if self.mode != Mode::Transcribing {
            return;
        }
        let Some(loaded) = self.recognizer.take() else {
            // Der Erkenner laedt noch; es geht weiter, sobald er da ist.
            self.want_recognizer();
            return;
        };
        let samples = std::mem::take(&mut self.utterance);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = transcribe(&loaded.recognizer, &samples);
            let _ = tx.send(Msg::Transcript(loaded, result));
        });
    }

    fn on_audio(&mut self, raw: Vec<f32>) {
        let Some((_, resampler)) = self.mic.as_ref() else {
            return;
        };
        self.pending.extend(resampler.resample(&raw, false));
        while self.pending.len() >= FRAME {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            self.on_frame(&frame);
        }
    }

    fn on_frame(&mut self, frame: &[f32]) {
        match self.mode {
            Mode::Wake => {
                self.preroll.extend_from_slice(frame);
                if self.preroll.len() > PREROLL * 4 {
                    let cut = self.preroll.len() - PREROLL * 4;
                    self.preroll.drain(..cut);
                }
                if self.wake.as_mut().is_some_and(|w| w.heard(frame)) {
                    log::info!("Aktivierungswort erkannt");
                    self.via_wake = true;
                    // Das Aktivierungswort selbst bleibt im Vorlauf; es wird spaeter aus dem Text entfernt.
                    self.start_listening();
                }
            }
            Mode::Listening => {
                let level = rms(frame);
                self.peak = self.peak.max(level);
                let loud = self.endpointer.feed(level);
                if self.endpointer.spoke || loud {
                    self.utterance.extend_from_slice(frame);
                } else {
                    // Vor dem ersten Wort nur einen kurzen Vorlauf behalten.
                    self.utterance.extend_from_slice(frame);
                    if self.utterance.len() > PREROLL * 4 {
                        let cut = self.utterance.len() - PREROLL;
                        self.utterance.drain(..cut);
                    }
                }
                if self.last_level.elapsed() > Duration::from_millis(60) {
                    self.last_level = Instant::now();
                    self.emit(VoiceEvent::Level { level: (level * 9.0).min(1.0) });
                }
                let waited = self.started.elapsed();
                if self.endpointer.spoke && self.endpointer.silence() >= end_silence(self.app.state::<SettingsStore>().get().speech_pause) || waited >= MAX_UTTERANCE {
                    self.finish();
                } else if !self.endpointer.spoke && waited >= START_TIMEOUT {
                    self.finish();
                }
            }
            Mode::Off | Mode::Transcribing => {}
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Audio(raw) => self.on_audio(raw),
            Msg::Toggle => match self.mode {
                Mode::Listening => self.finish(),
                Mode::Transcribing => {}
                Mode::Off | Mode::Wake => {
                    // Ohne Aktivierungswort beginnt die Aufnahme ohne alten Vorlauf.
                    if self.mode == Mode::Off {
                        self.preroll.clear();
                    }
                    self.start_listening();
                }
            },
            Msg::Cancel => {
                if matches!(self.mode, Mode::Listening | Mode::Transcribing) {
                    self.rest();
                }
            }
            Msg::Refresh => {
                self.wake = None;
                if matches!(self.mode, Mode::Off | Mode::Wake) {
                    self.rest();
                }
            }
            Msg::Suspend(on) => {
                self.suspended = on;
                if on {
                    self.recognizer = None;
                }
                self.rest();
            }
            Msg::Recognizer(result) => {
                self.loading = false;
                match result {
                    Ok(loaded) => {
                        self.recognizer = Some(loaded);
                        self.last_use = Instant::now();
                        self.try_transcribe();
                    }
                    Err(message) => {
                        self.notice(message);
                        if matches!(self.mode, Mode::Listening | Mode::Transcribing) {
                            self.rest();
                        }
                    }
                }
            }
            Msg::Transcript(loaded, result) => {
                self.recognizer = Some(loaded);
                self.last_use = Instant::now();
                if self.mode == Mode::Transcribing {
                    match result {
                        Ok(text) if !text.is_empty() => {
                            let text = if self.via_wake { strip_wake_rest(&text) } else { text };
                            self.emit(VoiceEvent::Heard { text });
                        }
                        Ok(_) => self.notice(tr!("Ich habe nichts verstanden.", "I didn't catch that.")),
                        Err(message) => self.notice(message),
                    }
                    self.rest();
                }
            }
            Msg::Inject(samples) => {
                // Wartet Milo gerade auf das Aktivierungswort, muss die Aufnahme es enthalten;
                // sonst gilt sie direkt als Aufnahme nach dem Hotkey.
                let waiting = self.mode == Mode::Wake;
                if !waiting {
                    self.utterance.clear();
                    self.endpointer = Endpointer::default();
                    self.started = Instant::now();
                    self.set_mode(Mode::Listening);
                    self.want_recognizer();
                }
                for frame in samples.chunks(FRAME) {
                    if !matches!(self.mode, Mode::Listening | Mode::Wake) {
                        break;
                    }
                    self.on_frame(frame);
                }
                if self.mode == Mode::Listening {
                    self.finish();
                }
            }
            Msg::MicError(message) => {
                self.mic = None;
                self.notice(message);
                self.set_mode(Mode::Off);
            }
        }
    }

    fn tick(&mut self) {
        // Erkenner nach einer Minute ohne Nutzung wieder freigeben.
        if self.recognizer.is_some() && matches!(self.mode, Mode::Off | Mode::Wake) && self.last_use.elapsed() > UNLOAD_AFTER {
            self.recognizer = None;
            log::info!("Spracherkenner entladen");
        }
    }
}

/// Startet den Sprach-Thread. Das Mikrofon bleibt zu, bis es gebraucht wird.
pub fn spawn(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    *app.state::<Voice>().tx.lock().unwrap() = Some(tx.clone());
    std::thread::spawn(move || {
        let dirs = model_dirs(&app);
        let mut worker = Worker {
            app,
            tx,
            dirs,
            mode: Mode::Off,
            suspended: false,
            mic: None,
            wake: None,
            recognizer: None,
            loading: false,
            last_use: Instant::now(),
            pending: Vec::new(),
            preroll: Vec::new(),
            utterance: Vec::new(),
            endpointer: Endpointer::default(),
            started: Instant::now(),
            last_level: Instant::now(),
            peak: 0.0,
            via_wake: false,
        };
        worker.rest();
        loop {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(msg) => worker.handle(msg),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            worker.tick();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_word_is_removed_from_the_transcript() {
        assert_eq!(clean_transcript("Hey Milo, wie wird das Wetter?"), "Wie wird das Wetter?");
        assert_eq!(clean_transcript("Hey Mylo, what's the weather?"), "What's the weather?");
        assert_eq!(clean_transcript("hey meelo  stell einen Timer"), "Stell einen Timer");
        assert_eq!(clean_transcript("Hey Milo."), "");
        assert_eq!(clean_transcript("  Öffne   Spotify. "), "Öffne Spotify.");
        assert_eq!(clean_transcript("Sag Hey Milo zu ihm"), "Sag Hey Milo zu ihm");
    }

    fn feed(e: &mut Endpointer, level: f32, frames: u32) {
        for _ in 0..frames {
            e.feed(level);
        }
    }

    #[test]
    fn speech_then_silence_ends_the_utterance() {
        let mut e = Endpointer::default();
        feed(&mut e, 0.003, 20);
        assert!(!e.spoke);
        feed(&mut e, 0.08, 30);
        assert!(e.spoke);
        assert!(e.silence() < END_SILENCE);
        feed(&mut e, 0.003, 65);
        assert!(e.silence() >= END_SILENCE);
    }

    #[test]
    fn soft_syllables_late_in_a_long_sentence_still_count() {
        let mut e = Endpointer::default();
        feed(&mut e, 0.003, 20);
        // Acht Sekunden normales Sprechen mit kurzen Atempausen ...
        for _ in 0..16 {
            feed(&mut e, 0.08, 12);
            feed(&mut e, 0.006, 3);
        }
        // ... dann ein leiser Satzteil: der muss noch als Sprechen gelten.
        assert!(e.feed(0.03));
        // Eine Denkpause von einer Sekunde beendet den Satz noch nicht.
        feed(&mut e, 0.004, 33);
        assert!(e.silence() < end_silence(2));
        assert!(end_silence(1) < end_silence(2) && end_silence(2) < end_silence(3));
    }

    #[test]
    fn a_single_click_is_not_speech() {
        let mut e = Endpointer::default();
        feed(&mut e, 0.003, 10);
        feed(&mut e, 0.5, 2);
        feed(&mut e, 0.003, 10);
        assert!(!e.spoke);
    }

    #[test]
    fn steady_background_noise_raises_the_floor() {
        let mut e = Endpointer::default();
        // Gleichmaessige Musik im Hintergrund: nach kurzer Zeit zaehlt sie nicht mehr als Sprechen.
        feed(&mut e, 0.01, 200);
        assert!(!e.spoke);
        feed(&mut e, 0.09, 20);
        assert!(e.spoke);
        feed(&mut e, 0.01, 65);
        assert!(e.silence() >= END_SILENCE);
    }

    #[test]
    fn leftovers_of_the_wake_word_are_dropped() {
        assert_eq!(strip_wake_rest("Oh, wie wird das Wetter?"), "Wie wird das Wetter?");
        assert_eq!(strip_wake_rest("Lo. stell einen Timer"), "Stell einen Timer");
        assert_eq!(strip_wake_rest("Oh je das ist viel"), "Oh je das ist viel");
        assert_eq!(strip_wake_rest("Öffne Spotify."), "Öffne Spotify.");
    }

    #[test]
    fn wake_keywords_follow_the_sensitivity() {
        let strict = wake_keywords(1);
        assert_eq!(strict.lines().count(), 3);
        assert!(strict.lines().all(|l| l.starts_with("▁HE Y ") && l.ends_with("#0.35 @hey_milo")));
        assert!(wake_keywords(3).contains(":2.5 #0.1"));
    }

    #[test]
    fn rms_of_silence_and_full_scale() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0.0; 10]), 0.0);
        assert!((rms(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
    }
}
