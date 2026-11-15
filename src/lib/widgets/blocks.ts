// Was ein Baustein anzeigt: Beschriftung, Wert und, wo es passt, ein Anteil (Balken oder Ring),
// ein Verlauf (kleine Kurve), eine Analoguhr oder Binaerziffern. Wird vom Widget selbst und von
// den Kacheln im Baukasten (Live-Vorschau jedes Bausteins) benutzt.
import { fileSize } from "../app.svelte";
import type { Feed } from "./feed.svelte";
import { ZONES, type Block } from "./types";
import { t, lang, locale } from "../i18n";

export interface View {
  label: string;
  value: string;
  /** 0 bis 100, wenn der Baustein ein Anteil ist (Balken oder Ring). */
  pct: number | null;
  sub?: string;
  /** Zwei kurze Zeilen uebereinander (Netz, Laufwerksaktivitaet: runter/hoch). */
  lines?: [string, string];
  big?: boolean;
  muted?: boolean;
  weather?: { code: number; isDay: boolean };
  dot?: string;
  /** Mondphase 0 bis 1 (0 = Neumond, 0,5 = Vollmond), als kleines Bild gezeichnet. */
  moon?: number;
  /** Verlauf der letzten Messwerte (0 bis 100) als kleine Kurve. */
  spark?: number[];
  /** Als Zifferblatt zeichnen. */
  analog?: { h: number; m: number; s: number };
  /** Binaeruhr: Stunden- und Minutenbits. */
  bits?: [string, string];
}

// ---------- Hilfen ----------

function letter(block: Block): string {
  return (block.text.trim()[0] ?? "C").toUpperCase();
}

const rate = (bytes: number) => `${fileSize(bytes)}/s`;
const two = (n: number) => String(n).padStart(2, "0");
const percent = (n: number | null) => (n === null ? "–" : `${Math.round(n)} %`);

function uptime(seconds: number): string {
  const d = Math.floor(seconds / 86400), h = Math.floor((seconds % 86400) / 3600), m = Math.floor((seconds % 3600) / 60);
  return d > 0 ? t(`${d} T ${h} Std`, `${d}d ${h}h`) : t(`${h}:${two(m)} Std`, `${h}:${two(m)} h`);
}

/** Kalenderwoche nach ISO 8601. */
export function isoWeek(date: Date): number {
  const d = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
  const day = d.getUTCDay() || 7;
  d.setUTCDate(d.getUTCDate() + 4 - day);
  const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
  return Math.ceil(((d.getTime() - yearStart.getTime()) / 86400000 + 1) / 7);
}

function zoneTime(zone: string, time: Date): { time: string; city: string } {
  const city = ZONES.find(([z]) => z === zone)?.[1] ?? zone.split("/").pop()?.replace(/_/g, " ") ?? zone;
  try {
    return { time: time.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", timeZone: zone }), city };
  } catch {
    return { time: "?", city };
  }
}

/** "Name|JJJJ-MM-TT" → Name und Tage bis dahin (negativ = vorbei). */
function countdown(text: string, time: Date): { name: string; days: number | null } {
  const [name, date] = text.includes("|") ? text.split("|") : ["", text];
  const match = (date ?? "").trim().match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return { name: name.trim() || "Countdown", days: null };
  const target = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  const today = new Date(time.getFullYear(), time.getMonth(), time.getDate());
  return { name: name.trim() || "Countdown", days: Math.round((target.getTime() - today.getTime()) / 86400000) };
}

function share(kind: "day" | "week" | "month" | "year", time: Date): number {
  const t = time.getTime();
  let start: number, end: number;
  if (kind === "day") {
    start = new Date(time.getFullYear(), time.getMonth(), time.getDate()).getTime();
    end = start + 86400000;
  } else if (kind === "week") {
    const monday = new Date(time.getFullYear(), time.getMonth(), time.getDate() - ((time.getDay() + 6) % 7));
    start = monday.getTime();
    end = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + 7).getTime();
  } else if (kind === "month") {
    start = new Date(time.getFullYear(), time.getMonth(), 1).getTime();
    end = new Date(time.getFullYear(), time.getMonth() + 1, 1).getTime();
  } else {
    start = new Date(time.getFullYear(), 0, 1).getTime();
    end = new Date(time.getFullYear() + 1, 0, 1).getTime();
  }
  return ((t - start) / (end - start)) * 100;
}

