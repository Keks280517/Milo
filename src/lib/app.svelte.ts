// Zentraler Zustand der Oberflaeche und die Verbindung zum Rust-Kern.

import { applyLanguage, lang, locale, t } from "./i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ArmStyle, Mood, OneShot } from "./character/engine";

/** Kostenloses Kontingent eines Gemini-Modells. */
export interface ModelLimit {
  model: string;
  rpm: number;
  tpm: number;
  rpd: number;
}

export interface Settings {
  name: string;
  armStyle: ArmStyle;
  /** Vor jeder Anfrage an Gemini nachfragen. */
  confirmCloud: boolean;
  /** Einmal zugestimmt, dass Anfragen an Google Gemini gehen. */
  consentGiven: boolean;
  timerSound: boolean;
  weatherPlace: string;
  /** Ab dieser Wertung (0 bis 100) gilt eine Anfrage als schwer (Flash-Modell). */
  cloudThreshold: number;
  modelEasy: string;
  modelHard: string;
  modelHard2: string;
  limits: ModelLimit[];
  /** Wertung jeder Anfrage im Chat anzeigen (Debug-Ansicht). */
  showRating: boolean;
  /** Die Insel versteckt sich, solange etwas im Vollbild laeuft. */
  hideInFullscreen: boolean;
  /** Tastenkombination fuers Zuhoeren; leer = keine. */
  hotkey: string;
  /** Auf "Hey Milo" warten (Mikrofon bleibt offen). */
  wakeWord: boolean;
  wakeLevel: number;
  sttModel: "parakeet" | "whisper";
  speakAnswers: boolean;
  /** Spielemodus: solange ein Spiel laeuft, zieht sich Milo vollstaendig zurueck. */
  gameMode: boolean;
  gamesAlways: string[];
  gamesNever: string[];
  /** 0 = ruhig, 1 = normal, 2 = verspielt. */
  liveliness: number;
  /** Pause bis zum Ende der Aufnahme: 1 = kurz, 2 = normal, 3 = lang. */
  speechPause: number;
  /** Name des Nutzers (richtige Schreibweise fuer Profile u. Ae.). */
  userName: string;
  briefing: boolean;
  eveningReview: boolean;
  eveningHour: number;
  /** Tastenkombination fuer die Text-Werkzeuge; leer = keine. */
  textHotkey: string;
  petMode: boolean;
  petFrequency: number;
  claudeVoice: boolean;
  claudeApprovals: boolean;
  groqEnabled: boolean;
  groqModel: string;
  groqConsent: boolean;
  /** "cloud", "mixed" (Leichtes lokal) oder "local" (nur die KI auf diesem PC). */
  aiMode: string;
  localModel: string;
  plantBornMs: number;
  /** Deko-Objekte in der Insel, in dieser Reihenfolge. */
  decor: string[];
  /** Deko-Objekte auf dem Desktop (x als Anteil der Bildschirmbreite). */
  deskItems: { kind: string; x: number; locked?: boolean }[];
  /** Kalender-Abos (ICS-Adressen), eine pro Zeile. */
  agendaIcs: string;
  /** Aussehen: Akzentfarbe, Inselgroesse (0 kompakt, 1 normal, 2 breit), Glas (0 matt, 1 normal, 2 klar), weniger Bewegung. */
  accent: string;
  islandSize: number;
  glass: number;
  reduceMotion: boolean;
  /** Karten der Startseite, in dieser Reihenfolge. */
  homeCards: string[];
  /** Eigene Groesse je Startseiten-Karte (s, m, w, t, l, f). */
  homeSizes: Record<string, string>;
  /** "auto" (Systemsprache), "de" oder "en". */
  language: string;
  /** Eigene Plaetze der Deko in der Insel (Anteil der Breite) und gesperrte Objekte. */
  decorPositions: Record<string, number>;
  decorLocked: string[];
  /** Leiste unten: eigene Reihenfolge (leer = Standard) und ausgeblendete Wuerfel. */
  shelfOrder: string[];
  shelfHidden: string[];
  /** Willkommens-Assistent durchlaufen. */
  onboarded: boolean;
  /** Minuten Vorlauf, ab denen ein Termin in der Pille steht; 0 = aus. */
  eventLead: number;
  /** Uhrzeit in der ruhenden Pille. */
  pillClock: boolean;
  /** Offene Insel klappt beim Klick in ein anderes Fenster zu; aus = nur ueber Pfeil oder Esc. */
  closeOnBlur: boolean;
  clipboardHistory: boolean;
  screenshotCard: boolean;
  downloadCard: boolean;
  batteryAlert: boolean;
  privacyDots: boolean;
  rainAlert: boolean;
  /** Tastenkombination fuer die Schnellnotiz; leer = aus. */
  noteHotkey: string;
  alarms: Alarm[];
}

/** off = Mikrofon zu, wake = wartet auf das Aktivierungswort, listening = nimmt auf, transcribing = wertet aus. */
export type VoiceState = "off" | "wake" | "listening" | "transcribing";

type VoiceEvent =
  | { kind: "state"; state: VoiceState }
  | { kind: "level"; level: number }
  | { kind: "heard"; text: string }
  | { kind: "notice"; message: string };

/** Was gerade abgespielt wird (meldet Windows fuer Spotify, Browser, Player ...). */
export interface NowPlaying {
  title: string;
  artist: string;
  album: string;
  subtitle: string;
  app: string;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  atMs: number;
  canNext: boolean;
  canPrevious: boolean;
  canSeek: boolean;
  cover: string;
}

export interface VoiceInfo {
  parakeet: boolean;
  whisper: boolean;
  wake: boolean;
  folder: string;
}

/** Wertung einer Anfrage durch das Bewertungssystem. */
export interface Rating {
  score: number;
  threshold: number;
  /** quick = Schnellbefehl ohne KI, easy/hard/report/browser = Art der Gemini-Anfrage. */
  kind: "quick" | "easy" | "hard" | "report" | "browser";
  /** Das Gemini-Modell, das die Anfrage bekommen hat. */
  model: string;
  parts: { label: string; points: number }[];
  reason: string;
}

/** Verbrauch eines Gemini-Modells heute und in der laufenden Minute. */
export interface UsageRow {
  model: string;
  role: string;
  today: number;
  rpd: number;
  minute: number;
  rpm: number;
  tokensToday: number;
  tokensMinute: number;
  tpm: number;
  exhausted: boolean;
}

export interface Usage {
  rows: UsageRow[];
  resetMs: number;
}

/** Messwerte fuer den System-Tab. */
export interface SystemSnapshot {
  cpu: number;
  cores: number[];
  cpuName: string;
  ramUsed: number;
  ramTotal: number;
  disks: { name: string; mount: string; total: number; free: number }[];
  netDown: number;
  netUp: number;
  top: { name: string; cpu: number; memory: number; count: number }[];
  uptime: number;
}

/** Text, der mit den Text-Werkzeugen bearbeitet werden soll. */
export interface TextOffer {
  text: string;
  fromSelection: boolean;
  chars: number;
}

export interface CloudStatus {
  hasKey: boolean;
  verified: boolean;
  message: string;
}

/** Ein Eintrag im Protokoll dessen, was Milo getan hat. */
export interface Action {
  id: number;
  at: string;
  tool: string;
  summary: string;
  ok: boolean;
  /** Gekuerzte Angaben des Aufrufs, Dauer in ms, laesst sich (noch) rueckgaengig machen. */
  detail: string;
  ms: number;
  undoable: boolean;
  undone: boolean;
}

