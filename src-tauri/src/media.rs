//! Medienanzeige: was gerade laeuft (Spotify, Browser, Player ...) und Steuerung dafuer.
//!
//! Benutzt die Windows-Schnittstelle, ueber die auch die Medientasten und die Lautstaerke-Einblendung
//! arbeiten (System Media Transport Controls). Es wird nur gelesen, was das abspielende Programm
//! selbst dort anmeldet; in kein Programm wird eingegriffen.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager_,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

pub const EVENT: &str = "milo://media";

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    /// Album und Untertitel, soweit das Programm sie liefert.
    pub album: String,
    pub subtitle: String,
    /// Lesbarer Name des abspielenden Programms, soweit erkennbar.
    pub app: String,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    /// Zeitpunkt (ms seit 1970), zu dem die Position gemessen wurde; die Oberflaeche zaehlt von dort weiter.
    pub at_ms: i64,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    /// Titelbild als data:-Adresse; leer, wenn das Programm keines liefert.
    pub cover: String,
}

#[derive(Default)]
pub struct Media {
    /// Angehalten (Spielemodus): nichts abfragen.
    pub suspended: AtomicBool,
    last: std::sync::Mutex<Option<NowPlaying>>,
}

impl Media {
    /// Was zuletzt gemeldet wurde (Titel, Interpret, Programm, laeuft oder pausiert).
    pub fn current(&self) -> Option<NowPlaying> {
        self.last.lock().unwrap().clone()
    }
}

/// Macht aus der Programmkennung einen lesbaren Namen ("Spotify.exe" -> "Spotify").
pub fn app_name(id: &str) -> String {
    let lower = id.to_lowercase();
    for (needle, name) in [
        ("spotify", "Spotify"), ("brave", "Brave"), ("chrome", "Chrome"), ("msedge", "Edge"), ("firefox", "Firefox"),
        ("opera", "Opera"), ("vlc", "VLC"), ("zunemusic", tr!("Medienwiedergabe", "Media Player")), ("zunevideo", tr!("Filme & TV", "Movies & TV")),
        ("applemusic", "Apple Music"), ("itunes", "iTunes"), ("discord", "Discord"), ("mpv", "mpv"), ("foobar", "foobar2000"),
        ("tidal", "TIDAL"), ("deezer", "Deezer"), ("amazon", "Amazon Music"), ("netflix", "Netflix"), ("disney", "Disney+"),
    ] {
        if lower.contains(needle) {
            return name.to_string();
        }
    }
    let base = id.rsplit(['\\', '/', '!']).next().unwrap_or(id);
    base.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()
}

fn manager() -> windows::core::Result<Manager_> {
    Manager_::RequestAsync()?.join()
}

fn current() -> windows::core::Result<Option<Session>> {
    Ok(manager()?.GetCurrentSession().ok())
}

fn ms(span: windows::Foundation::TimeSpan) -> i64 {
    span.Duration / 10_000
}

fn cover_of(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<String> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    // Ueber 3 MB waere kein Vorschaubild mehr.
    if size == 0 || size > 3_000_000 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    let kind = stream.ContentType().map(|s| s.to_string()).ok().filter(|k| k.starts_with("image/")).unwrap_or_else(|| "image/jpeg".into());
    Some(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Liest den aktuellen Stand. `previous` erspart das erneute Laden des Titelbilds beim selben Titel.
fn read(manager: &Manager_, previous: Option<&NowPlaying>) -> Option<NowPlaying> {
    let session = manager.GetCurrentSession().ok()?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
    if title.trim().is_empty() {
        return None;
    }
    let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
    let subtitle = props.Subtitle().map(|s| s.to_string()).unwrap_or_default();
    let info = session.GetPlaybackInfo().ok()?;
    let controls = info.Controls().ok();
    let timeline = session.GetTimelineProperties().ok();
    let app = session.SourceAppUserModelId().map(|s| app_name(&s.to_string())).unwrap_or_default();
    // Das Titelbild nur beim Titelwechsel holen; hat ein Titel keins, wird nicht jede Sekunde neu gefragt.
    let same = previous.is_some_and(|p| p.title == title && p.artist == artist);
    let cover = if same { previous.map(|p| p.cover.clone()).unwrap_or_default() } else { cover_of(&props).unwrap_or_default() };
    let (position_ms, duration_ms, at_ms) = timeline
        .as_ref()
        .map(|t| {
            let start = t.StartTime().map(ms).unwrap_or(0);
            let end = t.EndTime().map(ms).unwrap_or(0);
            let position = t.Position().map(ms).unwrap_or(0);
            // LastUpdatedTime zaehlt in 100-ns-Schritten seit 1601.
            let at = t.LastUpdatedTime().map(|d| d.UniversalTime / 10_000 - 11_644_473_600_000).unwrap_or(0);
            (position - start, end - start, at)
        })
        .unwrap_or((0, 0, 0));
    Some(NowPlaying {
        title,
        artist,
        album,
        subtitle,
        app,
        playing: info.PlaybackStatus().ok() == Some(Status::Playing),
        position_ms: position_ms.max(0),
        duration_ms: duration_ms.max(0),
        at_ms,
        can_next: controls.as_ref().and_then(|c| c.IsNextEnabled().ok()).unwrap_or(false),
        can_previous: controls.as_ref().and_then(|c| c.IsPreviousEnabled().ok()).unwrap_or(false),
        can_seek: controls.as_ref().and_then(|c| c.IsPlaybackPositionEnabled().ok()).unwrap_or(false),
        cover,
    })
}

/// Fragt einmal pro Sekunde ab, was laeuft, und meldet Aenderungen an die Oberflaeche.
pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        // Die Verbindung zu Windows wird einmal aufgebaut und dann wiederverwendet.
        let mut manager: Option<Manager_> = None;
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            let state = app.state::<Media>();
            if state.suspended.load(Ordering::Relaxed) {
                continue;
            }
            if manager.is_none() {
                manager = self::manager().ok();
            }
            let Some(manager) = manager.as_ref() else { continue };
            let previous = state.last.lock().unwrap().clone();
            let now = read(manager, previous.as_ref());
            if now != previous {
                *state.last.lock().unwrap() = now.clone();
                let _ = app.emit(EVENT, &now);
            }
        }
    });
}

