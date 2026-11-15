// Statuszeile fuer Claude Code: gibt Milo die offiziellen Nutzungsgrenzen (5 Stunden, Woche)
// weiter und zeigt sie zugleich kurz unten in Claude Code an.
//
// Claude Code uebergibt die Sitzungsdaten als JSON auf der Standardeingabe. Die Grenzen sind nur
// bei Pro- und Max-Abos dabei und erst nach der ersten Antwort einer Sitzung. Geschrieben wird
// nur eine kleine Datei in Milos Ordner, kein Netzwerk. Diese Datei legt Milo selbst an.

import { mkdirSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

let text = "";
for await (const chunk of process.stdin) text += chunk;
let data = {};
try {
  data = JSON.parse(text.replace(/^﻿/, "").trim());
} catch {}

const window = (w) => (w && typeof w.used_percentage === "number" ? { usedPercent: w.used_percentage, resetsAt: Number(w.resets_at ?? 0) } : null);
const limits = {
  fiveHour: window(data.rate_limits?.five_hour),
  sevenDay: window(data.rate_limits?.seven_day),
  model: String(data.model?.display_name ?? ""),
  context: typeof data.context_window?.used_percentage === "number" ? data.context_window.used_percentage : null,
  at: Date.now(),
};

try {
  const dir = join(process.env.USERPROFILE ?? "", ".claude", "milo", "bridge");
  mkdirSync(dir, { recursive: true });
  if (limits.fiveHour || limits.sevenDay) {
    writeFileSync(join(dir, "limits.json.tmp"), JSON.stringify(limits));
    renameSync(join(dir, "limits.json.tmp"), join(dir, "limits.json"));
  }
} catch {}

const parts = [limits.model];
if (limits.fiveHour) parts.push(`5h ${Math.round(limits.fiveHour.usedPercent)} %`);
if (limits.sevenDay) parts.push(`Woche ${Math.round(limits.sevenDay.usedPercent)} %`);
if (limits.context !== null) parts.push(`Kontext ${Math.round(limits.context)} %`);
process.stdout.write(parts.filter(Boolean).join(" · "));
