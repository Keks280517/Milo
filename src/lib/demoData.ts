import { t } from "./i18n";
import type { Action, Alarm, ClaudeStatus, DiskEntry, Note, Place, SystemSnapshot, Timer, Usage, Weather } from "./app.svelte";

// Beispieldaten fuer die gefuehrte Tour. Die echten Ansichten zeigen sie anstelle der geladenen Daten,
// solange ihr Tour-Schritt laeuft; so sieht alles genau aus wie spaeter in der App. Nichts davon wird
// gespeichert oder an den Kern geschickt.

const midnight = (ms: number) => {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
};

export function demoEvents() {
  const now = Date.now();
  const soon = Math.ceil(now / 900_000) * 900_000 + 30 * 60_000;
  const day = midnight(now);
  return [
    { id: "demo-1", title: t("Zahnarzt", "Dentist"), startMs: soon, endMs: soon + 45 * 60_000, allDay: false, location: t("Praxis am Markt", "Main Street practice"), source: "local" },
    { id: "demo-2", title: t("Training mit Lea", "Training with Lea"), startMs: soon + 150 * 60_000, endMs: soon + 240 * 60_000, allDay: false, location: t("Sporthalle", "Gym"), source: "local" },
    { id: "demo-3", title: t("Videocall Projekt", "Project video call"), startMs: soon + 330 * 60_000, endMs: soon + 360 * 60_000, allDay: false, location: "Teams", source: t("Arbeit", "Work") },
    { id: "demo-4", title: t("Omas Geburtstag", "Grandma's birthday"), startMs: day, endMs: day + 86_400_000, allDay: true, location: "", source: "local" },
  ];
}

export function demoTimers(): Timer[] {
  const now = Date.now();
  return [{ id: -1, label: t("Pizza", "Pizza"), dueMs: now + 9 * 60_000 + 41_000, totalMs: 15 * 60_000, ringing: false }];
}

export const demoAlarms = (): Alarm[] => [
  { id: "demo-a", time: "07:00", days: [0, 1, 2, 3, 4], label: t("Aufstehen", "Wake up"), enabled: true },
  { id: "demo-b", time: "09:30", days: [5, 6], label: t("Wochenende", "Weekend"), enabled: false },
];

export function demoNotes(): Note[] {
  const at = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
  return [
    { id: -1, text: t("Geschenkidee für Mia: Polaroid-Kamera, gesehen bei Foto Schneider für 89 €.", "Gift idea for Mia: instant camera, seen at the photo shop for $89."), createdAt: at(20), updatedAt: at(20) },
    { id: -2, text: t("Reifenwechsel bis Ende Oktober. Termin bei der Werkstatt machen.", "Change tyres by end of October. Book the garage."), createdAt: at(300), updatedAt: at(300) },
    { id: -3, text: t("Einkauf: Milch, Haferflocken, Äpfel, Kaffee", "Groceries: milk, oats, apples, coffee"), createdAt: at(1500), updatedAt: at(1500) },
  ];
}

export function demoActions(): Action[] {
  const at = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
  return [
    { id: -1, at: at(2), tool: "move_file", summary: t("Rechnung_Oktober.pdf nach Dokumente verschoben", "Moved Invoice_October.pdf to Documents"), ok: true, detail: "", ms: 120, undoable: true, undone: false },
    { id: -2, at: at(15), tool: "set_timer", summary: t("Timer „Pizza“ auf 15 Minuten gestellt", "Set timer \"Pizza\" for 15 minutes"), ok: true, detail: "", ms: 8, undoable: true, undone: false },
    { id: -3, at: at(42), tool: "open_app", summary: t("Spotify geöffnet", "Opened Spotify"), ok: true, detail: "", ms: 640, undoable: false, undone: false },
    { id: -4, at: at(95), tool: "web_research", summary: t("Recherche: beste Wanderwege in der Nähe", "Research: best hiking trails nearby"), ok: true, detail: "", ms: 4200, undoable: false, undone: false },
  ];
}

export const DEMO_DOWNLOADS = "C:\\Users\\Demo\\Downloads";

export const demoPlaces = (): Place[] => [
  { name: "Downloads", path: DEMO_DOWNLOADS, builtin: true },
  { name: t("Dokumente", "Documents"), path: "C:\\Users\\Demo\\Documents", builtin: true },
];

export function demoEntries(): DiskEntry[] {
  const now = Date.now();
  const entry = (name: string, kind: DiskEntry["kind"], size: number, minutes: number): DiskEntry => ({
    name,
    path: `${DEMO_DOWNLOADS}\\${name}`,
    dir: false,
    size,
    modifiedMs: now - minutes * 60_000,
    kind,
    partial: false,
  });
  return [
    entry(t("Rechnung_Oktober.pdf", "Invoice_October.pdf"), "pdf", 248_000, 3),
    entry(t("Urlaub_Gardasee.jpg", "Holiday_Lake.jpg"), "image", 3_100_000, 40),
    entry("Minecraft-Modpack.zip", "archive", 182_000_000, 90),
    entry(t("Bewerbung_final.docx", "Application_final.docx"), "office", 64_000, 300),
    entry(t("Podcast_Folge_12.mp3", "Podcast_episode_12.mp3"), "audio", 41_000_000, 1500),
  ];
}

