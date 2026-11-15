//! YouTube: ein passendes Video finden und direkt oeffnen ("spiel das erste Video von Kanal X zu Y").
//! Gelesen wird die normale Suchseite; gesendet wird nur die Suchanfrage.

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: String,
}

/// Sammelt alle Videos aus den Daten der Suchseite, in ihrer Reihenfolge.
pub fn videos_in(data: &Value, out: &mut Vec<Video>) {
    match data {
        Value::Object(map) => {
            if let Some(video) = map.get("videoRenderer") {
                let id = video["videoId"].as_str().unwrap_or_default().to_string();
                let title = video["title"]["runs"][0]["text"].as_str().unwrap_or_default().to_string();
                let channel = video["ownerText"]["runs"][0]["text"].as_str().unwrap_or_default().to_string();
                if !id.is_empty() {
                    out.push(Video { id, title, channel });
                }
                return;
            }
            map.values().for_each(|v| videos_in(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| videos_in(v, out)),
        _ => {}
    }
}

fn simple(text: &str) -> String {
    text.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Waehlt das erste Video; ist ein Kanal genannt, das erste von diesem Kanal.
pub fn pick<'a>(videos: &'a [Video], channel: &str) -> Option<&'a Video> {
    let wanted = simple(channel);
    if wanted.is_empty() {
        return videos.first();
    }
    videos.iter().find(|v| simple(&v.channel) == wanted).or_else(|| videos.iter().find(|v| simple(&v.channel).contains(&wanted)))
}

/// Holt die Suchergebnisse von YouTube.
pub async fn search(query: &str) -> Result<Vec<Video>> {
    let http = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36")
        .build()?;
    let page = http
        .get("https://www.youtube.com/results")
        .query(&[("search_query", query), ("hl", tr!("de", "en")), ("gl", tr!("DE", "US"))])
        // Ohne diese Angabe leitet YouTube in der EU auf die Einwilligungsseite um (nur die notwendige Auswahl).
        .header(reqwest::header::COOKIE, "SOCS=CAI")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let start = page.find("var ytInitialData = ").ok_or_else(|| anyhow!("YouTube hat keine Suchergebnisse geliefert."))? + 20;
    let rest = &page[start..];
    let end = rest.find(";</script>").ok_or_else(|| anyhow!("Die YouTube-Seite sieht anders aus als erwartet."))?;
    let data: Value = serde_json::from_str(&rest[..end])?;
    let mut videos = Vec::new();
    videos_in(&data, &mut videos);
    if videos.is_empty() {
        bail!("Auf YouTube habe ich dazu kein Video gefunden.");
    }
    Ok(videos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn video(id: &str, title: &str, channel: &str) -> Value {
        json!({ "videoRenderer": { "videoId": id, "title": { "runs": [{ "text": title }] }, "ownerText": { "runs": [{ "text": channel }] } } })
    }

    #[test]
    fn finds_videos_and_picks_by_channel() {
        let data = json!({ "contents": { "sectionListRenderer": { "contents": [{ "itemSectionRenderer": { "contents": [
            { "adSlotRenderer": {} },
            video("a1", "Lofi zum Lernen", "Chillhop Music"),
            video("b2", "Lofi Mix 2026", "Lofi Girl"),
        ]}}]}}});
        let mut found = Vec::new();
        videos_in(&data, &mut found);
        assert_eq!(found.len(), 2);
        assert_eq!(pick(&found, "").unwrap().id, "a1");
        assert_eq!(pick(&found, "lofi girl").unwrap().id, "b2");
        assert_eq!(pick(&found, "Lofi-Girl").unwrap().id, "b2");
        assert!(pick(&found, "Gibt es nicht").is_none());
    }
}

#[cfg(test)]
mod live {
    /// Fragt wirklich bei YouTube an; nur von Hand: cargo test youtube_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn youtube_live() {
        let videos = super::search("lofi hip hop radio lofi girl").await.unwrap();
        println!("{} Videos, erstes: {:?}", videos.len(), videos.first());
        println!("vom Kanal Lofi Girl: {:?}", super::pick(&videos, "Lofi Girl"));
        assert!(!videos.is_empty());
    }
}
