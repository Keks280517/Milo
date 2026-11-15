// Aktivierungswort "Hey Milo": deutsche (Mii-lo) und englische (Mai-lo) Aussprache.
// Positive: milo/de-*, milo/en-* (Windows-Stimmen). Negative: milo/neg-* und alle 42 alten Aufnahmen.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
const sherpa = createRequire(import.meta.url)("sherpa-onnx-node");
const dir = path.join(process.env.LOCALAPPDATA, "de.milo.assistant", "Sprachmodelle", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01");
const f = (n) => path.join(dir, n);
const variants = {
  A: ["▁HE Y ▁MI LO"],
  B: ["▁HE Y ▁MI LO", "▁HE Y ▁MY LO"],
  C: ["▁HE Y ▁MI LO", "▁HE Y ▁MY LO", "▁HE Y ▁ME LO"],
  D: ["▁HE Y ▁MI LO", "▁HE Y ▁MY LO", "▁HE Y ▁ME LO", "▁HE Y ▁MI L O", "▁HE Y ▁ME L O"],
};
const pos = fs.readdirSync("milo").filter((n) => /^(de|en)-.*\.wav$/.test(n)).map((n) => `milo/${n}`);
const negNew = fs.readdirSync("milo").filter((n) => n.startsWith("neg-")).map((n) => `milo/${n}`);
const negOld = ["ruhig", "laut"].flatMap((k) => fs.readdirSync(`recordings/${k}`).filter((n) => n.endsWith(".wav") && !n.endsWith("03.wav")).map((n) => `recordings/${k}/${n}`));
const all = [...pos, ...negNew, ...negOld];
const waves = Object.fromEntries(all.map((n) => [n, sherpa.readWave(n)]));
for (const [name, keywords] of Object.entries(variants)) {
  for (const [score, threshold] of [[1.5, 0.35], [2.0, 0.2], [2.5, 0.1]]) {
    const file = path.join(os.tmpdir(), `milo-kws-${name}.txt`);
    fs.writeFileSync(file, keywords.map((k) => `${k} :${score} #${threshold} @hey_milo`).join("\n") + "\n");
    const spotter = new sherpa.KeywordSpotter({
      featConfig: { sampleRate: 16000, featureDim: 80 },
      modelConfig: { transducer: { encoder: f("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"), decoder: f("decoder-epoch-12-avg-2-chunk-16-left-64.onnx"), joiner: f("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx") }, tokens: f("tokens.txt"), numThreads: 1, provider: "cpu", debug: 0 },
      keywordsFile: file,
    });
    const hits = [];
    for (const n of all) {
      const wave = waves[n];
      // Wie in der App: kurzer Vorlauf mit Stille, mehrere Versuche mit versetztem Vorlauf.
      let hit = false;
      for (const pre of [0.06, 0.18, 0.3, 0.42]) {
        const stream = spotter.createStream();
        stream.acceptWaveform({ sampleRate: 16000, samples: new Float32Array(Math.round(16000 * pre)) });
        stream.acceptWaveform({ sampleRate: wave.sampleRate, samples: wave.samples });
        stream.acceptWaveform({ sampleRate: 16000, samples: new Float32Array(16000 * 0.6) });
        while (spotter.isReady(stream)) {
          spotter.decode(stream);
          if (spotter.getResult(stream).keyword) { hit = true; break; }
        }
        if (hit) break;
      }
      if (hit) hits.push(n);
    }
    const p = hits.filter((n) => pos.includes(n));
    const missed = pos.filter((n) => !hits.includes(n)).map((n) => path.basename(n));
    const neg = hits.filter((n) => !pos.includes(n)).map((n) => path.basename(n));
    console.log(`${name} ${score}/${threshold}: erkannt ${p.length}/${pos.length} (verpasst: ${missed.join(",") || "-"}) | Fehlauslösungen ${neg.length}/${negNew.length + negOld.length} ${neg.join(",")}`);
  }
}
