// Daten fuer Widgets. Jede Quelle laeuft nur, solange ein Baustein sie braucht: Systemwerte ueber
// die vorhandene Messung (`monitor_watch`), Claude ueber dessen Ereignis, Wetter alle 15 Minuten,
// Akku alle 30 Sekunden. So kostet ein Widget fast nichts.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { BlockKind } from "./types";

export interface SystemSnapshot {
  cpu: number;
  ramUsed: number;
  ramTotal: number;
  disks: { name: string; mount: string; total: number; free: number; read: number; write: number }[];
  netDown: number;
  netUp: number;
  top: { name: string; cpu: number }[];
  uptime: number;
}

export interface Feed {
  now: number;
  system: SystemSnapshot | null;
  claude: { present: boolean; activity: string; project: string } | null;
  weather: {
    temp: number; code: number; isDay: boolean; place: string;
    feels: number; wind: number; humidity: number; rain: number; sunrise: string; sunset: string; min: number; max: number;
    tomorrow: { min: number; max: number; code: number } | null;
  } | null;
  /** Naechster Termin aus der Agenda. */
  agenda: { title: string; startMs: number; endMs: number } | null;
  /** Letzte Messwerte (0 bis 100) fuer die kleinen Kurven. */
  cpuHistory: number[];
  netHistory: number[];
  timer: { label: string; dueMs: number; ringing: boolean } | null;
  battery: { present: boolean; percent: number; charging: boolean } | null;
  media: { title: string; artist: string; playing: boolean } | null;
  note: string | null;
  quota: { used: number; limit: number } | null;
  claudeLimit: number | null;
}

export class WidgetFeed implements Feed {
  now = $state(Date.now());
  system = $state<SystemSnapshot | null>(null);
  claude = $state<Feed["claude"]>(null);
  weather = $state<Feed["weather"]>(null);
  timer = $state<Feed["timer"]>(null);
  battery = $state<Feed["battery"]>(null);
  media = $state<Feed["media"]>(null);
  note = $state<string | null>(null);
  quota = $state<Feed["quota"]>(null);
  claudeLimit = $state<number | null>(null);
  agenda = $state<Feed["agenda"]>(null);
  cpuHistory = $state<number[]>([]);
  netHistory = $state<number[]>([]);
  /** Hoechster Netzwert der letzten Minute, damit die Kurve sinnvoll skaliert. */
  private netPeak = 1;

  private stops: (() => void)[] = [];
  private running = "";

