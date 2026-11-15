//! Im Browser klicken und tippen, ueber die Windows-Bedienungshilfen (UI Automation).
//!
//! Das ist dieselbe Schnittstelle, die Bildschirmleser benutzen: Milo sucht auf der sichtbaren Seite
//! ein Element mit dem gewuenschten Namen ("Philipp", "The Middle", "Abspielen") und loest es aus.
//! Der Browser muss dafuer nicht mit einer Fernsteuer-Option laufen, und es wird nichts in ihn
//! hineingeladen. Schutzregeln: Kaufen, Bezahlen, Loeschen, Senden und Aehnliches wird nie geklickt,
//! in Passwortfelder wird nie getippt, und ausserhalb bekannter Streaming-Seiten fragt Milo vorher.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};

/// Browser, deren Fenster Milo bedienen darf (Dateiname in Kleinbuchstaben).
pub const BROWSERS: [&str; 6] = ["brave.exe", "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "vivaldi.exe"];

/// Seiten, auf denen Milo ohne Rueckfrage klicken darf (erkannt am Fenstertitel).
const TRUSTED: [&str; 14] = [
    "netflix", "youtube", "disney+", "prime video", "spotify", "twitch", "crunchyroll", "joyn", "ard mediathek", "zdf",
    "rtl+", "wow", "apple tv", "paramount+",
];

/// Woerter, bei denen Milo nie klickt: Geld, Loeschen, Senden, Konto.
const FORBIDDEN: [&str; 32] = [
    "kauf", "bestell", "bezahl", "zahlungspflichtig", "zur kasse", "checkout", "buy", "order", "pay", "purchase", "abonnier",
    "subscribe", "kündig", "cancel membership", "lösch", "delete", "entfern", "remove", "send", "abschick", "submit", "post",
    "veröffentlich", "publish", "abmelden", "sign out", "log out", "logout", "passwort", "password", "konto", "account",
];

/// Darf ein Element mit diesem Namen ueberhaupt geklickt werden?
pub fn allowed_target(name: &str) -> bool {
    let lower = name.to_lowercase();
    !FORBIDDEN.iter().any(|word| lower.contains(word))
}

/// Ist die Seite (nach Fenstertitel) eine bekannte Streaming- oder Videoseite?
pub fn trusted_page(title: &str) -> bool {
    let lower = title.to_lowercase();
    TRUSTED.iter().any(|site| lower.contains(site))
}

/// Bewertet, wie gut ein gefundener Name zum Gesuchten passt (hoeher = besser, None = gar nicht).
pub fn match_score(found: &str, wanted: &str) -> Option<i32> {
    let (found, wanted) = (found.trim().to_lowercase(), wanted.trim().to_lowercase());
    if found.is_empty() || wanted.is_empty() {
        return None;
    }
    if found == wanted {
        return Some(1000);
    }
    if found.starts_with(&wanted) {
        return Some(800 - (found.len() as i32 - wanted.len() as i32).min(300));
    }
    if found.contains(&wanted) {
        return Some(500 - (found.len() as i32 - wanted.len() as i32).min(300));
    }
    None
}

/// Woerter, die bei Titeln und Namen nichts unterscheiden (Artikel, auch englische).
const ARTICLES: [&str; 9] = ["der", "die", "das", "the", "a", "an", "ein", "eine", "den"];

/// Vereinfachte "Klang-Schreibweise": Gleich Klingendes sieht danach gleich aus.
/// Philipp/Filip, Mittel/Middle, Maier/Meyer ...
pub fn sound_key(text: &str) -> String {
    let lower: String = text
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'ä' | 'á' | 'à' | 'â' => 'a',
            'ö' | 'ó' | 'ò' | 'ô' => 'o',
            'ü' | 'ú' | 'ù' | 'û' => 'u',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ß' => 's',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect();
    let words: Vec<&str> = lower.split_whitespace().collect();
    // Artikel weglassen, solange noch etwas uebrig bleibt ("der Mittel" = "The Middle").
    let kept: Vec<&str> = words.iter().copied().filter(|w| !ARTICLES.contains(w)).collect();
    let kept = if kept.is_empty() { words } else { kept };
    kept.iter()
        .map(|word| {
            let mut w = word.to_string();
            // Englisches "-le" klingt wie deutsches "-el" (Middle/Mittel, Apple/Apfel).
            if w.len() > 3 && w.ends_with("le") && !w.ends_with("ele") {
                w.truncate(w.len() - 2);
                w.push_str("el");
            }
            for (from, to) in [
                ("sch", "s"), ("ch", "k"), ("ck", "k"), ("ph", "f"), ("pf", "f"), ("th", "d"), ("qu", "kw"), ("tz", "s"), ("ie", "i"),
                ("ee", "i"), ("ea", "i"), ("oo", "u"), ("ou", "u"), ("y", "i"), ("c", "k"), ("z", "s"), ("v", "f"), ("w", "f"),
                ("t", "d"), ("p", "b"), ("g", "k"), ("x", "ks"), ("h", ""),
            ] {
                w = w.replace(from, to);
            }
            // Doppelte Buchstaben zaehlen einfach.
            let mut out = String::new();
            for c in w.chars() {
                if !out.ends_with(c) {
                    out.push(c);
                }
            }
            out
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current.push((previous[j] + cost).min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

/// Aehnlichkeit 0..1 zweier Namen nach Klang. Bei laengeren Namen auf der Seite ("The Middle – Staffel 3")
/// zaehlt der passendste Ausschnitt mit gleich vielen Woertern.
pub fn similarity(found: &str, wanted: &str) -> f32 {
    let (found, wanted) = (sound_key(found), sound_key(wanted));
    if found.is_empty() || wanted.is_empty() {
        return 0.0;
    }
    let score = |a: &str, b: &str| {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        1.0 - edit_distance(&a, &b) as f32 / a.len().max(b.len()) as f32
    };
    let words: Vec<&str> = found.split(' ').collect();
    let size = wanted.split(' ').count();
    let mut best = score(&found, &wanted);
    if words.len() > size {
        for window in words.windows(size) {
            best = best.max(score(&window.join(" "), &wanted) - 0.03);
        }
    }
    best
}

/// Ab dieser Aehnlichkeit gilt ein anders geschriebener Name als gemeint.
pub const SIMILAR_ENOUGH: f32 = 0.74;

/// Das Browserfenster, das Milo zuletzt selbst geoeffnet hat, und wann. Folgende Schritte (klicken,
/// lesen, Tasten) gehen kurz danach in dieses Fenster, auch wenn der Nutzer inzwischen ein anderes
/// nach vorne geholt hat (z. B. Netflix auf dem Hauptbildschirm, waehrend Milo Twitch auf dem zweiten oeffnet).
static OPENED_BY_MILO: std::sync::Mutex<Option<(isize, Instant)>> = std::sync::Mutex::new(None);
const PREFER_OPENED_FOR: Duration = Duration::from_secs(60);

pub fn remember_opened(handle: isize) {
    *OPENED_BY_MILO.lock().unwrap() = Some((handle, Instant::now()));
}

fn recently_opened() -> Option<isize> {
    OPENED_BY_MILO.lock().unwrap().filter(|(_, at)| at.elapsed() < PREFER_OPENED_FOR).map(|(h, _)| h)
}

/// Suchseiten bekannter Seiten: (Namen, unter denen man sie nennt; Adresse mit {q} fuer die Suche;
/// Pfad statt Parameter = {p}). Statt sich durch Startseite, Vorschauen und Suchfeld zu klicken,
/// oeffnet Milo direkt die Trefferliste.
const SEARCH_SITES: [(&[&str], &str); 13] = [
    (&["netflix"], "https://www.netflix.com/search?q={q}"),
    (&["youtube", "yt"], "https://www.youtube.com/results?search_query={q}"),
    (&["spotify"], "https://open.spotify.com/search/{p}"),
    (&["twitch"], "https://www.twitch.tv/search?term={q}"),
    (&["prime video", "prime", "amazon prime", "primevideo"], "https://www.amazon.de/s?k={q}&i=instant-video"),
    (&["disney+", "disney plus", "disneyplus", "disney"], "https://www.disneyplus.com/search?q={q}"),
    (&["crunchyroll"], "https://www.crunchyroll.com/de/search?q={q}"),
    (&["ard", "ard mediathek", "ardmediathek"], "https://www.ardmediathek.de/suche/{p}"),
    (&["zdf", "zdf mediathek"], "https://www.zdf.de/suche?q={q}"),
    (&["amazon"], "https://www.amazon.de/s?k={q}"),
    (&["wikipedia"], "https://de.wikipedia.org/w/index.php?search={q}"),
    (&["google"], "https://www.google.com/search?q={q}"),
    (&["ebay"], "https://www.ebay.de/sch/i.html?_nkw={q}"),
];

/// Namen der Seiten, die `search_url` kennt (fuer die Werkzeugbeschreibung).
pub fn search_site_names() -> Vec<&'static str> {
    SEARCH_SITES.iter().map(|(names, _)| names[0]).collect()
}

/// Adresse der Trefferliste fuer `query` auf `site` (z. B. "Netflix", "Türkisch für Anfänger").
pub fn search_url(site: &str, query: &str) -> Option<String> {
    let site = site.trim().to_lowercase();
    let site = site.trim_start_matches("www.").split('.').next().unwrap_or_default().to_string();
    let site = if site.is_empty() { return None } else { site };
    let (_, template) = SEARCH_SITES.iter().find(|(names, _)| names.iter().any(|n| *n == site || n.replace(' ', "") == site.replace(' ', "")))?;
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    // Im Pfad sind Leerzeichen als %20 ueblich, nicht als +.
    let path = encoded.replace('+', "%20");
    Some(template.replace("{q}", &encoded).replace("{p}", &path))
}

pub struct Clicked {
    pub name: String,
    pub page: String,
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows::core::Interface;
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEINPUT, VIRTUAL_KEY, VK_RETURN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        SetCursorPos, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    fn automation() -> Result<IUIAutomation> {
        unsafe {
            // Jeder Aufruf laeuft in einem eigenen Hintergrund-Thread; dort COM einschalten.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            Ok(CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?)
        }
    }

    fn title_of(hwnd: HWND) -> String {
        let mut buffer = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut buffer) }.max(0) as usize;
        String::from_utf16_lossy(&buffer[..len])
    }

    fn is_browser(hwnd: HWND) -> bool {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        crate::gamemode::process_name(pid).is_some_and(|name| BROWSERS.contains(&name.as_str()))
    }

    /// Das Browserfenster, das zuletzt benutzt wurde: das vordere, sonst das oberste sichtbare.
    /// Nur echte Hauptfenster zaehlen: Sprechblasen des Browsers („Diese Seite übersetzen?“) und
    /// Menues sind eigene Fenster mit Besitzer und bleiben aussen vor.
    pub fn browser_window() -> Option<(HWND, String)> {
        let mains: Vec<HWND> = crate::system::windows()
            .into_iter()
            .map(|w| HWND(w.handle as *mut core::ffi::c_void))
            .filter(|h| is_browser(*h) && !title_of(*h).is_empty())
            .collect();
        if let Some(own) = recently_opened().map(|h| HWND(h as *mut core::ffi::c_void)).filter(|h| mains.contains(h)) {
            return Some((own, title_of(own)));
        }
        let front = unsafe { GetForegroundWindow() };
        if mains.contains(&front) {
            return Some((front, title_of(front)));
        }
        // system::windows liefert von vorne nach hinten; das erste Browserfenster gewinnt.
        mains.into_iter().next().map(|h| (h, title_of(h)))
    }

    /// Holt das Browserfenster nach vorne (noetig zum Tippen, hilfreich beim Klicken).
    pub fn bring_to_front(hwnd: HWND) {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }

    /// Sucht ein Element, dessen Name den Text enthaelt; wartet bis `wait`, falls die Seite noch laedt.
    fn find(auto: &IUIAutomation, root: &IUIAutomationElement, wanted: &str, wait: Duration) -> Result<IUIAutomationElement> {
        let started = Instant::now();
        let condition = unsafe {
            auto.CreatePropertyConditionEx(
                UIA_NamePropertyId,
                &VARIANT::from(wanted),
                PropertyConditionFlags(PropertyConditionFlags_IgnoreCase.0 | PropertyConditionFlags_MatchSubstring.0),
            )?
        };
        loop {
            let mut best: Option<(i32, IUIAutomationElement)> = None;
            if let Ok(list) = unsafe { root.FindAll(TreeScope_Descendants, &condition) } {
                let count = unsafe { list.Length() }.unwrap_or(0);
                for i in 0..count.min(200) {
                    let Ok(element) = (unsafe { list.GetElement(i) }) else { continue };
                    let name = unsafe { element.CurrentName() }.map(|b| b.to_string()).unwrap_or_default();
                    let Some(mut score) = match_score(&name, wanted) else { continue };
                    if unsafe { element.CurrentIsOffscreen() }.is_ok_and(|b| b.as_bool()) {
                        score -= 400;
                    }
                    let kind = unsafe { element.CurrentControlType() }.unwrap_or_default();
                    if [UIA_ButtonControlTypeId, UIA_HyperlinkControlTypeId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_TabItemControlTypeId]
                        .contains(&kind)
                    {
                        score += 150;
                    }
                    if kind == UIA_TextControlTypeId {
                        score -= 50;
                    }
                    if best.as_ref().is_none_or(|(s, _)| score > *s) {
                        best = Some((score, element));
                    }
                }
            }
            if let Some((_, element)) = best {
                return Ok(element);
            }
            // Nichts woertlich gefunden: nach aehnlich Klingendem suchen (andere Schreibweise, Aussprache, Englisch).
            let (similar, names) = similar_on_page(auto, root, wanted);
            if let Some(element) = similar {
                return Ok(element);
            }
            if started.elapsed() >= wait {
                if names.is_empty() {
                    bail!("{}", tr!(format!("Ich finde auf der Seite nichts mit „{wanted}“, und die Seite zeigt mir keine anklickbaren Namen."), format!("I can't find anything with \"{wanted}\" on the page, and the page shows me no clickable names.")));
                }
                bail!(
                    "Ich finde „{wanted}“ nicht, auch nicht ähnlich geschrieben. Anklickbar auf der Seite sind unter anderem: {}. \
Falls eins davon gemeint ist (andere Schreibweise, Aussprache oder Originaltitel), klick es mit genau diesem Namen an.",
                    names.join(", ")
                );
            }
            std::thread::sleep(Duration::from_millis(600));
        }
    }

    /// Sucht unter allen Namen der Seite den aehnlichsten. Liefert ihn (falls aehnlich genug) und eine
    /// Auswahl anklickbarer Namen, damit das Modell selbst entscheiden kann.
    fn similar_on_page(auto: &IUIAutomation, root: &IUIAutomationElement, wanted: &str) -> (Option<IUIAutomationElement>, Vec<String>) {
        let clickable = [UIA_ButtonControlTypeId, UIA_HyperlinkControlTypeId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_TabItemControlTypeId];
        let found = unsafe {
            let Ok(cache) = auto.CreateCacheRequest() else { return (None, Vec::new()) };
            let _ = cache.AddProperty(UIA_NamePropertyId);
            let _ = cache.AddProperty(UIA_ControlTypePropertyId);
            let _ = cache.AddProperty(UIA_IsOffscreenPropertyId);
            let Ok(condition) = auto.ControlViewCondition() else { return (None, Vec::new()) };
            root.FindAllBuildCache(TreeScope_Descendants, &condition, &cache)
        };
        let Ok(list) = found else { return (None, Vec::new()) };
        let count = unsafe { list.Length() }.unwrap_or(0).min(2000);
        let mut best: Option<(f32, IUIAutomationElement)> = None;
        let mut names: Vec<String> = Vec::new();
        for i in 0..count {
            let Ok(element) = (unsafe { list.GetElement(i) }) else { continue };
            let name = unsafe { element.CachedName() }.map(|b| b.to_string()).unwrap_or_default();
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 80 {
                continue;
            }
            let kind = unsafe { element.CachedControlType() }.unwrap_or_default();
            let hidden = unsafe { element.CachedIsOffscreen() }.is_ok_and(|b| b.as_bool());
            let mut score = similarity(name, wanted);
            if clickable.contains(&kind) {
                score += 0.02;
                if !hidden && names.len() < 30 && !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
            if hidden {
                score -= 0.05;
            }
            if score >= SIMILAR_ENOUGH && best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, element));
            }
        }
        (best.map(|(_, element)| element), names)
    }

    fn mouse_click_at(x: i32, y: i32) {
        unsafe {
            let mut before = POINT::default();
            let _ = GetCursorPos(&mut before);
            let _ = SetCursorPos(x, y);
            let make = |flags| INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: flags, ..Default::default() } },
            };
            SendInput(&[make(MOUSEEVENTF_LEFTDOWN), make(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
            std::thread::sleep(Duration::from_millis(60));
            // Den Mauszeiger wieder dorthin, wo der Nutzer ihn hatte.
            let _ = SetCursorPos(before.x, before.y);
        }
    }

    /// Klickt auf das Element mit dem Namen; `confirm` wird vor dem Klick gefragt (true = darf).
    pub fn click(wanted: &str, confirm: impl Fn(&str, &str) -> bool) -> Result<Clicked> {
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        let auto = automation()?;
        let root = unsafe { auto.ElementFromHandle(hwnd)? };
        let element = find(&auto, &root, wanted, Duration::from_secs(10))?;
        // Den Seitentitel erst jetzt lesen: waehrend des Ladens heisst die Seite oft noch "Unbenannt".
        let page = Some(title_of(hwnd)).filter(|t| !t.is_empty()).unwrap_or(page);
        let name = unsafe { element.CurrentName() }.map(|b| b.to_string()).unwrap_or_default();
        if !allowed_target(&name) {
            bail!("{}", tr!(format!("„{name}“ klicke ich aus Sicherheitsgründen nicht (Kaufen, Löschen, Senden, Konto). Das machst du bitte selbst."), format!("For safety reasons I won't click \"{name}\" (buy, delete, send, account). Please do that yourself.")));
        }
        if !confirm(&name, &page) {
            bail!("ABGELEHNT");
        }
        bring_to_front(hwnd);
        unsafe {
            // Bevorzugt die vorgesehene Aktion des Elements; sonst ein echter Mausklick auf seine Mitte.
            if let Ok(pattern) = element.GetCurrentPattern(UIA_InvokePatternId).and_then(|p| p.cast::<IUIAutomationInvokePattern>()) {
                if pattern.Invoke().is_ok() {
                    return Ok(Clicked { name, page });
                }
            }
            if let Ok(pattern) = element.GetCurrentPattern(UIA_LegacyIAccessiblePatternId).and_then(|p| p.cast::<IUIAutomationLegacyIAccessiblePattern>()) {
                if pattern.DoDefaultAction().is_ok() {
                    return Ok(Clicked { name, page });
                }
            }
            let rect: RECT = element.CurrentBoundingRectangle()?;
            if rect.right <= rect.left || rect.bottom <= rect.top {
                bail!("{}", tr!(format!("„{name}“ ist gerade nicht sichtbar."), format!("\"{name}\" isn't visible right now.")));
            }
            mouse_click_at((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
        }
        Ok(Clicked { name, page })
    }

    /// Steht der Mauszeiger ueber der Seite, schiebt ihn an den oberen Fensterrand. Streamingseiten
    /// (Netflix, Disney+, Prime) oeffnen sonst grosse Vorschauen ueber dem Titel unter dem Zeiger,
    /// die alles andere verdecken.
    pub fn park_mouse() {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
        let Some((hwnd, _)) = browser_window() else { return };
        unsafe {
            let mut rect = RECT::default();
            let mut at = POINT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() || GetCursorPos(&mut at).is_err() {
                return;
            }
            let over_page = at.x >= rect.left && at.x < rect.right && at.y > rect.top + 90 && at.y < rect.bottom;
            if over_page {
                let _ = SetCursorPos((rect.left + rect.right) / 2, rect.top + 14);
            }
        }
    }

    /// Wartet, bis der Tab einen neuen Titel hat (die Seite ist da), hoechstens `max`; dann noch
    /// kurz, damit auch nachgeladene Treffer stehen. Liefert den Titel.
    pub fn wait_for_page(before: &str, max: Duration) -> String {
        let start = Instant::now();
        let mut title = String::new();
        while start.elapsed() < max {
            std::thread::sleep(Duration::from_millis(250));
            title = browser_window().map(|(h, _)| title_of(h)).unwrap_or_default();
            if !title.is_empty() && title != before {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1500));
        browser_window().map(|(h, _)| title_of(h)).filter(|t| !t.is_empty()).unwrap_or(title)
    }

    /// Titel des aktuellen Browserfensters (leer, wenn keins offen ist).
    pub fn current_title() -> String {
        browser_window().map(|(_, t)| t).unwrap_or_default()
    }

    /// Tippt Text in das Feld, das im Browser gerade den Fokus hat. Nie in Passwortfelder.
    pub fn type_text(text: &str, enter: bool, confirm: impl Fn(&str) -> bool) -> Result<String> {
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        bring_to_front(hwnd);
        std::thread::sleep(Duration::from_millis(250));
        let auto = automation()?;
        if let Ok(focused) = unsafe { auto.GetFocusedElement() } {
            if unsafe { focused.CurrentIsPassword() }.is_ok_and(|b| b.as_bool()) {
                bail!(tr!("Das ist ein Passwortfeld. Dort tippe ich nie etwas ein.", "That's a password field. I never type anything there."));
            }
        }
        if !confirm(&page) {
            bail!("ABGELEHNT");
        }
        let mut inputs = Vec::new();
        for unit in text.encode_utf16() {
            for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 { ki: KEYBDINPUT { wScan: unit, dwFlags: flags, ..Default::default() } },
                });
            }
        }
        if enter {
            for flags in [Default::default(), KEYEVENTF_KEYUP] {
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(VK_RETURN.0), dwFlags: flags, ..Default::default() } },
                });
            }
        }
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        Ok(page)
    }

    /// Der Inhaltsbereich (Dokument) des Browserfensters.
    fn document(auto: &IUIAutomation, root: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        unsafe {
            let condition = auto.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_DocumentControlTypeId.0)).ok()?;
            root.FindFirst(TreeScope_Descendants, &condition).ok()
        }
    }

    /// Sichtbarer Text der Seite, gekuerzt auf `max` Zeichen.
    pub fn page_text(max: usize) -> Result<(String, String)> {
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        let auto = automation()?;
        let root = unsafe { auto.ElementFromHandle(hwnd)? };
        let doc = document(&auto, &root).unwrap_or(root.clone());
        // Bevorzugt das Textmuster des Dokuments (der ganze Seitentext am Stueck).
        let via_pattern = unsafe {
            doc.GetCurrentPattern(UIA_TextPatternId)
                .and_then(|p| p.cast::<IUIAutomationTextPattern>())
                .and_then(|p| p.DocumentRange())
                .and_then(|r| r.GetText(max as i32))
                .map(|b| b.to_string())
                .unwrap_or_default()
        };
        let text = if via_pattern.trim().chars().count() > 80 {
            via_pattern
        } else {
            // Sonst die Namen der Textelemente der Reihe nach einsammeln.
            let mut lines: Vec<String> = Vec::new();
            let mut total = 0;
            unsafe {
                let cache = auto.CreateCacheRequest()?;
                let _ = cache.AddProperty(UIA_NamePropertyId);
                let _ = cache.AddProperty(UIA_IsOffscreenPropertyId);
                let _ = cache.AddProperty(UIA_IsPasswordPropertyId);
                let list = doc.FindAllBuildCache(TreeScope_Descendants, &auto.ControlViewCondition()?, &cache)?;
                for i in 0..list.Length().unwrap_or(0).min(4000) {
                    let Ok(element) = list.GetElement(i) else { continue };
                    // Passwortfelder nie lesen
                    if element.CachedIsOffscreen().is_ok_and(|b| b.as_bool()) || element.CachedIsPassword().is_ok_and(|b| b.as_bool()) {
                        continue;
                    }
                    let name = element.CachedName().map(|b| b.to_string()).unwrap_or_default();
                    let name = name.trim();
                    if name.is_empty() || lines.last().is_some_and(|l| l == name) {
                        continue;
                    }
                    total += name.len() + 1;
                    lines.push(name.to_string());
                    if total > max {
                        break;
                    }
                }
            }
            lines.join("\n")
        };
        let page = Some(title_of(hwnd)).filter(|t| !t.is_empty()).unwrap_or(page);
        Ok((page, text.chars().take(max).collect()))
    }

    /// Sichtbare anklickbare Namen der Seite.
    pub fn clickable(limit: usize) -> Result<(String, Vec<String>)> {
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        let auto = automation()?;
        let root = unsafe { auto.ElementFromHandle(hwnd)? };
        let doc = document(&auto, &root).unwrap_or(root);
        let clickable = [UIA_ButtonControlTypeId, UIA_HyperlinkControlTypeId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_TabItemControlTypeId];
        let mut names: Vec<String> = Vec::new();
        unsafe {
            let cache = auto.CreateCacheRequest()?;
            let _ = cache.AddProperty(UIA_NamePropertyId);
            let _ = cache.AddProperty(UIA_ControlTypePropertyId);
            let _ = cache.AddProperty(UIA_IsOffscreenPropertyId);
            let _ = cache.AddProperty(UIA_ValueValuePropertyId);
            let _ = cache.AddProperty(UIA_IsPasswordPropertyId);
            let list = doc.FindAllBuildCache(TreeScope_Descendants, &auto.ControlViewCondition()?, &cache)?;
            let mut seen: Vec<String> = Vec::new();
            for i in 0..list.Length().unwrap_or(0).min(3000) {
                let Ok(element) = list.GetElement(i) else { continue };
                if element.CachedIsPassword().is_ok_and(|b| b.as_bool()) {
                    continue;
                }
                let kind = element.CachedControlType().unwrap_or_default();
                if !clickable.contains(&kind) || element.CachedIsOffscreen().is_ok_and(|b| b.as_bool()) {
                    continue;
                }
                let name = element.CachedName().map(|b| b.to_string()).unwrap_or_default();
                let name = name.trim();
                if name.is_empty() || name.chars().count() > 80 || seen.iter().any(|n| n == name) {
                    continue;
                }
                seen.push(name.to_string());
                // Bei Links die Adresse dazu: dann kann das Modell sie auch direkt oeffnen.
                let link = if kind == UIA_HyperlinkControlTypeId {
                    element
                        .GetCachedPropertyValue(UIA_ValueValuePropertyId)
                        .ok()
                        .and_then(|v| windows::core::BSTR::try_from(&v).ok())
                        .map(|b| b.to_string())
                        .filter(|v| v.starts_with("http"))
                } else {
                    None
                };
                match link {
                    Some(url) => names.push(format!("{name} → {}", url.chars().take(140).collect::<String>())),
                    None => names.push(name.to_string()),
                }
                if names.len() >= limit {
                    break;
                }
            }
        }
        Ok((page, names))
    }

    /// Blaettert die Seite mit dem Mausrad in der Fenstermitte (der Mauszeiger kehrt zurueck).
    pub fn scroll(down: bool, steps: u32) -> Result<String> {
        use windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_WHEEL;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        bring_to_front(hwnd);
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect)? };
        unsafe {
            let mut before = POINT::default();
            let _ = GetCursorPos(&mut before);
            let _ = SetCursorPos((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
            let delta: i32 = if down { -120 } else { 120 };
            for _ in 0..steps.clamp(1, 30) {
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 { mi: MOUSEINPUT { mouseData: delta as u32, dwFlags: MOUSEEVENTF_WHEEL, ..Default::default() } },
                };
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                std::thread::sleep(Duration::from_millis(40));
            }
            let _ = SetCursorPos(before.x, before.y);
        }
        Ok(page)
    }

    /// Die Tabs des Browserfensters (Reiterleiste oberhalb der Seite), in ihrer Reihenfolge.
    fn tab_items(auto: &IUIAutomation, root: &IUIAutomationElement) -> Result<Vec<(String, IUIAutomationElement)>> {
        let page_top = document(auto, root).and_then(|d| unsafe { d.CurrentBoundingRectangle() }.ok()).map(|r| r.top).unwrap_or(i32::MAX);
        let mut tabs = Vec::new();
        unsafe {
            let condition = auto.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_TabItemControlTypeId.0))?;
            let list = root.FindAll(TreeScope_Descendants, &condition)?;
            for i in 0..list.Length().unwrap_or(0).min(200) {
                let Ok(element) = list.GetElement(i) else { continue };
                // Reiter auf der Webseite selbst (z. B. „Videos | Clips“) liegen innerhalb der Seite.
                let rect = element.CurrentBoundingRectangle().unwrap_or_default();
                if rect.bottom > page_top.saturating_add(4) {
                    continue;
                }
                let name = element.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                let name = tab_title(&name);
                if !name.is_empty() {
                    tabs.push((name, element));
                }
            }
        }
        Ok(tabs)
    }

    /// Titel der offenen Tabs.
    pub fn tabs() -> Result<(String, Vec<String>)> {
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        let auto = automation()?;
        let root = unsafe { auto.ElementFromHandle(hwnd)? };
        Ok((page, tab_items(&auto, &root)?.into_iter().map(|(name, _)| name).collect()))
    }

    /// Wechselt zu dem Tab, dessen Titel am besten passt. Liefert seinen Titel.
    pub fn switch_tab(wanted: &str) -> Result<String> {
        let (hwnd, _) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        let auto = automation()?;
        let root = unsafe { auto.ElementFromHandle(hwnd)? };
        let tabs = tab_items(&auto, &root)?;
        let best = tabs
            .iter()
            .filter_map(|(name, element)| {
                let score = match_score(name, wanted).map(|s| s as f32 / 1000.0).unwrap_or_else(|| similarity(name, wanted));
                (score >= 0.5).then_some((score, name, element))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, name, element)) = best else {
            let names: Vec<&str> = tabs.iter().map(|(n, _)| n.as_str()).collect();
            bail!("{}", tr!(format!("Ich finde keinen Tab „{wanted}“. Offen sind: {}.", names.join(", ")), format!("I can't find a tab \"{wanted}\". Open: {}.", names.join(", "))));
        };
        bring_to_front(hwnd);
        unsafe {
            if let Ok(pattern) = element.GetCurrentPattern(UIA_SelectionItemPatternId).and_then(|p| p.cast::<IUIAutomationSelectionItemPattern>()) {
                if pattern.Select().is_ok() {
                    return Ok(name.clone());
                }
            }
            let rect = element.CurrentBoundingRectangle()?;
            mouse_click_at((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
        }
        Ok(name.clone())
    }

    /// Oeffnet eine Adresse im aktuellen Tab (Adressleiste, Adresse, Enter).
    pub fn navigate(url: &str) -> Result<String> {
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        bring_to_front(hwnd);
        std::thread::sleep(Duration::from_millis(200));
        send_keys(&[VK_CONTROL_KEY, VIRTUAL_KEY(b'L' as u16)]);
        std::thread::sleep(Duration::from_millis(150));
        let mut inputs = Vec::new();
        for unit in url.encode_utf16() {
            for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
                inputs.push(INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wScan: unit, dwFlags: flags, ..Default::default() } } });
            }
        }
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        std::thread::sleep(Duration::from_millis(80));
        send_keys(&[VIRTUAL_KEY(VK_RETURN.0)]);
        Ok(page)
    }

    const VK_CONTROL_KEY: VIRTUAL_KEY = windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL;

    fn send_keys(combo: &[VIRTUAL_KEY]) {
        let event = |vk: VIRTUAL_KEY, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } },
        };
        let mut inputs: Vec<INPUT> = combo.iter().map(|vk| event(*vk, false)).collect();
        inputs.extend(combo.iter().rev().map(|vk| event(*vk, true)));
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    }

    /// Drueckt eine Taste oder Tastenkombination im Browser (Namen siehe `KEYS`).
    pub fn press(key: &str) -> Result<String> {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let combo: &[VIRTUAL_KEY] = match key {
            "enter" => &[VK_RETURN],
            "escape" => &[VK_ESCAPE],
            "tab" => &[VK_TAB],
            "space" => &[VK_SPACE],
            "back" => &[VK_MENU, VK_LEFT],
            "forward" => &[VK_MENU, VK_RIGHT],
            "reload" => &[VK_F5],
            "new_tab" => &[VK_CONTROL, VIRTUAL_KEY(b'T' as u16)],
            "close_tab" => &[VK_CONTROL, VIRTUAL_KEY(b'W' as u16)],
            "next_tab" => &[VK_CONTROL, VK_TAB],
            "previous_tab" => &[VK_CONTROL, VK_SHIFT, VK_TAB],
            "fullscreen" => &[VIRTUAL_KEY(b'F' as u16)],
            "browser_fullscreen" => &[VK_F11],
            "theater" => &[VK_MENU, VIRTUAL_KEY(b'T' as u16)],
            "mute" => &[VIRTUAL_KEY(b'M' as u16)],
            "page_down" => &[VK_NEXT],
            "page_up" => &[VK_PRIOR],
            other => bail!("{}", tr!(format!("Die Taste „{other}“ kenne ich nicht."), format!("I don't know the key \"{other}\"."))),
        };
        let (hwnd, page) = browser_window().ok_or_else(|| anyhow!(tr!("Ich sehe kein offenes Browserfenster.", "I don't see an open browser window.")))?;
        bring_to_front(hwnd);
        std::thread::sleep(Duration::from_millis(150));
        let mut inputs = Vec::new();
        let event = |vk: VIRTUAL_KEY, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } },
        };
        for vk in combo {
            inputs.push(event(*vk, false));
        }
        for vk in combo.iter().rev() {
            inputs.push(event(*vk, true));
        }
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        Ok(page)
    }
}

