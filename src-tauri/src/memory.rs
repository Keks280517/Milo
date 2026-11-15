//! Gedaechtnis: Notizen, Timer und das Protokoll der Aktionen in einer lokalen SQLite-Datei.
//! Nichts davon verlaesst den PC.

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use chrono::Local;
use rusqlite::{params, Connection};
use serde::Serialize;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
    text, content='notes', content_rowid='id', tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO notes_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TABLE IF NOT EXISTS actions (
    id INTEGER PRIMARY KEY,
    at TEXT NOT NULL,
    tool TEXT NOT NULL,
    summary TEXT NOT NULL,
    ok INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    kind TEXT NOT NULL,
    added_at TEXT NOT NULL,
    text TEXT NOT NULL,
    note TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS timers (
    id INTEGER PRIMARY KEY,
    label TEXT NOT NULL,
    due_ms INTEGER NOT NULL,
    total_ms INTEGER NOT NULL
);
";

/// Fuellwoerter, die bei der Suche nichts beitragen und sonst fast jede Notiz treffen wuerden.
const STOPWORDS: &[&str] = &[
    "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer", "und", "oder", "ist", "sind",
    "war", "was", "wie", "wer", "wo", "wann", "mein", "meine", "meinen", "meinem", "meiner", "ich", "du", "mir", "mich",
    "von", "vom", "zu", "zum", "zur", "in", "im", "an", "am", "auf", "für", "fur", "mit", "hat", "habe", "noch", "nach",
    "über", "uber", "bei", "aus", "dass", "es", "nicht", "auch", "heute", "steht", "notiz", "notizen", "gemerkt",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub text: String,
    pub created_at: String,
    pub updated_at: String,
    /// Formatierter Inhalt (bereinigtes HTML aus dem Editor); leer bei reinen Textnotizen.
    pub rich: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: i64,
    pub at: String,
    pub tool: String,
    pub summary: String,
    pub ok: bool,
    /// Gekuerzte Angaben, mit denen das Werkzeug aufgerufen wurde.
    pub detail: String,
    pub ms: i64,
    /// Laesst sich rueckgaengig machen (und ist es noch nicht).
    pub undoable: bool,
    pub undone: bool,
}

/// So lange bleibt das Protokoll erhalten.
const KEEP_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredTimer {
    pub id: i64,
    pub label: String,
    /// Faelligkeit als Unix-Zeit in Millisekunden.
    pub due_ms: i64,
    pub total_ms: i64,
}

pub struct Memory {
    conn: Mutex<Connection>,
}

fn now() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}

impl Memory {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        // Spalten, die das Protokoll spaeter bekommen hat, in alten Datenbanken nachtragen.
        let columns: Vec<String> = conn.prepare("SELECT name FROM pragma_table_info('actions')")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        for (name, kind) in [("detail", "TEXT NOT NULL DEFAULT ''"), ("ms", "INTEGER NOT NULL DEFAULT 0"), ("undo", "TEXT"), ("undone", "INTEGER NOT NULL DEFAULT 0")] {
            if !columns.iter().any(|c| c == name) {
                conn.execute_batch(&format!("ALTER TABLE actions ADD COLUMN {name} {kind}"))?;
            }
        }
        // Formatierte Notizen: zweite Spalte neben dem reinen Text (der bleibt fuer Suche und Modell).
        let note_columns: Vec<String> = conn.prepare("SELECT name FROM pragma_table_info('notes')")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        if !note_columns.iter().any(|c| c == "rich") {
            conn.execute_batch("ALTER TABLE notes ADD COLUMN rich TEXT NOT NULL DEFAULT ''")?;
        }
        let oldest = (Local::now() - chrono::Duration::days(KEEP_DAYS)).format("%Y-%m-%d").to_string();
        conn.execute("DELETE FROM actions WHERE at < ?1", params![oldest])?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Zugriff auf die Datenbank fuer andere Module (Dateien).
    pub(crate) fn with<T>(&self, run: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        run(&self.conn.lock().unwrap())
    }

    // ---------- Notizen ----------