/** Mondphase aus dem bekannten Neumond am 6.1.2000 und der mittleren Monatslaenge. */
function moon(now: number): { phase: number; name: string } {
  const synodic = 29.530588853;
  const age = ((((now - Date.UTC(2000, 0, 6, 18, 14)) / 86400000) % synodic) + synodic) % synodic;
  const names = lang === "en"
    ? ["New moon", "Waxing crescent", "First quarter", "Waxing gibbous", "Full moon", "Waning gibbous", "Last quarter", "Waning crescent"]
    : ["Neumond", "zunehmend", "1. Viertel", "zunehmend", "Vollmond", "abnehmend", "3. Viertel", "abnehmend"];
  return { phase: age / synodic, name: names[Math.floor(((age / synodic) * 8 + 0.5) % 8)] };
}

function left(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(total / 3600), m = Math.floor((total % 3600) / 60), s = total % 60;
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

function hoursMinutes(ms: number): string {
  const total = Math.max(0, Math.round(ms / 60000));
  const h = Math.floor(total / 60), m = total % 60;
  return h > 0 ? t(`${h}:${two(m)} Std`, `${h}:${two(m)} h`) : t(`${m} Min`, `${m} min`);
}

/** Ostersonntag nach der Gausschen Osterformel (gregorianisch). */
function easter(year: number): Date {
  const a = year % 19, b = Math.floor(year / 100), c = year % 100, d = Math.floor(b / 4), e = b % 4;
  const f = Math.floor((b + 8) / 25), g = Math.floor((b - f + 1) / 3), h = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4), k = c % 4, l = (32 + 2 * e + 2 * i - h - k) % 7, m = Math.floor((a + 11 * h + 22 * l) / 451);
  const month = Math.floor((h + l - 7 * m + 114) / 31), day = ((h + l - 7 * m + 114) % 31) + 1;
  return new Date(year, month - 1, day);
}

/** Bundesweite gesetzliche Feiertage in Deutschland; auf Englisch die in vielen Laendern ueblichen. */
export function holidays(year: number): { date: Date; name: string }[] {
  const e = easter(year);
  const rel = (days: number) => new Date(e.getFullYear(), e.getMonth(), e.getDate() + days);
  if (lang === "en")
    return [
      { date: new Date(year, 0, 1), name: "New Year's Day" },
      { date: rel(-2), name: "Good Friday" },
      { date: rel(0), name: "Easter Sunday" },
      { date: rel(1), name: "Easter Monday" },
      { date: new Date(year, 11, 24), name: "Christmas Eve" },
      { date: new Date(year, 11, 25), name: "Christmas Day" },
      { date: new Date(year, 11, 26), name: "Boxing Day" },
      { date: new Date(year, 11, 31), name: "New Year's Eve" },
    ];
  return [
    { date: new Date(year, 0, 1), name: "Neujahr" },
    { date: rel(-2), name: "Karfreitag" },
    { date: rel(1), name: "Ostermontag" },
    { date: new Date(year, 4, 1), name: "Tag der Arbeit" },
    { date: rel(39), name: "Christi Himmelfahrt" },
    { date: rel(50), name: "Pfingstmontag" },
    { date: new Date(year, 9, 3), name: "Tag der Deutschen Einheit" },
    { date: new Date(year, 11, 25), name: "1. Weihnachtstag" },
    { date: new Date(year, 11, 26), name: "2. Weihnachtstag" },
  ];
}

function nextHoliday(time: Date): { name: string; days: number } {
  const today = new Date(time.getFullYear(), time.getMonth(), time.getDate()).getTime();
  const list = [...holidays(time.getFullYear()), ...holidays(time.getFullYear() + 1)].filter((h) => h.date.getTime() >= today);
  const next = list.sort((a, b) => a.date.getTime() - b.date.getTime())[0];
  return { name: next.name, days: Math.round((next.date.getTime() - today) / 86400000) };
}