/// Titel eines Tabs ohne die Zusaetze, die Chromium-Browser anhaengen
/// („Example Domain – Arbeitsspeichernutzung – 25,3 MB“, „– Ton wird wiedergegeben“).
pub fn tab_title(name: &str) -> String {
    let name = name.trim();
    let cut = name.find("\u{a0}– ").or_else(|| name.find("\u{a0}- ")).unwrap_or(name.len());
    name[..cut].trim().to_string()
}

/// Tasten, die `press` kennt.
pub const KEYS: [&str; 17] = [
    "enter", "escape", "tab", "space", "back", "forward", "reload", "new_tab", "close_tab", "next_tab", "previous_tab", "fullscreen", "browser_fullscreen",
    "theater", "mute", "page_down", "page_up",
];

#[cfg(windows)]
use native as ui;
pub use ui::{park_mouse, press, switch_tab, tabs};

#[cfg(not(windows))]
mod ui {
    use super::*;
    const ONLY: &str = "Nur unter Windows verfügbar.";
    pub fn park_mouse() {}
    pub fn wait_for_page(_before: &str, _max: Duration) -> String {
        String::new()
    }
    pub fn current_title() -> String {
        String::new()
    }
    pub fn tabs() -> Result<(String, Vec<String>)> {
        bail!(ONLY)
    }
    pub fn switch_tab(_wanted: &str) -> Result<String> {
        bail!(ONLY)
    }
    pub fn navigate(_url: &str) -> Result<String> {
        bail!(ONLY)
    }
    pub fn page_text(_max: usize) -> Result<(String, String)> {
        bail!(ONLY)
    }
    pub fn clickable(_limit: usize) -> Result<(String, Vec<String>)> {
        bail!(ONLY)
    }
    pub fn scroll(_down: bool, _steps: u32) -> Result<String> {
        bail!(ONLY)
    }
    pub fn press(_key: &str) -> Result<String> {
        bail!(ONLY)
    }
    pub fn click(_wanted: &str, _confirm: impl Fn(&str, &str) -> bool) -> Result<Clicked> {
        bail!(ONLY)
    }
    pub fn type_text(_text: &str, _enter: bool, _confirm: impl Fn(&str) -> bool) -> Result<String> {
        bail!(ONLY)
    }
}

