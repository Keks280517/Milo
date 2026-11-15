// Lokale KI (Ollama, LM Studio): Erkennung, Hardware-Pruefung, Empfehlung und Modell-Download.
// Ein gemeinsamer Zustand fuer Willkommens-Assistent, Einstellungen und Pille. Der Download selbst
// laeuft im Kern weiter, auch wenn ein Fenster zugeht; jedes Fenster hoert auf dieselben Meldungen.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { app, inTauri } from "./app.svelte";
import { t } from "./i18n";

export type LocalModel = { name: string; size: number; params: string };
export type LocalStatus = { server: string; version: string; models: LocalModel[]; ollamaInstalled: boolean };
export type HardwareCheck = {
  hardware: { cpu: string; cores: number; ramGb: number; gpu: string; vramGb: number };
  recommendation: { model: string; needsGb: number; onGpu: boolean; reason: string; alternative: string };
};
export type PullProgress = {
  model: string;
  /** "start", "download", "check", "done" oder "error" */
  phase: string;
  completed: number;
  total: number;
  percent: number;
  bytesPerSec: number;
  etaSecs: number | null;
  error: string;
};

const PULL_EVENT = "milo://local-pull";

class LocalAi {
  status = $state<LocalStatus | null>(null);
  check = $state<HardwareCheck | null>(null);
  checking = $state(false);
  starting = $state(false);
  pull = $state<PullProgress | null>(null);
  /** Wird von der Insel gesetzt: Download fertig oder fehlgeschlagen (fuer eine Karte in der Pille). */
  onFinish: (p: PullProgress) => void = () => {};

  private listening = false;
  private demoTimer = 0;

  /** Laeuft gerade ein Download? */
  get running(): boolean {
    return !!this.pull && ["start", "download", "check"].includes(this.pull.phase);
  }
  get models(): string[] {
    return this.status?.models.map((m) => m.name) ?? [];
  }
  get serverName(): string {
    return this.status?.server === "ollama" ? "Ollama" : this.status?.server === "lmstudio" ? "LM Studio" : "";
  }

  /** Liegt dieses Modell schon auf dem PC? („gemma4“ passt auch zu „gemma4:latest“.) */
  has(name: string): boolean {
    return this.models.some((m) => m === name || m === `${name}:latest` || (!name.includes(":") && m.split(":")[0] === name));
  }

  /** Einmal pro Fenster: Download-Meldungen mithoeren und einen laufenden Download uebernehmen. */
  async init() {
    if (!inTauri || this.listening) return;
    this.listening = true;
    await listen<PullProgress>(PULL_EVENT, ({ payload }) => this.onPull(payload));
    const current = await invoke<PullProgress | null>("local_ai_pull_state").catch(() => null);
    if (current && !this.pull) this.pull = current;
  }

  private onPull(next: PullProgress) {
    const before = this.pull?.phase;
    this.pull = next;
    if ((next.phase === "done" || next.phase === "error") && before !== next.phase) {
      this.onFinish(next);
      if (next.phase === "done") this.refresh();
    }
  }

  async refresh() {
    // Vorschau im Browser: so tun, als liefe Ollama ohne Modelle.
    if (!inTauri) {
      this.status ??= { server: "ollama", version: "0.13.0", models: [], ollamaInstalled: true };
      return;
    }
    this.status = await invoke<LocalStatus>("local_ai_status").catch(() => null);
    // Ist noch kein Modell gewaehlt, aber eins da: das empfohlene oder das erste vormerken.
    if (this.status && !app.settings.localModel && this.status.models.length > 0) {
      const rec = this.check?.recommendation.model;
      app.saveSettings({ localModel: rec && this.has(rec) ? rec : this.status.models[0].name });
    }
  }

