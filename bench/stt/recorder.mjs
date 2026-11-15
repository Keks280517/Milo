// Kleine lokale Aufnahmeseite fuer den Spracherkennungs-Vergleich.
//   node bench/stt/recorder.mjs   ->  http://localhost:5179
// Speichert 16-kHz-Mono-WAVs nach bench/stt/recordings/<ruhig|laut>/NN.wav. Nichts verlaesst den PC.

import { createServer } from "node:http";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const DIR = path.dirname(fileURLToPath(import.meta.url));
const REC = path.join(DIR, "recordings");
const SETS = ["ruhig", "laut"];
const PORT = 5179;
const MAX_BYTES = 20 * 2 ** 20;

const sentences = JSON.parse(readFileSync(path.join(DIR, "sentences.json"), "utf8"));
const fileFor = (set, i) => path.join(REC, set, String(i + 1).padStart(2, "0") + ".wav");

function status() {
  return Object.fromEntries(SETS.map((set) => {
    const dir = path.join(REC, set);
    const have = existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith(".wav")).map((f) => parseInt(f, 10) - 1) : [];
    return [set, have];
  }));
}

const PAGE = /* html */ `<!doctype html>
<html lang="de"><head><meta charset="utf-8"><title>Milo – Sprachaufnahmen</title>
<style>
  body { margin: 0; background: #0b0b0d; color: #f5f5f7; font: 16px "Segoe UI", system-ui, sans-serif; }
  main { max-width: 760px; margin: 0 auto; padding: 32px 20px 60px; }
  h1 { font-size: 22px; margin: 0 0 6px; } p { color: #a1a1a6; line-height: 1.5; }
  .tabs { display: flex; gap: 8px; margin: 22px 0; }
  .tabs button { flex: 1; padding: 12px; border-radius: 12px; border: 1px solid #2c2c2e; background: #1c1c1e; color: inherit; font: inherit; cursor: pointer; }
  .tabs button.on { background: #f5f5f7; color: #000; }
  .card { background: #1c1c1e; border-radius: 20px; padding: 28px; text-align: center; }
  .count { color: #a1a1a6; font-size: 14px; }
  .sentence { font-size: 26px; line-height: 1.35; margin: 18px 0 26px; min-height: 72px; }
  .rec { width: 100%; padding: 18px; border: 0; border-radius: 14px; font: 600 18px "Segoe UI", sans-serif; cursor: pointer; background: #f5f5f7; color: #000; }
  .rec.live { background: #ff453a; color: #fff; }
  .row { display: flex; gap: 8px; margin-top: 12px; }
  .row button { flex: 1; padding: 10px; border-radius: 10px; border: 1px solid #2c2c2e; background: transparent; color: inherit; font: inherit; cursor: pointer; }
  .row button:disabled { opacity: .35; cursor: default; }
  #msg { margin-top: 14px; min-height: 22px; color: #a1a1a6; font-size: 14px; }
  .dots { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 22px; justify-content: center; }
  .dots button { width: 34px; height: 34px; border-radius: 50%; border: 1px solid #3a3a3c; background: transparent; color: #a1a1a6; cursor: pointer; }
  .dots button.done { background: #30d158; border-color: #30d158; color: #000; }
  .dots button.cur { outline: 2px solid #f5f5f7; outline-offset: 2px; }
  .ok { color: #30d158; } .err { color: #ff453a; }
</style></head><body><main>
  <h1>Sprachaufnahmen für Milo</h1>
  <p>Lies jeden Satz so vor, wie du ihn dem Assistenten sagen würdest, in normalem Abstand zum Mikrofon.
     <b>Leertaste</b> startet und stoppt die Aufnahme. Danach geht es automatisch zum nächsten Satz.
     Die Dateien bleiben auf deinem PC.</p>
  <div class="tabs">
    <button data-set="ruhig">1. Durchgang: ruhig</button>
    <button data-set="laut">2. Durchgang: mit Musik oder Spiel im Hintergrund</button>
  </div>
  <div class="card">
    <div class="count" id="count"></div>
    <div class="sentence" id="sentence"></div>
    <button class="rec" id="rec">Aufnehmen (Leertaste)</button>
    <div class="row">
      <button id="prev">← Zurück</button>
      <button id="play" disabled>Anhören</button>
      <button id="next">Weiter →</button>
    </div>
    <div id="msg"></div>
  </div>
  <div class="dots" id="dots"></div>
</main>
<script type="module">
const $ = (id) => document.getElementById(id);
const sentences = await (await fetch("/sentences")).json();
let have = await (await fetch("/status")).json();
let set = "ruhig", index = 0, recording = false, ctx, node, stream, chunks = [], lastUrl = null;

const WORKLET = URL.createObjectURL(new Blob([
  "class Tap extends AudioWorkletProcessor { process(inputs) { const c = inputs[0][0]; if (c) this.port.postMessage(c.slice(0)); return true; } } registerProcessor('tap', Tap);"
], { type: "application/javascript" }));

function render() {
  for (const b of document.querySelectorAll(".tabs button")) b.classList.toggle("on", b.dataset.set === set);
  $("count").textContent = "Satz " + (index + 1) + " von " + sentences.length + (have[set].includes(index) ? " · schon aufgenommen" : "");
  $("sentence").textContent = sentences[index];
  $("dots").innerHTML = "";
  sentences.forEach((_, i) => {
    const b = document.createElement("button");
    b.textContent = i + 1;
    b.className = (have[set].includes(i) ? "done " : "") + (i === index ? "cur" : "");
    b.onclick = () => { if (!recording) { index = i; render(); } };
    $("dots").append(b);
  });
  const all = have[set].length === sentences.length;
  if (all && !recording) say(set === "ruhig"
    ? "Durchgang 1 ist komplett. Jetzt bitte oben den 2. Durchgang wählen und Musik oder ein Spiel laufen lassen."
    : "Durchgang 2 ist komplett. Du kannst die Seite schließen und Claude Bescheid geben.", "ok");
}
function say(text, cls = "") { $("msg").textContent = text; $("msg").className = cls; }

async function start() {
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: {
      channelCount: 1, echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
  } catch (err) { say("Kein Zugriff aufs Mikrofon: " + err.message, "err"); return; }
  ctx = new AudioContext();
  await ctx.audioWorklet.addModule(WORKLET);
  node = new AudioWorkletNode(ctx, "tap");
  chunks = [];
  node.port.onmessage = (e) => chunks.push(e.data);
  ctx.createMediaStreamSource(stream).connect(node);
  recording = true;
  $("rec").classList.add("live"); $("rec").textContent = "Stopp (Leertaste)";
  say("Aufnahme läuft …");
}

async function stop() {
  recording = false;
  $("rec").classList.remove("live"); $("rec").textContent = "Aufnehmen (Leertaste)";
  await new Promise((r) => setTimeout(r, 150)); // letzte Bloecke abwarten
  const rate = ctx.sampleRate;
  stream.getTracks().forEach((t) => t.stop());
  await ctx.close();
  const length = chunks.reduce((n, c) => n + c.length, 0);
  if (length < rate * 0.4) { say("Zu kurz, bitte noch einmal.", "err"); return; }
  const raw = new Float32Array(length);
  let o = 0; for (const c of chunks) { raw.set(c, o); o += c.length; }

  // Auf 16 kHz umrechnen (das Format, das alle Erkenner erwarten).
  const off = new OfflineAudioContext(1, Math.ceil(length * 16000 / rate), 16000);
  const buf = off.createBuffer(1, length, rate); buf.copyToChannel(raw, 0);
  const src = off.createBufferSource(); src.buffer = buf; src.connect(off.destination); src.start();
  const pcm = (await off.startRendering()).getChannelData(0);

  const wav = new DataView(new ArrayBuffer(44 + pcm.length * 2));
  const text = (p, s) => [...s].forEach((ch, i) => wav.setUint8(p + i, ch.charCodeAt(0)));
  text(0, "RIFF"); wav.setUint32(4, 36 + pcm.length * 2, true); text(8, "WAVEfmt ");
  wav.setUint32(16, 16, true); wav.setUint16(20, 1, true); wav.setUint16(22, 1, true);
  wav.setUint32(24, 16000, true); wav.setUint32(28, 32000, true); wav.setUint16(32, 2, true); wav.setUint16(34, 16, true);
  text(36, "data"); wav.setUint32(40, pcm.length * 2, true);
  let peak = 0;
  for (let i = 0; i < pcm.length; i++) {
    const s = Math.max(-1, Math.min(1, pcm[i])); peak = Math.max(peak, Math.abs(s));
    wav.setInt16(44 + i * 2, s * 0x7fff, true);
  }
  const blob = new Blob([wav], { type: "audio/wav" });
  const res = await fetch("/save?set=" + set + "&i=" + index, { method: "POST", body: blob });
  if (!res.ok) { say("Speichern fehlgeschlagen: " + await res.text(), "err"); return; }
  if (lastUrl) URL.revokeObjectURL(lastUrl);
  lastUrl = URL.createObjectURL(blob); $("play").disabled = false;
  have = await (await fetch("/status")).json();
  const quiet = peak < 0.05 ? " Achtung: sehr leise, bitte Mikrofon prüfen." : "";
  say("Gespeichert (" + (pcm.length / 16000).toFixed(1) + " s)." + quiet, quiet ? "err" : "ok");
  const next = sentences.findIndex((_, i) => !have[set].includes(i));
  if (next >= 0) index = next;
  render();
}

const toggle = () => (recording ? stop() : start());
$("rec").onclick = toggle;
$("prev").onclick = () => { if (!recording && index > 0) { index--; render(); } };
$("next").onclick = () => { if (!recording && index < sentences.length - 1) { index++; render(); } };
$("play").onclick = () => lastUrl && new Audio(lastUrl).play();
for (const b of document.querySelectorAll(".tabs button")) b.onclick = () => {
  if (recording) return;
  set = b.dataset.set; say("");
  const next = sentences.findIndex((_, i) => !have[set].includes(i)); index = next >= 0 ? next : 0; render();
};
addEventListener("keydown", (e) => { if (e.code === "Space" && !e.repeat) { e.preventDefault(); toggle(); } });
render();
</script></body></html>`;

