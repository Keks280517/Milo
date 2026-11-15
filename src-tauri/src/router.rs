//! Bewertungssystem: Wie schwer ist eine Anfrage (0 bis 100)? Danach bekommt sie das schnelle
//! leichte Modell, das staerkere Flash-Modell oder die Bericht-Routine.
//!
//! Die Wertung setzt sich aus nachvollziehbaren Teilen zusammen: feste Regeln (Geraeteaufgaben
//! sind immer leicht, Berichte immer Bericht) und messbare Merkmale des Textes. Sie kostet keine
//! Anfrage und keine Zeit. Jeder Teil steht mit Punkten in der Debug-Ansicht.

use serde::Serialize;

use crate::limits::Tier;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Part {
    pub label: String,
    pub points: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rating {
    pub score: u8,
    pub threshold: u8,
    /// "quick" (Schnellbefehl ohne KI), "easy", "hard", "report" oder "browser".
    pub kind: String,
    /// Das Modell, das die Anfrage bekommt; leer, solange noch keins gewaehlt ist.
    pub model: String,
    pub parts: Vec<Part>,
    /// In einem Satz, warum so entschieden wurde.
    pub reason: String,
}

impl Rating {
    pub fn tier(&self) -> Tier {
        match self.kind.as_str() {
            "hard" => Tier::Hard,
            "report" => Tier::Report,
            _ => Tier::Easy,
        }
    }
}

/// Zwischenergebnis der reinen Textbewertung.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub parts: Vec<Part>,
    /// Feste Regel hat entschieden; die Schwelle spielt dann keine Rolle.
    pub forced: Option<Tier>,
}

impl Draft {
    pub fn score(&self) -> u8 {
        self.parts.iter().map(|p| p.points).sum::<i32>().clamp(0, 100) as u8
    }

    fn add(&mut self, label: &str, points: i32) {
        self.parts.push(Part { label: label.to_string(), points });
    }
}

fn any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| text.contains(n))
}

fn count(text: &str, needles: &[&str]) -> usize {
    needles.iter().map(|n| text.matches(n).count()).sum()
}

/// Beginnt der Text (nach Anrede und Hoeflichkeit) mit einem dieser Woerter?
fn starts(text: &str, words: &[&str]) -> bool {
    let mut rest = text.trim_start_matches(|c: char| !c.is_alphanumeric());
    for filler in ["hey milo", "hey milo", "milo", "milo", "bitte", "kannst du", "könntest du", "würdest du", "mal", "mir", "bitte", "please", "can you", "could you", "would you"] {
        if let Some(stripped) = rest.strip_prefix(filler) {
            rest = stripped.trim_start_matches(|c: char| !c.is_alphanumeric());
        }
    }
    words.iter().any(|w| rest.starts_with(w))
}

