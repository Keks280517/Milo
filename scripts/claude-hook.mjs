// Hook fuer Claude Code: meldet Milo, was Claude gerade tut (arbeitet, fragt, braucht eine
// Bestaetigung, ist fertig). Claude Code startet dieses Skript bei den eingetragenen Ereignissen
// und uebergibt die Einzelheiten als JSON auf der Standardeingabe.
//
// Es schreibt pro Sitzung eine kleine Datei in Milos Ordner, kein Netzwerk. Uebertragen werden
// Ereignisname, Werkzeugname, Claudes Hinweistext, der Projektordner, der Pfad zum Verlauf der
// Sitzung und fuer die Live-Ansicht der Dateiname samt einem Ausschnitt dessen, was Claude gerade
// schreibt (alt und neu, damit Milo die Aenderung zeigen kann).
//
// Rueckfragen und Freigaben: Laeuft Milo und sind Freigaben dort eingeschaltet (Herzschlag-Datei
// frisch), zeigt Milo die Anfrage. Bei Freigaben wartet das Skript hoechstens 25 Sekunden, bei
// Rueckfragen von Claude (AskUserQuestion) hoechstens knapp 5 Minuten. Kommt keine Antwort, oder
// will der Nutzer lieber in Claude antworten, entscheidet es nichts und Claude fragt wie gewohnt
// selbst. Sonst endet es immer sofort und erfolgreich, damit Claude nie aufgehalten wird.
//
// Diese Datei legt Milo selbst an und haelt sie aktuell.

