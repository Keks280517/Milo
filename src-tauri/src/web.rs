//! Internet-Recherche und Wetter.
//!
//! Recherche: Websuche ueber die HTML-Seite von DuckDuckGo (kein offizielles API, kann sich
//! aendern), danach werden die besten Treffer abgerufen und auf lesbaren Text reduziert.
//! Faellt die Suche aus, springt die Wikipedia-Suche ein.
//! Wetter: Open-Meteo (frei, ohne Schluessel).
//! Hinaus geht jeweils nur die Suchanfrage bzw. der Ortsname.

use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use scraper::{Html, Selector};
use serde_json::Value;

const BROWSER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";
/// Wikimedia und Open-Meteo verlangen eine erkennbare Kennung statt einer Browser-Tarnung.
const OWN_AGENT: &str = concat!("Milo/", env!("CARGO_PKG_VERSION"), " (persoenlicher Desktop-Assistent)");
const MAX_PAGE_BYTES: usize = 1_500_000;
const EXCERPT_CHARS: usize = 1400;
const PAGES: usize = 3;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub struct Research {
    /// Text fuer das Modell: Treffer mit Auszuegen.
    pub text: String,
    pub sources: Vec<Hit>,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?)
}

pub async fn research(query: &str) -> Result<Research> {
    let query = query.trim();
    if query.is_empty() {
        bail!(tr!("Die Suchanfrage ist leer.", "The search query is empty."));
    }
    let http = client()?;
    let mut hits = match search_duckduckgo(&http, query).await {
        Ok(hits) if !hits.is_empty() => hits,
        Ok(_) => Vec::new(),
        Err(err) => {
            log::warn!("Websuche fehlgeschlagen: {err}");
            Vec::new()
        }
    };
    if hits.is_empty() {
        hits = search_wikipedia(&http, query).await.unwrap_or_default();
    }
    if hits.is_empty() {
        bail!(tr!("Die Suche hat keine Treffer geliefert.", "The search returned no results."));
    }
    hits.truncate(5);

    // Die ersten Treffer parallel abrufen; wer nicht rechtzeitig antwortet, bleibt beim Kurztext.
    let pages = futures_util::future::join_all(hits.iter().take(PAGES).map(|hit| fetch_text(&http, &hit.url))).await;
    let mut text = String::new();
    for (i, hit) in hits.iter().enumerate() {
        text.push_str(&format!("[{}] {} ({})\n", i + 1, hit.title, hit.url));
        if !hit.snippet.is_empty() {
            text.push_str(&tr!(format!("Kurztext: {}\n", hit.snippet), format!("Snippet: {}\n", hit.snippet)));
        }
        if let Some(Ok(page)) = pages.get(i) {
            if page.chars().count() > 200 {
                text.push_str(&tr!(format!("Auszug: {}\n", excerpt(page, query, EXCERPT_CHARS)), format!("Excerpt: {}\n", excerpt(page, query, EXCERPT_CHARS))));
            }
        }
        text.push('\n');
    }
    Ok(Research { text: text.trim_end().to_string(), sources: hits })
}

/// Schnelle Suche, die nur Titel, Adresse und Kurztext liefert (ohne die Seiten zu lesen).
/// Damit findet das Modell die richtige Seite und oeffnet sie direkt.
pub async fn find_links(query: &str) -> Result<Vec<Hit>> {
    let query = query.trim();
    if query.is_empty() {
        bail!(tr!("Die Suchanfrage ist leer.", "The search query is empty."));
    }
    let http = client()?;
    let mut hits = search_duckduckgo(&http, query).await.unwrap_or_else(|err| {
        log::warn!("Websuche fehlgeschlagen: {err}");
        Vec::new()
    });
    if hits.is_empty() {
        hits = search_wikipedia(&http, query).await.unwrap_or_default();
    }
    if hits.is_empty() {
        bail!(tr!("Die Suche hat keine Treffer geliefert.", "The search returned no results."));
    }
    hits.truncate(8);
    Ok(hits)
}

/// Eine gelesene Quelle der gruendlichen Recherche.
pub struct Source {
    pub hit: Hit,
    pub text: String,
}