/// Bewertet allein den Text. Rein und ohne Nebenwirkungen, damit es sich gut testen laesst.
pub fn draft(text: &str) -> Draft {
    let lower = text.to_lowercase();
    let t = lower.as_str();
    let chars = text.chars().count();
    let mut d = Draft { parts: Vec::new(), forced: None };

    // --- Feste Regeln ---
    if is_report(t) {
        d.add(tr!("Bericht oder Dokument mit Recherche: gründlich recherchieren und schreiben", "Report or document with research: research thoroughly and write"), 100);
        d.forced = Some(Tier::Report);
        return d;
    }
    if any(t, &["denk gründlich", "denk genau", "denk gut nach", "überleg gründlich", "überleg genau", "frag flash", "mit flash", "frag deinen freund", "frag deinen helfer", "hol deinen freund", "hol den helfer", "frag gemini", "mit gemini", "think hard", "think carefully", "think it through", "ask gemini", "use gemini"]) {
        d.add(tr!("Du hast ausdrücklich gründliches Nachdenken verlangt", "You explicitly asked for careful thinking"), 100);
        d.forced = Some(Tier::Hard);
        return d;
    }
    let device = starts(t, &["öffne", "oeffne", "starte", "start ", "schließ", "schliess", "beende", "mach ", "spiel", "stell", "stopp", "pausier", "lösch", "zeig", "sperr", "fahr", "merk", "notier", "erinner", "weck", "such", "google", "recherchier", "open ", "launch ", "close ", "quit ", "play ", "set ", "stop", "pause", "lock ", "shut ", "remind", "wake me", "remember", "note ", "search", "show "])
        || any(t, &["timer", "wecker", "erinner mich", "merk dir", "lauter", "leiser", "lautstärke", "stumm", "nächstes lied", "nächster titel", "pause", "wetter", "regnet", "herunterfahren", "neu starten", "neustart", "wie spät", "wie viel uhr", "welcher tag", "youtube", "netflix", "spotify", "bildschirm", "screenshot", "monitor", "per skript", "nutze ein skript", "energiespar", "spul", "zwischenablage", "speicherplatz", "arbeitsspeicher", "lautstärke auf", "klick", "alarm", "louder", "quieter", "volume", "mute", "next song", "weather", "raining", "shut down", "restart", "what time", "what day", "clipboard", "disk space", "memory", "click"]);
    if device && chars < 160 {
        d.add(tr!("Geräteaufgabe (Programm, Timer, Medien, Notiz, Wetter): schnelles Modell", "Device task (app, timer, media, note, weather): fast model"), 0);
        d.forced = Some(Tier::Easy);
        return d;
    }

    // --- Messbare Merkmale ---
    d.add(tr!("Grundwert", "Base value"), 22);
    match chars {
        0..=40 => d.add(tr!("Sehr kurze Anfrage", "Very short request"), -12),
        41..=140 => {}
        141..=400 => d.add(tr!("Längere Anfrage", "Longer request"), 8),
        401..=1200 => d.add(tr!("Lange Anfrage", "Long request"), 15),
        _ => d.add(tr!("Sehr lange Anfrage", "Very long request"), 22),
    }

    let steps = count(t, &[" und dann", "danach", "anschließend", "außerdem", "zusätzlich", "erstens", "zweitens", "drittens", "\n- ", "\n1.", "\n2.", "\n3."]);
    if steps > 0 {
        d.add(&tr!(format!("Mehrere Schritte ({steps})"), format!("Several steps ({steps})")), (steps as i32 * 7).min(20));
    }

    let code_words = ["programmier", "skript", "script", "funktion", "python", "javascript", "typescript", "rust", "c++", "c#", "java ", "html", "css", "sql", "regex", "regulären ausdruck", "algorithmus", "klasse ", "api ", "json", "quellcode", "code", "```", "bug", "kompilier", "debug"];
    let code = count(t, &code_words);
    if code > 0 {
        let writes = any(t, &["schreib", "programmier", "implementier", "erstell", "bau ", "entwickl", "refaktor", "optimier", "finde den fehler", "warum funktioniert", "debug"]);
        d.add(if writes { tr!("Programmieraufgabe", "Programming task") } else { tr!("Frage zu Code oder Technik", "Question about code or tech") }, if writes { 42 } else { 16 });
    }

    if any(t, &["beweise", "beweis ", "herleitung", "leite her", "leite ab", "integral", "ableitung", "differential", "gleichungssystem", "wahrscheinlichkeit", "kombinatorik", "matrix", "vektor", "logikrätsel", "rätsel", "optimierungsproblem", "stochastik"]) {
        d.add(tr!("Anspruchsvolle Mathematik oder Logik", "Demanding math or logic"), 40);
    } else if any(t, &["berechne", "rechne", "wie viel ist", "wieviel ist", "prozent", "dreisatz", "umrechnen"]) {
        d.add(tr!("Einfache Rechnung", "Simple calculation"), 4);
    }

    let long_form = ["aufsatz", "essay", "erörterung", "bewerbung", "anschreiben", "geschichte", "rede ", "gedicht", "artikel", "blogpost", "blogbeitrag", "referat", "präsentation", "konzept", "businessplan", "drehbuch", "kurzgeschichte", "songtext", "lebenslauf", "hausarbeit", "facharbeit", "gutachten"];
    if any(t, &["schreib", "verfass", "formulier", "entwirf", "erstell", "entwickl"]) && any(t, &long_form) {
        d.add(tr!("Längerer Text soll geschrieben werden", "A longer text is to be written"), 40);
    } else if any(t, &["schreib", "verfass", "formulier", "entwirf"]) && any(t, &["e-mail", "email", "mail", "brief", "nachricht", "antwort"]) {
        d.add(tr!("Kurzer Text soll geschrieben werden", "A short text is to be written"), 10);
    }

    let depth = count(t, &["ausführlich", "detailliert", "im detail", "schritt für schritt", "gründlich", "umfassend", "tiefgehend", "genau erklär", "fundiert"]);
    if depth > 0 {
        d.add(tr!("Ausführliche Antwort gewünscht", "Detailed answer wanted"), (depth as i32 * 12).min(22));
    }

    let analysis = count(t, &["analysier", "vergleich", "vor- und nachteile", "vorteile und nachteile", "bewerte", "beurteile", "interpretier", "erörter", "diskutier", "strategie", "plane ", "plan für", "konzept für", "argumente", "begründe", "kritisch", "zusammenhang zwischen", "unterschied zwischen", "auswirkungen"]);
    if analysis > 0 {
        d.add(tr!("Analyse, Vergleich oder Planung", "Analysis, comparison or planning"), (10 + analysis as i32 * 8).min(28));
    }

    if any(t, &["übersetz"]) && chars > 200 {
        d.add(tr!("Längere Übersetzung", "Longer translation"), 12);
    }
    if any(t, &["erklär", "erklaer", "was ist", "was sind", "wer ist", "wer war", "wie funktioniert", "warum"]) && analysis == 0 && depth == 0 && code == 0 {
        d.add(tr!("Einfache Wissens- oder Erklärfrage", "Simple knowledge or explanation question"), -6);
    }
    if any(t, &["hallo", "hi ", "wie geht", "danke", "guten morgen", "guten abend", "gute nacht", "witz", "wer bist du", "was kannst du"]) && chars < 80 {
        d.add(tr!("Smalltalk", "Small talk"), -15);
    }

    // --- Persoenliches bleibt lieber hier ---
    if any(t, &["meine notiz", "meinen notiz", "gemerkt", "meine datei", "die datei", "das dokument", "das arbeitsblatt", "diese datei", "die pdf", "mein passwort", "passwort", "kontonummer", "iban", "meine adresse", "privat"]) {
        d.add(tr!("Betrifft Persönliches (Dateien, Notizen): meist schnell erledigt", "About personal things (files, notes): usually quick"), -15);
    }
    if any(t, &["aktuell", "heute", "gestern", "nachrichten", "news", "ergebnis", "spielstand", "preis", "kurs ", "wann spielt"]) && code == 0 {
        d.add(tr!("Aktuelles: Milo recherchiert, das Modell fasst nur zusammen", "Current events: Milo researches, the model only summarizes"), -8);
    }
    d
}

