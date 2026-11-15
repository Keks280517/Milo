import { t } from "../i18n";
// Beschreibung eines eigenen Widgets (wie in widgets.rs) und die Bausteine, aus denen es besteht.

export type BlockKind =
  | "ram" | "ramfree" | "cpu" | "cpugraph" | "disk" | "diskio" | "net" | "netgraph" | "uptime" | "topapp" | "battery"
  | "clock" | "seconds" | "analog" | "binary" | "date" | "week" | "dayofyear" | "worldclock"
  | "countdown" | "holiday" | "weekend" | "workday" | "agenda"
  | "dayprogress" | "weekprogress" | "monthprogress" | "yearprogress" | "moon"
  | "weather" | "feels" | "wind" | "humidity" | "rain" | "forecast" | "sun" | "daylight"
  | "timer" | "media" | "note" | "quote" | "quota" | "claude" | "claudelimit" | "text";

/** Zustand der Wasserkugel in der KI-Hilfe. */
export type OrbState = "idle" | "listening" | "thinking" | "done";

export interface Block {
  kind: BlockKind;
  text: string;
  /** Eigene kurze Beschriftung statt der eingebauten. */
  label?: string;
}

export interface Widget {
  id: string;
  name: string;
  shape: "round" | "pill" | "card";
  size: "s" | "m" | "l";
  style: "hanging" | "floating";
  accent: string;
  blocks: Block[];
  x: number | null;
  locked: boolean;
}

/** Gruppen im Katalog (Anzeige und Filter). */
const G = { pc: "PC", time: t("Zeit", "Time"), plan: t("Planen", "Planning"), weather: t("Wetter", "Weather"), milo: "Milo" };