import { closeSync, existsSync, mkdirSync, openSync, readdirSync, readFileSync, readSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const event = process.argv[2] ?? "";
// Nicht in AppData: die Claude-Desktop-App leitet AppData-Schreibzugriffe in ihren Paketordner um.
const home = process.env.USERPROFILE ?? "";
const dir = join(home, ".claude", "milo", "bridge");
const SNIPPET = 4000;
const PERMISSION_WAIT_MS = 25000;
// Claude bricht den Hook nach 300 s ab (Zeitlimit in den Einstellungen); vorher selbst aufgeben.
const QUESTION_WAIT_MS = 285000;
// Frage am Ende eines Zugs: so lange in Milo beantwortbar (Zeitlimit des Stop-Hooks: 150 s).
const STOP_WAIT_MS = 120000;

async function readInput() {
  let text = "";
  for await (const chunk of process.stdin) text += chunk;
  try {
    // Manche Aufrufer stellen der Eingabe eine unsichtbare Markierung (BOM) voran.
    return JSON.parse(text.replace(/^﻿/, "").trim());
  } catch {
    return {};
  }
}

function writeAtomic(file, data) {
  // Erst daneben schreiben, dann umbenennen: so liest Milo nie eine halbe Datei.
  writeFileSync(file + ".tmp", JSON.stringify(data));
  renameSync(file + ".tmp", file);
}

/** Datei und Ausschnitt fuer die Live-Ansicht, mit dem ersetzten Text fuer die Aenderungsansicht. */
function liveView(tool, input) {
  const file = String(input.file_path ?? input.notebook_path ?? input.path ?? "");
  let snippet = "";
  let old = "";
  if (typeof input.new_string === "string") {
    snippet = input.new_string;
    old = typeof input.old_string === "string" ? input.old_string : "";
  } else if (typeof input.content === "string") snippet = input.content;
  else if (Array.isArray(input.edits)) {
    snippet = input.edits.map((e) => String(e?.new_string ?? "")).join("\n…\n");
    old = input.edits.map((e) => String(e?.old_string ?? "")).join("\n…\n");
  } else if (typeof input.new_source === "string") snippet = input.new_source;
  else if (tool === "Bash" && typeof input.command === "string") snippet = input.command;
  return { file, snippet: snippet.slice(0, SNIPPET), old: old.slice(0, SNIPPET) };
}

/** Text einer Nachricht (Zeichenkette oder Liste von Bausteinen). */
function textOf(content) {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content.filter((c) => c?.type === "text").map((c) => String(c.text ?? "")).join("\n");
}

/**
 * Was bedeutet dieses "Stop"? Claude hoert nicht nur auf, wenn es fertig ist:
 * - "question": Claude stellt am Ende eine Frage oder bietet eine Entscheidung an (z. B. die
 *   Rueckfrage im Naechster-Schritt-Kasten der Claude-App) und wartet auf dich.
 * - "background": im Hintergrund laeuft noch ein Befehl, dessen Ergebnis Claude abwartet;
 *   Claude macht danach von selbst weiter.
 * - "": wirklich fertig.
 * Liest nur das Ende des Sitzungsverlaufs (lokale Datei), nie mehr als 600 KB.
 */
function stopKind(transcript) {
  try {
    if (!transcript || !existsSync(transcript)) return { kind: "", message: "", options: [] };
    const size = statSync(transcript).size;
    const want = Math.min(size, 600 * 1024);
    const buffer = Buffer.alloc(want);
    const fd = openSync(transcript, "r");
    readSync(fd, buffer, 0, want, size - want);
    closeSync(fd);
    const tail = buffer.toString("utf8");

    // Hintergrundbefehle: gestartet, aber noch keine Meldung, dass sie fertig sind.
    const started = new Set([...tail.matchAll(/running in background with ID: ([A-Za-z0-9_-]+)/g)].map((m) => m[1]));
    for (const m of tail.matchAll(/<task-id>([A-Za-z0-9_-]+)<\/task-id>[\s\S]{0,600}?<status>(completed|failed|killed|stopped|cancelled)<\/status>/g)) started.delete(m[1]);
    for (const m of tail.matchAll(/task_id\\?"?\s*[:=]\s*\\?"?([A-Za-z0-9_-]+)[\s\S]{0,200}?(stopped|killed)/g)) started.delete(m[1]);

    // Letzter Zug von Claude: alle Assistenten-Eintraege nach der letzten echten Nutzernachricht.
    const lines = tail.split("\n").filter(Boolean);
    let lastText = "";
    let ask = "";
    const options = [];
    for (let i = lines.length - 1; i >= 0; i--) {
      let entry;
      try {
        entry = JSON.parse(lines[i]);
      } catch {
        continue;
      }
      const content = entry?.message?.content;
      if (entry?.type === "user") {
        const real = typeof content === "string" || (Array.isArray(content) && content.some((c) => c?.type === "text"));
        if (real) break;
        continue;
      }
      if (entry?.type !== "assistant" || !Array.isArray(content)) continue;
      if (!lastText) lastText = textOf(content).trim();
      for (const c of content) {
        if (c?.type !== "tool_use") continue;
        const name = String(c.name ?? "");
        // Naechster-Schritt-Kasten der Claude-App mit einer Frage an dich und Antwortknoepfen
        if (name.endsWith("wrap_up") && typeof c.input?.ask === "string" && c.input.ask.trim() && !ask) {
          ask = c.input.ask.trim();
          let actions = c.input.actions;
          if (typeof actions === "string") {
            try {
              actions = JSON.parse(actions);
            } catch {
              actions = [];
            }
          }
          for (const a of Array.isArray(actions) ? actions : []) {
            const label = String(a?.label ?? "").trim() || ACTION_LABEL[String(a?.action ?? "")] || "";
            if (label && !options.includes(label)) options.push(label);
          }
        }
      }
    }
    if (started.size > 0) return { kind: "background", message: "", options: [] };
    if (ask) return { kind: "question", message: ask, options };
    // Endet Claudes letzte Nachricht mit einer Frage, wartet es auf dich. Nummerierte Punkte davor
    // ("1. …", "2. …") werden zu Antwortmoeglichkeiten.
    const textLines = lastText.split("\n").map((l) => l.trim()).filter(Boolean);
    const lastLine = textLines[textLines.length - 1] ?? "";
    if (/[?？]\s*\**\s*$/.test(lastLine)) {
      const numbered = textLines
        .map((l) => l.match(/^\**\s*(\d{1,2})[.)]\s*\**\s*(.+)$/))
        .filter(Boolean)
        .slice(-6)
        .map((m) => `${m[1]}. ${m[2].replace(/\*+/g, "").replace(/\s*[:–-].*$/, "").slice(0, 70)}`);
      return { kind: "question", message: lastLine.replace(/\*+/g, "").slice(0, 300), options: numbered };
    }
    return { kind: "", message: "", options: [] };
  } catch {
    return { kind: "", message: "", options: [] };
  }
}

/** Knoepfe der Claude-App ohne eigenen Text. */
const ACTION_LABEL = {
  commit: "Committen",
  create_pr: "PR erstellen",
  merge: "Zusammenführen",
  verify: "Prüfen",
  fix_ci: "CI reparieren",
  address_comments: "Kommentare bearbeiten",
  resolve_conflicts: "Konflikte lösen",
};

