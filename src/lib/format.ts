// Sehr kleine, sichere Textformatierung fuer Antworten des Modells.
// Erst wird alles HTML entschaerft, danach werden nur wenige Markdown-Muster in feste Tags uebersetzt.
// Formeln ($...$, $$...$$, \(...\), \[...\]) setzt KaTeX. Das laeuft lokal und erzeugt nur eigenes,
// entschaerftes HTML (trust: false, also keine Links, Bilder oder Skripte aus der Formel).
import katex from "katex";
import "katex/dist/katex.min.css";

const ESCAPES: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (ch) => ESCAPES[ch]);
}

/** Formel als HTML; bei einem Fehler bleibt der Ausdruck lesbar stehen. */
function renderMath(expression: string, display: boolean): string {
  try {
    return katex.renderToString(expression, { displayMode: display, throwOnError: false, strict: "ignore", trust: false, output: "html" });
  } catch {
    return escapeHtml(expression);
  }
}

/**
 * Ersetzt Inline-Code und Formeln durch Platzhalter, damit die Markdown-Regeln sie nicht veraendern.
 * Dollarzeichen in Preisen ("5 $ und 10 $") gelten nicht als Formel: nach dem oeffnenden und vor dem
 * schliessenden $ muss ein Zeichen stehen, und auf das schliessende $ darf keine Ziffer folgen.
 */
function protect(text: string, stash: string[]): string {
  const keep = (html: string) => `\u0000${stash.push(html) - 1}\u0000`;
  return text
    .replace(/`([^`\n]+)`/g, (_, code: string) => keep(`<code>${escapeHtml(code)}</code>`))
    // Abgesetzte Formeln stehen ohnehin auf eigener Zeile; Leerzeilen drumherum fallen weg.
    .replace(/\n*\$\$([\s\S]+?)\$\$\n*/g, (_, expr: string) => `\n${keep(renderMath(expr.trim(), true))}\n`)
    .replace(/\n*\\\[([\s\S]+?)\\\]\n*/g, (_, expr: string) => `\n${keep(renderMath(expr.trim(), true))}\n`)
    .replace(/\\\(([\s\S]+?)\\\)/g, (_, expr: string) => keep(renderMath(expr.trim(), false)))
    .replace(/\$(?=\S)([^$\n]*?\S)\$(?!\d)/g, (_, expr: string) => keep(renderMath(expr, false)));
}

function restore(html: string, stash: string[]): string {
  return html.replace(/\u0000(\d+)\u0000/g, (_, index: string) => stash[Number(index)] ?? "");
}

/** Wandelt Modelltext in HTML um: fett, kursiv, Code, Formeln, Listen, Links, Zeilenumbrueche. */
export function formatAnswer(text: string): string {
  // Codebloecke (```) bleiben woertlich erhalten; nur der Text dazwischen wird formatiert.
  // Ein noch offener Block am Ende (Antwort laeuft gerade ein) zaehlt bis zum Textende.
  return text
    .trim()
    .split("```")
    .map((chunk, index) => {
      if (index % 2 === 0) return formatProse(chunk);
      const code = chunk.replace(/^[\w+#-]*\r?\n/, "").replace(/\s+$/, "");
      return `<pre><code>${escapeHtml(code)}</code></pre>`;
    })
    .join("");
}

function formatProse(text: string): string {
  const stash: string[] = [];
  const lines = escapeHtml(protect(text.trim(), stash)).split(/\r?\n/);
  const html = lines
    .map((line) => {
      const bullet = line.match(/^\s*(?:[-*•]|\d+[.)])\s+(.*)$/);
      const heading = line.match(/^\s*#{1,6}\s+(.*)$/);
      let body = bullet ? bullet[1] : heading ? heading[1] : line;
      body = body
        .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
        .replace(/(^|[\s(])\*([^*\s][^*]*)\*(?=[\s).,!?:;]|$)/g, "$1<em>$2</em>")
        .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" data-external>$1</a>');
      if (bullet) return `<span class="li">${body}</span>`;
      if (heading) return `<strong>${body}</strong>`;
      return body;
    })
    .join("<br>")
    .replace(/(<br>){3,}/g, "<br><br>")
    .replace(/<\/span><br>(?=<span class="li">)/g, "</span>");
  return restore(html, stash);
}
