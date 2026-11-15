// Schickt Eingaben an die laufende Milo-App und zeigt, was sie daraus gemacht hat
// (Werkzeuge, Antwort, Dauer). Nur fuer den Entwicklungsmodus, siehe cdp.mjs.
//
//   node scripts/ask.mjs "Stell einen Timer auf 10 Minuten" "Wie wird das Wetter in Hamburg?"
//   node scripts/ask.mjs --new "..."      vorher ein neues Gespraech beginnen
//   node scripts/ask.mjs --approve "..."  Rueckfragen automatisch bestaetigen (--deny: ablehnen)

import { connect, evaluate, waitFor } from "./lib/cdp-client.mjs";

const args = process.argv.slice(2);
const flags = new Set(args.filter((a) => a.startsWith("--")));
const prompts = args.filter((a) => !a.startsWith("--"));

const cdp = await connect();
try {
  if (flags.has("--new")) await evaluate(cdp, "__milo.clear()");
  for (const prompt of prompts) {
    const started = Date.now();
    await evaluate(cdp, `__milo.send(${JSON.stringify(prompt)})`);
    let done = false;
    while (!done && Date.now() - started < 180_000) {
      done = await waitFor(cdp, "!__milo.busy || __milo.questions.length > 0", 180);
      const question = await evaluate(cdp, "__milo.questions[0] ? { ...__milo.questions[0] } : null");
      if (question) {
        const approve = flags.has("--approve");
        console.log(`   Rückfrage: ${question.title} | ${question.detail.replaceAll("\n", " / ")} -> ${approve ? "bestätigt" : "abgelehnt"}`);
        await evaluate(cdp, `__milo.answer(__milo.questions[0], ${approve})`);
        done = false;
      }
    }
    const last = await evaluate(
      cdp,
      "(() => { const m = __milo.messages.at(-1); return { text: m.text, error: m.error, tools: m.tools.map((t) => ({ ...t, links: t.links.map((l) => l.url) })) }; })()",
    );
    console.log(`\n> ${prompt}   (${((Date.now() - started) / 1000).toFixed(1)} s)`);
    for (const tool of last.tools) {
      console.log(`   [${tool.status}] ${tool.name}: ${tool.summary}${tool.links.length ? "  " + tool.links.join("  ") : ""}`);
    }
    console.log(`   ${last.error ? "FEHLER: " : ""}${last.text.replaceAll("\n", "\n   ")}`);
  }
} finally {
  cdp.close();
}