/**
 * Claude hat am Ende seines Zugs gefragt: die Frage in Milo zeigen. Antwortest du dort, macht
 * Claude sofort mit deiner Antwort weiter (der Stop-Hook gibt sie als Anweisung zurueck). Ohne
 * Antwort, oder wenn du lieber in Claude antwortest, endet das Skript und Claude wartet wie gewohnt.
 */
async function stopQuestion(session, input, stop) {
  const questions = [{ question: stop.message, header: "", multiSelect: false, options: stop.options.map((label) => ({ label })) }];
  const answer = await askMilo(session, input, { kind: "ask", tool: "Stop", detail: stop.message, questions }, STOP_WAIT_MS);
  if (!answer || answer.passthrough || typeof answer.answers !== "object" || answer.answers === null) return;
  const text = String(Object.values(answer.answers)[0] ?? "").trim().slice(0, 2000);
  if (!text) return;
  process.stdout.write(JSON.stringify({ decision: "block", reason: `Der Nutzer hat in Milo auf deine Frage („${stop.message}“) geantwortet: ${text}\nMach damit weiter.` }));
}

/** Kurze Beschreibung einer Freigabe-Anfrage. */
function describe(tool, input) {
  if (typeof input.command === "string") return input.command.slice(0, 600);
  if (input.file_path) return String(input.file_path);
  if (input.url) return String(input.url);
  const text = JSON.stringify(input ?? {});
  return text.length > 600 ? text.slice(0, 600) + "…" : text;
}

/** Erste Frage einer Rueckfrage als kurzer Text fuer die Insel. */
function firstQuestion(input) {
  const questions = Array.isArray(input?.questions) ? input.questions : [];
  return String(questions[0]?.question ?? "").slice(0, 300);
}

/** Plantext zu ExitPlanMode: steht er nicht in der Anfrage, der eben geschriebene Plan im Plan-Ordner. */
function planText(input) {
  if (typeof input?.plan === "string" && input.plan.trim()) return input.plan.slice(0, 20000);
  try {
    const plans = join(home, ".claude", "plans");
    const newest = readdirSync(plans)
      .filter((n) => n.endsWith(".md"))
      .map((n) => ({ n, at: statSync(join(plans, n)).mtimeMs }))
      .sort((a, b) => b.at - a.at)[0];
    if (newest && Date.now() - newest.at < 30 * 60000) return readFileSync(join(plans, newest.n), "utf8").slice(0, 20000);
  } catch {}
  return "";
}

