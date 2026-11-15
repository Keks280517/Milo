// Spike Aktivierungswort: "Hey Milo" mit dem englischen Schluesselwort-Modell von sherpa-onnx.
// Positive: Satz 03 (ruhig + laut) beginnt mit "Hey Milo". Negative: alle anderen 42 Aufnahmen.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
const sherpa = createRequire(import.meta.url)("sherpa-onnx-node");

const dir = path.join(process.env.LOCALAPPDATA, "de.milo.assistant", "Sprachmodelle", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01");
const f = (n) => path.join(dir, n);
const variants = {
  A: ["▁HE Y ▁K U B O"],
  B: ["▁HE Y ▁K U B O", "▁HE Y ▁CO U B O", "▁HE Y ▁C U B O"],
  C: ["▁HE Y ▁K U B O", "▁HE Y ▁CO B O", "▁HE Y ▁C U B O", "▁HE Y ▁K O B O"],
};
const files = ["ruhig", "laut"].flatMap((kind) => fs.readdirSync(`recordings/${kind}`).filter((n) => n.endsWith(".wav")).map((n) => `${kind}/${n}`));
const waves = Object.fromEntries(files.map((n) => [n, sherpa.readWave(`recordings/${n}`)]));

for (const [name, keywords] of Object.entries(variants)) {
  for (const score of [1.0, 1.5, 2.0]) {
    for (const threshold of [0.35, 0.22, 0.12, 0.06]) {
      const file = path.join(os.tmpdir(), `milo-kws-${name}.txt`);
      fs.writeFileSync(file, keywords.map((k) => `${k} :${score} #${threshold} @hey_milo`).join("\n") + "\n");
      let spotter;
      try {
        spotter = new sherpa.KeywordSpotter({
          featConfig: { sampleRate: 16000, featureDim: 80 },
          modelConfig: {
            transducer: { encoder: f("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"), decoder: f("decoder-epoch-12-avg-2-chunk-16-left-64.onnx"), joiner: f("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx") },
            tokens: f("tokens.txt"), numThreads: 1, provider: "cpu", debug: 0,
          },
          keywordsFile: file,
        });
      } catch (err) {
        console.log(name, "nicht ladbar:", String(err).slice(0, 120));
        break;
      }
      const hits = [];
      let ms = 0, audio = 0;
      for (const [n, wave] of Object.entries(waves)) {
        const stream = spotter.createStream();
        const t = performance.now();
        stream.acceptWaveform({ sampleRate: wave.sampleRate, samples: wave.samples });
        stream.acceptWaveform({ sampleRate: wave.sampleRate, samples: new Float32Array(wave.sampleRate * 0.6) });
        let hit = false;
        while (spotter.isReady(stream)) {
          spotter.decode(stream);
          if (spotter.getResult(stream).keyword) { hit = true; spotter.reset(stream); }
        }
        ms += performance.now() - t;
        audio += wave.samples.length / wave.sampleRate;
        if (hit) hits.push(n);
      }
      const pos = hits.filter((n) => n.endsWith("03.wav")).length;
      const neg = hits.filter((n) => !n.endsWith("03.wav"));
      console.log(`${name} score ${score} schwelle ${threshold}: erkannt ${pos}/2, Fehlauslösungen ${neg.length}/42 ${neg.join(",")}  | Rechenzeit ${(100 * ms / 1000 / audio).toFixed(1)} % der Tonlänge`);
    }
  }
}
