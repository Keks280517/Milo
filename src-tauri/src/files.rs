//! Dateien, die der Nutzer Milo gibt. Jede Datei wird in Milos eigenen Ordner kopiert (das
//! Original bleibt unberuehrt), ihr Text wird ausgelesen und lokal gespeichert, damit Milo
//! Fragen dazu beantworten kann. Nichts davon verlaesst den PC.
//!
//! Auslesen klappt bei Text, Code, CSV, PDF mit Textebene sowie DOCX, XLSX und PPTX.
//! Nicht ausgelesen werden gescannte PDFs (nur Bilder), alte .doc/.xls/.ppt und Bilder;
//! solche Dateien werden trotzdem aufbewahrt und ehrlich als "ohne lesbaren Text" gefuehrt.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::Local;
use rusqlite::params;
use serde::Serialize;

use crate::memory::Memory;

/// Groessere Dateien werden nicht kopiert.
const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;
/// So viel Text wird hoechstens je Datei gespeichert.
const MAX_TEXT_CHARS: usize = 400_000;

const TEXT_TYPES: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "log", "html", "htm", "css",
    "js", "ts", "jsx", "tsx", "svelte", "vue", "py", "rs", "go", "java", "kt", "c", "h", "cpp", "hpp", "cs", "php", "rb",
    "sh", "ps1", "bat", "cmd", "sql", "tex", "srt", "rtf",
];
const IMAGE_TYPES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "heic"];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredFile {
    pub id: i64,
    pub name: String,
    /// Wo Milos Kopie liegt.
    pub path: String,
    pub size: i64,
    /// text, pdf, office, image oder other.
    pub kind: String,
    pub added_at: String,
    /// Zeichen ausgelesenen Texts; 0 = kein lesbarer Text.
    pub chars: usize,
    /// Hinweis fuer den Nutzer, z. B. warum kein Text gelesen werden konnte.
    pub note: String,
}

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn kind_of(path: &Path) -> &'static str {
    let ext = extension(path);
    match ext.as_str() {
        "pdf" => "pdf",
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" => "office",
        e if TEXT_TYPES.contains(&e) => "text",
        e if IMAGE_TYPES.contains(&e) => "image",
        _ => "other",
    }
}

/// Liest den Text einer Datei aus. Liefert den Text und einen Hinweis, falls etwas nicht ging.
pub fn extract_text(path: &Path) -> (String, String) {
    let result = match kind_of(path) {
        "text" => read_plain(path),
        "pdf" => read_pdf(path),
        "office" => read_office(path),
        "image" => return (String::new(), tr!("Bild: Text lese ich hier nicht aus.", "Image: I don't read text from it here.").into()),
        _ => {
            // Unbekannte Endung: vielleicht ist es trotzdem schlichter Text.
            return match read_plain(path) {
                Ok(text) if looks_like_text(&text) => (limit(text), String::new()),
                _ => (String::new(), tr!("Diesen Dateityp kann ich nicht lesen.", "I can't read this file type.").into()),
            };
        }
    };
    match result {
        Ok(text) if text.trim().is_empty() => {
            let why = if kind_of(path) == "pdf" { tr!("Kein Text gefunden, vermutlich ein gescanntes PDF.", "No text found, probably a scanned PDF.") } else { tr!("Die Datei enthält keinen lesbaren Text.", "The file contains no readable text.") };
            (String::new(), why.into())
        }
        Ok(text) => (limit(text), String::new()),
        Err(err) => {
            log::warn!("Auslesen von {} fehlgeschlagen: {err:#}", path.display());
            (String::new(), tr!("Der Inhalt ließ sich nicht auslesen.", "The content couldn't be read.").into())
        }
    }
}

fn limit(text: String) -> String {
    if text.chars().count() <= MAX_TEXT_CHARS { text } else { text.chars().take(MAX_TEXT_CHARS).collect() }
}

fn looks_like_text(text: &str) -> bool {
    !text.is_empty() && text.chars().take(2000).filter(|c| c.is_control() && !c.is_whitespace() || *c == '\u{fffd}').count() < 20
}

fn read_plain(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    // UTF-16 mit BOM kommt unter Windows haeufig vor (z. B. aus dem Editor oder PowerShell).
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|b| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
            .collect();
        return Ok(String::from_utf16_lossy(&units));
    }
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

fn read_pdf(path: &Path) -> Result<String> {
    // Die PDF-Bibliothek kann bei kaputten Dateien abstuerzen; das darf Milo nicht mitreissen.
    let path = path.to_path_buf();
    match std::panic::catch_unwind(move || pdf_extract::extract_text(&path)) {
        Ok(result) => Ok(tidy(&result?)),
        Err(_) => bail!(tr!("PDF nicht lesbar", "PDF not readable")),
    }
}

