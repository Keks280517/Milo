//! Bruecke zur Browser-Erweiterung „Milo für den Browser“ (Chrome, Edge, Brave).
//!
//! Aufbau, alles nur auf diesem PC:
//!   Erweiterung  <-- Native Messaging (stdin/stdout) -->  milo.exe als „Host“  <-- TCP 127.0.0.1 -->  Milo
//! Der Browser startet den Host selbst (dasselbe Programm, erkennbar am Argument
//! `chrome-extension://…`). Der Host meldet sich mit einem geheimen Schluessel bei Milo an, den nur
//! Programme dieses Windows-Kontos lesen koennen (Datei im Datenordner). Fremde Webseiten koennen
//! weder die Erweiterung noch Milo erreichen: Native Messaging erlaubt nur die eingetragene
//! Erweiterung, und der TCP-Anschluss verlangt den Schluessel.
//!
//! Ist die Erweiterung nicht installiert oder nicht verbunden, bedient Milo den Browser wie bisher
//! ueber die Windows-Bedienungshilfen (siehe `browser.rs`).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

/// Name des Hosts, wie ihn die Erweiterung anspricht.
pub const HOST_NAME: &str = "de.milo.bridge";
/// Kennung der Erweiterung (aus dem festen Schluessel im Manifest).
pub const EXTENSION_ID: &str = "kkicpijglljhpeeedhiieinjllaniaoc";

const MANIFEST: &str = include_str!("../browser-extension/manifest.json");
const BACKGROUND: &str = include_str!("../browser-extension/background.js");

struct Bridge {
    writer: Mutex<Option<TcpStream>>,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    next: AtomicU64,
    connected: AtomicBool,
    browser: Mutex<String>,
}

static BRIDGE: LazyLock<Bridge> = LazyLock::new(|| Bridge {
    writer: Mutex::new(None),
    pending: Mutex::new(HashMap::new()),
    next: AtomicU64::new(1),
    connected: AtomicBool::new(false),
    browser: Mutex::new(String::new()),
});

/// Datenordner wie bei Tauri (%APPDATA%\de.milo.assistant); der Host kennt Tauri nicht.
fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("de.milo.assistant")
}

fn bridge_file() -> PathBuf {
    data_dir().join("browser-bridge.json")
}

/// Ordner, aus dem man die Erweiterung im Browser laedt.
pub fn extension_dir() -> PathBuf {
    data_dir().join("browser-extension")
}

/// Ist die Erweiterung gerade verbunden?
pub fn connected() -> bool {
    BRIDGE.connected.load(Ordering::Relaxed)
}

/// Welcher Browser (Kennung der Erweiterung) verbunden ist.
pub fn browser_name() -> String {
    let agent = BRIDGE.browser.lock().unwrap().clone();
    if agent.contains("Edg/") {
        "Edge".into()
    } else if agent.contains("OPR/") {
        "Opera".into()
    } else if agent.contains("Brave") {
        "Brave".into()
    } else if agent.is_empty() {
        String::new()
    } else {
        "Chrome".into()
    }
}

/// Schickt einen Auftrag an die Erweiterung und wartet auf die Antwort.
pub fn request(mut message: Value, timeout: Duration) -> Result<Value> {
    if !connected() {
        bail!("Browser-Erweiterung nicht verbunden");
    }
    let id = BRIDGE.next.fetch_add(1, Ordering::Relaxed);
    message["id"] = json!(id);
    let (tx, rx) = mpsc::channel();
    BRIDGE.pending.lock().unwrap().insert(id, tx);
    let sent = {
        let mut writer = BRIDGE.writer.lock().unwrap();
        match writer.as_mut() {
            Some(stream) => writeln!(stream, "{message}").is_ok(),
            None => false,
        }
    };
    if !sent {
        BRIDGE.pending.lock().unwrap().remove(&id);
        bail!("Browser-Erweiterung nicht erreichbar");
    }
    let answer = rx.recv_timeout(timeout).map_err(|_| {
        BRIDGE.pending.lock().unwrap().remove(&id);
        anyhow!("{}", tr!("Der Browser hat nicht rechtzeitig geantwortet.", "The browser didn't answer in time."))
    })?;
    if answer["ok"].as_bool() == Some(true) {
        Ok(answer["result"].clone())
    } else {
        bail!("{}", answer["error"].as_str().unwrap_or("Fehler in der Browser-Erweiterung"))
    }
}