// ------------------------------------------------------------------ Erweiterung zuerst
// Ist die Browser-Erweiterung verbunden (siehe browser_bridge.rs), arbeitet Milo direkt in der
// Seite: sie sieht alle Titel und Knoepfe, auch unter Vorschauen, und klickt ohne die Maus zu
// bewegen. Sonst, oder wenn die Erweiterung nicht antwortet, wie bisher ueber UI Automation.

/// Auftrag an die Erweiterung, falls verbunden. `None` = nicht verbunden oder keine Antwort.
fn ext(message: serde_json::Value, wait: Duration) -> Option<serde_json::Value> {
    if !crate::browser_bridge::connected() {
        return None;
    }
    match crate::browser_bridge::request(message, wait) {
        Ok(result) => Some(result),
        Err(err) => {
            log::warn!("Browser-Erweiterung: {err}");
            None
        }
    }
}

fn title_of(result: &serde_json::Value) -> String {
    result["title"].as_str().unwrap_or_default().to_string()
}

/// Fehler aus der Seite (mit aehnlichen Namen, damit das Modell weitersuchen kann).
fn page_error(result: &serde_json::Value) -> Option<anyhow::Error> {
    let error = result["error"].as_str()?;
    let similar: Vec<&str> = result["similar"].as_array().map(|list| list.iter().filter_map(|v| v.as_str()).collect()).unwrap_or_default();
    Some(if similar.is_empty() {
        anyhow!("{error}")
    } else {
        anyhow!("{error} {} {}", tr!("Ähnlich auf der Seite:", "Similar on the page:"), similar.join(" | "))
    })
}

