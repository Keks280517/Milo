//! Kleine Waechter im Hintergrund, die der Pille Bescheid geben:
//! - Zwischenablage: Verlauf der letzten Texte (nur im Arbeitsspeicher, Passwoerter nie) und
//!   Bildschirmfotos (Win+Umschalt+S legt ein Bild in die Zwischenablage).
//! - Downloads: eine neue, fertige Datei im Downloads-Ordner.
//! - Akku: PC-Akku und Bluetooth-Geraete (Kopfhoerer, Maus) unter 15 %.
//! - Mikrofon und Kamera: welche Programme sie gerade benutzen (aus der Windows-Datenschutzliste).
//! Jeder Waechter laeuft in einem eigenen, sparsamen Thread und meldet nur Aenderungen.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::SettingsStore;

const HISTORY: usize = 20;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: u64,
    pub text: String,
    pub at_ms: i64,
}

#[derive(Default)]
pub struct Pulse {
    clips: Mutex<Vec<ClipItem>>,
    next_id: Mutex<u64>,
    /// Text, den Milo selbst gerade kopiert hat (nicht noch einmal in den Verlauf).
    own_copy: Mutex<Option<String>>,
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir())
}

/// Sieht der Text wie ein Passwort oder ein Schluessel aus? Dann kommt er nie in den Verlauf.
pub fn looks_secret(text: &str) -> bool {
    let t = text.trim();
    let prefixes = ["AIza", "gsk_", "sk-", "ghp_", "github_pat_", "xox", "eyJ"];
    if prefixes.iter().any(|p| t.starts_with(p)) && !t.contains(' ') {
        return true;
    }
    let len = t.chars().count();
    if !(8..=64).contains(&len) || t.chars().any(char::is_whitespace) {
        return false;
    }
    let lower = t.chars().any(|c| c.is_lowercase());
    let upper = t.chars().any(|c| c.is_uppercase());
    let digit = t.chars().any(|c| c.is_ascii_digit());
    let symbol = t.chars().any(|c| !c.is_alphanumeric());
    // Adressen und Pfade sind keine Passwoerter.
    let address = t.contains("://") || t.contains('\\') || t.contains('/') || t.contains('@') && t.contains('.');
    !address && [lower, upper, digit, symbol].iter().filter(|x| **x).count() >= 3
}

// ---------------------------------------------------------------- Zwischenablage

fn push_clip(app: &AppHandle, text: String) {
    let pulse = app.state::<Pulse>();
    if pulse.own_copy.lock().unwrap().as_deref() == Some(text.as_str()) {
        return;
    }
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 20_000 || looks_secret(trimmed) {
        return;
    }
    let list = {
        let mut clips = pulse.clips.lock().unwrap();
        clips.retain(|c| c.text != text);
        let mut id = pulse.next_id.lock().unwrap();
        *id += 1;
        clips.insert(0, ClipItem { id: *id, text, at_ms: now_ms() });
        clips.truncate(HISTORY);
        clips.clone()
    };
    let _ = app.emit("milo://clipboard", &list);
}

#[tauri::command]
pub fn clipboard_history(pulse: tauri::State<'_, Pulse>) -> Vec<ClipItem> {
    pulse.clips.lock().unwrap().clone()
}

/// Kopiert einen Eintrag aus dem Verlauf wieder in die Zwischenablage (und nach oben).
#[tauri::command]
pub fn clipboard_use(app: AppHandle, id: u64) -> Result<(), String> {
    let pulse = app.state::<Pulse>();
    let text = pulse.clips.lock().unwrap().iter().find(|c| c.id == id).map(|c| c.text.clone()).ok_or_else(|| tr!("Den Eintrag gibt es nicht mehr.", "That entry is gone.").to_string())?;
    *pulse.own_copy.lock().unwrap() = Some(text.clone());
    crate::textwork::clipboard_write(&text).map_err(|e| e.to_string())?;
    let list = {
        let mut clips = pulse.clips.lock().unwrap();
        if let Some(pos) = clips.iter().position(|c| c.id == id) {
            let item = clips.remove(pos);
            clips.insert(0, item);
        }
        clips.clone()
    };
    let _ = app.emit("milo://clipboard", &list);
    Ok(())
}

#[tauri::command]
pub fn clipboard_clear(app: AppHandle, id: Option<u64>) {
    let pulse = app.state::<Pulse>();
    let list = {
        let mut clips = pulse.clips.lock().unwrap();
        match id {
            Some(id) => clips.retain(|c| c.id != id),
            None => clips.clear(),
        }
        clips.clone()
    };
    let _ = app.emit("milo://clipboard", &list);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    pub path: String,
    /// Kleine Vorschau als data:-Adresse.
    pub thumb: String,
    pub width: u32,
    pub height: u32,
}