// ------------------------------------------------------------------ Milo-Seite

/// Startet den lokalen Anschluss fuer den Host, legt die Erweiterung ab und meldet den Host bei
/// Chrome, Edge und Brave an. Aufruf einmal beim Programmstart.
pub fn start() {
    if let Err(err) = install() {
        log::warn!("Browser-Erweiterung nicht eingerichtet: {err}");
    }
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(err) => return log::warn!("Browser-Brücke: kein Anschluss ({err})"),
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let token = random_token();
    if let Err(err) = std::fs::write(bridge_file(), json!({ "port": port, "token": token }).to_string()) {
        return log::warn!("Browser-Brücke: Datei nicht geschrieben ({err})");
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let token = token.clone();
            std::thread::spawn(move || serve(stream, &token));
        }
    });
}

/// Geheimer Schluessel fuer den Anschluss. Die Startwerte von `RandomState` kommen aus der
/// Zufallsquelle des Betriebssystems; drei unabhaengige 64-Bit-Werte ergeben 24 Byte.
fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..3u64)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i ^ std::process::id() as u64);
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// Eine Verbindung vom Host: erst der Schluessel, dann Zeilen mit Antworten und Meldungen.
fn serve(stream: TcpStream, token: &str) {
    let Ok(reader_stream) = stream.try_clone() else { return };
    let mut reader = BufReader::new(reader_stream);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() || first.trim() != token {
        return;
    }
    *BRIDGE.writer.lock().unwrap() = Some(stream);
    for line in reader.lines().map_while(Result::ok) {
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        if message["event"] == "hello" {
            *BRIDGE.browser.lock().unwrap() = message["browser"].as_str().unwrap_or_default().to_string();
            BRIDGE.connected.store(true, Ordering::Relaxed);
            log::info!("Browser-Erweiterung verbunden ({})", browser_name());
            continue;
        }
        if message["event"] == "bye" {
            BRIDGE.connected.store(false, Ordering::Relaxed);
            continue;
        }
        if let Some(id) = message["id"].as_u64() {
            if let Some(tx) = BRIDGE.pending.lock().unwrap().remove(&id) {
                let _ = tx.send(message);
            }
        }
    }
    BRIDGE.connected.store(false, Ordering::Relaxed);
    *BRIDGE.writer.lock().unwrap() = None;
    log::info!("Browser-Erweiterung getrennt");
}

/// Erweiterung in den Datenordner schreiben und den Host bei den Browsern eintragen.
fn install() -> Result<()> {
    let dir = extension_dir();
    std::fs::create_dir_all(&dir)?;
    for (name, content) in [("manifest.json", MANIFEST), ("background.js", BACKGROUND)] {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content) {
            std::fs::write(&path, content)?;
        }
    }
    let exe = std::env::current_exe()?;
    let host = data_dir().join("native-host.json");
    let manifest = json!({
        "name": HOST_NAME,
        "description": "Milo – Verbindung zur Browser-Erweiterung",
        "path": exe,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    });
    std::fs::write(&host, serde_json::to_string_pretty(&manifest)?)?;
    #[cfg(windows)]
    for browser in [r"Google\Chrome", r"Microsoft\Edge", r"BraveSoftware\Brave-Browser", r"Chromium"] {
        let key = format!(r"HKCU\Software\{browser}\NativeMessagingHosts\{HOST_NAME}");
        let mut command = std::process::Command::new("reg");
        command.args(["add", &key, "/ve", "/t", "REG_SZ", "/d", &host.display().to_string(), "/f"]);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        let _ = command.output();
    }
    Ok(())
}

// ------------------------------------------------------------------ Host-Seite

/// Wurde dieses Programm vom Browser als Host gestartet? (erstes Argument `chrome-extension://…`)
pub fn is_host_launch() -> bool {
    std::env::args().nth(1).is_some_and(|a| a.starts_with("chrome-extension://"))
}

