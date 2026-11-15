// Wie wake-test.mjs, aber wie im echten Betrieb: EIN durchlaufender Tonstrom, alle Aufnahmen in
// zufaelliger Reihenfolge hintereinander, dazwischen leises Rauschen. Je Einstellung 6 Durchlaeufe.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
const sherpa = createRequire(import.meta.url)("sherpa-onnx-node");
const dir = path.join(process.env.LOCALAPPDATA, "de.milo.assistant", "Sprachmodelle", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01");
const f = (n) => path.join(dir, n);
const spellings = ["▁HE Y ▁K U B O", "▁HE Y ▁CO U B O", "▁HE Y ▁C U B O"];
const files = ["ruhig", "laut"].flatMap((kind) => fs.readdirSync(`recordings/${kind}`).filter((n) => n.endsWith(".wav")).map((n) => `${kind}/${n}`));
const waves = Object.fromEntries(files.map((n) => [n, sherpa.readWave(`recordings/${n}`)]));
const noise = (rate, seconds) => Float32Array.from({ length: rate * seconds }, () => (Math.random() - 0.5) * 0.004);
let seed = 7;
const rnd = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);

for (const [score, threshold, trailing, pres, gate] of [[2, 0.2, 1, [0.27], 0.008], [2, 0.2, 1, [0.33], 0.008], [2, 0.2, 1, [0.45], 0.008], [2, 0.2, 1, [0.6], 0.008], [2, 0.2, 1, [0.09, 0.21, 0.33], 0.008], [2, 0.2, 1, [0.06, 0.18, 0.3, 0.42], 0.008], [2, 0.2, 1, [0.06, 0.18, 0.3, 0.42], 0.015]]) {
  const file = path.join(os.tmpdir(), "milo-kws2.txt");
  fs.writeFileSync(file, spellings.map((k) => `${k} :${score} #${threshold} @hey_milo`).join("\n") + "\n");
  const spotter = new sherpa.KeywordSpotter({
    featConfig: { sampleRate: 16000, featureDim: 80 },
    modelConfig: {
      transducer: { encoder: f("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"), decoder: f("decoder-epoch-12-avg-2-chunk-16-left-64.onnx"), joiner: f("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx") },
      tokens: f("tokens.txt"), numThreads: 1, provider: "cpu", debug: 0,
    },
    keywordsFile: file, numTrailingBlanks: trailing,
  });
  let pos = 0, posAll = 0, neg = 0, negAll = 0;
  const wrong = new Set();
  for (let run = 0; run < 12; run++) {
    const order = [...files].sort(() => rnd() - 0.5);
    let stream = spotter.createStream();
    // Wie in der App geplant: 30-ms-Fenster; Stille wird nicht gefuettert. Beim ersten lauten Fenster
    // nach einer Pause beginnt ein frischer Strom mit `pre` Sekunden Vorlauf.
    let quiet = 99, active = false, hist = [], streams = [];
    const maxPre = Math.max(...pres);
    for (const n of order) {
      const wave = waves[n];
      let hit = false;
      const step = Math.round(wave.sampleRate * 0.03);
      for (const samples of [wave.samples, noise(wave.sampleRate, 1.2)]) {
        for (let i = 0; i + step <= samples.length; i += step) {
          const frame = samples.subarray(i, i + step);
          let e = 0; for (const v of frame) e += v * v; e = Math.sqrt(e / frame.length);
          if (e > gate) {
            if (!active) {
              streams = pres.map((pre) => { const st = spotter.createStream(); for (const h of hist.slice(hist.length - Math.round(pre / 0.03))) st.acceptWaveform({ sampleRate: wave.sampleRate, samples: h }); return st; });
              active = true;
            }
            quiet = 0;
          } else quiet++;
          if (active) {
            for (const st of streams) {
              st.acceptWaveform({ sampleRate: wave.sampleRate, samples: frame });
              while (spotter.isReady(st)) { spotter.decode(st); if (spotter.getResult(st).keyword) { hit = true; spotter.reset(st); } }
            }
            if (quiet >= 25) active = false;
          }
          hist.push(frame); while (hist.length > Math.round(maxPre / 0.03) + 1) hist.shift();
        }
      }
      if (n.endsWith("03.wav")) { posAll++; if (hit) pos++; } else { negAll++; if (hit) { neg++; wrong.add(n); } }
    }
  }
  console.log(`score ${score} schwelle ${threshold} vorlauf ${pres.join("/")}s tor ${gate}: erkannt ${pos}/${posAll}, Fehlauslösungen ${neg}/${negAll} ${[...wrong].join(",")}`);
}
