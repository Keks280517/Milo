//! Dateien auf dem PC ansehen: Downloads, bekannte Ordner und eigene Favoriten. Dateien lassen sich
//! von hier verschieben oder kopieren (mit Eintrag im Aktionsprotokoll, rueckgaengig machbar) und in
//! einer Vorschau ansehen. Fuer die Vorschau liefert Windows selbst die Miniatur (wie im Explorer),
//! fuer Textdateien kommt der Anfang des Texts.
//!
//! Gelesen wird nur, solange die Ansicht offen ist: die Oberflaeche meldet sich dafuer regelmaessig
//! (`browse_watch`), sonst schlaeft die Ueberwachung ein.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::memory::Memory;

pub const EVENT: &str = "milo://browse";
const MAX_ENTRIES: usize = 400;
const TEXT_PREVIEW: usize = 12 * 1024;
/// Ohne Lebenszeichen der Oberflaeche hoert die Ueberwachung nach dieser Zeit auf.
const WATCH_IDLE: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub size: u64,
    pub modified_ms: i64,
    /// image, pdf, text, office, video, audio, archive, program, folder, other
    pub kind: &'static str,
    /// Download laeuft noch (.crdownload, .part, .download).
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub path: String,
    /// Eingebauter Ordner (Downloads, Desktop ...) oder eigener Favorit.
    pub builtin: bool,
}

fn kind_of(name: &str, dir: bool) -> &'static str {
    if dir {
        return "folder";
    }
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" | "svg" | "ico" | "tif" | "tiff" => "image",
        "pdf" => "pdf",
        "txt" | "md" | "csv" | "json" | "log" | "xml" | "yml" | "yaml" | "ini" | "toml" | "rs" | "ts" | "js" | "py" | "html" | "css" | "svelte" | "ps1" | "bat" => "text",
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "rtf" => "office",
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" => "video",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => "audio",
        "zip" | "rar" | "7z" | "tar" | "gz" => "archive",
        "exe" | "msi" | "msix" | "appx" => "program",
        _ => "other",
    }
}

fn is_partial(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".crdownload") || lower.ends_with(".part") || lower.ends_with(".download") || lower.ends_with(".partial")
}

fn millis(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(windows)]
fn hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    // FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM
    meta.file_attributes() & (0x2 | 0x4) != 0
}

#[cfg(not(windows))]
fn hidden(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Inhalt eines Ordners. Downloads: neueste zuerst; sonst Ordner zuerst, dann nach Name.
pub fn list(dir: &Path, newest_first: bool) -> Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = std::fs::read_dir(dir)
        .with_context(|| tr!(format!("Den Ordner {} kann ich nicht lesen.", dir.display()), format!("I can't read the folder {}.", dir.display())))?
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            if hidden(&meta) || name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") {
                return None;
            }
            let dir = meta.is_dir();
            Some(Entry {
                kind: kind_of(&name, dir),
                partial: is_partial(&name),
                path: e.path().to_string_lossy().to_string(),
                size: if dir { 0 } else { meta.len() },
                modified_ms: meta.modified().map(millis).unwrap_or(0),
                dir,
                name,
            })
        })
        .collect();
    if newest_first {
        entries.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    } else {
        entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    }
    entries.truncate(MAX_ENTRIES);
    Ok(entries)
}

/// Freier Name im Zielordner: "Bericht.pdf", sonst "Bericht (2).pdf" usw.
pub fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..10_000).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// Verschiebt eine Datei oder einen Ordner. Ueber Laufwerksgrenzen hinweg wird eine Datei kopiert
/// und das Original erst danach entfernt (nur wenn die Kopie vollstaendig ist).
pub fn move_path(from: &Path, to: &Path) -> Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        bail!(tr!("Ordner kann ich nur innerhalb desselben Laufwerks verschieben.", "I can only move folders within the same drive."));
    }
    let copied = std::fs::copy(from, to).with_context(|| tr!(format!("Kopieren nach {} ist fehlgeschlagen.", to.display()), format!("Copying to {} failed.", to.display())))?;
    if copied != std::fs::metadata(from)?.len() {
        let _ = std::fs::remove_file(to);
        bail!(tr!("Die Kopie ist unvollständig, das Original bleibt, wo es ist.", "The copy is incomplete; the original stays where it is."));
    }
    std::fs::remove_file(from).context("Kopiert, aber das Original ließ sich nicht entfernen.")?;
    Ok(())
}

fn copy_path(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)?.flatten() {
            copy_path(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ()).with_context(|| tr!(format!("Kopieren nach {} ist fehlgeschlagen.", to.display()), format!("Copying to {} failed.", to.display())))
    }
}