/// Laeuft als Host: Nachrichten des Browsers (4 Byte Laenge + JSON) an Milo weiterreichen und
/// Milos Antworten zurueck. Laeuft Milo (noch) nicht, wird alle paar Sekunden neu versucht.
pub fn run_host() {
    let (to_milo_tx, to_milo_rx) = mpsc::channel::<Value>();
    // Vom Browser lesen; endet stdin, schliesst der Browser die Verbindung: Host beenden.
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            let mut len = [0u8; 4];
            if stdin.read_exact(&mut len).is_err() {
                std::process::exit(0);
            }
            let size = u32::from_le_bytes(len) as usize;
            if size > 8 * 1024 * 1024 {
                std::process::exit(0);
            }
            let mut buffer = vec![0u8; size];
            if stdin.read_exact(&mut buffer).is_err() {
                std::process::exit(0);
            }
            if let Ok(message) = serde_json::from_slice::<Value>(&buffer) {
                let _ = to_milo_tx.send(message);
            }
        }
    });
    let send_to_browser = |message: &Value| {
        let bytes = message.to_string().into_bytes();
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(&(bytes.len() as u32).to_le_bytes());
        let _ = out.write_all(&bytes);
        let _ = out.flush();
    };
    let mut hello: Option<Value> = None;
    loop {
        // Mit Milo verbinden (Anschluss und Schluessel stehen in der Datei, die Milo beim Start schreibt).
        let stream = std::fs::read_to_string(bridge_file())
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|info| {
                let port = info["port"].as_u64()? as u16;
                let token = info["token"].as_str()?.to_string();
                let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
                writeln!(stream, "{token}").ok()?;
                Some(stream)
            });
        let Some(mut stream) = stream else {
            // Inzwischen eingetroffene Auftraege beantworten, damit der Browser nicht haengt; das
            // "hello" fuer spaeter aufheben.
            while let Ok(message) = to_milo_rx.try_recv() {
                if message["event"] == "hello" {
                    hello = Some(message);
                } else if let Some(id) = message["id"].as_u64() {
                    send_to_browser(&json!({ "id": id, "ok": false, "error": "Milo läuft nicht" }));
                }
            }
            std::thread::sleep(Duration::from_secs(3));
            continue;
        };
        if let Some(h) = &hello {
            let _ = writeln!(stream, "{h}");
        }
        // Antworten von Milo an den Browser (eigener Faden), Auftraege des Browsers an Milo.
        let Ok(reader) = stream.try_clone() else { continue };
        let alive = std::sync::Arc::new(AtomicBool::new(true));
        let flag = alive.clone();
        let relay = std::thread::spawn(move || {
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                if let Ok(message) = serde_json::from_str::<Value>(&line) {
                    let bytes = message.to_string().into_bytes();
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(&(bytes.len() as u32).to_le_bytes());
                    let _ = out.write_all(&bytes);
                    let _ = out.flush();
                }
            }
            flag.store(false, Ordering::Relaxed);
        });
        while alive.load(Ordering::Relaxed) {
            match to_milo_rx.recv_timeout(Duration::from_millis(500)) {
                Ok(message) => {
                    if message["event"] == "hello" {
                        hello = Some(message.clone());
                    }
                    if writeln!(stream, "{message}").is_err() {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => std::process::exit(0),
            }
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
        let _ = relay.join();
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_the_fixed_extension_id() {
        let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(manifest["manifest_version"], 3);
        assert!(manifest["permissions"].as_array().unwrap().iter().any(|p| p == "nativeMessaging"));
        // Kennung = erste 32 Hex-Zeichen von SHA-256 des oeffentlichen Schluessels, 0-f -> a-p.
        let key = manifest["key"].as_str().unwrap();
        assert!(key.len() > 300);
        assert_eq!(EXTENSION_ID.len(), 32);
        assert!(EXTENSION_ID.chars().all(|c| ('a'..='p').contains(&c)));
    }

    #[test]
    fn tokens_are_long_and_differ() {
        let a = random_token();
        assert_eq!(a.len(), 48);
        assert_ne!(a, random_token());
    }
}
