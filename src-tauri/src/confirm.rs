//! Rueckfragen vor riskanten Aktionen. Ein Werkzeug stellt die Frage, die Oberflaeche zeigt sie
//! an, und erst die Antwort des Nutzers laesst das Werkzeug weiterlaufen.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

pub const EVENT: &str = "milo://confirm";
/// Die Frage mit dieser Nummer ist erledigt (beantwortet, abgelaufen oder abgebrochen).
pub const DONE_EVENT: &str = "milo://confirm-done";
/// Ohne Antwort gilt die Frage nach dieser Zeit als abgelehnt.
const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: u64,
    pub title: String,
    /// Was genau passieren wuerde, z. B. der Skripttext oder der Name des Fensters.
    pub detail: String,
    /// Beschriftung des Zustimmen-Knopfs.
    pub action: String,
    pub danger: bool,
}

#[derive(Default)]
pub struct Confirmations {
    next: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
}

/// Fragt den Nutzer und wartet auf die Antwort. Liefert nur bei ausdruecklicher Zustimmung `true`.
pub async fn ask(app: &AppHandle, title: &str, detail: &str, action: &str, danger: bool) -> bool {
    let state = app.state::<Confirmations>();
    let id = state.next.fetch_add(1, Ordering::SeqCst) + 1;
    let (tx, rx) = oneshot::channel();
    state.pending.lock().unwrap().insert(id, tx);
    let question = Question { id, title: title.into(), detail: detail.into(), action: action.into(), danger };
    if app.emit(EVENT, &question).is_err() {
        state.pending.lock().unwrap().remove(&id);
        return false;
    }
    let approved = matches!(tokio::time::timeout(TIMEOUT, rx).await, Ok(Ok(true)));
    state.pending.lock().unwrap().remove(&id);
    let _ = app.emit(DONE_EVENT, id);
    approved
}

/// Lehnt alle offenen Fragen ab, z. B. wenn der Nutzer die laufende Aufgabe abbricht.
pub fn cancel_all(app: &AppHandle) {
    let pending: Vec<_> = app.state::<Confirmations>().pending.lock().unwrap().drain().collect();
    for (_, tx) in pending {
        let _ = tx.send(false);
    }
}

#[tauri::command]
pub fn answer_confirmation(state: State<'_, Confirmations>, id: u64, approve: bool) {
    if let Some(tx) = state.pending.lock().unwrap().remove(&id) {
        let _ = tx.send(approve);
    }
}