/// Verschiebt oder kopiert `from` in den Ordner `target`. Liefert den neuen Pfad.
pub fn transfer(from: &Path, target: &Path, copy: bool) -> Result<PathBuf> {
    if !from.exists() {
        bail!(tr!("Die Datei gibt es nicht mehr.", "That file doesn't exist anymore."));
    }
    if !target.is_dir() {
        bail!(tr!("Das Ziel ist kein Ordner.", "The target isn't a folder."));
    }
    if target.starts_with(from) {
        bail!(tr!("Ein Ordner kann nicht in sich selbst landen.", "A folder can't go inside itself."));
    }
    if !copy && from.parent() == Some(target) {
        bail!(tr!("Die Datei liegt schon in diesem Ordner.", "The file is already in this folder."));
    }
    let name = from.file_name().ok_or_else(|| anyhow!(tr!("Ungültiger Pfad.", "Invalid path.")))?.to_string_lossy().to_string();
    let to = free_name(target, &name);
    if copy { copy_path(from, &to)? } else { move_path(from, &to)? }
    Ok(to)
}

// ---------- Favoriten ----------

fn places_file(app: &AppHandle) -> PathBuf {
    app.path().app_config_dir().unwrap_or_default().join("ordner.json")
}

fn own_places(app: &AppHandle) -> Vec<String> {
    std::fs::read_to_string(places_file(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_places(app: &AppHandle, places: &[String]) -> Result<()> {
    let file = places_file(app);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, serde_json::to_string_pretty(places)?)?;
    Ok(())
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string())
}

#[tauri::command]
pub fn browse_places(app: AppHandle) -> Vec<Place> {
    let mut places: Vec<Place> = [("Downloads", "downloads"), ("Desktop", "desktop"), (tr!("Dokumente", "Documents"), "dokumente"), (tr!("Bilder", "Pictures"), "bilder"), ("Videos", "videos"), (tr!("Musik", "Music"), "musik")]
        .iter()
        .filter_map(|(label, key)| crate::pc::known_folder(key).map(|p| Place { name: label.to_string(), path: p.to_string_lossy().to_string(), builtin: true }))
        .collect();
    for path in own_places(&app) {
        if Path::new(&path).is_dir() && !places.iter().any(|p| p.path.eq_ignore_ascii_case(&path)) {
            places.push(Place { name: name_of(Path::new(&path)), path, builtin: false });
        }
    }
    places
}

#[tauri::command]
pub fn browse_add_place(app: AppHandle, path: String) -> Result<Vec<Place>, String> {
    if !Path::new(&path).is_dir() {
        return Err(tr!("Das ist kein Ordner.", "That's not a folder.").into());
    }
    let mut places = own_places(&app);
    if !places.iter().any(|p| p.eq_ignore_ascii_case(&path)) {
        places.push(path);
        save_places(&app, &places).map_err(|e| e.to_string())?;
    }
    Ok(browse_places(app))
}

#[tauri::command]
pub fn browse_remove_place(app: AppHandle, path: String) -> Result<Vec<Place>, String> {
    let mut places = own_places(&app);
    places.retain(|p| !p.eq_ignore_ascii_case(&path));
    save_places(&app, &places).map_err(|e| e.to_string())?;
    Ok(browse_places(app))
}

// ---------- Befehle ----------

#[tauri::command]
pub async fn browse_list(path: String, newest_first: bool) -> Result<Vec<Entry>, String> {
    tokio::task::spawn_blocking(move || list(Path::new(&path), newest_first)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

/// Verschiebt (oder kopiert) eine Datei in einen Ordner. Steht im Protokoll und laesst sich zuruecknehmen.
#[tauri::command]
pub async fn browse_transfer(app: AppHandle, from: String, target: String, copy: bool) -> Result<String, String> {
    let (source, folder) = (PathBuf::from(&from), PathBuf::from(&target));
    let result = tokio::task::spawn_blocking(move || transfer(&source, &folder, copy)).await.map_err(|e| e.to_string())?;
    let name = name_of(Path::new(&from));
    let memory = app.state::<Memory>();
    match result {
        Ok(to) => {
            let to_text = to.to_string_lossy().to_string();
            let undo = if copy { json!({ "kind": "copy", "path": to_text }) } else { json!({ "kind": "move", "from": from, "to": to_text }) };
            let what = if copy { tr!("kopiert", "copied") } else { tr!("verschoben", "moved") };
            memory.log_entry("file_move", &tr!(format!("„{name}“ nach {} {what}", name_of(Path::new(&target))), format!("\"{name}\" {what} to {}", name_of(Path::new(&target)))), true, &format!("{from} → {to_text}"), 0, Some(&undo.to_string()));
            Ok(to_text)
        }
        Err(err) => {
            memory.log_entry("file_move", &tr!(format!("„{name}“ nicht verschoben: {err}"), format!("\"{name}\" not moved: {err}")), false, &from, 0, None);
            Err(err.to_string())
        }
    }
}

#[tauri::command]
pub fn browse_open(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(tr!("Die Datei gibt es nicht mehr.", "That file doesn't exist anymore.").into());
    }
    crate::system::shell_open(&path).map_err(|e| e.to_string())
}

/// Anfang einer Textdatei fuer die Vorschau.
#[tauri::command]
pub async fn browse_text(path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::io::Read;
        let mut bytes = Vec::with_capacity(TEXT_PREVIEW);
        std::fs::File::open(&path)?.take(TEXT_PREVIEW as u64).read_to_end(&mut bytes)?;
        if bytes.iter().take(4096).any(|b| *b == 0) {
            bail!(tr!("Das ist keine Textdatei.", "That's not a text file."));
        }
        Ok(String::from_utf8_lossy(&bytes).to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Miniatur wie im Explorer (Bilder, PDFs, Videos, Office ...) als PNG-Daten-URL.
#[tauri::command]
pub async fn browse_thumb(path: String, size: u32) -> Result<String, String> {
    use base64::Engine;
    let size = size.clamp(32, 768);
    let png = tokio::task::spawn_blocking(move || thumbnail(Path::new(&path), size)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

#[cfg(windows)]
fn thumbnail(path: &Path, size: u32) -> Result<Vec<u8>> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT};

    // SAFETY: COM-Aufrufe in diesem Thread; die Bitmap wird nach dem Auslesen freigegeben.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        // Die Shell versteht nur Rueckstriche.
        let path = path.to_string_lossy().replace('/', "\\");
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path.as_str()), None)?;
        let bitmap = factory.GetImage(SIZE { cx: size as i32, cy: size as i32 }, SIIGBF_RESIZETOFIT)?;
        let mut info = BITMAP::default();
        GetObjectW(bitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast()));
        let (width, height) = (info.bmWidth, info.bmHeight.abs());
        if width <= 0 || height <= 0 {
            let _ = DeleteObject(bitmap.into());
            bail!(tr!("Keine Vorschau verfügbar.", "No preview available."));
        }
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, ..Default::default() },
            ..Default::default()
        };
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(dc, bitmap, 0, height as u32, Some(pixels.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(bitmap.into());
        if lines == 0 {
            bail!(tr!("Keine Vorschau verfügbar.", "No preview available."));
        }
        // BGRA → RGBA. Ist der Alphakanal ueberall 0 (Bitmaps ohne Transparenz), deckend machen.
        let opaque = pixels.chunks_exact(4).all(|p| p[3] == 0);
        for p in pixels.chunks_exact_mut(4) {
            p.swap(0, 2);
            if opaque {
                p[3] = 255;
            }
        }
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(&pixels)?;
        }
        Ok(png_bytes)
    }
}