export interface Link {
  title: string;
  url: string;
}

export interface Note {
  /** Formatierter Inhalt (bereinigtes HTML); leer bei reinen Textnotizen. */
  rich?: string;
  id: number;
  text: string;
  createdAt: string;
  updatedAt: string;
}

export interface Weather {
  place: string;
  region: string;
  now: { temp: number; feels: number; code: number; text: string; wind: number; humidity: number; isDay: boolean };
  hours: { hour: string; temp: number; code: number; rain: number; isDay: boolean }[];
  days: { date: string; label: string; min: number; max: number; code: number; text: string; rain: number; wind: number; sunrise: string; sunset: string }[];
}

/** Daten fuer eine fertige Ansicht: der Kern liefert sie, die Oberflaeche fuellt nur die Vorlage. */
export type Card =
  | ({ type: "weather" } & Weather)
  | { type: "timer"; id: number; label: string; totalMs: number; dueMs: number }
  | { type: "sources"; query: string; items: { title: string; url: string; snippet: string }[] }
  | { type: "note"; id: number; text: string }
  | { type: "notes"; items: Note[] }
  | { type: "app"; name: string }
  | { type: "link"; title: string; url: string }
  | { type: "power"; title: string; atMs: number; kind?: "shutdown" | "restart" | "sleep" }
  | { type: "files"; items: StoredFile[] }
  | { type: "script"; code: string; output: string; ok: boolean }
  | { type: "document"; title: string; path: string }
  | { type: "image"; title: string; path: string }
  | { type: "sketch"; text: string };

/** Geplantes Herunterfahren, Neustarten oder Energiesparen (der Kern passt darauf auf). */
export interface PowerPlan {
  action: "shutdown" | "restart" | "sleep";
  dueMs: number;
  createdMs: number;
}

export interface ClaudeWindow {
  usedPercent: number;
  /** Sekunden seit 1970. */
  resetsAt: number;
}

export interface ClaudeStatus {
  present: boolean;
  /** Die Hooks von Claude Code sind eingerichtet; ohne sie ist nur zu sehen, dass Claude offen ist. */
  connected: boolean;
  activity: "idle" | "working" | "question" | "confirm" | "done";
  text: string;
  project: string;
  /** Live-Ansicht: zuletzt benutztes Werkzeug, Datei, neuer und ersetzter Text. */
  tool: string;
  file: string;
  snippet: string;
  old: string;
  /** Aus dem Sitzungsverlauf: Claudes letzte Nachricht, Aufgabenliste, letzte Schritte. */
  said: string;
  todos: { text: string; status: "pending" | "in_progress" | "completed" | string }[];
  steps: { tool: string; what: string; at: string }[];
  limits: { fiveHour: ClaudeWindow | null; sevenDay: ClaudeWindow | null; model: string; context: number | null; at: number } | null;
  permission: ClaudeRequest | null;
}

/** Eine Frage von Claude (AskUserQuestion) mit ihren Antwortmoeglichkeiten. */
export interface ClaudeQuestion {
  question: string;
  header?: string;
  options: { label: string; description?: string }[];
  multiSelect?: boolean;
}

/** Worauf Claude wartet: Freigabe, Plan oder Rueckfrage. */
export interface ClaudeRequest {
  id: string;
  kind: "permission" | "plan" | "ask";
  tool: string;
  detail: string;
  cwd: string;
  questions: ClaudeQuestion[] | null;
  plan: string;
  until: number;
}

/** Eine Datei, die der Nutzer Milo gegeben hat (Milos eigene Kopie). */
export interface StoredFile {
  id: number;
  name: string;
  path: string;
  size: number;
  kind: "text" | "pdf" | "office" | "image" | "other";
  addedAt: string;
  /** Zeichen ausgelesenen Texts; 0 = kein lesbarer Text. */
  chars: number;
  note: string;
}

/** Datei oder Ordner auf dem PC (Downloads, Ordner-Ansicht). */
export interface DiskEntry {
  name: string;
  path: string;
  dir: boolean;
  size: number;
  modifiedMs: number;
  kind: "image" | "pdf" | "text" | "office" | "video" | "audio" | "archive" | "program" | "folder" | "other";
  /** Download laeuft noch. */
  partial: boolean;
}

/** Ordner zum schnellen Hinspringen und als Ziel beim Ziehen. */
export interface Place {
  name: string;
  path: string;
  builtin: boolean;
}