createServer((req, res) => {
  const url = new URL(req.url, "http://localhost");
  const json = (data) => { res.writeHead(200, { "content-type": "application/json; charset=utf-8" }); res.end(JSON.stringify(data)); };

  if (req.method === "GET" && url.pathname === "/") {
    res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    return res.end(PAGE);
  }
  if (req.method === "GET" && url.pathname === "/sentences") return json(sentences);
  if (req.method === "GET" && url.pathname === "/status") return json(status());

  if (req.method === "POST" && url.pathname === "/save") {
    const set = url.searchParams.get("set");
    const i = Number(url.searchParams.get("i"));
    if (!SETS.includes(set) || !Number.isInteger(i) || i < 0 || i >= sentences.length) {
      res.writeHead(400); return res.end("Ungültige Angabe");
    }
    const parts = [];
    let size = 0;
    req.on("data", (c) => {
      size += c.length;
      if (size > MAX_BYTES) { res.writeHead(413); res.end("Zu groß"); req.destroy(); return; }
      parts.push(c);
    });
    req.on("end", () => {
      if (res.writableEnded) return;
      mkdirSync(path.join(REC, set), { recursive: true });
      writeFileSync(fileFor(set, i), Buffer.concat(parts));
      console.log(`gespeichert: ${set}/${String(i + 1).padStart(2, "0")}.wav (${size} Bytes)`);
      res.writeHead(200); res.end("ok");
    });
    return;
  }
  res.writeHead(404); res.end("Nicht gefunden");
}).listen(PORT, "127.0.0.1", () => console.log(`Aufnahmeseite: http://localhost:${PORT}`));