  async checkHardware() {
    this.checking = true;
    try {
      if (inTauri) this.check = await invoke<HardwareCheck>("local_ai_hardware");
      else {
        // Vorschau im Browser: Beispielrechner
        await new Promise((r) => setTimeout(r, 600));
        this.check = {
          hardware: { cpu: "AMD Ryzen 5 7500F 6-Core Processor", cores: 12, ramGb: 31.7, gpu: "NVIDIA GeForce RTX 4070", vramGb: 12 },
          recommendation: { model: "gemma4:12b", needsGb: 9, onGpu: true, reason: t("Deine Grafikkarte hat genug Speicher für das gründlichere Modell: gute Antworten und zuverlässige Werkzeuge (24 von 25 im Test).", "Your graphics card has enough memory for the more thorough model: good answers and reliable tools (24 of 25 in testing)."), alternative: "qwen3:14b" },
        };
      }
    } finally {
      this.checking = false;
    }
  }

  async startOllama() {
    if (!inTauri) return;
    this.starting = true;
    try {
      await invoke<boolean>("local_ai_start");
      await this.refresh();
    } finally {
      this.starting = false;
    }
  }

  /** Modell herunterladen; danach verbindet der Kern es von selbst mit Milo. */
  async download(model: string) {
    if (this.running) return;
    this.pull = { model, phase: "start", completed: 0, total: 0, percent: 0, bytesPerSec: 0, etaSecs: null, error: "" };
    if (!inTauri) return this.demoDownload(model);
    try {
      await invoke("local_ai_pull", { model });
    } catch (err) {
      this.pull = { ...this.pull, phase: "error", error: String(err) };
    }
  }

  /** Ein vorhandenes Modell nehmen. Stand nur „Cloud“ da, wird gemischt bzw. ganz lokal gearbeitet. */
  use(model: string, mode?: string) {
    const current = app.settings.aiMode;
    const next = mode ?? (current === "cloud" ? (app.cloud.hasKey ? "mixed" : "local") : current);
    app.saveSettings({ localModel: model, aiMode: next });
  }

  dismiss() {
    if (!this.running) this.pull = null;
  }

  /** Vorschau im Browser: ein Download von 7,2 GB in etwa 20 Sekunden. */
  private demoDownload(model: string) {
    const total = 7.2 * 1024 ** 3;
    const speed = total / 20;
    let completed = 0;
    clearInterval(this.demoTimer);
    this.demoTimer = window.setInterval(() => {
      completed = Math.min(total, completed + speed / 4);
      const done = completed >= total;
      this.onPull({ model, phase: done ? "done" : "download", completed, total, percent: (completed / total) * 100, bytesPerSec: speed, etaSecs: Math.round((total - completed) / speed), error: "" });
      if (done) {
        clearInterval(this.demoTimer);
        this.status = { server: "ollama", version: "0.13.0", models: [...(this.status?.models ?? []), { name: model, size: total, params: "12B" }], ollamaInstalled: true };
        this.use(model);
      }
    }, 250);
  }
}

export const localAi = new LocalAi();

/** „noch 3 Min“, „noch 40 s“, „noch 1 Std 5 Min“. */
export function etaText(secs: number | null): string {
  if (secs === null || !Number.isFinite(secs)) return t("Restzeit wird berechnet …", "Estimating time left …");
  if (secs < 60) return t(`noch ${Math.max(1, secs)} s`, `${Math.max(1, secs)} s left`);
  const min = Math.ceil(secs / 60);
  if (min < 60) return t(`noch ${min} Min`, `${min} min left`);
  return t(`noch ${Math.floor(min / 60)} Std ${min % 60} Min`, `${Math.floor(min / 60)} h ${min % 60} min left`);
}

/** Kurze Restzeit fuer die Pille: „3 Min“, „40 s“, „1:05 h“. */
export function etaShort(secs: number | null): string {
  if (secs === null || !Number.isFinite(secs)) return "";
  if (secs < 60) return `${Math.max(1, secs)} s`;
  const min = Math.ceil(secs / 60);
  return min < 60 ? t(`${min} Min`, `${min} min`) : `${Math.floor(min / 60)}:${String(min % 60).padStart(2, "0")} h`;
}

export const gb = (bytes: number) => (bytes / 1024 ** 3).toFixed(1).replace(".", t(",", "."));
export const speedText = (bps: number) => (bps >= 1024 ** 2 ? `${(bps / 1024 ** 2).toFixed(0)} MB/s` : `${Math.max(1, Math.round(bps / 1024))} kB/s`);