/// Gruendliche Recherche fuer Berichte: mehr Treffer, laengere Auszuege und, falls vorhanden,
/// der passende Wikipedia-Artikel als Ganzes (Einleitung und Abschnitte als Klartext).
pub async fn deep_research(query: &str) -> Result<Vec<Source>> {
    const DEEP_PAGES: usize = 5;
    const DEEP_EXCERPT: usize = 3500;
    const WIKI_CHARS: usize = 12_000;
    let query = query.trim();
    if query.is_empty() {
        bail!(tr!("Die Suchanfrage ist leer.", "The search query is empty."));
    }
    let http = client()?;
    let (web, wiki) = tokio::join!(search_duckduckgo(&http, query), wikipedia_article(&http, query, WIKI_CHARS));
    let mut hits = web.unwrap_or_else(|err| {
        log::warn!("Websuche fehlgeschlagen: {err}");
        Vec::new()
    });
    let mut sources: Vec<Source> = Vec::new();
    if let Ok(Some((hit, text))) = wiki {
        hits.retain(|h| h.url != hit.url);
        sources.push(Source { hit, text });
    }
    // Wikipedia-Treffer der Websuche sind schon abgedeckt; Videos und soziale Netze liefern kaum Text.
    hits.retain(|h| !["youtube.com", "facebook.com", "instagram.com", "tiktok.com", "x.com/"].iter().any(|bad| h.url.contains(bad)));
    hits.truncate(8);
    let pages = futures_util::future::join_all(hits.iter().take(DEEP_PAGES).map(|hit| fetch_text(&http, &hit.url))).await;
    for (hit, page) in hits.iter().zip(pages) {
        match page {
            Ok(page) if page.chars().count() > 300 => sources.push(Source { hit: hit.clone(), text: excerpt(&page, query, DEEP_EXCERPT) }),
            _ if !hit.snippet.is_empty() => sources.push(Source { hit: hit.clone(), text: hit.snippet.clone() }),
            _ => {}
        }
    }
    if sources.is_empty() {
        bail!(tr!("Die Recherche hat nichts Lesbares gefunden.", "The research found nothing readable."));
    }
    Ok(sources)
}

/// Waehlt unter den Wikipedia-Treffern den, dessen Titel am meisten mit der Anfrage gemeinsam hat
/// („Frankfurt am Main Geschichte“ -> „Frankfurt am Main“, nicht „Offenbach am Main“).
fn best_article(hits: Vec<Hit>, query: &str) -> Option<Hit> {
    let query = query.to_lowercase();
    let score = |hit: &Hit| -> (usize, usize) {
        let title = hit.title.trim_end_matches(" – Wikipedia").to_lowercase();
        let words: Vec<&str> = title.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).collect();
        let found = words.iter().filter(|w| query.contains(*w)).count();
        // Mehr Treffer zuerst; bei Gleichstand weniger fremde Woerter.
        (found, usize::MAX - (words.len() - found))
    };
    let mut best: Option<(Hit, (usize, usize))> = None;
    for hit in hits {
        let s = score(&hit);
        if best.as_ref().is_none_or(|(_, b)| s > *b) {
            best = Some((hit, s));
        }
    }
    best.filter(|(_, (found, _))| *found > 0).map(|(hit, _)| hit)
}

/// Der passendste deutsche Wikipedia-Artikel als Klartext (gekuerzt auf `max` Zeichen).
async fn wikipedia_article(http: &reqwest::Client, query: &str, max: usize) -> Result<Option<(Hit, String)>> {
    let Some(first) = best_article(search_wikipedia(http, query).await?, query) else {
        return Ok(None);
    };
    let title = first.title.trim_end_matches(" – Wikipedia").to_string();
    let body: Value = http
        .get(tr!("https://de.wikipedia.org/w/api.php", "https://en.wikipedia.org/w/api.php"))
        .query(&[("action", "query"), ("prop", "extracts"), ("explaintext", "1"), ("exsectionformat", "plain"), ("titles", title.as_str()), ("format", "json"), ("redirects", "1")])
        .header(reqwest::header::USER_AGENT, OWN_AGENT)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let extract = body["query"]["pages"].as_object().and_then(|pages| pages.values().next()).and_then(|p| p["extract"].as_str()).unwrap_or_default();
    if extract.chars().count() < 300 {
        return Ok(None);
    }
    Ok(Some((first, truncate(extract, max))))
}

