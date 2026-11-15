//! Formatierte Notizen: bereinigt HTML aus dem Editor auf eine kleine, feste Auswahl von Elementen
//! und Klassen. Alles andere (Skripte, Ereignis-Attribute, Stile, Links, Bilder) faellt weg, der Text
//! darin bleibt erhalten. So kann die Oberflaeche den gespeicherten Inhalt gefahrlos anzeigen.

use scraper::{Html, Node};

/// Erlaubte Elemente (die aeussere `div` traegt Schriftart und -groesse der Notiz).
const TAGS: &[&str] = &["p", "br", "b", "strong", "i", "em", "u", "s", "h1", "h2", "h3", "ul", "ol", "li", "mark", "blockquote", "span", "div"];
/// Erlaubte Klassen: Checkliste, Schriftart, Schriftgroesse.
const CLASSES: &[&str] = &["check", "done", "f-sans", "f-serif", "f-mono", "s-sm", "s-md", "s-lg"];
/// Hoechstlaenge des gespeicherten HTML (eine Notiz, kein Buch).
const MAX: usize = 200_000;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn sanitize(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    let mut out = String::new();
    for child in fragment.root_element().children() {
        walk(child, &mut out, 0);
    }
    out.truncate(floor_char_boundary(&out, MAX));
    out
}

fn floor_char_boundary(s: &str, max: usize) -> usize {
    if s.len() <= max {
        return s.len();
    }
    let mut i = max;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn walk(node: ego_tree::NodeRef<'_, Node>, out: &mut String, depth: usize) {
    if depth > 40 {
        return;
    }
    match node.value() {
        Node::Text(text) => out.push_str(&escape(text)),
        Node::Element(element) => {
            let name = element.name();
            // Inhalte von Skripten und Stilen gar nicht uebernehmen.
            if matches!(name, "script" | "style" | "iframe" | "object" | "embed" | "template" | "noscript") {
                return;
            }
            if !TAGS.contains(&name) {
                for child in node.children() {
                    walk(child, out, depth + 1);
                }
                return;
            }
            let tag = name;
            if tag == "br" {
                out.push_str("<br>");
                return;
            }
            let classes: Vec<&str> = element.classes().filter(|c| CLASSES.contains(c)).collect();
            out.push('<');
            out.push_str(tag);
            if !classes.is_empty() {
                out.push_str(" class=\"");
                out.push_str(&classes.join(" "));
                out.push('"');
            }
            out.push('>');
            for child in node.children() {
                walk(child, out, depth + 1);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn keeps_formatting_and_drops_everything_dangerous() {
        let html = r#"<div class="f-serif s-lg"><h2>Einkauf</h2><ul class="check"><li class="done">Milch</li><li>Brot <b>frisch</b></li></ul><p onclick="x()">Hallo <script>alert(1)</script><img src=x onerror=alert(1)><a href="javascript:1">Link</a> <span style="color:red" class="evil f-mono">mono</span> <mark>wichtig</mark></p></div>"#;
        let clean = sanitize(html);
        assert!(clean.contains("<h2>Einkauf</h2>"));
        assert!(clean.contains(r#"<ul class="check"><li class="done">Milch</li>"#));
        assert!(clean.contains("<b>frisch</b>"));
        assert!(clean.contains(r#"<span class="f-mono">mono</span>"#));
        assert!(clean.contains("<mark>wichtig</mark>"));
        assert!(clean.contains("Link"));
        for bad in ["script", "alert", "onclick", "onerror", "<img", "<a", "javascript", "style", "evil"] {
            assert!(!clean.contains(bad), "{bad} in {clean}");
        }
    }

    #[test]
    fn escapes_text() {
        assert_eq!(sanitize("1 < 2 &amp; 3"), "1 &lt; 2 &amp; 3");
    }
}
