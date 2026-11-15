// Attrappe der Gemini-API fuer Tests ohne echten Schluessel und ohne Kontingent. Nur fuer
// Entwicklungsversionen:  MILO_GEMINI_BASE=http://127.0.0.1:8791  (siehe src-tauri/src/gemini.rs)
//
// Modus per Umgebungsvariable ATTRAPPE:
//   ok           Standard: Text, bei „timer“ ein Werkzeugaufruf, danach eine Bestaetigung
//   limit        jedes Modell meldet 429 (Kontingent aufgebraucht)
//   limit-flash  nur gemini-3.8-flash meldet 429; die anderen antworten
//   key          Schluessel abgelehnt
//   modell       gemini-3.7-flash gibt es nicht (404)
//   langsam      das erste Wort kommt erst nach 25 Sekunden
import http from "node:http";

const mode = process.env.ATTRAPPE ?? "ok";
const MODELS = ["gemini-3.5-flash-lite", "gemini-3.8-flash", "gemini-3.7-flash", "gemini-2.5-flash", "text-embedding-004"];
const answer = ["Das ist eine Antwort ", "der Attrappe, ", "nicht von Gemini. ", "Sie kommt in mehreren Stücken."];

const fail = (res, code, status, message) => {
  res.writeHead(code, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: { code, message, status } }));
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const chunk = (res, parts, usage) =>
  res.write(`data: ${JSON.stringify({ candidates: [{ content: { role: "model", parts } }], ...(usage ? { usageMetadata: { totalTokenCount: usage } } : {}) })}\r\n\r\n`);

http
  .createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", async () => {
      const model = (req.url.match(/models\/([^:?]+)/) ?? [])[1] ?? "";
      let request = {};
      try {
        request = JSON.parse(body || "{}");
      } catch {}
      const contents = request.contents ?? [];
      const last = contents.at(-1) ?? {};
      const lastText = (last.parts ?? []).map((p) => p.text ?? "").join(" ").toLowerCase();
      const tools = (request.tools ?? [])[0]?.functionDeclarations?.length ?? 0;
      console.log(new Date().toISOString().slice(11, 19), req.method, model || req.url, `Verlauf ${contents.length}`, `Werkzeuge ${tools}`, JSON.stringify(request.generationConfig ?? {}));

      if (mode === "key") return fail(res, 400, "INVALID_ARGUMENT", "API key not valid. Please pass a valid API key.");
      if (req.url.startsWith("/v1beta/models?")) {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify({ models: MODELS.map((n) => ({ name: `models/${n}`, supportedGenerationMethods: [n.includes("embedding") ? "embedContent" : "generateContent"] })) }));
      }
      if (mode === "limit" || (mode === "limit-flash" && model === "gemini-3.8-flash")) return fail(res, 429, "RESOURCE_EXHAUSTED", "Quota exceeded");
      if (mode === "modell" && model === "gemini-3.7-flash") return fail(res, 404, "NOT_FOUND", "model not found");

      res.writeHead(200, { "content-type": "text/event-stream" });
      await sleep(mode === "langsam" ? 25000 : 600);
      const answered = (last.parts ?? []).find((p) => p.functionResponse);
      if (answered) {
        chunk(res, [{ text: `Erledigt (Attrappe): ${String(answered.functionResponse.response?.result ?? "").slice(0, 80)}` }], 900);
      } else if (tools > 0 && lastText.includes("timer")) {
        chunk(res, [{ functionCall: { name: "set_timer", args: { duration_seconds: 60, label: "Attrappe" } }, thoughtSignature: "sig-attrappe" }], 1200);
      } else {
        for (const [i, text] of answer.entries()) {
          chunk(res, [{ text }], i === answer.length - 1 ? 1500 : 0);
          await sleep(200);
        }
      }
      res.end(`data: ${JSON.stringify({ candidates: [{ finishReason: "STOP" }] })}\r\n\r\n`);
    });
  })
  .listen(8791, "127.0.0.1", () => console.log(`Gemini-Attrappe auf 127.0.0.1:8791, Modus ${mode}`));