/** Kurze, gemeinfreie Sprichwoerter; jeden Tag ein anderes. */
const SAYINGS_EN = [
  "Practice makes perfect.", "Every journey begins with a single step.", "The early bird catches the worm.", "All's well that ends well.",
  "Slow and steady wins the race.", "Rome wasn't built in a day.", "Where there's a will, there's a way.", "Better late than never.",
  "Actions speak louder than words.", "Laughter is the best medicine.", "Two heads are better than one.", "Every cloud has a silver lining.",
  "Fortune favors the bold.", "Little strokes fell great oaks.", "Time and tide wait for no one.", "Don't count your chickens before they hatch.",
  "Knowledge is power.", "Patience is a virtue.", "The best things in life are free.", "Necessity is the mother of invention.",
];
const SAYINGS_DE = [
  "Übung macht den Meister.", "Aller Anfang ist schwer.", "Morgenstund hat Gold im Mund.", "Ende gut, alles gut.",
  "Wer rastet, der rostet.", "Steter Tropfen höhlt den Stein.", "In der Ruhe liegt die Kraft.", "Gut Ding will Weile haben.",
  "Probieren geht über Studieren.", "Wo ein Wille ist, ist auch ein Weg.", "Ohne Fleiß kein Preis.", "Viele Wege führen nach Rom.",
  "Einmal ist keinmal.", "Lachen ist die beste Medizin.", "Geteilte Freude ist doppelte Freude.", "Kommt Zeit, kommt Rat.",
  "Kleinvieh macht auch Mist.", "Was lange währt, wird endlich gut.", "Der Weg ist das Ziel.", "Reden ist Silber, Schweigen ist Gold.",
];
const SAYINGS = lang === "en" ? SAYINGS_EN : SAYINGS_DE;

function dayOfYear(time: Date): number {
  return Math.floor((new Date(time.getFullYear(), time.getMonth(), time.getDate()).getTime() - new Date(time.getFullYear(), 0, 1).getTime()) / 86400000) + 1;
}

/** "HH:MM" eines Zeitstempels oder einer Zeichenkette wie 2026-11-11T07:42. */
function clockOf(value: string | number): string {
  if (typeof value === "number") {
    const d = new Date(value);
    return `${two(d.getHours())}:${two(d.getMinutes())}`;
  }
  const m = value.match(/(\d{2}):(\d{2})/);
  return m ? `${m[1]}:${m[2]}` : "–";
}

/** Uhrzeit aus "…T07:42" als ms des heutigen Tages. */
function todayAt(value: string, time: Date): number | null {
  const m = value.match(/(\d{2}):(\d{2})/);
  if (!m) return null;
  return new Date(time.getFullYear(), time.getMonth(), time.getDate(), Number(m[1]), Number(m[2])).getTime();
}

const CLAUDE: Record<string, string> = { working: t("arbeitet", "working"), question: t("fragt", "asking"), confirm: t("Freigabe", "approval"), done: t("fertig", "done"), idle: t("ruht", "resting") };

/** Arbeitszeit aus "9-17" oder "8:30-16:45". */
function workHours(text: string): [number, number] {
  const m = text.match(/(\d{1,2})(?::(\d{2}))?\s*[-–]\s*(\d{1,2})(?::(\d{2}))?/);
  if (!m) return [9 * 60, 17 * 60];
  return [Number(m[1]) * 60 + Number(m[2] ?? 0), Number(m[3]) * 60 + Number(m[4] ?? 0)];
}

// ---------- Bausteine ----------