/// Spult um `delta_ms` vor (positiv) oder zurueck (negativ). Liefert die neue Position in ms.
pub fn seek_by(delta_ms: i64) -> Result<i64, String> {
    let session = current().map_err(|e| e.message().to_string())?.ok_or(tr!("Gerade läuft nichts.", "Nothing is playing right now."))?;
    let controls = session.GetPlaybackInfo().and_then(|i| i.Controls()).map_err(|e| e.message().to_string())?;
    if !controls.IsPlaybackPositionEnabled().unwrap_or(false) {
        return Err(tr!("Das abspielende Programm erlaubt kein Spulen.", "The playing app doesn't allow seeking.").into());
    }
    let timeline = session.GetTimelineProperties().map_err(|e| e.message().to_string())?;
    let start = timeline.StartTime().map(ms).unwrap_or(0);
    let end = timeline.EndTime().map(ms).unwrap_or(0);
    let mut position = timeline.Position().map(ms).unwrap_or(0);
    // Die gemeldete Position ist von der letzten Aktualisierung; laeuft es, ist seitdem Zeit vergangen.
    let playing = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).ok() == Some(Status::Playing);
    if playing {
        if let Ok(updated) = timeline.LastUpdatedTime() {
            let at = updated.UniversalTime / 10_000 - 11_644_473_600_000;
            let now = chrono::Utc::now().timestamp_millis();
            position += (now - at).clamp(0, 6 * 3_600_000);
        }
    }
    let target = (position + delta_ms).clamp(start, if end > start { end } else { i64::MAX });
    let ok = session.TryChangePlaybackPositionAsync(target * 10_000).and_then(|op| op.join()).map_err(|e| e.message().to_string())?;
    if !ok {
        return Err(tr!("Das Programm hat das Spulen nicht angenommen.", "The app didn't accept the seek.").into());
    }
    Ok(target - start)
}

#[tauri::command]
pub fn now_playing(media: tauri::State<'_, Media>) -> Option<NowPlaying> {
    media.last.lock().unwrap().clone()
}

/// Steuert die laufende Wiedergabe: toggle, next, previous oder seek (mit Position in ms).
#[tauri::command]
pub async fn media_command(action: String, position_ms: Option<i64>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = current().map_err(|e| e.message().to_string())?.ok_or(tr!("Gerade läuft nichts.", "Nothing is playing right now."))?;
        let done = match action.as_str() {
            "toggle" => session.TryTogglePlayPauseAsync().and_then(|op| op.join()),
            "next" => session.TrySkipNextAsync().and_then(|op| op.join()),
            "previous" => session.TrySkipPreviousAsync().and_then(|op| op.join()),
            // Relativ spulen: position_ms ist hier die Verschiebung (negativ = zurueck).
            "skip" => return seek_by(position_ms.unwrap_or(0)).map(|_| ()),
            "seek" => {
                let start = session.GetTimelineProperties().and_then(|t| t.StartTime()).map(|s| s.Duration).unwrap_or(0);
                session.TryChangePlaybackPositionAsync(start + position_ms.unwrap_or(0).max(0) * 10_000).and_then(|op| op.join())
            }
            _ => return Err(tr!("Unbekannter Befehl.", "Unknown command.").to_string()),
        }
        .map_err(|e| e.message().to_string())?;
        if done { Ok(()) } else { Err(tr!("Das Programm hat den Befehl nicht angenommen.", "The app didn't accept the command.").to_string()) }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::app_name;

    #[test]
    fn app_ids_become_readable_names() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(app_name("Brave"), "Brave");
        assert_eq!(app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), tr!("Medienwiedergabe", "Media Player"));
        assert_eq!(app_name(r"C:\Programme\Etwas\player.exe"), "player");
    }
}