export function demoClaude(): ClaudeStatus {
  return {
    present: true,
    connected: true,
    activity: "working",
    text: "",
    project: "Shop-App",
    tool: "Edit",
    file: "C:\\Projekte\\shop-app\\src\\Header.tsx",
    old: 'export function Header() {\n  return <h1 className="title">Shop</h1>;\n}',
    snippet: 'export function Header() {\n  const dark = useTheme() === "dark";\n  return <h1 className={dark ? "title dark" : "title"}>Shop</h1>;\n}',
    said: t("Ich stelle die Kopfzeile auf den Dark Mode um. Die Farben sind schon angelegt, jetzt folgt der Schalter in den Einstellungen.", "I'm switching the header to dark mode. The colors are in place; next comes the toggle in the settings."),
    todos: [
      { text: t("Farben für Dark Mode anlegen", "Add dark mode colors"), status: "completed" },
      { text: t("Kopfzeile umstellen", "Update the header"), status: "in_progress" },
      { text: t("Schalter in den Einstellungen", "Toggle in settings"), status: "pending" },
    ],
    steps: [
      { tool: "Read", what: "theme.ts", at: "14:02:11" },
      { tool: "Edit", what: "colors.css", at: "14:02:40" },
      { tool: "Edit", what: "Header.tsx", at: "14:03:05" },
    ],
    limits: { fiveHour: { usedPercent: 34, resetsAt: Math.round(Date.now() / 1000) + 2 * 3600 }, sevenDay: { usedPercent: 12, resetsAt: Math.round(Date.now() / 1000) + 4 * 86400 }, model: "Opus", context: 41, at: Date.now() },
    permission: null,
  };
}

export function demoSystem(cpu: number): SystemSnapshot {
  const gb = 1024 ** 3;
  return {
    cpu,
    cores: Array.from({ length: 12 }, (_, i) => Math.max(2, Math.min(98, cpu + Math.sin(i * 1.7 + cpu / 9) * 22))),
    cpuName: "AMD Ryzen 7 5800X",
    ramUsed: 11.4 * gb,
    ramTotal: 32 * gb,
    disks: [
      { name: "System", mount: "C:\\", total: 1000 * gb, free: 412 * gb },
      { name: t("Spiele", "Games"), mount: "D:\\", total: 2000 * gb, free: 640 * gb },
    ],
    netDown: 2_400_000,
    netUp: 180_000,
    top: [
      { name: "Chrome", cpu: 9.4, memory: 1.9 * gb, count: 14 },
      { name: "Discord", cpu: 2.1, memory: 0.6 * gb, count: 5 },
      { name: "Spotify", cpu: 1.2, memory: 0.4 * gb, count: 3 },
      { name: "Milo", cpu: 0.6, memory: 0.2 * gb, count: 2 },
    ],
    uptime: 5 * 3600 + 42 * 60,
  };
}

export function demoWeather(): Weather {
  const hour = new Date().getHours();
  const hours = Array.from({ length: 12 }, (_, i) => {
    const h = (hour + i) % 24;
    return { hour: `${String(h).padStart(2, "0")}:00`, temp: Math.round(14 - Math.abs(h - 15) * 0.6), code: i > 7 ? 3 : 1, rain: i > 8 ? 35 : 5, isDay: h >= 7 && h < 19 };
  });
  const day = (offset: number) => new Date(Date.now() + offset * 86_400_000).toISOString().slice(0, 10);
  const label = (offset: number) => new Date(Date.now() + offset * 86_400_000).toLocaleDateString(t("de-DE", "en-US"), { weekday: "short" });
  return {
    place: t("Hamburg", "London"),
    region: t("Deutschland", "United Kingdom"),
    now: { temp: 14, feels: 12, code: 1, text: t("Überwiegend sonnig", "Mostly sunny"), wind: 12, humidity: 58, isDay: hour >= 7 && hour < 19 },
    hours,
    days: [0, 1, 2, 3, 4].map((o) => ({ date: day(o), label: label(o), min: 6 + o, max: 15 - (o % 3), code: o === 2 ? 61 : o === 3 ? 3 : 1, text: o === 2 ? t("Regen", "Rain") : t("Sonnig", "Sunny"), rain: o === 2 ? 80 : 10, wind: 10 + o * 2, sunrise: "07:21", sunset: "16:48" })),
  };
}

export function demoUsage(): Usage {
  return {
    rows: [
      { model: "gemini-2.5-flash", role: t("Chat und Werkzeuge", "Chat and tools"), today: 46, rpd: 250, minute: 2, rpm: 10, tokensToday: 182_000, tokensMinute: 6_100, tpm: 250_000, exhausted: false },
      { model: "gemini-2.5-pro", role: t("Schwere Fragen", "Hard questions"), today: 9, rpd: 100, minute: 0, rpm: 5, tokensToday: 61_000, tokensMinute: 0, tpm: 250_000, exhausted: false },
      { model: "openai/gpt-oss-120b", role: "Groq", today: 31, rpd: 1000, minute: 1, rpm: 30, tokensToday: 95_000, tokensMinute: 2_400, tpm: 8_000, exhausted: false },
    ],
    resetMs: Date.now() + 7 * 3600_000,
  };
}