/// Rohes Bild (RGBA, von oben nach unten).
struct Rgba {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn encode_png(image: &Rgba) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image.pixels)?;
    }
    Ok(out)
}

/// Verkleinert (naechster Nachbar reicht fuer eine Vorschau).
fn shrink(image: &Rgba, max_w: u32) -> Rgba {
    if image.width <= max_w {
        return Rgba { width: image.width, height: image.height, pixels: image.pixels.clone() };
    }
    let width = max_w;
    let height = ((image.height as u64 * max_w as u64) / image.width as u64).max(1) as u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let sy = (y as u64 * image.height as u64 / height as u64) as u32;
        for x in 0..width {
            let sx = (x as u64 * image.width as u64 / width as u64) as u32;
            let i = ((sy * image.width + sx) * 4) as usize;
            pixels.extend_from_slice(&image.pixels[i..i + 4]);
        }
    }
    Rgba { width, height, pixels }
}

/// Wandelt eine DIB aus der Zwischenablage (BITMAPINFOHEADER + Pixel) in RGBA.
fn dib_to_rgba(data: &[u8]) -> Option<Rgba> {
    if data.len() < 40 {
        return None;
    }
    let u32_at = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let header = u32_at(0) as usize;
    let width = i32::from_le_bytes(data[4..8].try_into().unwrap());
    let raw_height = i32::from_le_bytes(data[8..12].try_into().unwrap());
    let bits = u16::from_le_bytes(data[14..16].try_into().unwrap());
    let compression = u32_at(16);
    let colors = u32_at(32) as usize;
    if width <= 0 || raw_height == 0 || !(bits == 24 || bits == 32) || width > 20_000 || raw_height.abs() > 20_000 {
        return None;
    }
    let (width, height) = (width as u32, raw_height.unsigned_abs());
    let top_down = raw_height < 0;
    // Bei BI_BITFIELDS (3) mit kleinem Kopf folgen drei Farbmasken.
    let masks = if compression == 3 && header == 40 { 12 } else { 0 };
    let offset = header + masks + colors * 4;
    let stride = ((width as usize * bits as usize + 31) / 32) * 4;
    if data.len() < offset + stride * height as usize {
        return None;
    }
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    // 32-Bit-Bilder ohne echten Alphakanal haben dort 0: dann deckend zeichnen.
    let alpha_used = bits == 32 && (0..height as usize).any(|row| {
        let start = offset + row * stride;
        (0..width as usize).any(|x| data[start + x * 4 + 3] != 0)
    });
    for row in 0..height as usize {
        let src_row = if top_down { row } else { height as usize - 1 - row };
        let start = offset + src_row * stride;
        for x in 0..width as usize {
            let p = start + x * (bits as usize / 8);
            let (b, g, r) = (data[p], data[p + 1], data[p + 2]);
            let a = if bits == 32 && alpha_used { data[p + 3] } else { 255 };
            pixels.extend_from_slice(&[r, g, b, a]);
        }
    }
    Some(Rgba { width, height, pixels })
}

fn handle_image(app: &AppHandle, dib: Vec<u8>) {
    let Some(image) = dib_to_rgba(&dib) else { return };
    // Winzige Bilder (Symbole aus Programmen) sind keine Bildschirmfotos.
    if image.width < 40 || image.height < 40 {
        return;
    }
    let dir = data_dir(app).join("Bildschirmfotos");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("zuletzt.png");
    let Ok(png) = encode_png(&image) else { return };
    if std::fs::write(&path, &png).is_err() {
        return;
    }
    let small = shrink(&image, 260);
    let thumb = encode_png(&small).map(|bytes| {
        use base64::Engine;
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
    });
    if let Ok(thumb) = thumb {
        let _ = app.emit("milo://screenshot", Shot { path: path.to_string_lossy().into(), thumb, width: image.width, height: image.height });
    }
}

#[cfg(windows)]
mod clip {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};

    pub fn sequence() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }

    pub fn has_text() -> bool {
        unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_ok() }
    }

    pub fn has_image() -> bool {
        unsafe { IsClipboardFormatAvailable(CF_DIB.0 as u32).is_ok() }
    }

    /// Liest die DIB des Bilds in der Zwischenablage.
    pub fn image() -> Option<Vec<u8>> {
        let mut opened = false;
        for _ in 0..20 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if !opened {
            return None;
        }
        let data = unsafe {
            GetClipboardData(CF_DIB.0 as u32).ok().and_then(|handle| {
                let memory = HGLOBAL(handle.0);
                let size = GlobalSize(memory);
                let ptr = GlobalLock(memory) as *const u8;
                if ptr.is_null() || size == 0 {
                    return None;
                }
                let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
                let _ = GlobalUnlock(memory);
                Some(bytes)
            })
        };
        unsafe {
            let _ = CloseClipboard();
        }
        data
    }
}

