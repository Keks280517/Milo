import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";
import { inTauri } from "./app.svelte";

// Claude-Modus im Chat: "/claude " am Anfang schaltet um, danach geht alles an Claude Code.
// Ziel ist die zuletzt benutzte Sitzung; in der Auswahl laesst sich eine andere Sitzung, ein neuer
// Chat im selben Projekt oder ein anderes Projekt waehlen. Der Verlauf hier ist nur die Ansicht in
// Milo; die echte Unterhaltung liegt bei Claude (dieselbe wie in der Claude-App).

export interface ClaudeSession {
  id: string;
  title: string;
  at: number;
}

export interface ClaudeProject {
  cwd: string;
  name: string;
  sessions: ClaudeSession[];
}

export interface ClaudeTarget {
  cwd: string;
  name: string;
  /** Leer = neuer Chat in diesem Projekt (die Sitzung entsteht mit der ersten Nachricht). */
  session: string;
  title: string;
}

export interface SlashCommand {
  name: string;
  description: string;
  source: string;
}

export interface ClaudeLine {
  id: number;
  role: "user" | "claude";
  text: string;
  tools: { name: string; detail: string }[];
  pending: boolean;
  error: boolean;
  login: boolean;
  /** Aus dem bisherigen Verlauf geladen (nicht in dieser Milo-Sitzung geschrieben). */
  past?: boolean;
}

const TARGET_KEY = "milo.claudeTarget";
const COMMANDS_KEY = "milo.claudeCommands";
/** Befehle, die Milo selbst uebersetzt. */
const OWN: SlashCommand[] = [{ name: "plan", description: t("Planmodus: Claude plant erst und ändert noch nichts", "Plan mode: Claude plans first and changes nothing yet"), source: "milo" }];

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* ohne Speicher eben nur fuer diese Sitzung */
  }
}

class ClaudeChat {
  active = $state(false);
  projects = $state<ClaudeProject[]>([]);
  target = $state<ClaudeTarget | null>(load<ClaudeTarget | null>(TARGET_KEY, null));
  lines = $state<ClaudeLine[]>([]);
  running = $state(false);
  /** Von Claude beim Start gemeldete Befehle je Projektordner (auch eingebaute ohne Datei). */
  private reported = $state<Record<string, string[]>>(load<Record<string, string[]>>(COMMANDS_KEY, {}));
  /** Befehle aus Dateien (eigene, Projekt, Plugins) mit Beschreibung, fuer das aktuelle Projekt. */
  private scanned = $state<SlashCommand[]>([]);
  /** Alle Slash-Befehle fuer die Schnellauswahl. */
  commands = $derived.by(() => {
    const list = [...OWN, ...this.scanned];
    const names = new Set(list.map((c) => c.name));
    for (const name of this.reported[this.target?.cwd ?? ""] ?? []) {
      if (!names.has(name)) {
        names.add(name);
        list.push({ name, description: "", source: "claude" });
      }
    }
    return list;
  });
  loading = $state(false);
  private nextId = 1;
  private listening = false;

  /** In den Claude-Modus wechseln; ohne gemerktes Ziel die zuletzt benutzte Sitzung nehmen. */
  async enter() {
    this.active = true;
    this.listen();
    await this.refresh();
    const current = this.target;
    const known = current && this.projects.some((p) => p.cwd === current.cwd);
    if (!known) {
      const last = this.projects[0];
      if (last) this.choose(last, last.sessions[0] ?? null);
    } else {
      this.loadCommands();
      if (this.lines.length === 0) this.loadHistory();
    }
  }

  /** Bisherigen Verlauf der gewaehlten Sitzung zeigen (auch was in der Claude-App geschrieben wurde). */
  async loadHistory() {
    const session = this.target?.session;
    if (!inTauri || !session) return;
    const past = await invoke<{ role: "user" | "claude"; text: string; tools: { name: string; detail: string }[] }[]>("claude_chat_history", { session }).catch(() => []);
    // Inzwischen eine andere Sitzung gewaehlt oder schon geschrieben: nichts ueberschreiben.
    if (this.target?.session !== session || this.lines.length > 0) return;
    this.lines = past.map((line) => ({ id: this.nextId++, role: line.role, text: line.text, tools: line.tools, pending: false, error: false, login: false, past: true }));
  }