/// DOCX, XLSX, PPTX und OpenDocument sind ZIP-Archive mit XML darin.
fn read_office(path: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?).context("kein gültiges Office-Dokument")?;
    let mut names: Vec<String> = archive
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.to_string()))
        .filter(|n| {
            n == "word/document.xml" || n == "content.xml" || n == "xl/sharedStrings.xml" || (n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
        })
        .collect();
    names.sort_by_key(|n| (n.len(), n.clone()));
    let mut out = String::new();
    for name in names {
        let mut xml = String::new();
        archive.by_name(&name)?.take(30_000_000).read_to_string(&mut xml)?;
        out.push_str(&xml_text(&xml));
        out.push('\n');
    }
    Ok(tidy(&out))
}

/// Holt den Text aus Office-XML: Tags fallen weg, Absatz- und Zellenenden werden zu Umbruechen.
fn xml_text(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() / 4);
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else { break };
        let tag = &rest[start + 1..start + end];
        if tag.starts_with("/w:p") || tag.starts_with("/a:p") || tag.starts_with("/text:p") || tag.starts_with("/si") || tag.starts_with("w:br") || tag.starts_with("/text:h") {
            out.push('\n');
        } else if tag.starts_with("w:tab") || tag.starts_with("/w:tc") {
            out.push(' ');
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'")
}