#[cfg(not(windows))]
mod clip {
    pub fn sequence() -> u32 {
        0
    }
    pub fn has_text() -> bool {
        false
    }
    pub fn has_image() -> bool {
        false
    }
    pub fn image() -> Option<Vec<u8>> {
        None
    }
}

fn spawn_clipboard(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = clip::sequence();
        loop {
            std::thread::sleep(Duration::from_millis(600));
            let now = clip::sequence();
            if now == last {
                continue;
            }
            last = now;
            // Kurz warten: manche Programme legen mehrere Formate nacheinander ab.
            std::thread::sleep(Duration::from_millis(120));
            if crate::guard::clipboard_protected() {
                continue;
            }
            let settings = app.state::<SettingsStore>().get();
            if clip::has_text() {
                if settings.clipboard_history {
                    if let Some(text) = crate::textwork::clipboard_read() {
                        push_clip(&app, text);
                    }
                }
            } else if clip::has_image() && settings.screenshot_card {
                if let Some(dib) = clip::image() {
                    handle_image(&app, dib);
                }
            }
        }
    });
}

/// Bildschirmfoto dauerhaft unter Bilder\Bildschirmfotos ablegen.
#[tauri::command]
pub fn screenshot_save(path: String) -> Result<String, String> {
    let target_dir = crate::pc::known_folder("bilder").map(|p| p.join("Screenshots")).ok_or_else(|| tr!("Den Bilder-Ordner finde ich nicht.", "I can't find the Pictures folder.").to_string())?;
    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
    let name = format!("Milo {}.png", chrono::Local::now().format("%Y-%m-%d %H-%M-%S"));
    let target = target_dir.join(name);
    std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into())
}

/// Text aus einem Bild lesen (Windows-Texterkennung, alles auf dem PC) und in die Zwischenablage legen.
#[tauri::command]
pub async fn screenshot_ocr(app: AppHandle, path: String) -> Result<String, String> {
    let script = format!(
        r#"[Console]::OutputEncoding = [Text.Encoding]::UTF8
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {{ $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' }})[0]
function Await($op, [Type]$type) {{ $t = $asTask.MakeGenericMethod($type).Invoke($null, @($op)); $t.Wait(-1) | Out-Null; $t.Result }}
[Windows.Storage.StorageFile,Windows.Storage,ContentType=WindowsRuntime] | Out-Null
[Windows.Media.Ocr.OcrEngine,Windows.Foundation,ContentType=WindowsRuntime] | Out-Null
[Windows.Graphics.Imaging.BitmapDecoder,Windows.Graphics,ContentType=WindowsRuntime] | Out-Null
$file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync('{}')) ([Windows.Storage.StorageFile])
$stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
$decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
$bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
$result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
$result.Lines | ForEach-Object {{ $_.Text }}"#,
        path.replace('\'', "''")
    );
    let text = crate::pc::powershell_script(&script).await.map_err(|e| e.to_string())?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(tr!("Auf dem Bild habe ich keinen Text gefunden.", "I couldn't find any text in the picture.").into());
    }
    *app.state::<Pulse>().own_copy.lock().unwrap() = Some(text.clone());
    crate::textwork::clipboard_write(&text).map_err(|e| e.to_string())?;
    Ok(text)
}

/// Datei im Explorer markiert zeigen.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(tr!("Die Datei gibt es nicht mehr.", "The file doesn't exist any more.").into());
    }
    std::process::Command::new("explorer.exe").arg(format!("/select,{path}")).spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- Downloads

fn partial(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('.') || lower.starts_with("~$") || [".crdownload", ".part", ".partial", ".tmp", ".download", ".opdownload"].iter().any(|e| lower.ends_with(e)) || lower == "desktop.ini"
}