  async loadCommands() {
    if (!inTauri || !this.target) return;
    this.scanned = await invoke<SlashCommand[]>("claude_chat_commands", { cwd: this.target.cwd }).catch(() => []);
  }

  leave() {
    this.active = false;
  }

  async refresh() {
    if (!inTauri) return;
    this.loading = true;
    try {
      this.projects = await invoke<ClaudeProject[]>("claude_chat_projects");
    } finally {
      this.loading = false;
    }
  }

  /** Sitzung waehlen; `session` null = neuer Chat in diesem Projekt. */
  choose(project: ClaudeProject, session: ClaudeSession | null) {
    const changed = this.target?.cwd !== project.cwd || (this.target?.session ?? "") !== (session?.id ?? "");
    this.target = { cwd: project.cwd, name: project.name, session: session?.id ?? "", title: session?.title ?? t("Neuer Chat", "New chat") };
    save(TARGET_KEY, this.target);
    if (changed) {
      this.lines = [];
      this.loadHistory();
    }
    this.loadCommands();
  }

  private listen() {
    if (this.listening || !inTauri) return;
    this.listening = true;
    listen<Record<string, unknown>>("milo://claude-chat", ({ payload }) => this.receive(payload));
  }

  private receive(event: Record<string, unknown>) {
    const line = this.lines.findLast((l) => l.role === "claude" && l.pending);
    switch (event.kind) {
      case "init": {
        const session = String(event.session ?? "");
        if (session && this.target) {
          this.target = { ...this.target, session };
          save(TARGET_KEY, this.target);
        }
        if (Array.isArray(event.commands) && event.commands.length && this.target) {
          this.reported = { ...this.reported, [this.target.cwd]: event.commands.map(String) };
          save(COMMANDS_KEY, this.reported);
        }
        break;
      }
      case "text":
        if (line) line.text = line.text ? `${line.text}\n\n${String(event.text ?? "")}` : String(event.text ?? "");
        break;
      case "tool":
        line?.tools.push({ name: String(event.name ?? ""), detail: String(event.detail ?? "") });
        break;
      case "done":
        this.running = false;
        if (line) {
          line.pending = false;
          line.error = !!event.error;
          line.login = !!event.login;
          // Bei Fehlern steht der Grund nur im Ergebnis; bei Erfolg ist der Text schon da.
          if (event.login) line.text = t("Claude Code ist auf diesem PC nicht (mehr) angemeldet. Die Claude-App hat ihre eigene Anmeldung; der Befehl braucht einmal eine eigene.", "Claude Code isn't signed in on this PC (anymore). The Claude app has its own sign-in; the command needs one of its own once.");
          else if (event.error && event.text && !line.text.includes(String(event.text))) line.text = line.text ? `${line.text}\n\n${event.text}` : String(event.text);
          if (!line.text && !line.error) line.text = String(event.text ?? "");
        }
        break;
    }
  }

  /** Nachricht an Claude. "/plan …" startet den Planmodus, andere Slash-Befehle gehen unveraendert durch. */
  async send(raw: string) {
    const text = raw.trim();
    if (!text || this.running || !this.target) return;
    let message = text;
    let plan = false;
    if (/^\/plan(\s|$)/i.test(text)) {
      plan = true;
      message = text.replace(/^\/plan\s*/i, "") || t("Mach einen Plan für den nächsten Schritt.", "Make a plan for the next step.");
    }
    this.lines.push({ id: this.nextId++, role: "user", text, tools: [], pending: false, error: false, login: false });
    this.lines.push({ id: this.nextId++, role: "claude", text: "", tools: [], pending: true, error: false, login: false });
    this.running = true;
    try {
      await invoke("claude_chat_send", { text: message, session: this.target.session || null, cwd: this.target.cwd, plan });
    } catch (err) {
      this.receive({ kind: "done", error: true, text: String(err) });
    }
  }

  stop() {
    if (inTauri) invoke("claude_chat_stop").catch(() => {});
  }

  login() {
    if (inTauri) invoke("claude_chat_login").catch(() => {});
  }
}

export const claude = new ClaudeChat();