async fn search_duckduckgo(http: &reqwest::Client, query: &str) -> Result<Vec<Hit>> {
    let body = http
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query), ("kl", "de-de")])
        .header(reqwest::header::USER_AGENT, BROWSER_AGENT)
        .header(reqwest::header::ACCEPT_LANGUAGE, tr!("de-DE,de;q=0.9", "en-US,en;q=0.9"))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_duckduckgo(&body))
}

fn parse_duckduckgo(html: &str) -> Vec<Hit> {
    let doc = Html::parse_document(html);
    let result = Selector::parse(".result").unwrap();
    let link = Selector::parse("a.result__a").unwrap();
    let snippet = Selector::parse(".result__snippet").unwrap();
    doc.select(&result)
        .filter(|r| !r.value().classes().any(|c| c == "result--ad"))
        .filter_map(|r| {
            let a = r.select(&link).next()?;
            let url = real_url(a.value().attr("href")?)?;
            Some(Hit {
                title: clean(&a.text().collect::<String>()),
                url,
                snippet: r.select(&snippet).next().map(|s| clean(&s.text().collect::<String>())).unwrap_or_default(),
            })
        })
        .filter(|h| !h.title.is_empty())
        .collect()
}

/// DuckDuckGo verlinkt ueber eine Weiterleitung; die echte Adresse steckt im Parameter `uddg`.
fn real_url(href: &str) -> Option<String> {
    let absolute = if href.starts_with("//") { format!("https:{href}") } else { href.to_string() };
    let url = url::Url::parse(&absolute).ok()?;
    let target = if url.host_str().is_some_and(|h| h.ends_with("duckduckgo.com")) && url.path().starts_with("/l/") {
        url.query_pairs().find(|(k, _)| k == "uddg").map(|(_, v)| v.into_owned())?
    } else {
        absolute
    };
    let parsed = url::Url::parse(&target).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.into())
}