export const BLOCKS: { kind: BlockKind; label: string; hint: string; group: string }[] = [
  { kind: "ram", label: t("Arbeitsspeicher", "Memory"), hint: t("Belegter RAM als Balken oder Ring", "Used RAM as a bar or ring"), group: G.pc },
  { kind: "ramfree", label: t("Freier RAM", "Free RAM"), hint: t("Wie viel Arbeitsspeicher noch frei ist", "How much memory is still free"), group: G.pc },
  { kind: "cpu", label: t("Prozessor", "Processor"), hint: t("CPU-Last als Ring oder Balken", "CPU load as a ring or bar"), group: G.pc },
  { kind: "cpugraph", label: t("CPU-Verlauf", "CPU history"), hint: t("Kleine Kurve der letzten Minute", "A small graph of the last minute"), group: G.pc },
  { kind: "disk", label: t("Laufwerk", "Drive"), hint: t("Belegung eines Laufwerks (Buchstabe wählbar)", "How full a drive is (pick the letter)"), group: G.pc },
  { kind: "diskio", label: t("Laufwerksaktivität", "Drive activity"), hint: t("Lesen und Schreiben gerade (alle oder ein Laufwerk)", "Current reads and writes (all drives or one)"), group: G.pc },
  { kind: "net", label: t("Netzwerk", "Network"), hint: t("Download und Upload gerade", "Current download and upload"), group: G.pc },
  { kind: "netgraph", label: t("Netz-Verlauf", "Network history"), hint: t("Kurve des Downloads der letzten Minute", "Download graph of the last minute"), group: G.pc },
  { kind: "uptime", label: t("Laufzeit", "Uptime"), hint: t("Wie lange der PC schon läuft", "How long the PC has been running"), group: G.pc },
  { kind: "topapp", label: t("Größter Verbraucher", "Top app"), hint: t("Programm mit der meisten Prozessorlast", "The program using the most CPU"), group: G.pc },
  { kind: "battery", label: t("Akku", "Battery"), hint: t("Ladestand (nur mit Akku)", "Charge level (only with a battery)"), group: G.pc },
  { kind: "clock", label: t("Uhrzeit", "Clock"), hint: t("Stunden und Minuten", "Hours and minutes"), group: G.time },
  { kind: "seconds", label: t("Uhr mit Sekunden", "Clock with seconds"), hint: t("Stunden, Minuten, Sekunden", "Hours, minutes, seconds"), group: G.time },
  { kind: "analog", label: t("Analoguhr", "Analog clock"), hint: t("Zifferblatt mit Zeigern", "A dial with hands"), group: G.time },
  { kind: "binary", label: t("Binäruhr", "Binary clock"), hint: t("Stunden und Minuten als Lichtpunkte", "Hours and minutes as dots of light"), group: G.time },
  { kind: "date", label: t("Datum", "Date"), hint: t("Wochentag und Datum", "Weekday and date"), group: G.time },
  { kind: "week", label: t("Kalenderwoche", "Week number"), hint: t("KW nach ISO", "ISO week number"), group: G.time },
  { kind: "dayofyear", label: t("Tag im Jahr", "Day of the year"), hint: t("Der wievielte Tag heute ist", "Which day of the year it is"), group: G.time },
  { kind: "worldclock", label: t("Weltzeit", "World clock"), hint: t("Uhrzeit in einer anderen Stadt", "The time in another city"), group: G.time },
  { kind: "dayprogress", label: t("Tagesfortschritt", "Day progress"), hint: t("Wie viel vom Tag vorbei ist", "How much of the day has passed"), group: G.time },
  { kind: "weekprogress", label: t("Wochenfortschritt", "Week progress"), hint: t("Wie viel von der Woche vorbei ist", "How much of the week has passed"), group: G.time },
  { kind: "monthprogress", label: t("Monatsfortschritt", "Month progress"), hint: t("Wie viel vom Monat vorbei ist", "How much of the month has passed"), group: G.time },
  { kind: "yearprogress", label: t("Jahresfortschritt", "Year progress"), hint: t("Wie viel vom Jahr vorbei ist", "How much of the year has passed"), group: G.time },
  { kind: "moon", label: t("Mondphase", "Moon phase"), hint: t("Berechnet, ohne Internet", "Calculated offline"), group: G.time },
  { kind: "agenda", label: t("Nächster Termin", "Next event"), hint: t("Aus deiner Agenda", "From your agenda"), group: G.plan },
  { kind: "countdown", label: t("Countdown", "Countdown"), hint: t("Tage bis zu einem Datum", "Days until a date"), group: G.plan },
  { kind: "holiday", label: t("Nächster Feiertag", "Next holiday"), hint: t("Bundesweite Feiertage", "Common public holidays"), group: G.plan },
  { kind: "weekend", label: t("Bis zum Wochenende", "Until the weekend"), hint: t("Zeit bis Freitag 18 Uhr", "Time until Friday 6 pm"), group: G.plan },
  { kind: "workday", label: t("Feierabend", "End of workday"), hint: t("Countdown deiner Arbeitszeit (z. B. 9-17)", "Countdown of your working hours (e.g. 9-17)"), group: G.plan },
  { kind: "timer", label: t("Timer", "Timer"), hint: t("Restzeit des nächsten Timers", "Time left on the next timer"), group: G.plan },
  { kind: "weather", label: t("Wetter", "Weather"), hint: t("Temperatur und Himmel am gespeicherten Ort", "Temperature and sky at your saved place"), group: G.weather },
  { kind: "feels", label: t("Gefühlt", "Feels like"), hint: t("Gefühlte Temperatur", "Feels-like temperature"), group: G.weather },
  { kind: "wind", label: t("Wind", "Wind"), hint: t("Windgeschwindigkeit", "Wind speed"), group: G.weather },
  { kind: "humidity", label: t("Luftfeuchte", "Humidity"), hint: t("Relative Luftfeuchtigkeit", "Relative humidity"), group: G.weather },
  { kind: "rain", label: t("Regenrisiko", "Chance of rain"), hint: t("Regenwahrscheinlichkeit heute", "Chance of rain today"), group: G.weather },
  { kind: "forecast", label: t("Morgen", "Tomorrow"), hint: t("Tiefst- und Höchstwert morgen", "Tomorrow's low and high"), group: G.weather },
  { kind: "sun", label: t("Sonne", "Sun"), hint: t("Nächster Sonnenauf- oder -untergang", "Next sunrise or sunset"), group: G.weather },
  { kind: "daylight", label: t("Tageslicht", "Daylight"), hint: t("Wie lange es noch hell ist", "How long it stays light"), group: G.weather },
  { kind: "media", label: t("Läuft gerade", "Now playing"), hint: t("Musik oder Video, das gerade spielt", "Music or video playing right now"), group: G.milo },
  { kind: "note", label: t("Neueste Notiz", "Latest note"), hint: t("Was du Milo zuletzt notiert hast", "What you last noted with Milo"), group: G.milo },
  { kind: "quote", label: t("Spruch des Tages", "Saying of the day"), hint: t("Jeden Tag ein anderes Sprichwort", "A different proverb every day"), group: G.milo },
  { kind: "quota", label: t("Gemini-Kontingent", "Gemini quota"), hint: t("Heute verbrauchte Anfragen", "Requests used today"), group: G.milo },
  { kind: "claude", label: t("Claude", "Claude"), hint: t("Was Claude Code gerade tut", "What Claude Code is doing"), group: G.milo },
  { kind: "claudelimit", label: t("Claude-Limit", "Claude limit"), hint: t("Verbrauch im 5-Stunden-Fenster", "Usage in the 5-hour window"), group: G.milo },
  { kind: "text", label: t("Text", "Text"), hint: t("Eigener kurzer Text", "Your own short text"), group: G.milo },
];