pub fn current_title() -> String {
    ext(serde_json::json!({ "action": "info" }), Duration::from_secs(3)).map(|r| title_of(&r)).unwrap_or_else(ui::current_title)
}

pub fn wait_for_page(before: &str, max: Duration) -> String {
    // Mit der Erweiterung wartet schon `navigate` auf das fertige Laden.
    if crate::browser_bridge::connected() {
        return current_title();
    }
    ui::wait_for_page(before, max)
}

pub fn navigate(url: &str) -> Result<String> {
    if let Some(result) = ext(serde_json::json!({ "action": "navigate", "url": url }), Duration::from_secs(20)) {
        return Ok(title_of(&result));
    }
    ui::navigate(url)
}

pub fn clickable(limit: usize) -> Result<(String, Vec<String>)> {
    if let Some(result) = ext(serde_json::json!({ "action": "list", "limit": limit }), Duration::from_secs(10)) {
        let items = result["items"].as_array().map(|list| list.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        return Ok((title_of(&result), items));
    }
    ui::clickable(limit)
}

pub fn page_text(max: usize) -> Result<(String, String)> {
    if let Some(result) = ext(serde_json::json!({ "action": "read", "max": max }), Duration::from_secs(10)) {
        return Ok((title_of(&result), result["text"].as_str().unwrap_or_default().to_string()));
    }
    ui::page_text(max)
}

pub fn scroll(down: bool, steps: u32) -> Result<String> {
    if let Some(result) = ext(serde_json::json!({ "action": "scroll", "down": down, "steps": steps }), Duration::from_secs(5)) {
        return Ok(title_of(&result));
    }
    ui::scroll(down, steps)
}

pub fn click(wanted: &str, confirm: impl Fn(&str, &str) -> bool) -> Result<Clicked> {
    // Erst nur suchen (nichts klicken), dann pruefen und nachfragen, dann genau dieses anklicken.
    if let Some(found) = ext(serde_json::json!({ "action": "click", "name": wanted, "forbidden": FORBIDDEN, "dryRun": true }), Duration::from_secs(10)) {
        if let Some(err) = page_error(&found) {
            return Err(err);
        }
        let name = found["name"].as_str().unwrap_or(wanted).to_string();
        let page = title_of(&found);
        if !allowed_target(&name) {
            bail!("{}", tr!(format!("„{name}“ klicke ich aus Sicherheitsgründen nicht (Kaufen, Löschen, Senden, Konto). Das machst du bitte selbst."), format!("For safety reasons I won't click \"{name}\" (buy, delete, send, account). Please do that yourself.")));
        }
        if !confirm(&name, &page) {
            bail!("ABGELEHNT");
        }
        let done = ext(serde_json::json!({ "action": "click", "name": name, "forbidden": FORBIDDEN }), Duration::from_secs(10))
            .ok_or_else(|| anyhow!("{}", tr!("Der Browser hat nicht geantwortet.", "The browser didn't answer.")))?;
        if let Some(err) = page_error(&done) {
            return Err(err);
        }
        return Ok(Clicked { name: done["name"].as_str().unwrap_or(&name).to_string(), page });
    }
    ui::click(wanted, confirm)
}

pub fn type_text(text: &str, enter: bool, confirm: impl Fn(&str) -> bool) -> Result<String> {
    if crate::browser_bridge::connected() {
        let page = current_title();
        if !confirm(&page) {
            bail!("ABGELEHNT");
        }
        if let Some(result) = ext(serde_json::json!({ "action": "type", "text": text, "enter": enter }), Duration::from_secs(10)) {
            if let Some(err) = page_error(&result) {
                return Err(err);
            }
            return Ok(title_of(&result));
        }
    }
    ui::type_text(text, enter, confirm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_delete_and_send_are_never_clicked() {
        for name in ["Jetzt kaufen", "Zahlungspflichtig bestellen", "Konto löschen", "Senden", "Abonnieren", "Mitgliedschaft kündigen", "Abmelden", "Buy now"] {
            assert!(!allowed_target(name), "{name}");
        }
        for name in ["Philipp", "The Middle", "Abspielen", "Weiter ansehen", "Nächste Folge"] {
            assert!(allowed_target(name), "{name}");
        }
    }

    #[test]
    fn streaming_pages_are_trusted_others_are_not() {
        assert!(trusted_page("Netflix - Brave"));
        assert!(trusted_page("(3) Lofi Musik - YouTube – Brave"));
        assert!(!trusted_page("Amazon.de: Warenkorb - Brave"));
        assert!(!trusted_page("Online-Banking - Brave"));
    }

    #[test]
    fn misheard_names_are_still_found() {
        // Spracherkennung schreibt anders als die Seite.
        assert!(similarity("Filip", "Philipp") >= SIMILAR_ENOUGH);
        assert!(similarity("The Middle", "der Mittel") >= SIMILAR_ENOUGH);
        assert!(similarity("The Middle – Staffel 3", "der Mittel") >= SIMILAR_ENOUGH);
        assert!(similarity("Philipp", "Filip") >= SIMILAR_ENOUGH);
        assert!(similarity("Stranger Things", "Strenger Sings") >= SIMILAR_ENOUGH);
        // ... aber Verschiedenes bleibt verschieden.
        assert!(similarity("Anna", "Philipp") < SIMILAR_ENOUGH);
        assert!(similarity("Kinder", "Philipp") < SIMILAR_ENOUGH);
        assert!(similarity("The Office", "The Middle") < SIMILAR_ENOUGH);
        assert!(similarity("Modern Family", "der Mittel") < SIMILAR_ENOUGH);
        assert_eq!(similarity("", "x"), 0.0);
    }

    #[test]
    fn tab_titles_lose_browser_extras() {
        assert_eq!(tab_title("Example Domain\u{a0}– Arbeitsspeichernutzung\u{a0}– 25,3 MB"), "Example Domain");
        assert_eq!(tab_title("VALORANT - Twitch"), "VALORANT - Twitch");
    }

    #[test]
    fn exact_names_beat_partial_ones() {
        let exact = match_score("Philipp", "philipp").unwrap();
        let prefix = match_score("Philipp Profil", "Philipp").unwrap();
        let inside = match_score("Profil von Philipp", "Philipp").unwrap();
        assert!(exact > prefix && prefix > inside);
        assert_eq!(match_score("Anna", "Philipp"), None);
        assert_eq!(match_score("", "x"), None);
    }
}