async fn search_wikipedia(http: &reqwest::Client, query: &str) -> Result<Vec<Hit>> {
    let body: Value = http
        .get(tr!("https://de.wikipedia.org/w/api.php", "https://en.wikipedia.org/w/api.php"))
        .query(&[("action", "query"), ("list", "search"), ("srsearch", query), ("format", "json"), ("srlimit", "5")])
        .header(reqwest::header::USER_AGENT, OWN_AGENT)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(body["query"]["search"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let title = item["title"].as_str()?;
                    Some(Hit {
                        title: format!("{title} – Wikipedia"),
                        url: format!("https://{}.wikipedia.org/wiki/{}", tr!("de", "en"), title.replace(' ', "_")),
                        snippet: clean(&strip_tags(item["snippet"].as_str().unwrap_or_default())),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

async fn fetch_text(http: &reqwest::Client, url: &str) -> Result<String> {
    let response = http
        .get(url)
        .header(reqwest::header::USER_AGENT, BROWSER_AGENT)
        .header(reqwest::header::ACCEPT_LANGUAGE, tr!("de-DE,de;q=0.9,en;q=0.5", "en-US,en;q=0.9"))
        .timeout(Duration::from_secs(7))
        .send()
        .await?
        .error_for_status()?;
    let is_html = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|t| t.contains("html") || t.contains("text/plain"));
    if !is_html {
        bail!(tr!("Kein lesbarer Text", "No readable text"));
    }
    if response.content_length().is_some_and(|len| len as usize > MAX_PAGE_BYTES) {
        bail!(tr!("Seite zu groß", "Page too large"));
    }
    let body = response.text().await?;
    if body.len() > MAX_PAGE_BYTES {
        bail!(tr!("Seite zu groß", "Page too large"));
    }
    Ok(readable_text(&body))
}

/// Holt den lesbaren Fliesstext aus einer HTML-Seite: Absaetze, Ueberschriften und Listenpunkte
/// aus dem Hauptbereich, ohne Navigation, Skripte und Fusszeile.
pub fn readable_text(html: &str) -> String {
    let doc = Html::parse_document(html);
    let content = Selector::parse("p, h1, h2, h3, li, td, dd, blockquote").unwrap();
    let skip = ["nav", "header", "footer", "aside", "script", "style", "noscript", "form", "button", "figure"];
    let root = ["article", "main", "[role=main]", "body"]
        .iter()
        .filter_map(|sel| Selector::parse(sel).ok())
        .filter_map(|sel| doc.select(&sel).next())
        .find(|el| el.text().map(str::len).sum::<usize>() > 400)
        .or_else(|| Selector::parse("body").ok().and_then(|sel| doc.select(&sel).next()));
    let Some(root) = root else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    for el in root.select(&content) {
        let hidden = el.ancestors().filter_map(scraper::ElementRef::wrap).any(|a| skip.contains(&a.value().name()));
        if hidden {
            continue;
        }
        let text = clean(&el.text().collect::<String>());
        // Sehr kurze Stuecke sind meist Menuepunkte; doppelte entstehen durch verschachtelte Listen.
        if text.chars().count() >= 30 && !parts.iter().any(|p| p.contains(&text)) {
            parts.push(text);
        }
    }
    parts.join("\n")
}

fn strip_tags(html: &str) -> String {
    Html::parse_fragment(html).root_element().text().collect()
}

fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Waehlt aus dem Seitentext die Absaetze, die am meisten mit der Suchanfrage zu tun haben,
/// und gibt sie in ihrer urspruenglichen Reihenfolge zurueck. Passt nichts, kommt der Seitenanfang.
fn excerpt(page: &str, query: &str, max_chars: usize) -> String {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 3)
        .collect();
    let parts: Vec<&str> = page.lines().collect();
    let mut scored: Vec<(usize, usize)> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let lower = part.to_lowercase();
            (words.iter().filter(|w| lower.contains(w.as_str())).count(), i)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    if scored.is_empty() {
        return truncate(page, max_chars);
    }
    // Beste Absaetze zuerst, bei Gleichstand die frueheren.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut chosen: Vec<usize> = Vec::new();
    let mut used = 0;
    for (_, i) in scored {
        let len = parts[i].chars().count();
        if used + len > max_chars && !chosen.is_empty() {
            continue;
        }
        chosen.push(i);
        used += len;
        if used >= max_chars {
            break;
        }
    }
    chosen.sort_unstable();
    truncate(&chosen.iter().map(|i| parts[*i]).collect::<Vec<_>>().join("\n"), max_chars)
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    // Am letzten Satz- oder Wortende abschneiden, damit kein halbes Wort stehen bleibt.
    let end = cut.rfind(['.', '!', '?', '\n']).filter(|i| *i > max_chars / 2).map(|i| i + 1).or_else(|| cut.rfind(' ')).unwrap_or(cut.len());
    format!("{} …", cut[..end].trim_end())
}

// ---------- Wetter ----------

/// Wetterdaten in fester Form: die Oberflaeche fuellt damit ihre Wetter-Vorlage, das Modell
/// bekommt denselben Inhalt als Text (`report`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub place: String,
    /// Bundesland und Land, zur Unterscheidung gleichnamiger Orte.
    pub region: String,
    pub now: WeatherNow,
    /// Die naechsten Stunden ab jetzt.
    pub hours: Vec<WeatherHour>,
    pub days: Vec<WeatherDay>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherNow {
    pub temp: f64,
    pub feels: f64,
    pub code: i64,
    pub text: String,
    pub wind: f64,
    pub humidity: f64,
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherHour {
    /// Stunde als Text, z. B. "15".
    pub hour: String,
    pub temp: f64,
    pub code: i64,
    /// Regenwahrscheinlichkeit in Prozent.
    pub rain: f64,
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherDay {
    pub date: String,
    /// tr!("Heute", "Today"), tr!("Morgen", "Tomorrow") oder der Wochentag.
    pub label: String,
    pub min: f64,
    pub max: f64,
    pub code: i64,
    pub text: String,
    pub rain: f64,
    pub wind: f64,
    pub sunrise: String,
    pub sunset: String,
}

const HOURS_SHOWN: usize = 12;

/// Wetter fuer einen Ort: jetzt, die naechsten Stunden und die naechsten Tage.
pub async fn weather(place: &str) -> Result<Weather> {
    let place = place.trim();
    if place.is_empty() {
        bail!(tr!("Für welchen Ort? Sag mir bitte eine Stadt.", "For which place? Please tell me a city."));
    }
    let http = client()?;
    let geo: Value = http
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name", place), ("count", "1"), ("language", tr!("de", "en")), ("format", "json")])
        .header(reqwest::header::USER_AGENT, OWN_AGENT)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let spot = geo["results"].get(0).ok_or_else(|| anyhow!("{}", tr!(format!("Den Ort „{place}“ kenne ich nicht. Vielleicht ist er falsch geschrieben (Spracherkennung)."), format!("I don't know the place \"{place}\". It may be misspelled (speech recognition)."))))?;
    let (lat, lon) = (spot["latitude"].as_f64().unwrap_or_default(), spot["longitude"].as_f64().unwrap_or_default());
    let forecast: Value = http
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("current", "temperature_2m,apparent_temperature,weather_code,wind_speed_10m,relative_humidity_2m,is_day".into()),
            ("hourly", "temperature_2m,weather_code,precipitation_probability,is_day".into()),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,wind_speed_10m_max,sunrise,sunset".into(),
            ),
            ("timezone", "auto".into()),
            ("forecast_days", "6".into()),
        ])
        .header(reqwest::header::USER_AGENT, OWN_AGENT)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let name = spot["name"].as_str().unwrap_or(place);
    let region = [spot["admin1"].as_str(), spot["country"].as_str()]
        .into_iter()
        .flatten()
        .filter(|part| *part != name)
        .collect::<Vec<_>>()
        .join(", ");
    Ok(parse_weather(name, &region, &forecast))
}

/// Regen in den naechsten zwei Stunden (Viertelstunden-Vorhersage). Liefert den Beginn in ms und
/// die Menge im ersten nassen Viertel; `None`, wenn es trocken bleibt oder schon regnet.
pub async fn rain_soon(place: &str) -> Result<Option<(i64, f64)>> {
    use std::sync::Mutex;
    static SPOTS: Mutex<Vec<(String, f64, f64)>> = Mutex::new(Vec::new());
    let place = place.trim();
    if place.is_empty() {
        return Ok(None);
    }
    let http = client()?;
    let cached = SPOTS.lock().unwrap().iter().find(|(p, ..)| p == place).map(|(_, a, b)| (*a, *b));
    let (lat, lon) = match cached {
        Some(spot) => spot,
        None => {
            let geo: Value = http
                .get("https://geocoding-api.open-meteo.com/v1/search")
                .query(&[("name", place), ("count", "1"), ("format", "json")])
                .header(reqwest::header::USER_AGENT, OWN_AGENT)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let spot = &geo["results"][0];
            let (Some(lat), Some(lon)) = (spot["latitude"].as_f64(), spot["longitude"].as_f64()) else { return Ok(None) };
            SPOTS.lock().unwrap().push((place.to_string(), lat, lon));
            (lat, lon)
        }
    };
    let data: Value = http
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("minutely_15", "precipitation".into()),
            ("forecast_minutely_15", "10".into()),
            ("timeformat", "unixtime".into()),
        ])
        .header(reqwest::header::USER_AGENT, OWN_AGENT)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let times = data["minutely_15"]["time"].as_array().cloned().unwrap_or_default();
    let rain = data["minutely_15"]["precipitation"].as_array().cloned().unwrap_or_default();
    let now = chrono::Local::now().timestamp();
    let slots: Vec<(i64, f64)> = times
        .iter()
        .zip(rain.iter())
        .filter_map(|(t, r)| Some((t.as_i64()?, r.as_f64().unwrap_or(0.0))))
        .filter(|(t, _)| *t + 15 * 60 > now)
        .collect();
    Ok(first_rain(&slots))
}