fn spawn_downloads(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(dir) = crate::pc::known_folder("downloads") else { return };
        let list = |dir: &Path| -> HashMap<String, u64> {
            std::fs::read_dir(dir)
                .map(|it| {
                    it.flatten()
                        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                        .filter_map(|e| Some((e.file_name().to_string_lossy().to_string(), e.metadata().ok()?.len())))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut known: HashSet<String> = list(&dir).into_keys().collect();
        // Neue Datei -> (Groesse, wie oft unveraendert gesehen)
        let mut pending: HashMap<String, (u64, u8)> = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = list(&dir);
            for (name, size) in &now {
                if known.contains(name) || partial(name) {
                    continue;
                }
                let entry = pending.entry(name.clone()).or_insert((*size, 0));
                if entry.0 == *size && *size > 0 {
                    entry.1 += 1;
                } else {
                    *entry = (*size, 0);
                }
                if entry.1 >= 1 {
                    known.insert(name.clone());
                    pending.remove(name);
                    if app.state::<SettingsStore>().get().download_card {
                        let path = dir.join(name);
                        let _ = app.emit("milo://download", serde_json::json!({ "path": path.to_string_lossy(), "name": name, "size": size }));
                    }
                }
            }
            // Geloeschte vergessen, damit eine erneut geladene Datei wieder gemeldet wird.
            known.retain(|n| now.contains_key(n));
            pending.retain(|n, _| now.contains_key(n));
        }
    });
}

// ---------------------------------------------------------------- Akku

#[cfg(windows)]
fn pc_battery() -> Option<(u8, bool)> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut status).ok()? };
    // 128 = kein Akku, 255 = unbekannt
    if status.BatteryFlag == 128 || status.BatteryFlag == 255 || status.BatteryLifePercent > 100 {
        return None;
    }
    Some((status.BatteryLifePercent, status.ACLineStatus == 1))
}

#[cfg(not(windows))]
fn pc_battery() -> Option<(u8, bool)> {
    None
}

/// Bluetooth-Geraete mit Akkustand (Name, Prozent). Fragt Windows per PowerShell, deshalb selten.
async fn bluetooth_batteries() -> Vec<(String, u8)> {
    let script = r#"[Console]::OutputEncoding = [Text.Encoding]::UTF8
Get-PnpDevice -Class Bluetooth -Status OK -ErrorAction SilentlyContinue | ForEach-Object { $p = Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName '{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2' -ErrorAction SilentlyContinue; if ($p -and $p.Data -ne $null) { "$($_.FriendlyName)`t$($p.Data)" } }"#;
    crate::pc::powershell_script(script)
        .await
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once('\t')?;
            Some((name.trim().to_string(), value.trim().parse::<u8>().ok()?))
        })
        .collect()
}