    pub fn add_note(&self, text: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let at = now();
        conn.execute("INSERT INTO notes (text, created_at, updated_at) VALUES (?1, ?2, ?2)", params![text.trim(), at])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_note(&self, id: i64, text: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        // Wer den Text direkt aendert (Modell, alte Ansicht), verwirft die Formatierung.
        Ok(conn.execute("UPDATE notes SET text = ?1, rich = '', updated_at = ?2 WHERE id = ?3", params![text.trim(), now(), id])? > 0)
    }

    /// Formatierte Notiz speichern: `text` ist der reine Text, `rich` das bereits bereinigte HTML.
    pub fn save_rich_note(&self, id: Option<i64>, text: &str, rich: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let at = now();
        match id {
            Some(id) => {
                conn.execute("UPDATE notes SET text = ?1, rich = ?2, updated_at = ?3 WHERE id = ?4", params![text.trim(), rich, at, id])?;
                Ok(id)
            }
            None => {
                conn.execute("INSERT INTO notes (text, rich, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)", params![text.trim(), rich, at])?;
                Ok(conn.last_insert_rowid())
            }
        }
    }

    pub fn delete_note(&self, id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM notes WHERE id = ?1", params![id])? > 0)
    }

    pub fn recent_notes(&self, limit: usize) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, text, created_at, updated_at, rich FROM notes ORDER BY updated_at DESC, id DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit as i64], row_to_note)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Volltextsuche; ohne brauchbare Suchwoerter kommen die neuesten Notizen zurueck.
    pub fn search_notes(&self, query: &str, limit: usize) -> Result<Vec<Note>> {
        let Some(fts) = fts_query(query) else {
            return self.recent_notes(limit);
        };
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT n.id, n.text, n.created_at, n.updated_at, n.rich FROM notes_fts f JOIN notes n ON n.id = f.rowid
             WHERE notes_fts MATCH ?1 ORDER BY bm25(notes_fts) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts, limit as i64], row_to_note)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---------- Protokoll ----------

    pub fn log_action(&self, tool: &str, summary: &str, ok: bool) {
        self.log_entry(tool, summary, ok, "", 0, None);
    }

    /// Ausfuehrlicher Eintrag: Angaben, Dauer und wie es sich rueckgaengig machen laesst.
    pub fn log_entry(&self, tool: &str, summary: &str, ok: bool, detail: &str, ms: i64, undo: Option<&str>) {
        let conn = self.conn.lock().unwrap();
        if let Err(err) = conn.execute(
            "INSERT INTO actions (at, tool, summary, ok, detail, ms, undo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![Local::now().format("%Y-%m-%d %H:%M:%S").to_string(), tool, summary, ok, detail, ms, undo],
        ) {
            log::warn!("Protokolleintrag fehlgeschlagen: {err}");
        }
    }

    pub fn recent_actions(&self, limit: usize) -> Result<Vec<Action>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, at, tool, summary, ok, detail, ms, undo IS NOT NULL AND undone = 0, undone FROM actions ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(Action {
                id: r.get(0)?,
                at: r.get(1)?,
                tool: r.get(2)?,
                summary: r.get(3)?,
                ok: r.get(4)?,
                detail: r.get(5)?,
                ms: r.get(6)?,
                undoable: r.get(7)?,
                undone: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Rueckgaengig-Angabe eines Eintrags, der noch nicht rueckgaengig gemacht wurde.
    pub fn undo_of(&self, id: i64) -> Result<Option<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT undo, summary FROM actions WHERE id = ?1 AND undone = 0 AND undo IS NOT NULL")?;
        let mut rows = stmt.query(params![id])?;
        Ok(match rows.next()? {
            Some(row) => Some((row.get(0)?, row.get(1)?)),
            None => None,
        })
    }

    pub fn mark_undone(&self, id: i64) -> Result<()> {
        self.conn.lock().unwrap().execute("UPDATE actions SET undone = 1 WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- Timer ----------

    pub fn add_timer(&self, label: &str, due_ms: i64, total_ms: i64) -> Result<StoredTimer> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO timers (label, due_ms, total_ms) VALUES (?1, ?2, ?3)", params![label, due_ms, total_ms])?;
        Ok(StoredTimer { id: conn.last_insert_rowid(), label: label.to_string(), due_ms, total_ms })
    }

    /// Setzt Faelligkeit und Gesamtdauer eines Timers neu (Verlaengern, Schlummern).
    pub fn set_timer(&self, id: i64, due_ms: i64, total_ms: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("UPDATE timers SET due_ms = ?1, total_ms = ?2 WHERE id = ?3", params![due_ms, total_ms, id])? > 0)
    }

    pub fn remove_timer(&self, id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM timers WHERE id = ?1", params![id])? > 0)
    }

    pub fn timers(&self) -> Result<Vec<StoredTimer>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, label, due_ms, total_ms FROM timers ORDER BY due_ms")?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredTimer { id: r.get(0)?, label: r.get(1)?, due_ms: r.get(2)?, total_ms: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn row_to_note(r: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note { id: r.get(0)?, text: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)?, rich: r.get(4)? })
}