/// Erstes nasses Viertel, wenn es vorher trocken ist (sonst regnet es schon: keine Warnung).
fn first_rain(slots: &[(i64, f64)]) -> Option<(i64, f64)> {
    const WET: f64 = 0.1;
    if slots.first().is_some_and(|(_, mm)| *mm >= WET) {
        return None;
    }
    slots.iter().find(|(_, mm)| *mm >= WET).map(|(t, mm)| (t * 1000, *mm))
}

/// Uhrzeit "HH:MM" aus einem Zeitstempel wie "2026-10-13T07:42".
fn clock_part(stamp: &str) -> String {
    stamp.split('T').nth(1).unwrap_or_default().chars().take(5).collect()
}

fn weekday_name(date: &str) -> String {
    const DAYS_DE: [&str; 7] = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];
    const DAYS_EN: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    let days = tr!(DAYS_DE, DAYS_EN);
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| days[chrono::Datelike::weekday(&d).num_days_from_monday() as usize].to_string())
        .unwrap_or_else(|_| date.to_string())
}

fn parse_weather(place: &str, region: &str, data: &Value) -> Weather {
    let num = |v: &Value| v.as_f64().unwrap_or_default();
    let current = &data["current"];
    let now = WeatherNow {
        temp: num(&current["temperature_2m"]),
        feels: num(&current["apparent_temperature"]),
        code: current["weather_code"].as_i64().unwrap_or(-1),
        text: weather_text(current["weather_code"].as_i64().unwrap_or(-1)).to_string(),
        wind: num(&current["wind_speed_10m"]),
        humidity: num(&current["relative_humidity_2m"]),
        is_day: current["is_day"].as_i64().unwrap_or(1) == 1,
    };

    // Stundenwerte beginnen um Mitternacht; gezeigt wird ab der laufenden Stunde.
    let hourly = &data["hourly"];
    let this_hour: String = current["time"].as_str().unwrap_or_default().chars().take(13).collect();
    let times: Vec<&str> = hourly["time"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let first = times.iter().position(|t| !this_hour.is_empty() && t.starts_with(&this_hour)).unwrap_or(0);
    let hours = times
        .iter()
        .enumerate()
        .skip(first)
        .take(HOURS_SHOWN)
        .map(|(i, time)| WeatherHour {
            hour: clock_part(time).chars().take(2).collect(),
            temp: num(&hourly["temperature_2m"][i]),
            code: hourly["weather_code"][i].as_i64().unwrap_or(-1),
            rain: num(&hourly["precipitation_probability"][i]),
            is_day: hourly["is_day"][i].as_i64().unwrap_or(1) == 1,
        })
        .collect();

    let daily = &data["daily"];
    let days = daily["time"]
        .as_array()
        .map(|dates| {
            dates
                .iter()
                .filter_map(Value::as_str)
                .enumerate()
                .map(|(i, date)| WeatherDay {
                    date: date.to_string(),
                    label: match i {
                        0 => tr!("Heute", "Today").into(),
                        1 => tr!("Morgen", "Tomorrow").into(),
                        _ => weekday_name(date),
                    },
                    min: num(&daily["temperature_2m_min"][i]),
                    max: num(&daily["temperature_2m_max"][i]),
                    code: daily["weather_code"][i].as_i64().unwrap_or(-1),
                    text: weather_text(daily["weather_code"][i].as_i64().unwrap_or(-1)).to_string(),
                    rain: num(&daily["precipitation_probability_max"][i]),
                    wind: num(&daily["wind_speed_10m_max"][i]),
                    sunrise: clock_part(daily["sunrise"][i].as_str().unwrap_or_default()),
                    sunset: clock_part(daily["sunset"][i].as_str().unwrap_or_default()),
                })
                .collect()
        })
        .unwrap_or_default();

    Weather { place: place.to_string(), region: region.to_string(), now, hours, days }
}

impl Weather {
    /// Derselbe Inhalt als kurzer deutscher Text fuer das Modell.
    pub fn report(&self) -> String {
        let place = if self.region.is_empty() { self.place.clone() } else { format!("{}, {}", self.place, self.region) };
        let mut out = tr!(format!("Wetter für {place} (Quelle: Open-Meteo)\n"), format!("Weather for {place} (source: Open-Meteo)\n"));
        out.push_str(&tr!(format!("Jetzt: {:.0} °C (gefühlt {:.0} °C), {}, Wind {:.0} km/h, Luftfeuchte {:.0} %\n",
            self.now.temp, self.now.feels, self.now.text, self.now.wind, self.now.humidity
        ), format!("Now: {:.0} °C (feels like {:.0} °C), {}, wind {:.0} km/h, humidity {:.0} %\n",
            self.now.temp, self.now.feels, self.now.text, self.now.wind, self.now.humidity
        )));
        for day in self.days.iter().take(4) {
            out.push_str(&tr!(format!("{} ({}): {:.0} bis {:.0} °C, {}, Regenwahrscheinlichkeit {:.0} %, Wind bis {:.0} km/h\n",
                day.label, day.date, day.min, day.max, day.text, day.rain, day.wind
            ), format!("{} ({}): {:.0} to {:.0} °C, {}, chance of rain {:.0} %, wind up to {:.0} km/h\n",
                day.label, day.date, day.min, day.max, day.text, day.rain, day.wind
            )));
        }
        out.trim_end().to_string()
    }
}
/// WMO-Wettercodes, wie Open-Meteo sie liefert, als deutscher Text.
fn weather_text(code: i64) -> &'static str {
    match code {
        0 => "klar",
        1 => tr!("überwiegend klar", "mostly clear"),
        2 => tr!("teils bewölkt", "partly cloudy"),
        3 => "bedeckt",
        45 | 48 => tr!("Nebel", "fog"),
        51 | 53 | 55 => tr!("Nieselregen", "drizzle"),
        56 | 57 => tr!("gefrierender Nieselregen", "freezing drizzle"),
        61 => tr!("leichter Regen", "light rain"),
        63 => tr!("Regen", "rain"),
        65 => tr!("starker Regen", "heavy rain"),
        66 | 67 => tr!("gefrierender Regen", "freezing rain"),
        71 => tr!("leichter Schneefall", "light snow"),
        73 => tr!("Schneefall", "snow"),
        75 => tr!("starker Schneefall", "heavy snow"),
        77 => tr!("Schneegriesel", "snow grains"),
        80 | 81 => tr!("Regenschauer", "rain showers"),
        82 => tr!("heftige Regenschauer", "heavy rain showers"),
        85 | 86 => tr!("Schneeschauer", "snow showers"),
        95 => tr!("Gewitter", "thunderstorm"),
        96 | 99 => tr!("Gewitter mit Hagel", "thunderstorm with hail"),
        _ => "unbekannt",
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn rain_is_found_only_when_dry_first() {
        assert_eq!(first_rain(&[(100, 0.0), (200, 0.0), (300, 0.4)]), Some((300_000, 0.4)));
        assert_eq!(first_rain(&[(100, 0.5), (200, 0.0)]), None);
        assert_eq!(first_rain(&[(100, 0.0), (200, 0.05)]), None);
    }

    use super::*;

    #[test]
    fn wikipedia_picks_the_article_matching_the_query() {
        let hit = |t: &str| Hit { title: format!("{t} – Wikipedia"), url: String::new(), snippet: String::new() };
        let hits = vec![hit("Goethe-Universität"), hit("Offenbach am Main"), hit("Frankfurt am Main")];
        assert_eq!(best_article(hits, "Frankfurt am Main Geschichte Historie").unwrap().title, "Frankfurt am Main – Wikipedia");
        assert!(best_article(vec![hit("Ganz anders")], "Frankfurt").is_none());
    }
    use serde_json::json;

    #[test]
    fn parses_duckduckgo_results_and_skips_ads() {
        let html = r#"<html><body>
          <div class="result result--ad"><a class="result__a" href="https://werbung.example/">Anzeige</a></div>
          <div class="result results_links">
            <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.wetter.example%2Fhamburg%3Fa%3D1&amp;rut=abc"> Wetter  Hamburg </a>
            <a class="result__snippet">Morgen 12 Grad   und Regen.</a>
          </div>
          <div class="result"><a class="result__a" href="https://direkt.example/seite">Direkt</a></div>
          <div class="result"><a class="result__a" href="javascript:alert(1)">Böse</a></div>
        </body></html>"#;
        let hits = parse_duckduckgo(html);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], Hit { title: "Wetter Hamburg".into(), url: "https://www.wetter.example/hamburg?a=1".into(), snippet: "Morgen 12 Grad und Regen.".into() });
        assert_eq!(hits[1].url, "https://direkt.example/seite");
    }

    #[test]
    fn readable_text_keeps_content_and_drops_chrome() {
        let filler = "Das ist ein ausreichend langer Absatz mit echtem Inhalt zum Thema. ".repeat(4);
        let html = format!(
            "<html><body><nav><p>Startseite Kontakt Impressum und noch mehr Navigationstext hier</p></nav>
             <main><h1>Eine aussagekräftige Überschrift zum Artikel</h1><p>{filler}</p><p>Kurz</p>
             <script>var x = 'das darf nicht im Text landen, egal wie lang es ist';</script></main>
             <footer><p>Copyright und rechtliche Hinweise in der Fußzeile der Seite</p></footer></body></html>"
        );
        let text = readable_text(&html);
        assert!(text.contains("aussagekräftige Überschrift"));
        assert!(text.contains("echtem Inhalt"));
        assert!(!text.contains("Navigationstext"));
        assert!(!text.contains("Fußzeile"));
        assert!(!text.contains("darf nicht"));
        assert!(!text.contains("Kurz"));
    }

    #[test]
    fn excerpt_prefers_passages_about_the_query() {
        let page = "Willkommen auf unserer Seite mit vielen Themen.\n\
                    Der Eintritt ins Museum kostet zwölf Euro für Erwachsene.\n\
                    Unser Newsletter erscheint jede Woche neu.\n\
                    Das Museum hat dienstags bis sonntags geöffnet.";
        let found = excerpt(page, "Museum Eintritt Preis", 80);
        assert!(found.starts_with("Der Eintritt ins Museum kostet"), "{found}");
        assert!(!found.contains("Newsletter"));
        // Ohne passende Stelle bleibt der Seitenanfang.
        assert!(excerpt(page, "Fahrrad", 200).starts_with("Willkommen"));
    }

    #[test]
    fn truncate_cuts_at_sentence_end() {
        let text = "Erster Satz ist hier. Zweiter Satz folgt danach und ist länger.";
        assert_eq!(truncate(text, 30), "Erster Satz ist hier. …");
        assert_eq!(truncate("kurz", 30), "kurz");
    }

    #[test]
    fn parses_weather_into_template_data() {
        let data = json!({
            "current": {"time": "2026-10-13T14:15", "temperature_2m": 13.5, "apparent_temperature": 12.1, "weather_code": 3,
                        "wind_speed_10m": 14.4, "relative_humidity_2m": 71, "is_day": 1},
            "hourly": {
                "time": ["2026-10-13T13:00", "2026-10-13T14:00", "2026-10-13T15:00", "2026-10-13T16:00"],
                "temperature_2m": [13.0, 13.6, 13.1, 12.2], "weather_code": [2, 3, 61, 61],
                "precipitation_probability": [0, 5, 40, 60], "is_day": [1, 1, 1, 0]
            },
            "daily": {
                "time": ["2026-10-13", "2026-10-14", "2026-10-15"],
                "weather_code": [3, 63, 0],
                "temperature_2m_max": [15.2, 12.0, 14.0], "temperature_2m_min": [9.6, 7.4, 6.0],
                "precipitation_probability_max": [5, 90, 0], "wind_speed_10m_max": [20.1, 25.0, 9.0],
                "sunrise": ["2026-10-13T07:42", "2026-10-14T07:44", "2026-10-15T07:45"],
                "sunset": ["2026-10-13T18:27", "2026-10-14T18:25", "2026-10-15T18:22"]
            }
        });
        let weather = parse_weather("Hamburg", "Hamburg, Deutschland", &data);
        assert_eq!(weather.now.text, "bedeckt");
        assert!(weather.now.is_day);
        // Die Stunden beginnen bei der laufenden Stunde, nicht um Mitternacht.
        assert_eq!(weather.hours.iter().map(|h| h.hour.as_str()).collect::<Vec<_>>(), ["14", "15", "16"]);
        assert_eq!(weather.hours[1].rain, 40.0);
        assert!(!weather.hours[2].is_day);
        assert_eq!(weather.days.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(), [tr!("Heute", "Today"), tr!("Morgen", "Tomorrow"), "Donnerstag"]);
        assert_eq!((weather.days[0].sunrise.as_str(), weather.days[0].sunset.as_str()), ("07:42", "18:27"));

        let text = weather.report();
        assert!(text.starts_with("Wetter für Hamburg, Hamburg, Deutschland"));
        assert!(text.contains("Jetzt: 14 °C (gefühlt 12 °C), bedeckt, Wind 14 km/h, Luftfeuchte 71 %"));
        assert!(text.contains("Morgen (2026-10-14): 7 bis 12 °C, Regen, Regenwahrscheinlichkeit 90 %, Wind bis 25 km/h"));
    }

    #[test]
    fn weather_survives_missing_fields() {
        let weather = parse_weather("Nirgendwo", "", &json!({}));
        assert!(weather.hours.is_empty() && weather.days.is_empty());
        assert_eq!(weather.now.text, "unbekannt");
        assert!(weather.report().starts_with("Wetter für Nirgendwo (Quelle"));
    }
    #[test]
    fn real_url_only_allows_web_links() {
        assert_eq!(real_url("https://a.example/x").as_deref(), Some("https://a.example/x"));
        assert_eq!(real_url("//duckduckgo.com/l/?uddg=ftp%3A%2F%2Fa.example"), None);
        assert_eq!(real_url("kein link"), None);
    }
}
