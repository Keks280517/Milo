//! Word-Dokumente (.docx) erstellen, z. B. fuer Berichte nach einer Recherche.
//!
//! Das Modell liefert nur den Inhalt (Titel, Abschnitte, Quellen); die Gestaltung ist fest:
//! klare Ueberschriften, gut lesbare Schrift, Quellenliste am Ende.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use docx_rs::{AlignmentType, Docx, LineSpacing, Paragraph, Run, RunFonts, SpecialIndentType};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Section {
    pub heading: String,
    pub paragraphs: Vec<String>,
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SourceRef {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Document {
    pub title: String,
    pub subtitle: String,
    pub sections: Vec<Section>,
    pub sources: Vec<SourceRef>,
}

const FONT: &str = "Calibri";
/// Schriftgroessen in halben Punkt (Word-Masseinheit).
const BODY: usize = 22;
const HEADING: usize = 30;
const TITLE: usize = 44;
/// Farbe der Ueberschriften.
const ACCENT: &str = "1F4E79";

fn text_run(text: &str, size: usize) -> Run {
    Run::new().add_text(text).size(size).fonts(RunFonts::new().ascii(FONT).hi_ansi(FONT).cs(FONT))
}

fn spaced(p: Paragraph, before: u32, after: u32) -> Paragraph {
    p.line_spacing(LineSpacing::new().before(before).after(after))
}

/// Baut das Dokument im Speicher.
pub fn build(doc: &Document) -> Result<Vec<u8>> {
    if doc.title.trim().is_empty() {
        bail!(tr!("Das Dokument braucht einen Titel.", "The document needs a title."));
    }
    if doc.sections.iter().all(|s| s.paragraphs.is_empty() && s.bullets.is_empty()) {
        bail!(tr!("Das Dokument hat keinen Inhalt.", "The document has no content."));
    }
    let mut docx = Docx::new().default_fonts(RunFonts::new().ascii(FONT).hi_ansi(FONT).cs(FONT)).default_size(BODY);
    docx = docx.add_paragraph(spaced(Paragraph::new().add_run(text_run(doc.title.trim(), TITLE).bold().color(ACCENT)), 0, 120));
    if !doc.subtitle.trim().is_empty() {
        docx = docx.add_paragraph(spaced(Paragraph::new().add_run(text_run(doc.subtitle.trim(), BODY + 2).italic().color("595959")), 0, 240));
    }
    let date = chrono::Local::now().format("%d.%m.%Y").to_string();
    docx = docx.add_paragraph(spaced(Paragraph::new().add_run(text_run(&tr!(format!("Erstellt von Milo am {date}"), format!("Created by Milo on {date}")), BODY - 4).color("7F7F7F")), 0, 360));

    for section in &doc.sections {
        if !section.heading.trim().is_empty() {
            docx = docx.add_paragraph(spaced(Paragraph::new().keep_next(true).add_run(text_run(section.heading.trim(), HEADING).bold().color(ACCENT)), 280, 100));
        }
        for paragraph in section.paragraphs.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
            docx = docx.add_paragraph(spaced(Paragraph::new().align(AlignmentType::Both).add_run(text_run(paragraph, BODY)), 0, 140));
        }
        for bullet in section.bullets.iter().map(|b| b.trim()).filter(|b| !b.is_empty()) {
            docx = docx.add_paragraph(spaced(
                Paragraph::new().indent(Some(500), Some(SpecialIndentType::Hanging(280)), None, None).add_run(text_run(&format!("•\t{bullet}"), BODY)),
                0,
                60,
            ));
        }
    }

    let sources: Vec<&SourceRef> = doc.sources.iter().filter(|s| !s.url.trim().is_empty() || !s.title.trim().is_empty()).collect();
    if !sources.is_empty() {
        docx = docx.add_paragraph(spaced(Paragraph::new().keep_next(true).add_run(text_run("Quellen", HEADING).bold().color(ACCENT)), 360, 100));
        for (i, source) in sources.iter().enumerate() {
            let line = match (source.title.trim(), source.url.trim()) {
                (t, u) if !t.is_empty() && !u.is_empty() => format!("[{}] {t}: {u}", i + 1),
                (t, "") => format!("[{}] {t}", i + 1),
                (_, u) => format!("[{}] {u}", i + 1),
            };
            docx = docx.add_paragraph(spaced(Paragraph::new().add_run(text_run(&line, BODY - 2).color("404040")), 0, 60));
        }
    }

    let mut bytes = std::io::Cursor::new(Vec::new());
    docx.build().pack(&mut bytes)?;
    Ok(bytes.into_inner())
}

/// Dateiname aus dem Titel: nur erlaubte Zeichen, nicht zu lang.
pub fn file_name(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_.,()äöüÄÖÜß".contains(c) { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|w| !w.chars().all(|c| c == '.'))
        .collect::<Vec<_>>()
        .join(" ");
    let short: String = clean.trim_matches('.').chars().take(70).collect();
    if short.trim().is_empty() { tr!("Bericht", "Report").into() } else { short.trim().to_string() }
}

/// Speichert das Dokument in `dir`; gibt es den Namen schon, wird durchnummeriert.
pub fn save(doc: &Document, dir: &Path) -> Result<PathBuf> {
    let bytes = build(doc)?;
    std::fs::create_dir_all(dir)?;
    let base = file_name(&doc.title);
    let mut path = dir.join(format!("{base}.docx"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base} ({n}).docx"));
        n += 1;
    }
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        Document {
            title: "Deutschland: ein Überblick".into(),
            subtitle: "Geschichte, Politik, Wirtschaft".into(),
            sections: vec![Section {
                heading: "Geografie".into(),
                paragraphs: vec!["Deutschland liegt in Mitteleuropa.".into()],
                bullets: vec!["16 Bundesländer".into()],
            }],
            sources: vec![SourceRef { title: "Wikipedia".into(), url: "https://de.wikipedia.org/wiki/Deutschland".into() }],
        }
    }

    #[test]
    fn builds_a_real_docx_with_the_text() {
        let bytes = build(&sample()).unwrap();
        // Eine .docx ist ein ZIP-Archiv mit word/document.xml.
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").unwrap(), &mut xml).unwrap();
        assert!(xml.contains("Deutschland liegt in Mitteleuropa."));
        assert!(xml.contains("Quellen"));
        assert!(xml.contains("16 Bundesländer"));
    }

    #[test]
    fn empty_documents_are_refused() {
        assert!(build(&Document { title: "".into(), ..sample() }).is_err());
        assert!(build(&Document { sections: vec![Section { heading: "Nur Titel".into(), ..Section::default() }], ..sample() }).is_err());
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("Deutschland: ein Überblick"), "Deutschland ein Überblick");
        assert_eq!(file_name("../../etc/passwd"), "etc passwd");
        assert_eq!(file_name("???"), tr!("Bericht", "Report"));
    }

    #[test]
    fn saving_numbers_duplicates() {
        let dir = std::env::temp_dir().join(format!("milo-docs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = save(&sample(), &dir).unwrap();
        let b = save(&sample(), &dir).unwrap();
        assert_ne!(a, b);
        assert!(b.file_name().unwrap().to_string_lossy().contains("(2)"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