/// Bericht mit Recherche, der als Dokument gespeichert werden soll?
fn is_report(t: &str) -> bool {
    let make = any(t, &["erstell", "schreib", "mach", "verfass", "anfertig", "ausarbeit", "zusammenstell", "create", "write", "make", "put together", "draft", "compile"]);
    let document = any(t, &["word", "docx", "dokument", "bericht", "ausarbeitung", "dossier", "referat", "hausarbeit", "facharbeit", "document", "report", "paper"]);
    let research = any(t, &["recherchier", "informier dich", "infos", "informationen", "internet", "online", "research", "look up", "information"]);
    make && document && (research || any(t, &["word", "docx", "bericht", "report"]))
}

/// Fasst alles zur Entscheidung zusammen.
pub fn decide(draft: Draft, threshold: u8) -> Rating {
    let score = draft.score();
    let tier = draft.forced.unwrap_or(if score >= threshold { Tier::Hard } else { Tier::Easy });
    let (kind, reason) = match (tier, draft.forced.is_some()) {
        (Tier::Report, _) => ("report", tr!("Bericht: gründliche Recherche, geschrieben vom Flash-Modell, gespeichert als Word-Datei.", "Report: thorough research, written by the Flash model, saved as a Word file.").to_string()),
        (Tier::Hard, true) => ("hard", tr!("Du hast gründliches Nachdenken verlangt: Flash-Modell.", "You asked for careful thinking: Flash model.").to_string()),
        (Tier::Hard, false) => ("hard", tr!(format!("{score} erreicht die Schwelle {threshold}: das Flash-Modell denkt gründlicher nach."), format!("{score} reaches the threshold {threshold}: the Flash model thinks it through."))),
        (Tier::Easy, true) => ("easy", tr!("Feste Regel: das schnelle Modell reicht.", "Fixed rule: the fast model is enough.").to_string()),
        (Tier::Easy, false) => ("easy", tr!(format!("{score} liegt unter der Schwelle {threshold}: das schnelle Modell reicht."), format!("{score} is below the threshold {threshold}: the fast model is enough."))),
    };
    Rating { score, threshold, kind: kind.into(), model: String::new(), parts: draft.parts, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLD: u8 = 60;

    fn rate(text: &str) -> Rating {
        decide(draft(text), THRESHOLD)
    }

    /// Testreihe: alltaegliche Befehle und leichte Fragen bekommen das schnelle Modell.
    #[test]
    fn everyday_requests_are_easy() {
        let local = [
            "Öffne Spotify",
            "Starte bitte den Rechner",
            "Stell einen Timer auf 10 Minuten",
            "Erinner mich um 18 Uhr an die Wäsche",
            "Mach die Musik lauter",
            "Nächstes Lied",
            "Wie wird das Wetter morgen in Hamburg?",
            "Merk dir, dass mein Zahnarzttermin am Freitag ist",
            "Was habe ich mir zum Zahnarzt gemerkt?",
            "Such auf YouTube nach Katzenvideos",
            "Öffne netflix.com",
            "Fahr den PC herunter",
            "Wie spät ist es?",
            "Hallo Milo, wie geht's?",
            "Erzähl mir einen Witz",
            "Was ist die Hauptstadt von Australien?",
            "Wer war Albert Einstein?",
            "Wie viel ist 17 mal 23?",
            "Was sind 15 Prozent von 80?",
            "Fass die Datei zusammen",
            "Was steht in dem Arbeitsblatt, das ich dir gegeben habe?",
            "Wie hat Bayern gestern gespielt?",
            "Was gibt es heute für Nachrichten?",
            "Übersetz 'Guten Morgen' ins Englische",
            "Erklär mir kurz, was ein Dreisatz ist",
            "Schreib eine kurze Mail an Tom, dass ich später komme",
            "Danke!",
            "Was kannst du alles?",
            "Schließ Discord",
            "Schau bitte auf meinen Bildschirm und sag mir, welches Programm im Vordergrund ist",
            "Was siehst du gerade auf meinem Bildschirm?",
            "Welches Programm verbraucht gerade am meisten Arbeitsspeicher?",
            "Zähl per Skript die Dateien auf meinem Desktop",
            "Pausiere das Video",
        ];
        let wrong: Vec<String> = local
            .iter()
            .map(|text| (text, rate(text)))
            .filter(|(_, rating)| rating.kind != "easy")
            .map(|(text, rating)| format!("„{text}“ bekam {} ({:?})", rating.score, rating.parts))
            .collect();
        assert!(wrong.is_empty(), "falsch eingeordnet:
{}", wrong.join("
"));
    }

    /// Testreihe: schwere Aufgaben bekommen das Flash-Modell.
    #[test]
    fn heavy_requests_are_hard() {
        let cloud = [
            "Schreib mir ein Python-Skript, das alle Bilder in einem Ordner nach Aufnahmedatum sortiert und umbenennt",
            "Schreibe einen ausführlichen Aufsatz über die Ursachen des Ersten Weltkriegs",
            "Beweise, dass die Wurzel aus 2 irrational ist, Schritt für Schritt",
            "Analysiere die Vor- und Nachteile von Wärmepumpen gegenüber Gasheizungen und begründe eine Empfehlung ausführlich",
            "Verfasse eine Bewerbung als Fachinformatiker für Anwendungsentwicklung",
            "Denk gründlich nach: wie viele Wochenenden hat ein Schaltjahr?",
            "Implementiere in Rust eine Funktion, die einen Binärbaum balanciert, und erkläre den Algorithmus",
            "Entwickle ein Konzept für eine Schülerzeitung, erstens mit Rubriken, zweitens mit Zeitplan und drittens mit Aufgabenverteilung",
            "Frag Gemini, was der Sinn des Lebens ist",
            "Finde den Fehler in diesem Code und optimiere ihn: function f(a){for(var i=0;i<a.length;i++){if(a[i]=3)return i}}",
            "Löse das Gleichungssystem 3x + 2y - z = 1, 2x - 2y + 4z = -2, -x + y/2 - z = 0 mit Herleitung",
            "Vergleiche ausführlich die Strategie von Apple und Samsung im Smartphone-Markt und bewerte kritisch die Auswirkungen",
            "Schreib eine Kurzgeschichte über einen Roboter, der das Meer entdeckt, detailliert und mit überraschendem Ende",
        ];
        let wrong: Vec<String> = cloud
            .iter()
            .map(|text| (text, rate(text)))
            .filter(|(_, rating)| rating.kind != "hard")
            .map(|(text, rating)| format!("„{text}“ bekam {} ({:?})", rating.score, rating.parts))
            .collect();
        assert!(wrong.is_empty(), "falsch eingeordnet:
{}", wrong.join("
"));
    }

    #[test]
    fn reports_are_recognised() {
        for text in [
            "Recherchiere online und erstelle eine Word-Datei mit allen Infos über Deutschland",
            "Schreib mir einen Bericht über die Geschichte von Rom",
            "Erstelle ein Dokument mit allen Informationen aus dem Internet zu Wärmepumpen",
        ] {
            let r = rate(text);
            assert_eq!(r.kind, "report", "{text}");
            assert_eq!(r.tier(), Tier::Report);
        }
        assert_eq!(rate("Öffne das Dokument von gestern").kind, "easy");
        assert_eq!(rate("Schreib eine kurze Mail an Tom").kind, "easy");
    }

    #[test]
    fn personal_things_lower_the_score() {
        let with = draft("Analysiere bitte meine Datei mit den Kontonummern und bewerte sie").score();
        let without = draft("Analysiere bitte diesen Markt und bewerte ihn").score();
        assert!(with < without);
    }

    #[test]
    fn threshold_moves_the_decision() {
        let text = "Vergleiche Katzen und Hunde als Haustiere und begründe, was besser passt";
        let score = draft(text).score();
        assert_eq!(decide(draft(text), score).kind, "hard");
        assert_eq!(decide(draft(text), score + 1).kind, "easy");
    }

    #[test]
    fn score_is_clamped() {
        let d = Draft { parts: vec![Part { label: "a".into(), points: 90 }, Part { label: "b".into(), points: 60 }], forced: None };
        assert_eq!(d.score(), 100);
        let d = Draft { parts: vec![Part { label: "a".into(), points: -30 }], forced: None };
        assert_eq!(d.score(), 0);
    }
}