fn spawn_battery(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Wer schon gewarnt wurde (bis er wieder ueber 20 % ist).
        let mut warned: HashSet<String> = HashSet::new();
        let mut last_bt = Instant::now() - Duration::from_secs(3600);
        let mut bt: Vec<(String, u8)> = Vec::new();
        loop {
            if app.state::<SettingsStore>().get().battery_alert {
                let mut levels: Vec<(String, u8, bool)> = Vec::new();
                if let Some((percent, charging)) = pc_battery() {
                    levels.push((tr!("Akku", "Battery").to_string(), percent, charging));
                }
                if last_bt.elapsed() > Duration::from_secs(600) {
                    last_bt = Instant::now();
                    bt = bluetooth_batteries().await;
                }
                levels.extend(bt.iter().map(|(n, p)| (n.clone(), *p, false)));
                for (name, percent, charging) in levels {
                    if charging || percent > 20 {
                        warned.remove(&name);
                    } else if percent <= 15 && warned.insert(name.clone()) {
                        let _ = app.emit("milo://battery", serde_json::json!({ "device": name, "percent": percent }));
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

// ---------------------------------------------------------------- Mikrofon und Kamera

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Privacy {
    pub mic: Vec<String>,
    pub cam: Vec<String>,
}

/// Anzeigename aus einem Eintrag der Datenschutzliste („C:#Program Files#Zoom#bin#Zoom.exe“ -> „Zoom“).
fn app_name(key: &str) -> String {
    let last = key.rsplit('#').next().unwrap_or(key);
    let last = last.split('_').next().unwrap_or(last);
    let last = last.rsplit('.').find(|part| !part.eq_ignore_ascii_case("exe") && !part.is_empty()).unwrap_or(last);
    last.to_string()
}

#[cfg(windows)]
mod registry {
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ};

    fn open(parent: HKEY, path: &str) -> Option<HKEY> {
        let mut key = HKEY::default();
        let name = HSTRING::from(path);
        let status = unsafe { RegOpenKeyExW(parent, PCWSTR(name.as_ptr()), Some(0), KEY_READ, &mut key) };
        (status == ERROR_SUCCESS).then_some(key)
    }

    fn subkeys(key: HKEY) -> Vec<String> {
        let mut out = Vec::new();
        for index in 0.. {
            let mut buffer = [0u16; 512];
            let mut len = buffer.len() as u32;
            let status = unsafe { RegEnumKeyExW(key, index, Some(PWSTR(buffer.as_mut_ptr())), &mut len, None, None, None, None) };
            if status != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buffer[..len as usize]));
        }
        out
    }

    fn qword(key: HKEY, name: &str) -> Option<u64> {
        let name = HSTRING::from(name);
        let mut value = 0u64;
        let mut size = 8u32;
        let status = unsafe { RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, Some(&mut value as *mut u64 as *mut u8), Some(&mut size)) };
        (status == ERROR_SUCCESS).then_some(value)
    }

    /// Eintraege, die gerade benutzt werden (Beginn gesetzt, Ende 0).
    fn active_in(key: HKEY, out: &mut Vec<String>) {
        for name in subkeys(key) {
            if name == "NonPackaged" {
                if let Some(inner) = open(key, &name) {
                    active_in(inner, out);
                    unsafe {
                        let _ = RegCloseKey(inner);
                    }
                }
                continue;
            }
            if let Some(entry) = open(key, &name) {
                let start = qword(entry, "LastUsedTimeStart").unwrap_or(0);
                let stop = qword(entry, "LastUsedTimeStop").unwrap_or(1);
                if start != 0 && stop == 0 {
                    out.push(name);
                }
                unsafe {
                    let _ = RegCloseKey(entry);
                }
            }
        }
    }

    pub fn active(capability: &str) -> Vec<String> {
        let path = format!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{capability}");
        let Some(key) = open(HKEY_CURRENT_USER, &path) else { return Vec::new() };
        let mut out = Vec::new();
        active_in(key, &mut out);
        unsafe {
            let _ = RegCloseKey(key);
        }
        out
    }
}

#[cfg(not(windows))]
mod registry {
    pub fn active(_capability: &str) -> Vec<String> {
        Vec::new()
    }
}

fn privacy_now() -> Privacy {
    // Milo selbst (Spracherkennung, „Hey Milo“) zaehlt nicht.
    let names = |cap: &str| {
        let mut list: Vec<String> = registry::active(cap).into_iter().filter(|k| !k.to_lowercase().contains("milo")).map(|k| app_name(&k)).collect();
        list.sort();
        list.dedup();
        list
    };
    Privacy { mic: names("microphone"), cam: names("webcam") }
}

#[tauri::command]
pub fn privacy_status() -> Privacy {
    privacy_now()
}

fn spawn_privacy(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = Privacy::default();
        loop {
            let now = if app.state::<SettingsStore>().get().privacy_dots { privacy_now() } else { Privacy::default() };
            if now != last {
                let _ = app.emit("milo://privacy", &now);
                last = now;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

// ---------------------------------------------------------------- Start

pub fn spawn(app: AppHandle) {
    spawn_clipboard(app.clone());
    spawn_downloads(app.clone());
    spawn_battery(app.clone());
    spawn_privacy(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_recognized() {
        assert!(looks_secret("AIzaSyD-abcdefghijklmnopqrstu"));
        assert!(looks_secret("gsk_abc123"));
        assert!(looks_secret("Sonne!2026xY"));
        assert!(!looks_secret("Hallo Welt"));
        assert!(!looks_secret("https://example.com/Seite?a=1"));
        assert!(!looks_secret("Einkaufsliste"));
        assert!(!looks_secret("max.mustermann@example.com"));
    }

    #[test]
    fn app_names_are_short() {
        assert_eq!(app_name("C:#Program Files#Zoom#bin#Zoom.exe"), "Zoom");
        assert_eq!(app_name("Microsoft.WindowsCamera_8wekyb3d8bbwe"), "WindowsCamera");
    }

    #[test]
    fn partial_downloads_are_skipped() {
        assert!(partial("film.mp4.crdownload"));
        assert!(partial("datei.part"));
        assert!(!partial("Rechnung.pdf"));
    }

    #[test]
    fn dib_bottom_up_24_bit() {
        // 1x2 Bild, unten rot, oben blau (BGR, Zeilen auf 4 Byte aufgefuellt).
        let mut dib = vec![0u8; 40];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1i32.to_le_bytes());
        dib[8..12].copy_from_slice(&2i32.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        dib.extend_from_slice(&[0, 0, 255, 0]); // untere Zeile: rot
        dib.extend_from_slice(&[255, 0, 0, 0]); // obere Zeile: blau
        let image = dib_to_rgba(&dib).unwrap();
        assert_eq!(&image.pixels, &[0, 0, 255, 255, 255, 0, 0, 255]);
    }
}