export function viewOf(block: Block, feed: Feed): View {
  const s = feed.system;
  const time = new Date(feed.now);
  const w = feed.weather;
  const v = ((): View => {
    switch (block.kind) {
      case "ram": {
        const pct = s ? (s.ramUsed / Math.max(1, s.ramTotal)) * 100 : null;
        return { label: "RAM", value: percent(pct), pct, sub: s ? `${fileSize(s.ramUsed)} von ${fileSize(s.ramTotal)}` : undefined };
      }
      case "ramfree":
        return { label: t("RAM frei", "RAM free"), value: s ? fileSize(Math.max(0, s.ramTotal - s.ramUsed)) : "–", pct: null };
      case "cpu":
        return { label: "CPU", value: percent(s ? s.cpu : null), pct: s ? s.cpu : null };
      case "cpugraph":
        return { label: "CPU", value: percent(s ? s.cpu : null), pct: null, spark: feed.cpuHistory };
      case "disk": {
        const d = feed.system?.disks.find((x) => x.mount.toUpperCase().startsWith(letter(block))) ?? null;
        const pct = d ? ((d.total - d.free) / Math.max(1, d.total)) * 100 : null;
        return { label: `${letter(block)}:`, value: percent(pct), pct, sub: d ? `${fileSize(d.free)} frei` : undefined };
      }
      case "diskio": {
        const all = !block.text.trim();
        const list = all ? (s?.disks ?? []) : (s?.disks ?? []).filter((d) => d.mount.toUpperCase().startsWith(letter(block)));
        const read = list.reduce((a, d) => a + d.read, 0);
        const write = list.reduce((a, d) => a + d.write, 0);
        return { label: all ? t("Laufwerke", "Drives") : `${letter(block)}:`, value: s ? `↓ ${rate(read)}  ↑ ${rate(write)}` : "–", pct: null, lines: s ? [`↓ ${rate(read)}`, `↑ ${rate(write)}`] : undefined };
      }
      case "net":
        return { label: t("Netz", "Network"), value: s ? `↓ ${rate(s.netDown)}  ↑ ${rate(s.netUp)}` : "–", pct: null, lines: s ? [`↓ ${rate(s.netDown)}`, `↑ ${rate(s.netUp)}`] : undefined };
      case "netgraph":
        return { label: t("Netz", "Network"), value: s ? `↓ ${rate(s.netDown)}` : "–", pct: null, spark: feed.netHistory };
      case "uptime":
        return { label: t("läuft", "up"), value: s ? uptime(s.uptime) : "–", pct: null };
      case "topapp": {
        const top = s?.top[0];
        return { label: t("Last", "Load"), value: top ? `${top.name.replace(/\.exe$/i, "")} ${Math.round(top.cpu)} %` : "–", pct: null };
      }
      case "clock":
        return { label: "", value: `${two(time.getHours())}:${two(time.getMinutes())}`, pct: null, big: true };
      case "seconds":
        return { label: "", value: `${two(time.getHours())}:${two(time.getMinutes())}:${two(time.getSeconds())}`, pct: null, big: true };
      case "analog":
        return { label: "", value: `${two(time.getHours())}:${two(time.getMinutes())}`, pct: null, analog: { h: time.getHours(), m: time.getMinutes(), s: time.getSeconds() } };
      case "binary":
        return {
          label: t("binär", "binary"), value: `${two(time.getHours())}:${two(time.getMinutes())}`, pct: null,
          bits: [time.getHours().toString(2).padStart(5, "0"), time.getMinutes().toString(2).padStart(6, "0")],
        };
      case "date":
        return { label: "", value: time.toLocaleDateString(locale, { weekday: "short", day: "numeric", month: "short" }), pct: null };
      case "week":
        return { label: "KW", value: String(isoWeek(time)), pct: null };
      case "dayofyear":
        return { label: t("Tag", "Day"), value: `${dayOfYear(time)}`, pct: null, sub: t(`von ${new Date(time.getFullYear(), 1, 29).getMonth() === 1 ? 366 : 365}`, `of ${new Date(time.getFullYear(), 1, 29).getMonth() === 1 ? 366 : 365}`) };
      case "worldclock": {
        const z = zoneTime(block.text || "America/New_York", time);
        return { label: z.city, value: z.time, pct: null };
      }
      case "countdown": {
        const c = countdown(block.text, time);
        const value = c.days === null ? t("Datum?", "Date?") : c.days === 0 ? t("heute!", "today!") : c.days > 0 ? t(`noch ${c.days} ${c.days === 1 ? "Tag" : "Tage"}`, `${c.days} ${c.days === 1 ? "day" : "days"} left`) : t("vorbei", "over");
        return { label: c.name, value, pct: null };
      }
      case "holiday": {
        const h = nextHoliday(time);
        return { label: h.name, value: h.days === 0 ? t("heute", "today") : h.days === 1 ? t("morgen", "tomorrow") : t(`in ${h.days} Tagen`, `in ${h.days} days`), pct: null };
      }
      case "weekend": {
        const day = time.getDay();
        if (day === 0 || day === 6) return { label: t("Wochenende", "Weekend"), value: t("jetzt!", "now!"), pct: 100 };
        const friday = new Date(time.getFullYear(), time.getMonth(), time.getDate() + (5 - day), 18).getTime();
        return { label: t("Wochenende", "Weekend"), value: friday > feed.now ? `in ${hoursMinutes(friday - feed.now)}` : t("gleich", "soon"), pct: (share("week", time) / (5 / 7 + 18 / 24 / 7)) };
      }
      case "workday": {
        const [from, to] = workHours(block.text);
        const minute = time.getHours() * 60 + time.getMinutes();
        if (minute < from) return { label: t("Arbeitsbeginn", "Work starts"), value: `in ${hoursMinutes((from - minute) * 60000)}`, pct: 0 };
        if (minute >= to) return { label: t("Feierabend", "Day's done"), value: t("geschafft", "made it"), pct: 100 };
        return { label: t("Feierabend", "Day's done"), value: `in ${hoursMinutes((to - minute) * 60000)}`, pct: ((minute - from) / Math.max(1, to - from)) * 100 };
      }
      case "dayprogress": {
        const pct = share("day", time);
        return { label: t("Tag", "Day"), value: `${Math.floor(pct)} %`, pct };
      }
      case "weekprogress": {
        const pct = share("week", time);
        return { label: t("Woche", "Week"), value: `${Math.floor(pct)} %`, pct };
      }
      case "monthprogress": {
        const pct = share("month", time);
        return { label: time.toLocaleDateString(locale, { month: "long" }), value: `${Math.floor(pct)} %`, pct };
      }
      case "yearprogress": {
        const pct = share("year", time);
        return { label: String(time.getFullYear()), value: `${Math.floor(pct)} %`, pct };
      }
      case "moon": {
        const m = moon(feed.now);
        return { label: t("Mond", "Moon"), value: m.name, pct: null, moon: m.phase };
      }
      case "weather":
        return w ? { label: w.place, value: `${Math.round(w.temp)}°`, pct: null, weather: { code: w.code, isDay: w.isDay } } : { label: t("Wetter", "Weather"), value: "…", pct: null, muted: true };
      case "feels":
        return w ? { label: t("gefühlt", "feels like"), value: `${Math.round(w.feels)}°`, pct: null } : { label: t("gefühlt", "feels like"), value: "…", pct: null, muted: true };
      case "wind":
        return w ? { label: "Wind", value: `${Math.round(w.wind)} km/h`, pct: null } : { label: "Wind", value: "…", pct: null, muted: true };
      case "humidity":
        return w ? { label: t("Feuchte", "Humidity"), value: `${Math.round(w.humidity)} %`, pct: w.humidity } : { label: t("Feuchte", "Humidity"), value: "…", pct: null, muted: true };
      case "rain":
        return w ? { label: t("Regen heute", "Rain today"), value: `${Math.round(w.rain)} %`, pct: w.rain } : { label: t("Regen", "Rain"), value: "…", pct: null, muted: true };
      case "forecast":
        return w?.tomorrow
          ? { label: t("Morgen", "Tomorrow"), value: `${Math.round(w.tomorrow.min)}° / ${Math.round(w.tomorrow.max)}°`, pct: null, weather: { code: w.tomorrow.code, isDay: true } }
          : { label: t("Morgen", "Tomorrow"), value: "…", pct: null, muted: true };
      case "sun": {
        if (!w?.sunrise) return { label: t("Sonne", "Sun"), value: "…", pct: null, muted: true };
        const set = todayAt(w.sunset, time) ?? 0;
        const up = feed.now < set;
        return { label: up ? t("Untergang", "Sunset") : t("Aufgang", "Sunrise"), value: clockOf(up ? w.sunset : w.sunrise), pct: null, sub: t(`Aufgang ${clockOf(w.sunrise)}`, `Sunrise ${clockOf(w.sunrise)}`) };
      }
      case "daylight": {
        if (!w?.sunrise) return { label: t("Tageslicht", "Daylight"), value: "…", pct: null, muted: true };
        const rise = todayAt(w.sunrise, time) ?? 0, set = todayAt(w.sunset, time) ?? 0;
        if (feed.now >= set) return { label: t("Tageslicht", "Daylight"), value: t("vorbei", "over"), pct: 0 };
        if (feed.now < rise) return { label: t("Sonne in", "Sun in"), value: hoursMinutes(rise - feed.now), pct: 100 };
        return { label: t("Tageslicht", "Daylight"), value: t(`noch ${hoursMinutes(set - feed.now)}`, `${hoursMinutes(set - feed.now)} left`), pct: ((set - feed.now) / Math.max(1, set - rise)) * 100 };
      }
      case "timer":
        return feed.timer
          ? { label: feed.timer.label, value: feed.timer.ringing ? "klingelt" : left(feed.timer.dueMs - feed.now), pct: null }
          : { label: t("Timer", "Timer"), value: t("keiner", "none"), pct: null, muted: true };
      case "agenda": {
        const e = feed.agenda;
        if (!e) return { label: t("Termine", "Events"), value: t("frei", "free"), pct: null, muted: true };
        const running = e.startMs <= feed.now;
        const d = new Date(e.startMs);
        const sameDay = d.toDateString() === time.toDateString();
        const when = running ? t("läuft", "now") : e.startMs - feed.now < 3_600_000 ? t(`in ${Math.max(1, Math.round((e.startMs - feed.now) / 60000))} Min`, `in ${Math.max(1, Math.round((e.startMs - feed.now) / 60000))} min`) : sameDay ? clockOf(e.startMs) : d.toLocaleDateString(locale, { weekday: "short" }) + " " + clockOf(e.startMs);
        return { label: when, value: e.title, pct: null };
      }
      case "media":
        return feed.media
          ? { label: feed.media.playing ? t("spielt", "playing") : t("Pause", "paused"), value: `${feed.media.title}${feed.media.artist ? ` · ${feed.media.artist}` : ""}`, pct: null }
          : { label: t("Musik", "Music"), value: t("nichts läuft", "nothing playing"), pct: null, muted: true };
      case "note":
        return { label: t("Notiz", "Note"), value: feed.note || t("keine", "none"), pct: null, muted: !feed.note };
      case "quote":
        return { label: "", value: SAYINGS[(dayOfYear(time) + time.getFullYear()) % SAYINGS.length], pct: null };
      case "quota": {
        const pct = feed.quota ? (feed.quota.used / Math.max(1, feed.quota.limit)) * 100 : null;
        return { label: "Gemini", value: feed.quota ? `${feed.quota.used}/${feed.quota.limit}` : "–", pct };
      }
      case "claudelimit":
        return { label: "Claude 5 h", value: percent(feed.claudeLimit), pct: feed.claudeLimit };
      case "claude": {
        const on = feed.claude?.present;
        return { label: "Claude", value: on ? CLAUDE[feed.claude!.activity] ?? "" : t("aus", "off"), pct: null, dot: on ? feed.claude!.activity : "off" };
      }
      case "battery":
        return feed.battery?.present
          ? { label: feed.battery.charging ? t("Akku lädt", "Charging") : t("Akku", "Battery"), value: `${feed.battery.percent} %`, pct: feed.battery.percent }
          : { label: t("Akku", "Battery"), value: t("keiner", "none"), pct: null, muted: true };
      case "text":
        return { label: "", value: block.text || t("Text", "Text"), pct: null };
      default:
        return { label: block.kind, value: "?", pct: null, muted: true };
    }
  })();
  return block.label ? { ...v, label: block.label } : v;
}