#[cfg(not(windows))]
fn thumbnail(_path: &Path, _size: u32) -> Result<Vec<u8>> {
    bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
}

// ---------- Ueberwachung ----------

/// Ordner, den die Oberflaeche gerade zeigt, und wann sie sich zuletzt gemeldet hat.
static WATCH: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);
static WATCHING: AtomicBool = AtomicBool::new(false);

fn changed_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Die Oberflaeche zeigt `path` (oder nichts mehr). Solange sie sich alle paar Sekunden meldet,
/// prueft ein Thread einmal pro Sekunde, ob sich der Ordner geaendert hat, und meldet das.
/// Neue, umbenannte und geloeschte Dateien aendern den Zeitstempel des Ordners; mehr wird nicht gelesen.
#[tauri::command]
pub fn browse_watch(app: AppHandle, path: Option<String>) {
    *WATCH.lock().unwrap() = path.map(|p| (PathBuf::from(p), Instant::now()));
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let mut last: Option<(PathBuf, Option<SystemTime>)> = None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let current = WATCH.lock().unwrap().clone();
            let Some((path, ping)) = current.filter(|(_, ping)| ping.elapsed() < WATCH_IDLE) else {
                break;
            };
            let _ = ping;
            let stamp = changed_at(&path);
            if let Some((seen, before)) = &last {
                if *seen == path && *before != stamp {
                    let _ = app.emit(EVENT, path.to_string_lossy().to_string());
                }
            }
            last = Some((path, stamp));
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("milo-browse-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn kinds_and_partial_downloads() {
        assert_eq!(kind_of("Foto.JPG", false), "image");
        assert_eq!(kind_of("Rechnung.pdf", false), "pdf");
        assert_eq!(kind_of("Ordner", true), "folder");
        assert!(is_partial("setup.exe.crdownload"));
        assert!(!is_partial("setup.exe"));
    }

    #[test]
    fn transfer_moves_copies_and_never_overwrites() {
        let root = temp("transfer");
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("bericht.txt"), "eins").unwrap();
        std::fs::write(b.join("bericht.txt"), "schon da").unwrap();
        let copied = transfer(&a.join("bericht.txt"), &b, true).unwrap();
        assert_eq!(copied.file_name().unwrap(), "bericht (2).txt");
        assert!(a.join("bericht.txt").exists());
        let moved = transfer(&a.join("bericht.txt"), &b, false).unwrap();
        assert_eq!(moved.file_name().unwrap(), "bericht (3).txt");
        assert!(!a.join("bericht.txt").exists());
        assert_eq!(std::fs::read_to_string(b.join("bericht.txt")).unwrap(), "schon da");
        assert!(transfer(&b, &b.join("x"), false).is_err());
        let listed = list(&b, true).unwrap();
        assert_eq!(listed.len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }
}
