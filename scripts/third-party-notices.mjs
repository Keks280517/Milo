// Erzeugt THIRD_PARTY_NOTICES.md: alle Bausteine, die in Milo mitgeliefert werden (Rust-Crates fuer
// Windows ohne Entwicklungs-Abhaengigkeiten, die Frontend-Pakete im Bundle und die Schriften), mit
// Lizenz, Quelle, Copyright-Zeilen und den Lizenztexten (gleiche Texte nur einmal).
//   node scripts/third-party-notices.mjs
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// ---------- Rust ----------
const tree = execFileSync("cargo", ["tree", "-e", "normal", "--target", "x86_64-pc-windows-msvc", "--prefix", "none", "-f", "{p}"], { cwd: join(root, "src-tauri"), encoding: "utf8" });
const used = new Set(
  tree
    .split(/\r?\n/)
    .map((l) => l.replace(/ \(.*\)$/, "").trim())
    .filter(Boolean)
    .map((l) => l.split(" ").slice(0, 2).join(" ")),
);
const meta = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--locked"], { cwd: join(root, "src-tauri"), encoding: "utf8", maxBuffer: 1 << 28 }));
const crates = meta.packages
  .filter((p) => used.has(`${p.name} v${p.version}`) && p.name !== "milo")
  .map((p) => ({ name: p.name, version: p.version, license: p.license || "see license file", url: p.repository || p.homepage || `https://crates.io/crates/${p.name}`, dir: dirname(p.manifest_path), authors: p.authors || [] }));

// ---------- Frontend (im Bundle) ----------
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const seen = new Map();
const visit = (name) => {
  if (seen.has(name)) return;
  const dir = join(root, "node_modules", name);
  if (!existsSync(join(dir, "package.json"))) return;
  const p = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  const repo = typeof p.repository === "string" ? p.repository : p.repository?.url;
  seen.set(name, { name, version: p.version, license: p.license || "see license file", url: (repo || p.homepage || `https://www.npmjs.com/package/${name}`).replace(/^git\+/, "").replace(/\.git$/, ""), dir, authors: [typeof p.author === "string" ? p.author : p.author?.name].filter(Boolean) });
  for (const dep of Object.keys(p.dependencies || {})) visit(dep);
};
// Laufzeit, die im gebauten Frontend landet: die Abhaengigkeiten plus Svelte und SvelteKit.
for (const name of [...Object.keys(pkg.dependencies || {}), "svelte", "@sveltejs/kit"]) visit(name);
const web = [...seen.values()].sort((a, b) => a.name.localeCompare(b.name));

// ---------- Lizenztexte ----------
const licenseFiles = (dir) => {
  try {
    return readdirSync(dir).filter((f) => /^(licen[cs]e|copying|notice)/i.test(f) && !/\.(rs|toml|json|js)$/i.test(f));
  } catch {
    return [];
  }
};
const texts = new Map(); // Text -> Namen
const copyrights = (text) =>
  [...new Set(text.split(/\r?\n/).map((l) => l.trim()).filter((l) => /^(©|\(c\)|copyright\b)/i.test(l) && !/^\(c\) (you|if|the|any)\b/i.test(l) &&!/copyright (notice|owner|holder|license|statement)/i.test(l) && l.length < 200))];
const entries = [];
for (const item of [...crates, ...web]) {
  const notes = [];
  for (const file of licenseFiles(item.dir)) {
    const text = readFileSync(join(item.dir, file), "utf8").replace(/\r\n/g, "\n").trim();
    notes.push(...copyrights(text));
    const key = text.replace(/copyright.*$/gim, "").replace(/\s+/g, " ").trim();
    if (!texts.has(key)) texts.set(key, { text, names: [] });
    texts.get(key).names.push(`${item.name} ${item.version}`);
  }
  if (!notes.length && item.authors.length) notes.push(`Copyright (c) ${item.authors.join(", ").replace(/ <[^>]*>/g, "")}`);
  entries.push({ ...item, notes: [...new Set(notes)] });
}

const row = (e) => `| ${e.name} | ${e.version} | ${e.license} | ${e.url} |`;
const out = [
  "# Third-party notices",
  "",
  "Milo includes the following open-source components. Each is used under its own license; the copyright notices and license texts follow below.",
  "",
  "Fonts: **Inter** (© The Inter Project Authors) and **JetBrains Mono** (© The JetBrains Mono Project Authors), both under the SIL Open Font License 1.1, bundled via Fontsource.",
  "",
  "Voice input runs on **sherpa-onnx** (Apache-2.0). The speech models are downloaded separately and are not part of Milo: NVIDIA **Parakeet** (CC-BY-4.0) and optionally OpenAI **Whisper** (MIT).",
  "",
  `## Rust crates (${crates.length})`,
  "",
  "| Name | Version | License | Source |",
  "|---|---|---|---|",
  ...entries.filter((e) => crates.includes(crates.find((c) => c.name === e.name && c.version === e.version))).map(row),
  "",
  `## Frontend packages (${web.length})`,
  "",
  "| Name | Version | License | Source |",
  "|---|---|---|---|",
  ...entries.filter((e) => web.some((w) => w.name === e.name)).map(row),
  "",
  "## Copyright notices",
  "",
  ...entries.filter((e) => e.notes.length).map((e) => `- **${e.name} ${e.version}:** ${e.notes.join("; ")}`),
  "",
  "## License texts",
  "",
  `Components that ship no license file of their own (${entries.filter((e) => !licenseFiles(e.dir).length).map((e) => `${e.name} ${e.version}`).join(", ") || "none"}) are covered by the standard text of the license named in the tables above (for MIT and Apache-2.0, the texts below; others at https://spdx.org/licenses/).`,
  "",
  ...[...texts.values()].flatMap((t) => [`### Used by: ${t.names.join(", ")}`, "", "```", t.text, "```", ""]),
];
writeFileSync(join(root, "THIRD_PARTY_NOTICES.md"), out.join("\n"));
console.log(`${crates.length} Rust-Crates, ${web.length} Frontend-Pakete, ${texts.size} verschiedene Lizenztexte`);
