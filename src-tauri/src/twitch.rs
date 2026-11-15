//! Ist ein Twitch-Kanal gerade live? Milo liest dafuer die normale Kanalseite: Twitch legt dort fuer
//! Suchmaschinen strukturierte Daten ab (schema.org, `application/ld+json`) mit Live-Status und
//! Streamtitel. Kein Konto, kein Schluessel; hinaus geht nur der Kanalname.

use std::time::Duration;

use anyhow::Result;
use scraper::{Html, Selector};
use serde::Serialize;
use serde_json::Value;

const BROWSER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Channel {
    /// Kanalname in der Adresse (twitch.tv/<login>).
    pub login: String,
    pub display: String,
    pub exists: bool,
    pub live: bool,
    /// Titel des laufenden Streams.
    pub title: String,
    /// Beginn des Streams (ISO-Zeit), falls live.
    pub started: String,
    pub followers: Option<u64>,
}

impl Channel {
    pub fn url(&self) -> String {
        format!("https://www.twitch.tv/{}", self.login)
    }

    /// Eine Zeile fuer das Modell.
    pub fn describe(&self) -> String {
        if !self.exists {
            return tr!(format!("{}: Diesen Kanal gibt es nicht (oder Twitch hat keine Daten geliefert).", self.login), format!("{}: This channel doesn't exist (or Twitch returned no data).", self.login));
        }
        let followers = self.followers.map(|f| tr!(format!(", {} Follower", group(f)), format!(", {} followers", group(f)))).unwrap_or_default();
        if self.live {
            let since = since_text(&self.started);
            tr!(format!("{} ({}): LIVE{since} – „{}“{followers}. Adresse: {}", self.display, self.login, self.title, self.url()), format!("{} ({}): LIVE{since} – \"{}\"{followers}. Address: {}", self.display, self.login, self.title, self.url()))
        } else {
            tr!(format!("{} ({}): gerade offline{followers}. Adresse: {}", self.display, self.login, self.url()), format!("{} ({}): offline right now{followers}. Address: {}", self.display, self.login, self.url()))
        }
    }
}

fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

fn since_text(started: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(started)
        .map(|t| tr!(format!(" seit {} Uhr", t.with_timezone(&chrono::Local).format("%H:%M")), format!(" since {}", t.with_timezone(&chrono::Local).format("%H:%M"))))
        .unwrap_or_default()
}

/// Macht aus „twitch.tv/Valorant“, „@valorant“ oder „VALORANT“ den Kanalnamen.
pub fn login_of(input: &str) -> Option<String> {
    let mut text = input.trim().to_lowercase();
    if let Some(pos) = text.find("twitch.tv/") {
        text = text[pos + "twitch.tv/".len()..].to_string();
    }
    let login: String = text.trim_start_matches('@').split(['/', '?', '#']).next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
    let valid = (2..=25).contains(&login.len()) && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(login)
}