/** Dateigroesse lesbar, z. B. "1,4 MB". */
export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toLocaleString(locale, { maximumFractionDigits: value < 10 ? 1 : 0 })} ${units[unit]}`;
}

export interface ToolNote {
  name: string;
  summary: string;
  status: "start" | "ok" | "error" | "declined";
  links: Link[];
  card: Card | null;
}

export interface Message {
  id: number;
  role: "user" | "milo";
  text: string;
  tools: ToolNote[];
  pending: boolean;
  error: boolean;
  /** Wie die Anfrage bewertet wurde, auf die diese Antwort folgt. */
  rating?: Rating | null;
}

/** Wecker: einmalig (days leer) oder an Wochentagen (0 = Montag). */
export interface Alarm {
  id: string;
  time: string;
  days: number[];
  label: string;
  enabled: boolean;
}

/** Eintrag im Verlauf der Zwischenablage. */
export interface ClipItem {
  id: number;
  text: string;
  atMs: number;
}

/** Karte, mit der die Pille kurz waechst (Download, Akku, Bildschirmfoto, Regen, Fokus ...). */
export interface PillCard {
  id: string;
  kind: "download" | "battery" | "screenshot" | "rain" | "focusDone" | "breakDone" | "review" | "note" | "info";
  title: string;
  sub: string;
  path?: string;
  thumb?: string;
  /** Verschwindet von selbst um diese Zeit (ms); ohne Angabe erst per Knopf. */
  until?: number;
  /** Etwas ging schief: Ausrufezeichen statt Haken. */
  failed?: boolean;
}

/** Laufender Fokus oder die Pause danach. */
export interface Focus {
  phase: "focus" | "break";
  endMs: number;
  totalMs: number;
  /** Fokus-Dauer dieser Runde (fuer „noch eine Runde“). */
  minutes: number;
}

/** Termin aus der Agenda (eigene und abonnierte Kalender). */
export interface AgendaEvent {
  id: string;
  title: string;
  startMs: number;
  endMs: number;
  allDay: boolean;
  location: string;
  source: string;
}

/** Schluessel eines Termins zum Ausblenden: Abo-Termine haben nicht immer stabile IDs. */
export const eventKey = (e: AgendaEvent) => `${e.id}|${e.startMs}`;

export interface Timer {
  id: number;
  label: string;
  dueMs: number;
  totalMs: number;
  ringing: boolean;
}

/** Rueckfrage vor einer riskanten Aktion. */
export interface Question {
  id: number;
  title: string;
  detail: string;
  action: string;
  danger: boolean;
}

export type TabId = "home" | "chat" | "media" | "agenda" | "timer" | "weather" | "files" | "notes" | "system" | "claude" | "usage" | "history" | "widgets" | "settings";

type AgentState = "thinking" | "answering" | "working" | "collab" | "idle";
export type HelperPhase = "arrive" | "working" | "talking" | "done";

type AgentEvent =
  | { kind: "state"; state: AgentState }
  | { kind: "token"; text: string }
  | { kind: "tool"; name: string; summary: string; status: ToolNote["status"]; links?: Link[]; card?: Card | null }
  | { kind: "route"; rating: Rating }
  | { kind: "helper"; phase: HelperPhase | "leave"; model: string }
  | { kind: "done"; text: string }
  | { kind: "error"; message: string };

/** Laeuft die Seite in der App (und nicht nur im Browser zum Entwickeln)? */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const STATUS: Record<AgentState, string> = {
  idle: t("Bereit", "Ready"),
  thinking: t("Denkt nach", "Thinking"),
  answering: t("Antwortet", "Answering"),
  working: t("Erledigt das", "On it"),
  collab: t("Denkt gründlich nach", "Thinking it through"),
};

/** Werkzeuge, bei denen Milo sucht statt tippt. */
const SEARCH_TOOLS = new Set([
  "web_research", "deep_research", "search_notes", "browser_search", "get_weather", "browser_read", "browser_list", "find_links", "twitch_live", "browser_tabs",
]);

const EMPTY_CLAUDE: ClaudeStatus = {
  present: false, connected: false, activity: "idle", text: "", project: "", tool: "", file: "", snippet: "", old: "", said: "", todos: [], steps: [], limits: null, permission: null,
};

/** Restzeit als 4:05 oder 1:02:03. */
export function clock(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(total / 3600), m = Math.floor((total % 3600) / 60), s = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

/** Macht aus "2026-10-13 19:33" das gewohnte "13.10.2026, 19:33". */
export function niceDate(stamp: string): string {
  const match = stamp.match(/^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}:\d{2}))?/);
  if (!match) return stamp;
  const [, year, month, day, time] = match;
  return lang === "en" ? `${month}/${day}/${year}${time ? `, ${time}` : ""}` : `${day}.${month}.${year}${time ? `, ${time}` : ""}`;
}

/** Uhrzeit HH:MM eines Zeitpunkts. */
export function timeOfDay(ms: number): string {
  return new Date(ms).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
}

class App {
  settings = $state<Settings>({
    name: "Milo", armStyle: "capsule", confirmCloud: false, consentGiven: false,
    timerSound: true, weatherPlace: "",
    cloudThreshold: 60, modelEasy: "gemini-3.5-flash-lite", modelHard: "gemini-3.8-flash", modelHard2: "gemini-3.7-flash", limits: [],
    showRating: false, hideInFullscreen: true,
    hotkey: "Ctrl+Alt+K", wakeWord: false, wakeLevel: 2, sttModel: "parakeet", speakAnswers: false, gameMode: true, gamesAlways: [], gamesNever: [], liveliness: 1, speechPause: 2, userName: "",
    briefing: true, eveningReview: true, eveningHour: 21, textHotkey: "Ctrl+Alt+T", petMode: true, petFrequency: 1, claudeVoice: true, claudeApprovals: true, groqEnabled: true, groqModel: "openai/gpt-oss-120b", groqConsent: false, aiMode: "cloud", localModel: "", plantBornMs: 0,
    decor: ["plant", "mailbox", "lantern"], deskItems: [], agendaIcs: "", accent: "#64d2ff", islandSize: 1, glass: 1, reduceMotion: false, homeCards: ["media", "agenda", "weather"], homeSizes: {}, language: "auto", decorPositions: {}, decorLocked: [], shelfOrder: [], shelfHidden: [], onboarded: true, eventLead: 30, pillClock: true, closeOnBlur: true, clipboardHistory: true, screenshotCard: true, downloadCard: true, batteryAlert: true, privacyDots: true, rainAlert: true, noteHotkey: "Ctrl+Alt+N", alarms: [],
  });
  /** Die echten Einstellungen sind geladen (vorher gelten die Vorgaben). */
  settingsLoaded = $state(false);
  /** Ob ein Gemini-Schluessel hinterlegt ist. Der Schluessel selbst kommt nie in die Oberflaeche zurueck. */
  cloud = $state<CloudStatus>({ hasKey: false, verified: false, message: "" });
  /** Der Helfer-Wuerfel (staerkeres Modell) steht gerade mit auf der Buehne und tut das. */
  helper = $state<HelperPhase | null>(null);
  /** Messwerte des System-Tabs; null, solange er nicht offen war. */
  system = $state<SystemSnapshot | null>(null);
  /** Verbrauch der Gemini-Kontingente. */
  usage = $state<Usage | null>(null);
  /** Text-Werkzeuge: der markierte Text wartet auf eine Aktion. */
  textOffer = $state<TextOffer | null>(null);
  textBusy = $state(false);
  textError = $state("");
  /** Ergebnis der letzten Text-Aktion, wenn es nicht eingefuegt wurde (Zusammenfassen, Erklaeren). */
  textResult = $state("");
  /** Laufende Wiedergabe; null = es laeuft nichts. */
  media = $state<NowPlaying | null>(null);
  /** Was gerade geplant ist; `undefined`, solange der Kern noch nicht gefragt wurde. */
  powerPlan = $state<PowerPlan | null | undefined>(undefined);
  /** Was das Mikrofon gerade tut. Alles ausser "off" heisst: es ist offen. */
  voice = $state<VoiceState>("off");
  /** Lautstaerke 0..1 waehrend der Aufnahme. */
  voiceLevel = $state(0);
  /** Die laufende Anfrage wurde gesprochen (dann wird die Antwort auf Wunsch vorgelesen). */
  private spoken = false;
  /** Ein Spiel wurde erkannt: die Insel verabschiedet sich, gleich schliesst der Kern das Fenster. */
  sleeping = $state(false);
  /** Auf dem Monitor der Insel laeuft gerade etwas im Vollbild. */
  fullscreen = $state(false);
  messages = $state<Message[]>([]);
  busy = $state(false);
  timers = $state<Timer[]>([]);
  questions = $state<Question[]>([]);
  /** Zuletzt abgerufenes Wetter; fuellt die Wetter-Ansicht. */
  weather = $state<Weather | null>(null);
  weatherBusy = $state(false);
  /** Wann das Wetter zuletzt geholt wurde (ms). */
  weatherAt = 0;
  weatherError = $state("");
  /** Claude: ob es offen ist und was Claude Code gerade tut. */
  claude = $state<ClaudeStatus>(EMPTY_CLAUDE);
  /** Die Rueckfrage "Milo loeschen" ist offen (aus dem Menue des Symbols in der Taskleiste). */
  wiping = $state(false);
  /** Dateien, die Milo bekommen hat, neueste zuerst. */
  files = $state<StoredFile[]>([]);
  /** Der Nutzer zieht gerade Dateien ueber die Insel. */
  dragging = $state(false);
  /** Eine Datei wird gerade aus der Insel herausgezogen. */
  handing = $state(false);
  /** Was gerade aus Milo herausgezogen wird (zum Verschieben auf einen Ordner-Knopf). */
  handingPaths: string[] = [];
  /** Ordner-Knopf, ueber dem beim Ziehen gerade die Maus ist. */
  dropFolder = $state<string | null>(null);
  /** Widget, das per Rechtsklick zum Bearbeiten geoeffnet werden soll. */
  editWidget = $state("");
  /** Erhoeht sich nach jedem Verschieben, damit offene Ordner-Ansichten neu laden. */
  filesChanged = $state(0);
  tab = $state<TabId>("chat");
  /** Milo wurde aus der Insel gezogen und ist auf dem Desktop unterwegs. */
  miloOnDesk = $state(false);
  /** Rechtsklick auf ein Deko-Objekt in der Insel: dieses in der Bibliothek hervorheben. */
  decorFocus = $state("");
  /** Laufende Uhr fuer die Restzeit-Anzeige; tickt nur, solange Timer laufen. */
  now = $state(Date.now());

  /** Was der Agent gerade tut. */
  private agentState = $state<AgentState>("idle");
  /** Name des Werkzeugs, das gerade laeuft. */
  private runningTool = $state("");

  ringing = $derived(this.timers.filter((t) => t.ringing));
  /** Der Timer, der als naechster ablaeuft. */
  nextTimer = $derived(
    this.timers.filter((t) => !t.ringing).reduce<Timer | null>((best, t) => (!best || t.dueMs < best.dueMs ? t : best), null),
  );
  /** Braucht gerade etwas die Aufmerksamkeit des Nutzers? Dann bleibt die Insel offen. */
  needsAttention = $derived(this.questions.length > 0 || this.ringing.length > 0 || this.wiping || this.claude.permission !== null || this.textOffer !== null);
  /** Die Insel ist nach oben weggezogen; nur etwas Dringendes holt sie im Vollbild hervor. */
  hidden = $derived(this.sleeping || (this.fullscreen && this.settings.hideInFullscreen && !this.needsAttention));

  /** Stimmung des Charakters: Klingeln geht vor, dann die Arbeit des Agenten, dann ein laufender Timer. */
  mood = $derived.by<Mood>(() => {
    if (this.voice === "listening") return "listening";
    if (this.voice === "transcribing") return "thinking";
    if (this.dragging) return "fileHover";
    if (this.handing) return "fileOut";
    if (this.ringing.length > 0) return "ringing";
    if (this.agentState === "working") return SEARCH_TOOLS.has(this.runningTool) ? "searching" : "working";
    if (this.agentState !== "idle") return this.agentState;
    if (this.nextTimer && this.urgency > 0) return "timer";
    // Laeuft Musik und sonst nichts, hoert Milo mit.
    if (this.media?.playing) return "music";
    return this.nextTimer ? "timer" : "idle";
  });

  /** Anteil der Restzeit des naechsten Timers (1 = gerade gestellt, 0 = abgelaufen). */
  timerLeft = $derived(
    this.nextTimer ? Math.min(1, Math.max(0, (this.nextTimer.dueMs - this.now) / Math.max(1, this.nextTimer.totalMs))) : 0,
  );
  /** 0..1: steigt in den letzten zehn Sekunden eines Timers an. */
  urgency = $derived(this.nextTimer ? Math.min(1, Math.max(0, 1 - (this.nextTimer.dueMs - this.now) / 10_000)) : 0);

  /** Was der Agent gerade tut, als kurzer Text; leer im Leerlauf. */
  activity = $derived(this.agentState === "idle" ? "" : STATUS[this.agentState]);

  status = $derived.by(() => {
    if (this.ringing.length > 0) return t(`${this.ringing[0].label} ist abgelaufen`, `${this.ringing[0].label} is up`);
    if (this.questions.length > 0) return t("Wartet auf dich", "Waiting for you");
    if (this.voice === "listening") return t("Hört zu …", "Listening …");
    if (this.voice === "transcribing") return t("Versteht …", "Understanding …");
    if (this.agentState !== "idle") return STATUS[this.agentState] + " …";
    if (this.nextTimer) return `${this.nextTimer.label} · ${clock(this.nextTimer.dueMs - this.now)}`;
    return STATUS.idle;
  });

  /** Wird von der Insel gesetzt, damit Ereignisse Gesten am Charakter ausloesen koennen. */
  gesture: (kind: OneShot) => void = () => {};
  /** Wird von der Insel gesetzt: eine Antwort ist da, bitte zeigen. */
  onAnswer: () => void = () => {};
  /** Wird von der Insel gesetzt: kurzer Hinweis, den Milo in der Pille sagt. */
  onNotice: (text: string) => void = () => {};
  /** Wird von der Insel gesetzt: ein Textstueck kam an (Sprechtakt des Charakters). */
  onToken: () => void = () => {};

  private nextId = 1;
  private ticker = 0;

  // ---------- Pille: Uhr und Termine ----------

  /** Grobe Uhr fuer die Pille (alle paar Sekunden), unabhaengig von laufenden Timern. */
  clockNow = $state(Date.now());
  /** Termine der naechsten Stunden. */
  upcoming = $state<AgendaEvent[]>([]);
  /** In der Pille ausgeblendete Termine (per Rechtsklick). */
  hiddenEvents = $state<string[]>(readList("milo.hiddenEvents"));
  // ---------- Pille: Karten, Fokus, Zwischenablage, Datenschutz ----------

  /** Karten, die nacheinander in der Pille erscheinen. */
  pillCards = $state<PillCard[]>([]);
  /** Verlauf der Zwischenablage (nur im Arbeitsspeicher des Kerns). */
  clips = $state<ClipItem[]>([]);
  /** Programme, die gerade Mikrofon oder Kamera benutzen. */
  privacy = $state<{ mic: string[]; cam: string[] }>({ mic: [], cam: [] });
  /** Fokus-Modus (bleibt ueber einen Neustart erhalten). */
  focus = $state<Focus | null>(readFocus());
  private lastRainCheck = 0;
  private rainWarned = "";

  pushCard(card: PillCard) {
    this.pillCards = [...this.pillCards.filter((c) => c.id !== card.id), card];
  }

  dropCard(id: string) {
    this.pillCards = this.pillCards.filter((c) => c.id !== id);
  }

  startFocus(minutes = 25) {
    const ms = Math.round(minutes * 60_000);
    this.focus = { phase: "focus", endMs: Date.now() + ms, totalMs: ms, minutes };
    saveFocus(this.focus);
    this.pillCards = this.pillCards.filter((c) => c.kind !== "focusDone" && c.kind !== "breakDone");
    this.gesture("nod");
  }

  startBreak(minutes = 5) {
    const ms = Math.round(minutes * 60_000);
    this.focus = { phase: "break", endMs: Date.now() + ms, totalMs: ms, minutes: this.focus?.minutes ?? 25 };
    saveFocus(this.focus);
    this.pillCards = this.pillCards.filter((c) => c.kind !== "focusDone");
  }

  stopFocus() {
    this.focus = null;
    saveFocus(null);
    this.pillCards = this.pillCards.filter((c) => c.kind !== "focusDone" && c.kind !== "breakDone");
  }

  /** Jede Sekunde: Fokus-Ende, abgelaufene Karten, Regen (alle 20 Minuten), Abendrueckblick. */
  private pillTick() {
    const now = Date.now();
    const focus = this.focus;
    if (focus && now >= focus.endMs) {
      const minutes = focus.minutes;
      this.focus = null;
      saveFocus(null);
      invoke("play_chime").catch(() => {});
      this.gesture("success");
      if (focus.phase === "focus") {
        bumpStat("focus");
        this.pushCard({ id: "focus", kind: "focusDone", title: t("Fokus geschafft!", "Focus done!"), sub: t(`${minutes} Min am Stück. Kurze Pause?`, `${minutes} min straight. Short break?`) });
      } else {
        this.pushCard({ id: "focus", kind: "breakDone", title: t("Pause vorbei", "Break's over"), sub: t("Bereit für die nächste Runde?", "Ready for the next round?") });
      }
    }
    if (this.pillCards.some((c) => c.until && c.until < now)) this.pillCards = this.pillCards.filter((c) => !c.until || c.until >= now);
    if (now - this.lastRainCheck > 20 * 60_000) {
      this.lastRainCheck = now;
      void this.checkRain();
    }
    this.checkEvening();
  }

  private async checkRain() {
    if (!inTauri || !this.settings.rainAlert || !this.settings.weatherPlace) return;
    const soon = await invoke<[number, number] | null>("rain_soon").catch(() => null);
    if (!soon) return;
    const [start, mm] = soon;
    const minutes = Math.round((start - Date.now()) / 60_000);
    // Nur einmal je Regenbeginn, und nur, wenn es in der naechsten Stunde losgeht.
    const key = String(Math.round(start / 900_000));
    if (minutes > 75 || key === this.rainWarned) return;
    this.rainWarned = key;
    const at = new Date(start).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
    const strength = mm >= 2 ? t("kräftig", "heavy") : mm >= 0.5 ? t("mäßig", "moderate") : t("leicht", "light");
    this.pushCard({ id: "rain", kind: "rain", title: t(`Regen ab ${at}`, `Rain from ${at}`), sub: t(`in etwa ${Math.max(0, minutes)} Min · ${strength} · ${this.settings.weatherPlace}`, `in about ${Math.max(0, minutes)} min · ${strength} · ${this.settings.weatherPlace}`), until: Date.now() + 20_000 });
  }

  /** Feierabend: einmal am Tag eine Karte mit dem Tag in Zahlen. */
  private checkEvening() {
    if (!this.settings.eveningReview || !this.settingsLoaded) return;
    const now = new Date();
    if (now.getHours() < this.settings.eveningHour) return;
    const today = now.toDateString();
    try {
      if (localStorage.getItem("milo.reviewCard") === today) return;
      localStorage.setItem("milo.reviewCard", today);
    } catch {
      return;
    }
    const stats = readStats();
    const parts = [
      stats.timers ? t(`${stats.timers} Timer`, `${stats.timers} timer${stats.timers === 1 ? "" : "s"}`) : "",
      stats.focus ? t(`${stats.focus}× Fokus`, `${stats.focus}× focus`) : "",
      stats.claude ? t(`Claude: ${stats.claude} Aufgaben`, `Claude: ${stats.claude} tasks`) : "",
    ].filter(Boolean);
    const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    void (inTauri ? invoke<AgendaEvent[]>("agenda_list", { from: startOfDay, to: startOfDay + 86_400_000 }) : Promise.resolve([] as AgendaEvent[]))
      .catch(() => [] as AgendaEvent[])
      .then((events) => {
        if (events.length) parts.splice(1, 0, t(`${events.length} Termine`, `${events.length} event${events.length === 1 ? "" : "s"}`));
        this.pushCard({ id: "review", kind: "review", title: t("Feierabend! So war dein Tag", "Done for today! Your day"), sub: parts.length ? parts.join(" · ") : t("Ein ruhiger Tag.", "A quiet day.") });
      });
  }

  /** Termine, deren Beginn schon angekuendigt (geklingelt) wurde. */
  private announced = new Set<string>();
  private pillTimer = 0;
  private agendaTimer = 0;

  /** Der Termin, der gerade in der Pille steht: ab dem Vorlauf bis eine Minute nach Beginn. */
  nextEvent = $derived.by(() => {
    const lead = this.settings.eventLead;
    if (!lead) return null;
    const now = this.clockNow;
    return (
      this.upcoming.find(
        (e) => !e.allDay && !this.hiddenEvents.includes(eventKey(e)) && e.startMs - now <= lead * 60_000 && now < e.startMs + 60_000,
      ) ?? null
    );
  });
  /** Der Termin hat gerade begonnen: die Pille waechst kurz und zeigt ihn gross. */
  eventNow = $derived(this.nextEvent && this.clockNow >= this.nextEvent.startMs ? this.nextEvent : null);

  /** Startet die Uhr der Pille und das regelmaessige Nachladen der Termine. */
  private startPill() {
    clearInterval(this.pillTimer);
    clearInterval(this.agendaTimer);
    const tick = () => {
      this.clockNow = Date.now();
      const now = this.eventNow;
      if (now && !this.announced.has(eventKey(now))) {
        this.announced.add(eventKey(now));
        invoke("play_chime").catch(() => {});
        this.gesture("perk");
      }
    };
    // Kurz vor einem Termin jede Sekunde (Countdown), sonst alle 5 Sekunden.
    this.pillTimer = window.setInterval(() => {
      const e = this.nextEvent;
      if (this.focus || !e || Date.now() - this.clockNow >= 5000 || e.startMs - Date.now() < 120_000) tick();
      this.pillTick();
    }, 1000);
    void this.loadUpcoming();
    this.agendaTimer = window.setInterval(() => void this.loadUpcoming(), 120_000);
  }

  async loadUpcoming() {
    if (!inTauri) return;
    const now = Date.now();
    const lead = Math.max(60, this.settings.eventLead || 0);
    this.upcoming = await invoke<AgendaEvent[]>("agenda_list", { from: now - 3_600_000, to: now + (lead + 30) * 60_000 }).catch(() => this.upcoming);
  }

  /** Rechtsklick in der Pille: diesen Termin nicht mehr zeigen. */
  hideEvent(e: AgendaEvent) {
    const key = eventKey(e);
    this.hiddenEvents = [...this.hiddenEvents.filter((k) => k !== key), key].slice(-200);
    writeList("milo.hiddenEvents", this.hiddenEvents);
  }
  private helperTimer = 0;

  /** Nebenfenster (Einstellungen): nur Einstellungen und Status, keine Ablaeufe der Insel. */
  async initLite() {
    if (!inTauri) return;
    this.settings = await invoke<Settings>("get_settings");
    applyLanguage(this.settings.language);
    await listen<Settings>("milo://settings", ({ payload }) => {
      this.settings = payload;
      applyLanguage(payload.language);
    });
    await listen<ClaudeStatus>("milo://claude", ({ payload }) => (this.claude = payload));
    this.claude = await invoke<ClaudeStatus>("claude_status");
    this.cloud = await invoke<CloudStatus>("cloud_status");
  }

  async init() {
    if (!inTauri) return;
    this.settings = await invoke<Settings>("get_settings");
    this.settingsLoaded = true;
    applyLanguage(this.settings.language);
    // Aenderungen aus dem Einstellungsfenster sofort uebernehmen (bei neuer Sprache: neu laden).
    await listen<Settings>("milo://settings", ({ payload }) => {
      this.settings = payload;
      applyLanguage(payload.language);
    });
    await listen<AgentEvent>("milo://agent", ({ payload }) => this.handle(payload));
    await listen<Timer[]>("milo://timers", ({ payload }) => this.setTimers(payload));
    await listen<Question>("milo://confirm", ({ payload }) => this.questions.push(payload));
    await listen<number>("milo://confirm-done", ({ payload }) => {
      this.questions = this.questions.filter((q) => q.id !== payload);
    });
    this.setTimers(await invoke<Timer[]>("list_timers"));
    this.loadFiles();
    await listen("milo://wipe-request", () => (this.wiping = true));
    await listen<ClaudeStatus>("milo://claude", ({ payload }) => this.setClaude(payload));
    this.claude = await invoke<ClaudeStatus>("claude_status");
    this.cloud = await invoke<CloudStatus>("cloud_status");
    await listen<NowPlaying | null>("milo://media", ({ payload }) => (this.media = payload));
    this.media = await invoke<NowPlaying | null>("now_playing");
    await listen<PowerPlan | null>("milo://power", ({ payload }) => (this.powerPlan = payload));
    this.powerPlan = await invoke<PowerPlan | null>("power_plan");
    await listen<VoiceEvent>("milo://voice", ({ payload }) => this.onVoice(payload));
    await listen<{ sleeping: boolean }>("milo://gamemode", ({ payload }) => (this.sleeping = payload.sleeping));
    await listen<boolean>("milo://fullscreen", ({ payload }) => (this.fullscreen = payload));
    this.fullscreen = await invoke<boolean>("is_fullscreen");
    await listen<SystemSnapshot>("milo://monitor", ({ payload }) => (this.system = payload));
    await listen<{ offer: TextOffer | null; error: string | null }>("milo://textwork", ({ payload }) => this.onTextOffer(payload));
    this.startPill();
    // Kleine Waechter des Kerns: jede Meldung wird eine Karte in der Pille.
    await listen<{ path: string; name: string }>("milo://download", ({ payload }) =>
      this.pushCard({ id: "download", kind: "download", title: t("Download fertig", "Download finished"), sub: payload.name, path: payload.path, until: Date.now() + 15_000 }),
    );
    await listen<{ device: string; percent: number }>("milo://battery", ({ payload }) =>
      this.pushCard({ id: `battery-${payload.device}`, kind: "battery", title: t(`${payload.device}: ${payload.percent} %`, `${payload.device}: ${payload.percent} %`), sub: t("Bald aufladen", "Charge soon"), until: Date.now() + 20_000 }),
    );
    await listen<{ path: string; thumb: string }>("milo://screenshot", ({ payload }) =>
      this.pushCard({ id: "screenshot", kind: "screenshot", title: t("Bildschirmfoto", "Screenshot"), sub: t("In der Zwischenablage", "On the clipboard"), path: payload.path, thumb: payload.thumb, until: Date.now() + 25_000 }),
    );
    await listen<{ mic: string[]; cam: string[] }>("milo://privacy", ({ payload }) => (this.privacy = payload));
    this.privacy = await invoke<{ mic: string[]; cam: string[] }>("privacy_status").catch(() => this.privacy);
    await listen<ClipItem[]>("milo://clipboard", ({ payload }) => (this.clips = payload));
    this.clips = await invoke<ClipItem[]>("clipboard_history").catch(() => []);
    await listen<{ action: string; minutes: number }>("milo://focus", ({ payload }) => (payload.action === "stop" ? this.stopFocus() : this.startFocus(payload.minutes)));
    await listen("milo://quicknote", () => this.pushCard({ id: "note", kind: "note", title: t("Schnellnotiz", "Quick note"), sub: "" }));
  }

  // ---------- Claude ----------

  /** Neuer Zustand von Claude Code; bei Wechseln auf Wunsch eine gesprochene Ansage. */
  private setClaude(next: ClaudeStatus) {
    const before = this.claude;
    this.claude = next;
    if (before.present && before.activity === "working" && next.activity === "done") bumpStat("claude");
    if (!this.settings.claudeVoice || !next.present) return;
    const announce = (text: string) => {
      this.onNotice(text);
      if (inTauri) invoke("speak", { text }).catch(() => {});
    };
    if (next.activity === "confirm" && before.activity !== "confirm") announce(t("Claude braucht deine Freigabe.", "Claude needs your approval."));
    else if (next.activity === "question" && before.activity !== "question" && next.text) announce(t("Claude hat eine Frage.", "Claude has a question."));
    else if (next.activity === "done" && before.activity === "working") announce(t("Claude ist fertig.", "Claude is done."));
  }

  /** Antwort auf eine Freigabe-Anfrage von Claude Code. Nur auf Klick des Nutzers. */
  async answerClaude(allow: boolean, message = "") {
    await this.replyClaude(message ? { allow, message } : { allow });
  }

  /** Antwort auf eine Rueckfrage von Claude: je Frage die gewaehlte Option oder eigener Text. */
  async answerClaudeQuestion(answers: Record<string, string>) {
    await this.replyClaude({ answers });
  }

  /** Lieber in Claude selbst antworten: Milo gibt die Frage sofort an Claude zurueck. */
  async passClaudeQuestion() {
    await this.replyClaude({ passthrough: true });
  }

  private async replyClaude(reply: Record<string, unknown>) {
    const request = this.claude.permission;
    if (!request || !inTauri) return;
    this.claude = { ...this.claude, permission: null };
    try {
      await invoke("claude_reply", { id: request.id, reply });
    } catch (err) {
      this.onNotice(String(err));
    }
  }

  // ---------- System und Verbrauch ----------

  private monitorTimer = 0;

  /** Systemmesswerte holen, solange der System-Tab offen ist. */
  watchSystem(on: boolean) {
    clearInterval(this.monitorTimer);
    if (!inTauri) return;
    invoke("monitor_watch", { on });
    if (on) this.monitorTimer = window.setInterval(() => invoke("monitor_watch", { on: true }), 4000);
  }

  async loadUsage() {
    if (inTauri) this.usage = await invoke<Usage>("gemini_usage").catch(() => this.usage);
  }

  geminiModels(): Promise<string[]> {
    return invoke<string[]>("gemini_models");
  }

  // ---------- Text-Werkzeuge ----------

  private onTextOffer(payload: { offer: TextOffer | null; error: string | null }) {
    this.textResult = "";
    this.textError = payload.error ?? "";
    this.textOffer = payload.offer;
    this.onTextWork();
  }

  /** Wird von der Insel gesetzt: das Menue der Text-Werkzeuge zeigen. */
  onTextWork: () => void = () => {};

  async textTransform(action: string) {
    if (!this.textOffer || this.textBusy) return;
    this.textBusy = true;
    this.textError = "";
    try {
      const done = await invoke<{ result: string; pasted: boolean; copied: boolean }>("text_transform", { action });
      this.gesture("success");
      if (done.pasted) {
        this.textOffer = null;
        this.onNotice(t("Eingefügt.", "Pasted."));
      } else {
        this.textResult = done.result;
      }
    } catch (err) {
      this.textError = String(err);
      this.gesture("error");
    } finally {
      this.textBusy = false;
    }
  }

  textCancel() {
    this.textOffer = null;
    this.textResult = "";
    this.textError = "";
    if (inTauri) invoke("text_cancel");
  }

  // ---------- Tagesablauf ----------

  /** Morgen-Briefing oder Abendfrage, hoechstens einmal am Tag. */
  async dailyCheck() {
    if (!inTauri || this.busy) return;
    const message = await invoke<{ kind: string; text: string } | null>("daily_check").catch(() => null);
    if (!message) return;
    this.messages.push({ id: this.nextId++, role: "milo", text: message.text, tools: [], pending: false, error: false });
    this.gesture(message.kind === "briefing" ? "wave" : "tilt");
    if (this.settings.speakAnswers) invoke("speak", { text: message.text }).catch(() => {});
    this.onAnswer();
  }

  /** Steuert die laufende Wiedergabe. */
  mediaCommand(action: "toggle" | "next" | "previous" | "seek" | "skip", positionMs?: number): Promise<void> {
    return invoke("media_command", { action, positionMs });
  }

  // ---------- Sprache ----------

  private onVoice(event: VoiceEvent) {
    switch (event.kind) {
      case "state":
        this.voice = event.state;
        if (event.state !== "listening") this.voiceLevel = 0;
        // Wer spricht, will nicht gleichzeitig vorgelesen bekommen.
        if (event.state === "listening") invoke("stop_speaking");
        break;
      case "level":
        this.voiceLevel = event.level;
        break;
      case "heard":
        // Hoert gerade die Widget-KI zu, geht der Text dorthin statt in den Chat.
        if (this.voiceTarget) {
          const target = this.voiceTarget;
          this.voiceTarget = null;
          target(event.text);
        } else if (this.busy) {
          this.onNotice(t("Moment, ich bin noch beschäftigt.", "One moment, I'm still busy."));
        } else {
          this.spoken = true;
          this.send(event.text);
        }
        break;
      case "notice":
        this.onNotice(event.message);
        break;
    }
  }

  /** Empfaenger fuer den naechsten erkannten Satz (z. B. die Widget-KI) statt des Chats. */
  voiceTarget: ((text: string) => void) | null = null;

  /** Startet das Zuhoeren oder beendet die laufende Aufnahme. */
  toggleVoice() {
    if (inTauri) invoke("voice_toggle");
  }

  cancelVoice() {
    if (inTauri) invoke("voice_cancel");
  }

  voiceInfo(): Promise<VoiceInfo> {
    return invoke<VoiceInfo>("voice_info");
  }

  // ---------- Einstellungen und Gemini ----------

  /** Aendert einzelne Einstellungen und speichert sie. */
  async saveSettings(patch: Partial<Settings>) {
    const wanted = { ...this.settings, ...patch };
    this.settings = wanted;
    if (inTauri) this.settings = await invoke<Settings>("save_settings", { settings: wanted });
    // Vorschau im Browser: die Sprache trotzdem umschalten (laedt neu).
    else if (patch.language) applyLanguage(patch.language);
  }

  /** Hinterlegt den Gemini-Schluessel; der Kern prueft ihn und behaelt einen abgelehnten nicht. */
  async setCloudKey(key: string) {
    this.cloud = await invoke<CloudStatus>("set_gemini_key", { key });
    this.settings = await invoke<Settings>("get_settings");
  }

  async deleteCloudKey() {
    this.cloud = await invoke<CloudStatus>("delete_gemini_key");
  }

  /** Bewertet einen Text, ohne ihn auszufuehren. */
  rate(text: string): Promise<Rating> {
    return invoke<Rating>("rate_text", { text });
  }

  /** Was zuletzt an Gemini ging (oder abgelehnt wurde). */
  async cloudLog(): Promise<Action[]> {
    if (!inTauri) return [];
    return (await invoke<Action[]>("list_actions")).filter((a) => a.tool === "cloud");
  }

  /** Verbindet Milo mit Claude Code (Hooks und Statuszeile) oder trennt die Verbindung. */
  async connectClaude(enable: boolean) {
    await invoke("connect_claude", { enable });
    this.claude = await invoke<ClaudeStatus>("claude_status");
  }

  private setTimers(timers: Timer[]) {
    const ringingBefore = new Set(this.timers.filter((x) => x.ringing).map((x) => x.id));
    for (const timer of timers) if (timer.ringing && !ringingBefore.has(timer.id)) bumpStat("timers");
    this.timers = timers;
    this.now = Date.now();
    clearInterval(this.ticker);
    this.ticker = 0;
    if (timers.some((t) => !t.ringing)) {
      this.ticker = window.setInterval(() => (this.now = Date.now()), 250);
    }
  }

  private current(): Message {
    let last = this.messages.at(-1);
    if (!last || last.role !== "milo" || !last.pending) {
      this.messages.push({ id: this.nextId++, role: "milo", text: "", tools: [], pending: true, error: false });
      last = this.messages.at(-1)!;
    }
    return last;
  }

  private handle(event: AgentEvent) {
    switch (event.kind) {
      case "state":
        this.agentState = event.state;
        if (event.state !== "working") this.runningTool = "";
        break;
      case "token":
        this.current().text += event.text;
        this.onToken();
        break;
      case "tool": {
        if (event.card?.type === "weather") this.weatherAt = Date.now();
        const message = this.current();
        const note: ToolNote = {
          name: event.name, summary: event.summary, status: event.status, links: event.links ?? [], card: event.card ?? null,
        };
        const running = message.tools.find((t) => t.name === event.name && t.status === "start");
        if (running && event.status !== "start") Object.assign(running, note);
        else message.tools.push(note);
        this.runningTool = event.status === "start" ? event.name : "";
        if (event.status === "ok") this.gesture("success");
        if (event.status === "error") this.gesture("error");
        if (event.status === "declined") this.gesture("shrug");
        if (note.card?.type === "weather") this.weather = note.card;
        if (event.name === "cloud" || event.status !== "start") this.loadUsage();
        break;
      }
      case "route":
        this.current().rating = event.rating;
        break;
      case "helper":
        clearTimeout(this.helperTimer);
        this.helper = event.phase === "leave" ? null : event.phase;
        // Nach der Uebergabe verabschiedet sich der Helfer von selbst.
        if (event.phase === "done") this.helperTimer = window.setTimeout(() => (this.helper = null), 3200);
        break;
      case "done": {
        const message = this.current();
        message.text = event.text || message.text;
        message.pending = false;
        this.busy = false;
        if (this.spoken && this.settings.speakAnswers && message.text) invoke("speak", { text: message.text }).catch(() => {});
        this.spoken = false;
        this.loadUsage();
        this.onAnswer();
        break;
      }
      case "error": {
        const message = this.current();
        message.text = event.message;
        message.pending = false;
        message.error = true;
        this.busy = false;
        this.gesture("error");
        this.onAnswer();
        break;
      }
    }
  }

  async send(text: string) {
    text = text.trim();
    if (!text || this.busy) return;
    this.messages.push({ id: this.nextId++, role: "user", text, tools: [], pending: false, error: false });
    this.busy = true;
    this.tab = "chat";
    try {
      await invoke("send_message", { text });
    } catch (err) {
      this.handle({ kind: "error", message: String(err) });
    }
  }

  /** Verschiebt oder kopiert Dateien in einen Ordner (steht im Verlauf, laesst sich zuruecknehmen). */
  async transferFiles(paths: string[], target: string, copy: boolean) {
    const errors: string[] = [];
    for (const from of paths) {
      try {
        await invoke<string>("browse_transfer", { from, target, copy });
      } catch (err) {
        errors.push(String(err));
      }
    }
    this.filesChanged++;
    const name = target.split(/[\/]/).pop() ?? target;
    const done = paths.length - errors.length;
    if (done > 0) this.onNotice(t(`${done === 1 ? "Datei" : `${done} Dateien`} nach ${name} ${copy ? "kopiert" : "verschoben"}. Rückgängig im Verlauf.`, `${done === 1 ? "File" : `${done} files`} ${copy ? "copied" : "moved"} to ${name}. Undo in History.`));
    if (errors.length > 0) throw new Error(errors[0]);
  }

  cancel() {
    if (inTauri) {
      invoke("cancel");
      invoke("stop_speaking");
    }
    // Der Kern meldet den Abbruch sofort; kommt trotzdem nichts, nicht haengen bleiben.
    window.setTimeout(() => {
      if (!this.busy) return;
      const last = this.messages.at(-1);
      if (last?.role === "milo" && last.pending) {
        last.pending = false;
        last.text ||= t("Abgebrochen.", "Stopped.");
      }
      this.busy = false;
    }, 1500);
  }

  async clear() {
    this.messages = [];
    if (inTauri) await invoke("clear_chat");
  }

  /** Verbindung zu Google vorwaermen, sobald der Nutzer sich der Insel zuwendet. */
  warmUp() {
    if (inTauri && !this.busy) invoke("warm_up").catch(() => {});
  }

  /** Beantwortet eine Rueckfrage; die Karte verschwindet, sobald der Kern das bestaetigt. */
  answer(question: Question, approve: boolean) {
    this.questions = this.questions.filter((q) => q.id !== question.id);
    if (inTauri) invoke("answer_confirmation", { id: question.id, approve });
  }

  // ---------- Timer ----------

  /** Bricht einen Timer ab oder stellt einen klingelnden ab. */
  dismissTimer(timer: Pick<Timer, "id">) {
    if (inTauri) invoke("cancel_timer", { id: timer.id });
  }

  /** Stellt einen Timer direkt, ohne das Modell zu bemuehen. */
  async startTimer(label: string, seconds: number) {
    if (!inTauri) return;
    await invoke("start_timer", { label, seconds });
    this.gesture("nod");
  }

  /** Gibt einem Timer mehr Zeit; ein klingelnder schlummert. */
  extendTimer(timer: Pick<Timer, "id">, seconds: number) {
    if (inTauri) invoke("extend_timer", { id: timer.id, seconds }).catch(() => {});
  }

  // ---------- Wetter ----------

  /** Holt das Wetter fuer die Wetter-Ansicht neu. */
  async refreshWeather(place = this.weather?.place || this.settings.weatherPlace) {
    place = place.trim();
    if (!inTauri || !place || this.weatherBusy) return;
    this.weatherBusy = true;
    this.weatherError = "";
    try {
      this.weather = await invoke<Weather>("fetch_weather", { place });
      this.weatherAt = Date.now();
      if (this.settings.weatherPlace !== this.weather.place) {
        this.settings = await invoke<Settings>("save_settings", { settings: { ...this.settings, weatherPlace: this.weather.place } });
      }
    } catch (err) {
      this.weatherError = String(err);
    } finally {
      this.weatherBusy = false;
    }
  }

  // ---------- Notizen ----------

  listNotes(query: string): Promise<Note[]> {
    return inTauri ? invoke<Note[]>("list_notes", { query }) : Promise.resolve([]);
  }

  addNote(text: string): Promise<number> {
    return invoke<number>("add_note", { text });
  }

  updateNote(id: number, text: string): Promise<boolean> {
    return invoke<boolean>("update_note", { id, text });
  }

  deleteNote(id: number): Promise<boolean> {
    return invoke<boolean>("delete_note", { id });
  }

  // ---------- Dateien ----------

  async loadFiles() {
    if (inTauri) this.files = await invoke<StoredFile[]>("list_files").catch(() => this.files);
  }

  /** Nimmt hineingezogene Dateien an und zeigt im Gespraech, was daraus geworden ist. */
  async importFiles(paths: string[]) {
    if (!inTauri || paths.length === 0) return;
    this.tab = "chat";
    try {
      const report = await invoke<{ files: StoredFile[]; errors: string[] }>("import_files", { paths });
      await this.loadFiles();
      const got = report.files;
      const unreadable = got.filter((f) => f.chars === 0);
      const lines: string[] = [];
      if (got.length === 1) lines.push(t(`Ich habe „${got[0].name}“ bekommen.`, `Got "${got[0].name}".`));
      else if (got.length > 1) lines.push(t(`Ich habe ${got.length} Dateien bekommen.`, `Got ${got.length} files.`));
      if (got.length > unreadable.length) lines.push(t("Frag mich etwas dazu oder lass sie dir zusammenfassen.", "Ask me about them or have me summarize them."));
      for (const file of unreadable) lines.push(t(`„${file.name}“ bewahre ich auf, kann sie aber nicht lesen: ${file.note}`, `I'll keep "${file.name}", but I can't read it: ${file.note}`));
      lines.push(...report.errors);
      this.messages.push({
        id: this.nextId++, role: "milo", text: lines.join("\n"), pending: false, error: got.length === 0,
        tools: got.length > 0 ? [{ name: "file_in", summary: t("Dateien angenommen", "Files received"), status: "ok", links: [], card: { type: "files", items: got } }] : [],
      });
      this.gesture(got.length > 0 ? "gulp" : "error");
    } catch (err) {
      this.messages.push({ id: this.nextId++, role: "milo", text: String(err), tools: [], pending: false, error: true });
      this.gesture("error");
    }
    this.onAnswer();
  }

  /** Loescht Milos Kopie einer Datei (nie das Original). */
  async deleteFile(file: StoredFile) {
    await invoke("delete_file", { id: file.id });
    await this.loadFiles();
  }

  /** Bricht ein angekuendigtes Herunterfahren, einen Neustart oder ein geplantes Energiesparen ab. */
  abortPower(): Promise<void> {
    return invoke("abort_power");
  }
}

function readFocus(): Focus | null {
  try {
    const value = JSON.parse(localStorage.getItem("milo.focus") ?? "null");
    return value && typeof value.endMs === "number" && (value.phase === "focus" || value.phase === "break") ? value : null;
  } catch {
    return null;
  }
}

function saveFocus(focus: Focus | null) {
  try {
    if (focus) localStorage.setItem("milo.focus", JSON.stringify(focus));
    else localStorage.removeItem("milo.focus");
  } catch {
    /* ohne Speicher gilt der Fokus nur bis zum Neustart */
  }
}

/** Zahlen fuer den Abendrueckblick (nur fuer heute, lokal). */
type DayStats = { date: string; timers: number; focus: number; claude: number };
function readStats(): DayStats {
  const today = new Date().toDateString();
  try {
    const value = JSON.parse(localStorage.getItem("milo.dayStats") ?? "null");
    if (value && value.date === today) return value;
  } catch {
    /* neu anfangen */
  }
  return { date: today, timers: 0, focus: 0, claude: 0 };
}

function bumpStat(key: "timers" | "focus" | "claude") {
  const stats = readStats();
  stats[key] += 1;
  try {
    localStorage.setItem("milo.dayStats", JSON.stringify(stats));
  } catch {
    /* egal */
  }
}

function readList(key: string): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value.filter((v) => typeof v === "string") : [];
  } catch {
    return [];
  }
}

function writeList(key: string, list: string[]) {
  try {
    localStorage.setItem(key, JSON.stringify(list));
  } catch {
    /* ohne Speicher gilt es nur bis zum Neustart */
  }
}

export const app = new App();

// Nur im Entwicklungsmodus: Zugriff fuer die automatischen Pruefskripte (scripts/cdp.mjs).
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __milo: App }).__milo = app;
}
