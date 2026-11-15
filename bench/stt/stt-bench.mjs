// Vergleich lokaler Spracherkenner mit den eigenen Aufnahmen des Nutzers.
//   node bench/stt/stt-bench.mjs            -> alle Kandidaten, je in eigenem Prozess (saubere RAM-Messung)
//   node bench/stt/stt-bench.mjs --one <id> -> ein Kandidat, Ergebnis als JSON auf stdout
//
// Alles laeuft auf der CPU (int8), damit der Grafikspeicher dem Sprachmodell bleibt.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const DIR = path.dirname(fileURLToPath(import.meta.url));
const MODELS = path.join(process.env.LOCALAPPDATA, "de.milo.assistant", "Sprachmodelle");
const THREADS = 4;
const SETS = ["ruhig", "laut"];

const find = (dir, pattern) => {
  const hit = readdirSync(dir).find((f) => pattern.test(f));
  if (!hit) throw new Error(`Datei ${pattern} fehlt in ${dir}`);
  return path.join(dir, hit);
};
const whisper = (name) => () => {
  const dir = path.join(MODELS, `sherpa-onnx-whisper-${name}`);
  return {
    whisper: {
      encoder: find(dir, /encoder\.int8\.onnx$/), decoder: find(dir, /decoder\.int8\.onnx$/),
      language: "de", task: "transcribe",
    },
    tokens: find(dir, /tokens\.txt$/),
  };
};

const CANDIDATES = {
  "parakeet-tdt-0.6b-v3": () => {
    const dir = path.join(MODELS, "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8");
    return {
      transducer: {
        encoder: find(dir, /^encoder.*\.onnx$/), decoder: find(dir, /^decoder.*\.onnx$/), joiner: find(dir, /^joiner.*\.onnx$/),
      },
      tokens: find(dir, /tokens\.txt$/),
      modelType: "nemo_transducer",
    };
  },
  "whisper-base": whisper("base"),
  "whisper-small": whisper("small"),
  "whisper-turbo": whisper("turbo"),
};

// ---------- Fehlerquote ----------

const NUMBERS = { "10": "zehn", "15": "fünfzehn", "20": "zwanzig", "1": "eine" };
/** Vereinheitlicht Schreibweisen, die fuer die Bedeutung egal sind (Gross/Klein, Satzzeichen, Ziffern). */
export function normalize(text) {
  return text
    .toLowerCase()
    .replace(/(\d+)\s*uhr/g, (_, n) => `${NUMBERS[n] ?? n} uhr`)
    .replace(/\b\d+\b/g, (n) => NUMBERS[n] ?? n)
    .replace(/ß/g, "ss")
    .replace(/[-‐–]/g, " ")
    .replace(/[^\p{L}\p{N}\s]/gu, "")
    .replace(/\s+/g, " ")
    .trim()
    .split(" ")
    .filter(Boolean);
}

/** Wort-Abstand nach Levenshtein: wie viele Woerter ersetzt, eingefuegt oder ausgelassen wurden. */
export function wordErrors(ref, hyp) {
  const d = Array.from({ length: ref.length + 1 }, (_, i) => [i, ...Array(hyp.length).fill(0)]);
  for (let j = 0; j <= hyp.length; j++) d[0][j] = j;
  for (let i = 1; i <= ref.length; i++) {
    for (let j = 1; j <= hyp.length; j++) {
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (ref[i - 1] === hyp[j - 1] ? 0 : 1));
    }
  }
  return d[ref.length][hyp.length];
}

// ---------- Ein Kandidat (laeuft im Kindprozess) ----------

function readWav(file) {
  const b = readFileSync(file);
  const n = (b.length - 44) / 2;
  const samples = new Float32Array(n);
  for (let i = 0; i < n; i++) samples[i] = b.readInt16LE(44 + i * 2) / 32768;
  return { samples, sampleRate: b.readUInt32LE(24) };
}