/// Raeumt ausgelesenen Text auf: ueberzaehlige Leerzeichen und Leerzeilen weg.
fn tidy(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() && lines.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// Freier Dateiname im Zielordner: bei Namensgleichheit wird " (2)", " (3)" ... angehaengt.
fn free_target(dir: &Path, name: &str) -> PathBuf {
    let source = Path::new(name);
    let stem = source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Datei".into());
    let ext = source.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut target = dir.join(name);
    let mut n = 2;
    while target.exists() {
        target = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    target
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredFile> {
    Ok(StoredFile {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        size: r.get(3)?,
        kind: r.get(4)?,
        added_at: r.get(5)?,
        chars: r.get::<_, i64>(6)? as usize,
        note: r.get(7)?,
    })
}

const COLUMNS: &str = "id, name, path, size, kind, added_at, length(text), note";

/// Kopiert eine Datei in Milos Ordner, liest ihren Text aus und merkt sie sich.
pub fn import(memory: &Memory, dir: &Path, source: &Path) -> Result<StoredFile> {
    let meta = std::fs::metadata(source).with_context(|| tr!(format!("„{}“ ist nicht lesbar", source.display()), format!("\"{}\" isn't readable", source.display())))?;
    if meta.is_dir() {
        bail!(tr!("Ordner nehme ich nicht an, nur einzelne Dateien.", "I don't take folders, only single files."));
    }
    if meta.len() > MAX_FILE_BYTES {
        bail!(tr!("Die Datei ist größer als 200 MB.", "The file is larger than 200 MB."));
    }
    std::fs::create_dir_all(dir)?;
    let name = source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Datei".into());
    let target = free_target(dir, &name);
    std::fs::copy(source, &target).with_context(|| tr!(format!("„{name}“ ließ sich nicht kopieren (ist die Datei gerade geöffnet?)"), format!("\"{name}\" couldn't be copied (is the file open right now?)")))?;
    let (text, note) = extract_text(&target);
    let stored_name = target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(name);
    memory.with(|conn| {
        conn.execute(
            "INSERT INTO files (name, path, size, kind, added_at, text, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![stored_name, target.to_string_lossy(), meta.len() as i64, kind_of(&target), Local::now().format("%Y-%m-%d %H:%M").to_string(), text, note],
        )?;
        let id = conn.last_insert_rowid();
        Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM files WHERE id = ?1"), params![id], row)?)
    })
}

pub fn list(memory: &Memory, limit: usize) -> Result<Vec<StoredFile>> {
    memory.with(|conn| {
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM files ORDER BY id DESC LIMIT ?1"))?;
        let rows = stmt.query_map(params![limit as i64], row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    })
}

/// Findet eine Datei ueber ihren Namen (ungefaehr) und liefert sie mit ihrem Text.
pub fn find(memory: &Memory, name: &str) -> Result<Option<(StoredFile, String)>> {
    let wanted = name.trim().to_lowercase();
    let files = list(memory, 500)?;
    let hit = if wanted.is_empty() {
        files.first()
    } else {
        files
            .iter()
            .find(|f| f.name.to_lowercase() == wanted)
            .or_else(|| files.iter().find(|f| f.name.to_lowercase().contains(&wanted)))
            .or_else(|| {
                // Einzelne Woerter der Anfrage im Dateinamen, z. B. "die Rechnung" -> "Rechnung_Mai.pdf".
                let words: Vec<&str> = wanted.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 4).collect();
                files.iter().find(|f| words.iter().any(|w| f.name.to_lowercase().contains(w)))
            })
    };
    let Some(file) = hit.cloned() else {
        return Ok(None);
    };
    let text: String = memory.with(|conn| Ok(conn.query_row("SELECT text FROM files WHERE id = ?1", params![file.id], |r| r.get(0))?))?;
    Ok(Some((file, text)))
}

/// Entfernt Milos Kopie und den Eintrag. Das Original des Nutzers wird nie angefasst.
pub fn remove(memory: &Memory, dir: &Path, id: i64) -> Result<bool> {
    let path: Option<String> =
        memory.with(|conn| Ok(conn.query_row("SELECT path FROM files WHERE id = ?1", params![id], |r| r.get(0)).ok()))?;
    let Some(path) = path else {
        return Ok(false);
    };
    let path = PathBuf::from(path);
    // Nur loeschen, was wirklich in Milos eigenem Ordner liegt.
    if path.starts_with(dir) && path.is_file() {
        std::fs::remove_file(&path).with_context(|| tr!("Die Kopie ließ sich nicht löschen", "The copy couldn't be deleted"))?;
    }
    memory.with(|conn| Ok(conn.execute("DELETE FROM files WHERE id = ?1", params![id])? > 0))
}

/// Ausschnitt eines Texts ab Zeichen `from`, hoechstens `max` Zeichen.
pub fn slice(text: &str, from: usize, max: usize) -> String {
    text.chars().skip(from).take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("milo-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn kinds_follow_the_extension() {
        assert_eq!(kind_of(Path::new("a.TXT")), "text");
        assert_eq!(kind_of(Path::new("b.pdf")), "pdf");
        assert_eq!(kind_of(Path::new("c.docx")), "office");
        assert_eq!(kind_of(Path::new("d.PNG")), "image");
        assert_eq!(kind_of(Path::new("e.exe")), "other");
    }

    #[test]
    fn office_xml_becomes_readable_text() {
        let xml = "<w:p><w:r><w:t>Hallo</w:t></w:r><w:r><w:t xml:space=\"preserve\"> Welt &amp; mehr</w:t></w:r></w:p><w:p><w:r><w:t>Zweiter Absatz</w:t></w:r></w:p>";
        assert_eq!(tidy(&xml_text(xml)), "Hallo Welt & mehr\nZweiter Absatz");
    }

    #[test]
    fn import_copies_reads_and_never_overwrites() {
        let work = temp_dir("import");
        let source = work.join("Einkauf.txt");
        std::fs::write(&source, "Milch\n\n\n  Brot   und  Eier ").unwrap();
        let store = work.join("ablage");
        let memory = Memory::in_memory().unwrap();

        let first = import(&memory, &store, &source).unwrap();
        assert_eq!((first.name.as_str(), first.kind.as_str()), ("Einkauf.txt", "text"));
        assert!(first.chars > 0 && first.note.is_empty());
        // Gleicher Name noch einmal: die erste Kopie bleibt, die zweite bekommt eine Nummer.
        let second = import(&memory, &store, &source).unwrap();
        assert_eq!(second.name, "Einkauf (2).txt");
        assert!(source.exists(), "das Original bleibt unberührt");

        let (found, text) = find(&memory, "die einkauf liste").unwrap().unwrap();
        assert!(found.name.starts_with("Einkauf"));
        assert!(text.contains("Brot"));
        assert_eq!(list(&memory, 10).unwrap().len(), 2);

        assert!(remove(&memory, &store, first.id).unwrap());
        assert!(!Path::new(&first.path).exists());
        assert!(source.exists());
        assert!(!remove(&memory, &store, first.id).unwrap());
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn unreadable_files_are_kept_with_an_honest_note() {
        let work = temp_dir("binary");
        let source = work.join("bild.png");
        std::fs::write(&source, [0x89, b'P', b'N', b'G', 0, 1, 2, 3]).unwrap();
        let memory = Memory::in_memory().unwrap();
        let file = import(&memory, &work.join("ablage"), &source).unwrap();
        assert_eq!(file.chars, 0);
        assert!(file.note.contains("Bild"));
        assert!(import(&memory, &work.join("ablage"), &work).is_err(), "Ordner werden abgelehnt");
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn utf16_text_is_decoded() {
        let work = temp_dir("utf16");
        let path = work.join("notiz.txt");
        let mut bytes = vec![0xff, 0xfe];
        for unit in "Grüße".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(extract_text(&path).0, "Grüße");
        let _ = std::fs::remove_dir_all(&work);
    }
}