/** Laeuft Milo und will Freigaben zeigen? */
function miloListens() {
  try {
    const beat = join(dir, "milo-approvals.json");
    return existsSync(beat) && Date.now() - statSync(beat).mtimeMs < 15000;
  } catch {
    return false;
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Legt eine Anfrage fuer Milo ab und wartet auf die Antwort des Nutzers (oder null). */
async function askMilo(session, input, request, waitMs) {
  const id = `${session}-${Date.now().toString(36)}`;
  const file = join(dir, `frage-${id}.json`);
  const answer = join(dir, `antwort-${id}.json`);
  const until = Date.now() + waitMs;
  writeAtomic(file, { id, session, cwd: String(input.cwd ?? ""), until, ...request });
  try {
    while (Date.now() < until) {
      if (existsSync(answer)) return JSON.parse(readFileSync(answer, "utf8"));
      // Jemand hat die Frage weggeraeumt (der Nutzer hat in Claude selbst weitergemacht): nicht weiter warten.
      if (!existsSync(file)) return null;
      await sleep(200);
    }
    return null;
  } finally {
    rmSync(file, { force: true });
    rmSync(answer, { force: true });
  }
}

/**
 * Raeumt offene Fragen dieser Sitzung weg, die sich erledigt haben. Antwortet der Nutzer direkt in
 * Claude (oder schreibt einfach weiter), erfaehrt Milo davon sonst nichts: der wartende Hook wird
 * oft beendet, ohne aufzuraeumen, und die Karte bliebe bis zum Ablauf stehen.
 */
function dropAnswered(session, isDone) {
  let names = [];
  try { names = readdirSync(dir); } catch { return; }
  for (const name of names) {
    if (!name.startsWith(`frage-${session}-`) || !name.endsWith(".json")) continue;
    const path = join(dir, name);
    let request = null;
    try { request = JSON.parse(readFileSync(path, "utf8")); } catch {}
    if (!request || request.session !== session || isDone(request)) rmSync(path, { force: true });
  }
}

/** Welche offenen Fragen hat dieses Ereignis erledigt? */
function answeredBy(event, tool) {
  switch (event) {
    // Neue Nachricht, Antwort fertig, Sitzung neu: alles Offene ist vorbei.
    case "UserPromptSubmit": case "Stop": case "SessionStart": case "SessionEnd":
      return () => true;
    // Claude arbeitet weiter: die Frage am Ende der letzten Antwort ist beantwortet, eine
    // Rueckfrage auch, sobald ein anderes Werkzeug startet.
    case "PreToolUse":
      return (r) => r.tool === "Stop" || (r.tool === "AskUserQuestion" && tool !== "AskUserQuestion");
    case "PermissionRequest":
      return (r) => r.tool === "Stop";
    // Das Werkzeug ist gelaufen: seine Freigabe bzw. Rueckfrage hat sich erledigt.
    case "PostToolUse":
      return (r) => r.tool === "Stop" || r.tool === tool;
    default:
      return null;
  }
}

function reply(hookSpecificOutput) {
  process.stdout.write(JSON.stringify({ hookSpecificOutput }));
}

/** Freigabe (auch Plan annehmen) ueber Milo. */
async function permission(session, input) {
  const tool = String(input.tool_name ?? "");
  const toolInput = input.tool_input ?? {};
  // Rueckfragen beantwortet Milo schon vorher (PreToolUse); hier nicht nochmal als Freigabe zeigen.
  if (tool === "AskUserQuestion") return;
  const request =
    tool === "ExitPlanMode"
      ? { kind: "plan", tool, detail: "", plan: planText(toolInput) }
      : { kind: "permission", tool, detail: describe(tool, toolInput) };
  const answer = await askMilo(session, input, request, PERMISSION_WAIT_MS);
  if (!answer || answer.passthrough) return;
  const message = String(answer.message ?? "").slice(0, 2000) || "Der Nutzer hat in Milo abgelehnt.";
  const decision = answer.allow ? { behavior: "allow" } : { behavior: "deny", message };
  reply({ hookEventName: "PermissionRequest", decision });
}

/** Rueckfrage von Claude (AskUserQuestion) in Milo beantworten. */
async function question(session, input) {
  const toolInput = input.tool_input ?? {};
  const questions = Array.isArray(toolInput.questions) ? toolInput.questions : [];
  if (questions.length === 0) return;
  const answer = await askMilo(session, input, { kind: "ask", tool: "AskUserQuestion", detail: firstQuestion(toolInput), questions }, QUESTION_WAIT_MS);
  if (!answer || answer.passthrough || typeof answer.answers !== "object" || answer.answers === null) return;
  const answers = {};
  for (const [q, a] of Object.entries(answer.answers)) answers[String(q)] = String(a).slice(0, 2000);
  reply({ hookEventName: "PreToolUse", permissionDecision: "allow", updatedInput: { ...toolInput, answers } });
}

try {
  const input = await readInput();
  const session = String(input.session_id ?? "unbekannt").replace(/[^A-Za-z0-9_-]/g, "").slice(0, 64) || "unbekannt";
  const file = join(dir, `${session}.json`);
  const done = answeredBy(event, String(input.tool_name ?? ""));
  if (done) dropAnswered(session, done);
  if (event === "SessionEnd") {
    rmSync(file, { force: true });
  } else {
    mkdirSync(dir, { recursive: true });
    const tool = String(input.tool_name ?? "");
    const toolInput = input.tool_input ?? {};
    const asks = event === "PreToolUse" && tool === "AskUserQuestion";
    const stop = event === "Stop" ? stopKind(String(input.transcript_path ?? "")) : null;
    const report = {
      event,
      kind: stop ? stop.kind : String(input.notification_type ?? ""),
      tool,
      message: (asks ? firstQuestion(toolInput) : stop ? stop.message : String(input.message ?? "")).slice(0, 300),
      cwd: String(input.cwd ?? ""),
      transcript: String(input.transcript_path ?? ""),
      at: Date.now(),
      ...(event === "PreToolUse" || event === "PostToolUse" ? liveView(tool, toolInput) : {}),
    };
    writeAtomic(file, report);
    if (event === "PermissionRequest" && miloListens()) await permission(session, input);
    else if (asks && miloListens()) await question(session, input);
    // Kommt die Nachricht aus Milos Chat (/claude), antwortest du dort ohnehin: nicht zusaetzlich warten.
    else if (stop?.kind === "question" && stop.message && !process.env.MILO_CHAT && miloListens()) await stopQuestion(session, input, stop);
  }
} catch {
  // Milo ist nicht eingerichtet oder der Ordner fehlt: stillschweigend nichts tun.
}
process.exit(0);