/** Bausteine mit eigener Angabe und was dort hineingehoert. */
export const PARAM: Partial<Record<BlockKind, { placeholder: string; initial: string }>> = {
  text: { placeholder: t("Text", "Text"), initial: t("Hallo", "Hello") },
  disk: { placeholder: t("Laufwerk, z. B. C oder D", "Drive, e.g. C or D"), initial: "C" },
  diskio: { placeholder: t("Laufwerk (leer = alle)", "Drive (empty = all)"), initial: "" },
  worldclock: { placeholder: t("Zeitzone, z. B. Asia/Tokyo", "Time zone, e.g. Asia/Tokyo"), initial: "America/New_York" },
  countdown: { placeholder: t("Name|JJJJ-MM-TT", "Name|YYYY-MM-DD"), initial: `${t("Silvester", "New Year's Eve")}|${new Date().getFullYear()}-12-31` },
  workday: { placeholder: t("Arbeitszeit, z. B. 9-17", "Working hours, e.g. 9-17"), initial: "9-17" },
};

/** Zeitzonen zur Auswahl (Weltzeit). */
export const ZONES: [string, string][] = [
  ["America/New_York", "New York"], ["America/Los_Angeles", "Los Angeles"], ["America/Sao_Paulo", "São Paulo"], ["Europe/London", "London"],
  ["Europe/Istanbul", "Istanbul"], ["Asia/Dubai", "Dubai"], ["Asia/Kolkata", t("Indien", "India")], ["Asia/Bangkok", "Bangkok"], ["Asia/Shanghai", t("Peking", "Beijing")],
  ["Asia/Tokyo", t("Tokio", "Tokyo")], ["Australia/Sydney", "Sydney"],
];

export const SHAPES = [
  { id: "round", label: t("Rund", "Round"), room: 2 },
  { id: "pill", label: t("Pille", "Pill"), room: 2 },
  { id: "card", label: t("Karte", "Card"), room: 4 },
] as const;

export const ACCENTS = ["#64d2ff", "#30d158", "#ffd60a", "#ff9f0a", "#ff6961", "#bf5af2", "#f5b08a", "#f5f5f7"];

/** Sichtbare Groesse in logischen Pixeln (wie `face_size` in widgets.rs). */
export function faceSize(shape: Widget["shape"], size: Widget["size"]): { w: number; h: number } {
  const step = size === "s" ? 0 : size === "l" ? 2 : 1;
  if (shape === "round") return [{ w: 72, h: 72 }, { w: 96, h: 96 }, { w: 128, h: 128 }][step];
  if (shape === "card") return [{ w: 150, h: 88 }, { w: 200, h: 118 }, { w: 260, h: 156 }][step];
  return [{ w: 150, h: 40 }, { w: 210, h: 48 }, { w: 290, h: 58 }][step];
}

export function roomOf(shape: Widget["shape"]): number {
  return SHAPES.find((s) => s.id === shape)?.room ?? 2;
}

export function emptyWidget(): Widget {
  return { id: "", name: "Mein Widget", shape: "pill", size: "m", style: "hanging", accent: "#64d2ff", blocks: [{ kind: "ram", text: "" }, { kind: "cpu", text: "" }], x: null, locked: false };
}