/// Baut aus freier Eingabe eine FTS5-Abfrage: jedes sinnvolle Wort als Praefix, mit ODER verknuepft.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 2 && !STOPWORDS.contains(&w.as_str()))
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if words.is_empty() { None } else { Some(words.join(" OR ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_roundtrip_and_search() {
        let m = Memory::in_memory().unwrap();
        let wifi = m.add_note("Der WLAN-Schlüssel vom Büro ist Sonne123").unwrap();
        m.add_note("Freitag um 15 Uhr Zahnarzt").unwrap();
        m.add_note("Milch, Eier und Brot kaufen").unwrap();

        let hits = m.search_notes("Wie ist der WLAN Schlüssel vom Büro?", 5).unwrap();
        assert_eq!(hits[0].id, wifi);
        // Umlaute duerfen fehlen, Wortanfaenge reichen
        assert_eq!(m.search_notes("buro", 5).unwrap()[0].id, wifi);
        assert_eq!(m.search_notes("zahn", 5).unwrap().len(), 1);
        assert!(m.search_notes("fahrrad", 5).unwrap().is_empty());
    }

    #[test]
    fn empty_query_returns_recent_first() {
        let m = Memory::in_memory().unwrap();
        m.add_note("erste").unwrap();
        m.add_note("zweite").unwrap();
        let notes = m.search_notes("was steht in meinen notizen", 5).unwrap();
        assert_eq!(notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["zweite", "erste"]);
    }

    #[test]
    fn update_and_delete_keep_search_index_in_sync() {
        let m = Memory::in_memory().unwrap();
        let id = m.add_note("Vermieter heißt Herr Öztürk").unwrap();
        assert!(m.update_note(id, "Vermieterin heißt Frau Schmidt").unwrap());
        assert!(m.search_notes("Öztürk", 5).unwrap().is_empty());
        assert_eq!(m.search_notes("Schmidt", 5).unwrap().len(), 1);
        assert!(m.delete_note(id).unwrap());
        assert!(m.search_notes("Schmidt", 5).unwrap().is_empty());
        assert!(!m.delete_note(id).unwrap());
    }

    #[test]
    fn query_with_quotes_does_not_break_fts() {
        let m = Memory::in_memory().unwrap();
        m.add_note("Passwort Tresor").unwrap();
        assert!(m.search_notes("\"tresor\" OR (", 5).is_ok());
    }

    #[test]
    fn timers_and_actions_persist() {
        let m = Memory::in_memory().unwrap();
        let t = m.add_timer("Pizza", 2_000, 1_000).unwrap();
        m.add_timer("Wäsche", 1_000, 500).unwrap();
        assert_eq!(m.timers().unwrap().iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["Wäsche", "Pizza"]);
        assert!(m.remove_timer(t.id).unwrap());
        assert_eq!(m.timers().unwrap().len(), 1);

        m.log_action("open_app", "Spotify geöffnet", true);
        m.log_action("open_app", "Foo nicht gefunden", false);
        let actions = m.recent_actions(10).unwrap();
        assert_eq!(actions.len(), 2);
        assert!(!actions[0].ok);
        assert!(!actions[0].undoable);
        m.log_entry("set_volume", "Lautstärke 20 %", true, "percent: 20", 12, Some(r#"{"kind":"volume","percent":50}"#));
        let newest = &m.recent_actions(1).unwrap()[0];
        assert!(newest.undoable && newest.ms == 12);
        assert!(m.undo_of(newest.id).unwrap().is_some());
        m.mark_undone(newest.id).unwrap();
        assert!(m.undo_of(newest.id).unwrap().is_none());
        assert!(m.recent_actions(1).unwrap()[0].undone);
    }

    #[test]
    fn stopwords_only_gives_no_fts_query() {
        assert_eq!(fts_query("was ist in der"), None);
        assert_eq!(fts_query("WLAN-Schlüssel").unwrap(), "\"wlan\"* OR \"schlüssel\"*");
    }
}