/// Liest Live-Status und Titel aus der Kanalseite.
pub fn parse_page(login: &str, html: &str) -> Channel {
    let mut channel = Channel { login: login.to_string(), display: login.to_string(), ..Default::default() };
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse(r#"script[type="application/ld+json"]"#) else { return channel };
    for script in document.select(&selector) {
        let Ok(data) = serde_json::from_str::<Value>(&script.inner_html()) else { continue };
        let graph = data["@graph"].as_array().cloned().unwrap_or_else(|| vec![data.clone()]);
        for item in &graph {
            match item["@type"].as_str() {
                Some("ProfilePage") => {
                    channel.exists = true;
                    let person = &item["mainEntity"];
                    if let Some(name) = person["name"].as_str() {
                        channel.display = name.to_string();
                    }
                    channel.followers = person["interactionStatistic"]["userInteractionCount"].as_u64();
                }
                // Der laufende Stream steht als eigenes VideoObject mit Sendung (publication) im Graphen;
                // aufgezeichnete Videos stecken dagegen in einer ItemList.
                Some("VideoObject") if item["publication"]["isLiveBroadcast"].as_bool() == Some(true) => {
                    channel.exists = true;
                    channel.live = true;
                    channel.title = item["description"].as_str().unwrap_or_default().trim().to_string();
                    channel.started = item["publication"]["startDate"].as_str().unwrap_or_default().to_string();
                    if let Some(name) = item["name"].as_str().and_then(|n| n.strip_suffix(" - Twitch")) {
                        channel.display = name.to_string();
                    }
                }
                Some("BreadcrumbList") => channel.exists = true,
                _ => {}
            }
        }
    }
    channel
}

/// Prueft mehrere Kanaele gleichzeitig.
pub async fn check(channels: &[String]) -> Result<Vec<Channel>> {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .user_agent(BROWSER_AGENT)
        .build()?;
    let names: Vec<(String, Option<String>)> = channels.iter().take(8).map(|c| (c.trim().to_string(), login_of(c))).collect();
    let logins: Vec<String> = names.iter().filter_map(|(_, login)| login.clone()).collect();
    let pages = futures_util::future::join_all(logins.iter().map(|login| {
        let http = http.clone();
        let url = format!("https://www.twitch.tv/{login}");
        async move { http.get(url).header("Accept-Language", "de-DE,de;q=0.9,en;q=0.8").send().await?.text().await }
    }))
    .await;
    let mut fetched = logins.iter().zip(pages);
    let mut found = Vec::new();
    for (raw, login) in names {
        match login {
            Some(_) => {
                let (login, page) = fetched.next().expect("je Name eine Seite");
                found.push(parse_page(login, &page?));
            }
            // Kein gueltiger Kanalname (4 bis 25 Zeichen, Buchstaben, Ziffern, Unterstrich).
            None => found.push(Channel { login: raw.clone(), display: raw, ..Default::default() }),
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(json: &str) -> String {
        format!(r#"<html><head><title>Twitch</title><script type="application/ld+json">{json}</script></head><body></body></html>"#)
    }

    const LIVE: &str = r#"{"@context":"http://schema.org","@graph":[{"@type":"ProfilePage","name":"VALORANT - Twitch","mainEntity":{"@type":"Person","name":"VALORANT","alternateName":"valorant","interactionStatistic":{"@type":"InteractionCounter","userInteractionCount":4920436}}},{"@type":"BreadcrumbList","itemListElement":[]},{"@type":"ItemList","itemListElement":[{"@type":"VideoObject","name":"LOUD vs. VIT — Day 16","url":"https://www.twitch.tv/videos/1"}]},{"@type":"VideoObject","description":"100T vs. VIT — VALORANT Champions Shanghai — Lower Final","name":"VALORANT - Twitch","publication":{"@type":"BroadcastEvent","startDate":"2026-10-17T05:30:32Z","isLiveBroadcast":true}}]}"#;

    #[test]
    fn live_channel_with_title() {
        let channel = parse_page("valorant", &page(LIVE));
        assert!(channel.exists && channel.live);
        assert_eq!(channel.title, "100T vs. VIT — VALORANT Champions Shanghai — Lower Final");
        assert_eq!(channel.display, "VALORANT");
        assert_eq!(channel.followers, Some(4_920_436));
        let line = channel.describe();
        assert!(line.contains("LIVE") && line.contains("4.920.436 Follower") && line.contains("https://www.twitch.tv/valorant"), "{line}");
    }

    #[test]
    fn offline_channel_is_not_live_even_with_old_videos() {
        let json = r#"{"@graph":[{"@type":"BreadcrumbList","itemListElement":[{"name":"Ninja"}]},{"@type":"ItemList","itemListElement":[{"@type":"VideoObject","name":"WEEK 5 NFL RECAP"}]}]}"#;
        let channel = parse_page("ninja", &page(json));
        assert!(channel.exists);
        assert!(!channel.live);
        assert!(channel.describe().contains("offline"));
    }

    #[test]
    fn missing_channel() {
        let channel = parse_page("gibtsnicht", "<html><head><title>Twitch</title></head></html>");
        assert!(!channel.exists && !channel.live);
        assert!(channel.describe().contains("gibt es nicht"));
    }

    #[test]
    fn channel_names_from_spoken_or_typed_input() {
        assert_eq!(login_of("VALORANT").as_deref(), Some("valorant"));
        assert_eq!(login_of("https://www.twitch.tv/valorant_emea?x=1").as_deref(), Some("valorant_emea"));
        assert_eq!(login_of("@riotgames").as_deref(), Some("riotgames"));
        assert_eq!(login_of("Valorant Champions").as_deref(), Some("valorantchampions"));
        assert_eq!(login_of("x"), None);
        assert_eq!(login_of("böse<script>"), None);
    }

    #[test]
    #[ignore = "braucht Internet"]
    fn real_twitch_page() {
        let found = tauri::async_runtime::block_on(check(&["valorant".into(), "gibtsnicht_xyz_987".into(), "viel zu langer kanalname mit leerzeichen".into()])).unwrap();
        for channel in &found {
            println!("{}", channel.describe());
        }
        assert!(found[0].exists);
        assert!(!found[1].exists);
        assert!(!found[2].exists);
    }
}