  /** Startet genau die Quellen, die diese Bausteine brauchen (und beendet die anderen). */
  use(kinds: BlockKind[]) {
    const key = [...new Set(kinds)].sort().join(",");
    if (key === this.running) return;
    this.stop();
    this.running = key;
    const need = (k: BlockKind) => kinds.includes(k);

    const tick = window.setInterval(() => (this.now = Date.now()), 1000);
    this.stops.push(() => clearInterval(tick));

    const any = (...list: BlockKind[]) => list.some(need);
    if (any("ram", "ramfree", "cpu", "cpugraph", "disk", "diskio", "net", "netgraph", "uptime", "topapp")) {
      const ping = () => invoke("monitor_watch", { on: true }).catch(() => {});
      ping();
      const timer = window.setInterval(ping, 3000);
      this.stops.push(() => clearInterval(timer));
      this.listen<SystemSnapshot>("milo://monitor", (s) => {
        this.system = s;
        this.cpuHistory = [...this.cpuHistory, s.cpu].slice(-40);
        const raw = [...this.netHistory.map((v) => (v / 100) * this.netPeak), s.netDown].slice(-40);
        this.netPeak = Math.max(1, ...raw);
        this.netHistory = raw.map((v) => (v / this.netPeak) * 100);
      });
    }
    if (need("claude") || need("claudelimit")) {
      type Claude = NonNullable<Feed["claude"]> & { limits: { fiveHour: { usedPercent: number } | null } | null };
      const take = (c: Claude) => {
        this.claude = c;
        this.claudeLimit = c.limits?.fiveHour?.usedPercent ?? null;
      };
      invoke<Claude>("claude_status").then(take).catch(() => {});
      this.listen<Claude>("milo://claude", take);
    }
    if (need("media")) {
      invoke<Feed["media"]>("now_playing").then((m) => (this.media = m)).catch(() => {});
      this.listen<Feed["media"]>("milo://media", (m) => (this.media = m));
    }
    if (need("note")) {
      const load = () => invoke<{ text: string }[]>("list_notes", { query: "" }).then((list) => (this.note = list[0]?.text ?? "")).catch(() => {});
      load();
      const timer = window.setInterval(load, 20000);
      this.stops.push(() => clearInterval(timer));
    }
    if (need("quota")) {
      const load = () =>
        invoke<{ rows: { today: number; rpd: number }[] }>("gemini_usage")
          .then((u) => (this.quota = { used: u.rows.reduce((a, r) => a + r.today, 0), limit: u.rows.reduce((a, r) => a + r.rpd, 0) }))
          .catch(() => {});
      load();
      const timer = window.setInterval(load, 30000);
      this.stops.push(() => clearInterval(timer));
    }
    if (need("timer")) {
      const load = () => invoke<{ label: string; dueMs: number; ringing: boolean }[]>("list_timers").then((list) => (this.timer = list.sort((a, b) => a.dueMs - b.dueMs)[0] ?? null)).catch(() => {});
      load();
      this.listen("milo://timers", () => load());
      const timer = window.setInterval(load, 15000);
      this.stops.push(() => clearInterval(timer));
    }
    if (any("agenda")) {
      const load = () =>
        invoke<{ title: string; startMs: number; endMs: number; allDay: boolean }[]>("agenda_list", { from: Date.now(), to: Date.now() + 14 * 86400000 })
          .then((list) => (this.agenda = list.find((e) => !e.allDay && e.endMs > Date.now()) ?? null))
          .catch(() => {});
      load();
      const timer = window.setInterval(load, 120000);
      this.stops.push(() => clearInterval(timer));
    }
    if (any("weather", "feels", "wind", "humidity", "rain", "forecast", "sun", "daylight")) {
      const load = async () => {
        try {
          const settings = await invoke<{ weatherPlace: string }>("get_settings");
          if (!settings.weatherPlace) return;
          type Raw = {
            place: string;
            now: { temp: number; feels: number; code: number; isDay: boolean; wind: number; humidity: number };
            days: { min: number; max: number; code: number; rain: number; sunrise: string; sunset: string }[];
          };
          const w = await invoke<Raw>("fetch_weather", { place: settings.weatherPlace });
          const today = w.days[0], next = w.days[1];
          this.weather = {
            temp: w.now.temp, code: w.now.code, isDay: w.now.isDay, place: w.place, feels: w.now.feels, wind: w.now.wind, humidity: w.now.humidity,
            rain: today?.rain ?? 0, sunrise: today?.sunrise ?? "", sunset: today?.sunset ?? "", min: today?.min ?? w.now.temp, max: today?.max ?? w.now.temp,
            tomorrow: next ? { min: next.min, max: next.max, code: next.code } : null,
          };
        } catch {
          /* ohne Netz bleibt der letzte Stand */
        }
      };
      load();
      const timer = window.setInterval(load, 15 * 60000);
      this.stops.push(() => clearInterval(timer));
    }
    if (need("battery")) {
      const load = () => invoke<NonNullable<Feed["battery"]>>("battery_status").then((b) => (this.battery = b)).catch(() => {});
      load();
      const timer = window.setInterval(load, 30000);
      this.stops.push(() => clearInterval(timer));
    }
  }

  private listen<T>(event: string, handle: (payload: T) => void) {
    let stop: (() => void) | null = null;
    let dead = false;
    listen<T>(event, ({ payload }) => handle(payload)).then((fn) => (dead ? fn() : (stop = fn)));
    this.stops.push(() => {
      dead = true;
      stop?.();
    });
  }

  stop() {
    for (const stop of this.stops) stop();
    this.stops = [];
    this.running = "";
  }
}