function runOne(id) {
  const sherpa = createRequire(import.meta.url)("sherpa-onnx-node");
  const sentences = JSON.parse(readFileSync(path.join(DIR, "sentences.json"), "utf8"));
  const alternatives = JSON.parse(readFileSync(path.join(DIR, "alternatives.json"), "utf8"));
  const rss = () => Math.round(process.memoryUsage().rss / 2 ** 20);

  const rssBefore = rss();
  const t0 = performance.now();
  const recognizer = new sherpa.OfflineRecognizer({
    featConfig: { sampleRate: 16000, featureDim: 80 },
    modelConfig: { ...CANDIDATES[id](), numThreads: THREADS, provider: "cpu", debug: 0 },
  });
  const loadMs = Math.round(performance.now() - t0);
  const rssLoaded = rss();
  let rssPeak = rssLoaded;

  const decode = (wav) => {
    const stream = recognizer.createStream();
    stream.acceptWaveform({ sampleRate: wav.sampleRate, samples: wav.samples });
    recognizer.decode(stream);
    return recognizer.getResult(stream).text.trim();
  };

  // Ein Aufwaermlauf, damit der erste gemessene Satz nicht die einmaligen Startkosten traegt.
  const warm = readWav(path.join(DIR, "recordings", "ruhig", "01.wav"));
  const tw = performance.now();
  decode(warm);
  const firstDecodeMs = Math.round(performance.now() - tw);

  const sets = {};
  for (const set of SETS) {
    const rows = [];
    for (let i = 0; i < sentences.length; i++) {
      const wav = readWav(path.join(DIR, "recordings", set, String(i + 1).padStart(2, "0") + ".wav"));
      const cpu0 = process.cpuUsage();
      const t = performance.now();
      const text = decode(wav);
      const ms = performance.now() - t;
      const cpu = process.cpuUsage(cpu0);
      rssPeak = Math.max(rssPeak, rss());
      const hyp = normalize(text);
      // Bestes Ergebnis ueber Vorlesetext und zulaessige Abweichungen (siehe alternatives.json).
      const ref = [sentences[i], ...(alternatives[i + 1] ?? [])]
        .map(normalize)
        .reduce((best, cand) => (wordErrors(cand, hyp) < wordErrors(best, hyp) ? cand : best));
      // Zeichenfehler ohne Leerzeichen: bestraft "Zahnarzt Termin" statt "Zahnarzttermin" nicht.
      const refChars = [...ref.join("")], hypChars = [...hyp.join("")];
      rows.push({
        n: i + 1, text, errors: wordErrors(ref, hyp), words: ref.length,
        charErrors: wordErrors(refChars, hypChars), chars: refChars.length,
        ms: Math.round(ms), audioS: wav.samples.length / wav.sampleRate,
        cpuCores: (cpu.user + cpu.system) / 1000 / ms,
      });
    }
    const errors = rows.reduce((a, r) => a + r.errors, 0);
    const words = rows.reduce((a, r) => a + r.words, 0);
    const times = rows.map((r) => r.ms).sort((a, b) => a - b);
    sets[set] = {
      werPercent: Math.round((errors / words) * 1000) / 10,
      wordErrors: errors, words,
      cerPercent: Math.round((rows.reduce((a, r) => a + r.charErrors, 0) / rows.reduce((a, r) => a + r.chars, 0)) * 1000) / 10,
      sentencesPerfect: rows.filter((r) => r.errors === 0).length,
      delayMedianMs: times[Math.floor(times.length / 2)],
      delayMaxMs: times.at(-1),
      realtimeFactor: Math.round((rows.reduce((a, r) => a + r.ms, 0) / 1000 / rows.reduce((a, r) => a + r.audioS, 0)) * 100) / 100,
      cpuCoresAvg: Math.round((rows.reduce((a, r) => a + r.cpuCores, 0) / rows.length) * 10) / 10,
      rows,
    };
  }
  // Fuer die Gesamt-RAM-Messung: Erkenner geladen halten, waehrend von aussen gemessen wird.
  const hold = Number(process.env.MILO_STT_HOLD_SECONDS ?? 0);
  if (hold > 0) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, hold * 1000);
  return { id, threads: THREADS, loadMs, firstDecodeMs, ramLoadedMiB: rssLoaded - rssBefore, ramPeakMiB: rssPeak - rssBefore, sets };
}

// ---------- Ablauf ----------

if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args[0] === "--one") {
    process.stdout.write("\n@@RESULT@@" + JSON.stringify(runOne(args[1])));
  } else {
    const sentences = JSON.parse(readFileSync(path.join(DIR, "sentences.json"), "utf8"));
    const results = [];
    for (const id of args.length ? args : Object.keys(CANDIDATES)) {
      console.log(`\n===== ${id} =====`);
      try {
        const out = execFileSync(process.execPath, [fileURLToPath(import.meta.url), "--one", id], {
          maxBuffer: 64 * 2 ** 20, stdio: ["ignore", "pipe", "pipe"],
        }).toString();
        const r = JSON.parse(out.slice(out.indexOf("@@RESULT@@") + 10));
        results.push(r);
        console.log(`Laden ${r.loadMs} ms, erster Satz ${r.firstDecodeMs} ms, RAM geladen ${r.ramLoadedMiB} MiB, Spitze ${r.ramPeakMiB} MiB`);
        for (const set of SETS) {
          const s = r.sets[set];
          console.log(`[${set}] Fehlerquote ${s.werPercent} % (${s.wordErrors}/${s.words}), Zeichenfehler ${s.cerPercent} %, ${s.sentencesPerfect}/22 Saetze fehlerfrei, Verzoegerung Median ${s.delayMedianMs} ms / max ${s.delayMaxMs} ms, ${s.cpuCoresAvg} Kerne`);
          for (const row of s.rows) if (row.errors) console.log(`   ${String(row.n).padStart(2)} (${row.errors}) ${row.text}\n        soll: ${sentences[row.n - 1]}`);
        }
      } catch (err) {
        console.error(`Abbruch bei ${id}: ${err.stderr?.toString().slice(-600) || err}`);
      }
    }
    const dir = path.join(DIR, "results");
    mkdirSync(dir, { recursive: true });
    writeFileSync(path.join(dir, "stt.json"), JSON.stringify({ date: new Date().toISOString(), cpu: os.cpus()[0].model, results }, null, 2));
  }
}
