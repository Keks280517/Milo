<script lang="ts">
  import { t, lang } from "./i18n";
  // Die Insel: geschlossen eine breite Pille, in der Milo lebt und rechts das Wichtigste steht,
  // geoeffnet ein Panel mit festen Reitern (Chat, Wetter, Dateien, Notizen, System, Claude,
  // Verbrauch) und solchen, die nur bei Bedarf erscheinen (Wiedergabe, Timer).
  import { onMount, tick, untrack } from "svelte";
  import { SHELF_ICONS, shelfOrder } from "./shelf";
  import { Spring } from "svelte/motion";
  import { fade, fly } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { sceneryWeather } from "./weather";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Stage from "./character/Stage.svelte";
  import type { StageLayout } from "./character/stage";
  import Chat from "./Chat.svelte";
  import Notices from "./Notices.svelte";
  import WipeDialog from "./WipeDialog.svelte";
  import TimerView from "./views/TimerView.svelte";
  import WeatherView from "./views/WeatherView.svelte";
  import NotesView from "./views/NotesView.svelte";
  import FilesView from "./views/FilesView.svelte";
  import SettingsView from "./views/SettingsView.svelte";
  import MediaView from "./views/MediaView.svelte";
  import SystemView from "./views/SystemView.svelte";
  import ClaudeView from "./views/ClaudeView.svelte";
  import UsageView from "./views/UsageView.svelte";
  import HistoryView from "./views/HistoryView.svelte";
  import WidgetsView from "./views/WidgetsView.svelte";
  import TextWork from "./TextWork.svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import WeatherIcon from "./cards/WeatherIcon.svelte";
  import AgendaView from "./views/AgendaView.svelte";
  import HomeView from "./views/HomeView.svelte";
  import DecorLibrary from "./DecorLibrary.svelte";
  import Welcome from "./Welcome.svelte";
  import Tour from "./Tour.svelte";
  import TourCard from "./TourCard.svelte";
  import { tour } from "./tourState.svelte";
  import { app, clock, inTauri, type AgendaEvent, type PillCard, type TabId } from "./app.svelte";
  import { pop, paneIn } from "./motion";
  import { lens } from "./lens";
  import GlassDefs from "./GlassDefs.svelte";
  import { DECOR } from "./character/decor";
  import { etaShort, localAi } from "./localAi.svelte";

  const PILL = { w: 480, h: 64 };
  /** Geoeffnet: breit und flach, damit die Insel nicht weit in den Bildschirm haengt. */
  const PANELS = [{ w: 760, h: 420 }, { w: 880, h: 460 }, { w: 1000, h: 480 }];
  /** Der Widget-Baukasten braucht mehr Platz: dort wird die Insel breiter und hoeher. */
  const PANEL = $derived.by(() => {
    const base = PANELS[app.settings.islandSize] ?? PANELS[1];
    return app.tab === "widgets" ? { w: Math.min(1060, base.w + 80), h: base.h + 170 } : base;
  });
  /** Hoehe der Kopfzeile: geschlossen die ganze Pille, offen eine flache Leiste mit Milo in der Mitte. */
  const HEAD = { pill: PILL.h, open: 66 };
  /** Zeichenflaeche der Buehne: so gross wie die offene Insel, damit Milo auf Flaechen im Inhalt springen kann. */
  const STAGE = { w: 1000, h: 480 };
  /** Bodenlinie und Koerpergroesse; ueber Milo bleibt Platz fuer Spruenge und Denk-Punkte. */
  const FLOOR = { pill: 55, open: 58 };
  const BODY = { pill: 34, open: 34 };
  /** Milos fester Platz in der offenen Insel (Koerpermitte, von links): genau in der Mitte. */
  const HOME = $derived(PANEL.w / 2);
  /** Hoehe des Wuerfelregals unten; die Ansichten lassen darunter Platz. */
  const BAR = 86;

  let open = $state(false);
  /** Von selbst aufgeklappt, um etwas zu zeigen; klappt nach Lesezeit wieder zu. */
  let peeking = $state(false);
  let hovered = $state(false);
  let stage = $state<Stage>();
  let chat = $state<Chat>();
  /** Breite der Anzeige rechts in der Pille; so viel Platz laesst Milo frei. */
  let liveWidth = $state(0);
  /** Breite der Knoepfe oben rechts in der offenen Insel. */
  let buttonsWidth = $state(0);
  /** Was Milo gerade von sich aus sagt. */
  let said = $state<string | null>(null);
  /** Milo drueckt gegen eine Wand: die Insel gibt ein Stueck nach. */
  let push = $state<{ side: -1 | 1; amount: number }>({ side: 1, amount: 0 });
  let peekTimer = 0;
  /** Weggezogen ruht die Animation, aber erst, wenn die Insel wirklich aus dem Bild ist. */
  let stagePaused = $state(false);
  $effect(() => {
    if (!app.hidden) {
      stagePaused = false;
      return;
    }
    const timer = window.setTimeout(() => (stagePaused = true), 700);
    return () => clearTimeout(timer);
  });
  let sayTimer = 0;

  // ---------- Kulisse ----------

  /** Stunde fuer Licht und Nacht; wird im Stundentakt nachgefuehrt. */
  let hour = $state(new Date().getHours());
  /** Wachstum der Pflanze aus ihrem Alter in Tagen: Keimling, klein, mittel, gross, bluehend. */
  const growth = $derived.by(() => {
    const born = app.settings.plantBornMs;
    if (!born) return 0;
    const days = (Date.now() - born) / 86_400_000;
    return days < 1 ? 0.5 : days < 3 ? 1.5 : days < 7 ? 2.5 : days < 14 ? 3.5 : 4;
  });
  const scenery = $derived({
    hour,
    weather: app.weather ? sceneryWeather(app.weather.now.code) : null,
    mail: app.claude.present && (app.claude.activity === "question" || app.claude.activity === "confirm"),
    growth,
  });

  // ---------- Ausfluege auf den Desktop ----------

  /** Milo ist gerade auf der Taskleiste unterwegs. */
  let petAway = $state(false);
  let petStarting = false;
  let lastTrip = Date.now();
  const BUSY_MOODS = new Set(["listening", "thinking", "answering", "working", "searching", "collab", "fileHover", "fileOut", "ringing"]);
  /** Braucht der Nutzer Milo gerade? Dann kein Ausflug, und ein laufender wird beendet. */
  const needed = $derived(open || hovered || app.hidden || app.needsAttention || app.busy || BUSY_MOODS.has(app.mood) || app.voice === "listening");

  async function goOnTrip() {
    if (petAway || petStarting || !inTauri || needed || app.miloOnDesk) return;
    petStarting = true;
    lastTrip = Date.now();
    const ms = stage?.play("vanish") ?? 600;
    await new Promise((resolve) => setTimeout(resolve, ms));
    petStarting = false;
    if (needed) {
      stage?.play("appear");
      return;
    }
    petAway = true;
    try {
      await invoke("pet_out", { seconds: 30 + Math.random() * 60, from: 0.3 + Math.random() * 0.4 });
    } catch {
      petAway = false;
      stage?.play("appear");
    }
  }

  $effect(() => {
    if (needed && petAway) untrack(() => invoke("pet_recall").catch(() => {}));
  });

  // ---------- Live-Karte: die Pille waechst kurz, wenn etwas fertig ist oder beginnt ----------

  /** Claude war am Arbeiten und ist gerade fertig geworden: kurz gross zeigen. */
  let claudeFlash = $state(false);
  let claudeWas = "";
  let claudeFlashTimer = 0;
  $effect(() => {
    const now = app.claude.present ? app.claude.activity : "";
    if (now === claudeWas) return;
    const before = claudeWas;
    claudeWas = now;
    if (before === "working" && now === "done") {
      claudeFlash = true;
      clearTimeout(claudeFlashTimer);
      claudeFlashTimer = window.setTimeout(() => (claudeFlash = false), 7000);
    } else if (now !== "done") {
      claudeFlash = false;
    }
  });

  // ---------- Letzter Countdown vor dem Herunterfahren ----------
  // Milo zaehlt selbst (der Kern gibt Windows erst am Ende den Befehl). In den letzten 45 Sekunden
  // zeigt die Pille das gross und rot, auch ueber dem Spielemodus, mit Abbrechen (Knopf oder Esc).
  const POWER_FINAL_MS = 45_000;
  let powerNow = $state(Date.now());
  $effect(() => {
    if (!app.powerPlan) return;
    powerNow = Date.now();
    const timer = window.setInterval(() => (powerNow = Date.now()), 250);
    return () => clearInterval(timer);
  });
  const powerLeft = $derived(app.powerPlan ? Math.max(0, app.powerPlan.dueMs - powerNow) : Infinity);
  const powerFinal = $derived(!!app.powerPlan && powerLeft <= POWER_FINAL_MS);
  const powerLabel = $derived(
    app.powerPlan?.action === "restart" ? t("Neustart", "Restart") : app.powerPlan?.action === "sleep" ? t("Energie sparen", "Sleep") : t("Herunterfahren", "Shutdown"),
  );
  // Beginnt der Countdown, klappt die Insel zu, damit die Karte sofort zu sehen ist.
  $effect(() => {
    if (powerFinal && untrack(() => open)) closePanel(false);
  });
  function abortPower() {
    if (inTauri) invoke("abort_power").catch((err) => say(String(err), 4));
    else app.powerPlan = null;
  }

  /** Was die aufgeklappte Pille zeigt; null = normale Pille. */
  const card = $derived.by(() => {
    if (powerFinal) return { kind: "power" as const };
    if (app.hidden) return null;
    // Die Schnellnotiz hat der Nutzer selbst gerufen: sie kommt zuerst.
    const note = app.pillCards.find((c) => c.kind === "note");
    if (note) return { kind: "generic" as const, item: note };
    const ring = app.ringing[0];
    if (ring) return { kind: "done" as const, timer: ring };
    if (app.eventNow) return { kind: "event" as const, event: app.eventNow };
    if (claudeFlash) return { kind: "claude" as const };
    const next = app.pillCards[0];
    if (next) return { kind: "generic" as const, item: next };
    return null;
  });

  // ---------- Aktionen der allgemeinen Karten ----------

  let noteText = $state("");
  let noteInput = $state<HTMLInputElement>();
  let cardBusy = $state(false);
  // Schnellnotiz: Feld sofort fokussieren.
  $effect(() => {
    if (card?.kind === "generic" && card.item.kind === "note" && noteInput) noteInput.focus();
  });
  async function saveQuickNote() {
    const text = noteText.trim();
    if (!text) return app.dropCard("note");
    try {
      if (inTauri) await invoke("add_note", { text });
      noteText = "";
      app.dropCard("note");
      app.pushCard({ id: "noted", kind: "info", title: t("Notiz gespeichert", "Note saved"), sub: text, until: Date.now() + 2500 });
      stage?.play("nod");
      if (inTauri) invoke("release_focus").catch(() => {});
    } catch (err) {
      app.pushCard({ id: "noted", kind: "info", title: t("Nicht gespeichert", "Not saved"), sub: String(err), until: Date.now() + 5000 });
    }
  }
  function cancelQuickNote() {
    noteText = "";
    app.dropCard("note");
    if (inTauri) invoke("release_focus").catch(() => {});
  }
  async function cardAction(item: PillCard, action: string) {
    if (cardBusy) return;
    cardBusy = true;
    try {
      switch (action) {
        case "open":
          if (item.path) await invoke("browse_open", { path: item.path });
          app.dropCard(item.id);
          break;
        case "folder":
          if (item.path) await invoke("reveal_path", { path: item.path });
          app.dropCard(item.id);
          break;
        case "saveShot": {
          const target = await invoke<string>("screenshot_save", { path: item.path });
          app.pushCard({ ...item, sub: t(`Gespeichert in Bilder\\Screenshots`, `Saved to Pictures\\Screenshots`), path: target, until: Date.now() + 6000 });
          break;
        }
        case "ocr": {
          app.pushCard({ ...item, sub: t("Lese den Text …", "Reading the text …"), until: Date.now() + 30_000 });
          const text = await invoke<string>("screenshot_ocr", { path: item.path });
          app.pushCard({ ...item, title: t("Text kopiert", "Text copied"), sub: text.replace(/\s+/g, " ").slice(0, 120), until: Date.now() + 8000 });
          break;
        }
        case "ask":
          if (item.path) app.importFiles([item.path]);
          app.dropCard(item.id);
          app.tab = "chat";
          openPanel(true);
          break;
        case "weather":
          app.dropCard(item.id);
          app.tab = "weather";
          openPanel(false);
          peeking = false;
          break;
        case "break":
          app.startBreak(5);
          break;
        case "again":
          app.startFocus(app.focus?.minutes ?? 25);
          break;
        case "stopFocus":
          app.stopFocus();
          break;
        case "review":
          app.dropCard(item.id);
          openPanel(true);
          app.tab = "chat";
          void app.dailyCheck();
          break;
        default:
          app.dropCard(item.id);
      }
    } catch (err) {
      app.pushCard({ ...item, sub: String(err), until: Date.now() + 6000 });
    } finally {
      cardBusy = false;
    }
  }
  const CARD = { w: 580, h: 124 };
  /** Groesse der geschlossenen Insel: Pille oder aufgeklappte Pille. */
  const CLOSED = $derived(card ? CARD : PILL);

  // Groesse der Insel als Feder: weich und mit leichtem Nachschwingen.
  const box = new Spring({ w: PILL.w, h: PILL.h }, { stiffness: 0.11, damping: 0.58 });
  $effect(() => {
    box.target = open ? PANEL : CLOSED;
  });
  /** 0 = Pille, 1 = offen. */
  const grow = $derived(Math.min(1, Math.max(0, (box.current.w - CLOSED.w) / (PANEL.w - CLOSED.w))));
  const headHeight = $derived(HEAD.pill + (HEAD.open - HEAD.pill) * grow);

  const layout = $derived.by<StageLayout>(() => {
    const w = box.current.w;
    const left = (STAGE.w - w) / 2;
    const right = left + w;
    const base = { left, right, floor: FLOOR.pill + (FLOOR.open - FLOOR.pill) * grow, size: BODY.pill + (BODY.open - BODY.pill) * grow };
    if (open) return { ...base, roamMin: left + w / 2, roamMax: left + w / 2, buttons: buttonsWidth > 0 ? buttonsWidth + 12 : undefined };
    // Geschlossen gehoert Milo die ganze Pille, abzueglich der Anzeige rechts.
    const reserve = liveWidth > 0 ? liveWidth + 14 : 0;
    // Abstand zum Rand, damit kein Arm an der Inselkante klebt.
    return { ...base, roamMin: left + 48, roamMax: Math.max(left + 48, right - 48 - reserve) };
  });

  /** Lautstaerke per Mausrad: kurz in der Pille anzeigen. */
  let volume = $state<number | null>(null);
  let volumeTimer = 0;
  let wheelBusy = false;
  async function onPillWheel(e: WheelEvent) {
    if (open || !inTauri) return;
    e.preventDefault();
    if (wheelBusy) return;
    wheelBusy = true;
    try {
      volume = await invoke<number>("volume_step", { delta: e.deltaY < 0 ? 4 : -4 });
      clearTimeout(volumeTimer);
      volumeTimer = window.setTimeout(() => (volume = null), 1600);
    } catch {
      /* ohne Audiogeraet eben nichts */
    } finally {
      wheelBusy = false;
    }
  }

  /** Bis zum Termin: „in 24 Min“, „in 1 Std 5 Min“, in der letzten Minute „gleich“. */
  function untilText(e: AgendaEvent): string {
    const ms = e.startMs - app.clockNow;
    if (ms <= 0) return t("jetzt", "now");
    if (ms < 60_000) return t("gleich", "soon");
    const min = Math.ceil(ms / 60_000);
    if (min < 60) return t(`in ${min} Min`, `in ${min} min`);
    return t(`in ${Math.floor(min / 60)} Std ${min % 60} Min`, `in ${Math.floor(min / 60)} h ${min % 60} min`);
  }
  const clockText = $derived(new Date(app.clockNow).toLocaleTimeString(lang === "en" ? "en-US" : "de-DE", { hour: "2-digit", minute: "2-digit" }));

  /** Was rechts in der geschlossenen Pille steht; das Wichtigste gewinnt. */
  const live = $derived.by(() => {
    if (card) return null;
    if (volume !== null) return { kind: "volume" as const, text: `${volume} %` };
    if (app.dragging) return { kind: "text" as const, text: t("Loslassen, dann nehme ich sie", "Let go and I'll take it") };
    if (app.voice === "listening") return { kind: "voice" as const, text: t("Ich höre zu", "Listening") };
    if (app.voice === "transcribing") return { kind: "activity" as const, text: t("Versteht", "Understanding") };
    if (app.ringing.length > 0) return { kind: "ringing" as const, text: t(`${app.ringing[0].label} ist abgelaufen`, `${app.ringing[0].label} is up`) };
    if (app.questions.length > 0) return { kind: "text" as const, text: t("Wartet auf dich", "Waiting for you") };
    if (app.claude.present && app.claude.activity === "confirm") return { kind: "claude" as const, text: t("Claude braucht deine Bestätigung", "Claude needs your approval") };
    if (app.claude.present && app.claude.activity === "question") return { kind: "claude" as const, text: app.claude.text ? t(`Claude fragt: ${app.claude.text}`, `Claude asks: ${app.claude.text}`) : t("Claude wartet auf dich", "Claude is waiting for you") };
    if (app.activity) return { kind: "activity" as const, text: app.activity };
    if (localAi.running && localAi.pull) return { kind: "pull" as const, text: localAi.pull.model };
    if (app.focus) return { kind: "focus" as const, text: app.focus.phase === "focus" ? t("Fokus", "Focus") : t("Pause", "Break") };
    // Ein Termin in den naechsten zehn Minuten ist wichtiger als ein laufender Timer.
    const soon = app.nextEvent;
    if (soon && soon.startMs - app.clockNow <= 600_000) return { kind: "event" as const, text: soon.title, event: soon };
    if (app.nextTimer) return { kind: "timer" as const, text: app.nextTimer.label };
    if (soon) return { kind: "event" as const, text: soon.title, event: soon };
    if (app.claude.present && app.claude.activity === "done") return { kind: "claude" as const, text: t("Claude ist fertig", "Claude is done") };
    if (said) return { kind: "said" as const, text: said };
    if (app.media?.playing || (hovered && app.media?.title)) return { kind: "media" as const, text: app.media.title };
    return null;
  });

  const ICONS = SHELF_ICONS;

  const tabs = $derived.by(() => {
    const list: { id: TabId; label: string; badge: string; alert?: boolean }[] = [{ id: "home", label: t("Übersicht", "Home"), badge: "" }, { id: "chat", label: "Chat", badge: "" }];
    if (app.media) list.push({ id: "media", label: t("Musik", "Music"), badge: app.media.playing ? t("läuft", "playing") : "" });
    list.push({ id: "agenda", label: "Agenda", badge: "" });
    // Timer, Fokus und Wecker: immer erreichbar.
    {
      const badge = app.ringing.length > 0 ? t("klingelt", "ringing") : app.nextTimer ? clock(app.nextTimer.dueMs - app.now) : app.focus ? clock(Math.max(0, app.focus.endMs - app.clockNow)) : "";
      list.push({ id: "timer", label: "Timer", badge, alert: app.ringing.length > 0 });
    }
    list.push({ id: "weather", label: t("Wetter", "Weather"), badge: app.weather ? `${Math.round(app.weather.now.temp)}°` : "" });
    list.push({ id: "files", label: t("Dateien", "Files"), badge: app.files.length > 0 ? String(app.files.length) : "" });
    list.push({ id: "notes", label: t("Notizen", "Notes"), badge: "" });
    list.push({ id: "system", label: "System", badge: "" });
    const request = app.claude.permission;
    const claudeBadge = request ? (request.kind === "ask" ? t("Frage", "Question") : request.kind === "plan" ? t("Plan", "Plan") : t("Freigabe", "Approval")) : app.claude.present && app.claude.activity === "working" ? t("arbeitet", "working") : "";
    list.push({ id: "claude", label: "Claude", badge: claudeBadge, alert: !!app.claude.permission });
    list.push({ id: "usage", label: t("Verbrauch", "Usage"), badge: "" });
    list.push({ id: "history", label: t("Verlauf", "History"), badge: "" });
    // Eigene Reihenfolge und Auswahl aus den Einstellungen. Ein ausgeblendeter Wuerfel taucht nur
    // auf, solange er offen ist oder etwas Dringendes meldet (z. B. eine Freigabe fuer Claude).
    const order = shelfOrder(app.settings.shelfOrder ?? []);
    const hidden = app.settings.shelfHidden ?? [];
    return list
      .filter((tab) => !hidden.includes(tab.id) || tab.alert || app.tab === tab.id)
      .sort((x, y) => order.indexOf(x.id) - order.indexOf(y.id));
  });
  // Verschwindet der Grund fuer einen Reiter, geht es zurueck zum Chat.
  $effect(() => {
    if (app.tab !== "settings" && app.tab !== "widgets" && !tabs.some((t) => t.id === app.tab)) app.tab = "chat";
  });

  /** Meldet dem Rust-Kern die Flaeche, auf der Mauseingaben ankommen duerfen. */
  function reportHitRect(w: number, h: number): Promise<unknown> {
    if (!inTauri) return Promise.resolve();
    // Weggezogen nimmt die Insel gar keine Mauseingaben an.
    if (app.hidden) w = h = 0;
    const dpr = window.devicePixelRatio;
    const x = (window.innerWidth - w) / 2;
    return invoke("set_hit_rect", { x: x * dpr, y: 0, w: w * dpr, h: h * dpr }).catch(() => {});
  }
  // Beim Zuklappen erst die kleine Flaeche melden, wenn die Insel wirklich wieder klein ist.
  $effect(() => {
    const { w, h } = box.current;
    const size = CLOSED;
    if (!open && Math.abs(w - size.w) < 1.5 && Math.abs(h - size.h) < 1.5) reportHitRect(size.w, untrack(() => decorPop || eventPop) ? size.h + POP_H : size.h);
  });

  // ---------- Mini-Menue fuer Deko in der geschlossenen Pille ----------

  /** Angetipptes Deko-Objekt: kleines Menue direkt darunter statt der ganzen Bibliothek. */
  let decorPop = $state<{ kind: string; left: number } | null>(null);
  const POP_H = 260;
  let popTimer = 0;
  $effect(() => {
    const pop = decorPop || eventPop;
    const size = CLOSED;
    untrack(() => {
      // Waechst die Pille, sofort die grosse Flaeche melden; beim Schrumpfen erst, wenn sie klein ist.
      if (!open && (pop || size === CARD)) reportHitRect(size.w, pop ? size.h + POP_H : size.h);
      else if (!open) reportHitRect(Math.max(size.w, box.current.w), Math.max(size.h, box.current.h));
    });
  });
  $effect(() => {
    if (open) decorPop = eventPop = null;
  });

  /** Rechtsklick auf einen Termin in der Pille: ausblenden oder Hinweise abschalten. */
  let eventPop = $state<{ event: AgendaEvent; left: number } | null>(null);
  function onLiveContext(e: MouseEvent) {
    const item = live;
    if (open || item?.kind !== "event") return;
    e.preventDefault();
    e.stopPropagation();
    decorPop = null;
    const islandLeft = (window.innerWidth - CLOSED.w) / 2;
    eventPop = { event: item.event, left: Math.max(0, Math.min(CLOSED.w - 230, e.clientX - islandLeft - 115)) };
  }
  function showDecorPop(kind: string, clientX: number) {
    eventPop = null;
    const islandLeft = (window.innerWidth - PILL.w) / 2;
    decorPop = { kind, left: Math.max(0, Math.min(PILL.w - 230, clientX - islandLeft - 115)) };
  }
  function toggleDecorLock(kind: string) {
    const locked = app.settings.decorLocked.includes(kind);
    app.saveSettings({ decorLocked: locked ? app.settings.decorLocked.filter((d) => d !== kind) : [...app.settings.decorLocked, kind] });
  }
  /** Direkt auf den Desktop: das Objekt faellt dort ueber der Insel herunter. */
  async function decorToDesk(kind: string, clientX: number) {
    decorPop = null;
    if (!inTauri) return;
    const screenLeft = window.screenX - ((screen as Screen & { availLeft?: number }).availLeft ?? 0);
    const x = (screenLeft + clientX) / screen.availWidth;
    await invoke("desk_place", { kind, x }).catch((err) => say(String(err), 4));
  }
  function decorRemove(kind: string) {
    decorPop = null;
    app.saveSettings({ decor: app.settings.decor.filter((d) => d !== kind) });
  }

  // Waechst oder schrumpft die offene Insel (Widget-Baukasten), die Klickflaeche mitnehmen.
  // Waehrend der Tour gehoert der Erklaerkasten unter der Insel zur Klickflaeche.
  const TOUR_SPACE = 250;
  $effect(() => {
    const { w, h } = PANEL;
    const extra = tour.active && app.settings.onboarded ? TOUR_SPACE : 0;
    if (open) untrack(() => reportHitRect(Math.max(w, 540), h + extra));
  });

  async function openPanel(focusInput: boolean) {
    clearTimeout(peekTimer);
    peeking = !focusInput;
    stage?.stir();
    if (!open) {
      open = true;
      // Beim Aufklappen sofort die grosse Flaeche melden.
      reportHitRect(PANEL.w, PANEL.h);
    }
    if (focusInput) {
      app.warmUp();
      app.dailyCheck();
      await tick();
      if (app.tab === "chat") chat?.focus();
    }
  }

  function closePanel(giveFocusBack: boolean) {
    clearTimeout(peekTimer);
    peeking = false;
    open = false;
    if (giveFocusBack && inTauri) invoke("release_focus");
  }

  // Milo am Koerper packen und aus der Insel ziehen: ab ein paar Pixeln Bewegung wird aus dem
  // Klick ein Tragen. Ab da fuehrt der Rust-Kern die Maus (auch ausserhalb des Fensters) und die
  // Desktop-Ebene zeichnet Milo am Zeiger; beim Loslassen faellt er auf die Taskleiste.
  let carried = false;
  function onHeadDown(e: PointerEvent) {
    carried = false;
    if (e.button !== 0) return;
    // Milo hat Vorrang vor Deko, die zufaellig vor ihm steht.
    const onMilo = !petAway && !app.miloOnDesk && !!stage?.hits(e.clientX, e.clientY);
    const decor = !open && !onMilo ? stage?.decorAt(e.clientX, e.clientY) : null;
    if (decor) return grabDecor(e, decor);
    if (!inTauri || !onMilo) return;
    const start = { x: e.clientX, y: e.clientY };
    const move = (m: PointerEvent) => {
      if (Math.hypot(m.clientX - start.x, m.clientY - start.y) < 7) return;
      stop();
      carried = true;
      app.miloOnDesk = true;
      invoke("desk_carry", { kind: "milo", fromX: null }).catch(() => (app.miloOnDesk = false));
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  }

  /**
   * Deko in der geschlossenen Pille: waagrecht ziehen verschiebt sie (der Platz wird gespeichert),
   * nach unten aus der Insel gezogen faellt sie auf den Desktop. Gesperrte Objekte bleiben stehen.
   */
  function grabDecor(e: PointerEvent, kind: string) {
    const start = { x: e.clientX, y: e.clientY };
    const locked = app.settings.decorLocked.includes(kind);
    let dragging = false;
    let ratio = 0;
    const move = (m: PointerEvent) => {
      if (!dragging && Math.hypot(m.clientX - start.x, m.clientY - start.y) < 5) return;
      carried = true;
      if (locked) {
        say(t(`${DECOR.find((d) => d.kind === kind)?.label ?? "Das"} ist gesperrt`, `${DECOR.find((d) => d.kind === kind)?.label ?? "That"} is locked`), 3);
        stop();
        return;
      }
      dragging = true;
      if (m.clientY > PILL.h + 24 && inTauri) {
        // Unten aus der Insel heraus: ab jetzt traegt die Desktop-Ebene es weiter.
        stage?.dragDecor(null);
        stop();
        invoke("desk_carry", { kind, fromX: null, fromIsland: true }).catch(() => {});
        return;
      }
      ratio = stage?.dragDecor(kind, m.clientX) ?? 0;
    };
    const up = () => {
      stop();
      if (!dragging) {
        // Nur angetippt (Linksklick): kein Menue, das gibt es nur per Rechtsklick. Ein offenes
        // Mini-Menue schliesst sich, sonst verhaelt sich der Klick wie auf die Pille.
        if (decorPop) {
          carried = true;
          decorPop = null;
        }
        return;
      }
      stage?.dragDecor(null);
      app.saveSettings({ decorPositions: { ...app.settings.decorPositions, [kind]: ratio } });
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  /** Rechtsklick auf Deko in der Pille: kleines Menue (Sperren, Auf den Desktop, Entfernen). */
  function onHeadContext(e: MouseEvent) {
    const decor = !open ? stage?.decorAt(e.clientX, e.clientY) : null;
    if (!decor) {
      // Rechts in der Pille steht ein Termin: dessen Menue.
      if (!open && live?.kind === "event" && e.clientX > window.innerWidth / 2) onLiveContext(e);
      return;
    }
    e.preventDefault();
    showDecorPop(decor, e.clientX);
  }

  /** Aus dem Mini-Menue: Insel oeffnen und das Objekt in der ganzen Bibliothek zeigen. */
  function openLibraryFor(kind: string) {
    decorPop = null;
    app.decorFocus = kind;
    openPanel(false);
    peeking = false;
    library = true;
  }

  // Offen steht Milo auf Flaechen im Inhalt: dort ebenfalls greifen, ohne dass der Klick an die
  // Flaeche unter ihm geht. Nur angetippt kichert er.
  function onIslandDown(e: PointerEvent) {
    if (!open || e.button !== 0 || !stage?.hits(e.clientX, e.clientY)) return;
    if ((e.target as HTMLElement).closest("header")) return;
    e.preventDefault();
    e.stopPropagation();
    const swallow = (c: MouseEvent) => {
      c.stopPropagation();
      c.preventDefault();
    };
    window.addEventListener("click", swallow, { capture: true, once: true });
    window.setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 600);
    const start = { x: e.clientX, y: e.clientY };
    let dragged = false;
    const move = (m: PointerEvent) => {
      if (Math.hypot(m.clientX - start.x, m.clientY - start.y) < 7) return;
      dragged = true;
      stop();
      if (!inTauri || petAway || app.miloOnDesk) return;
      app.miloOnDesk = true;
      invoke("desk_carry", { kind: "milo", fromX: null }).catch(() => (app.miloOnDesk = false));
    };
    const up = () => {
      stop();
      if (!dragged) stage?.poke(start.x, start.y);
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function onHeadClick(e: MouseEvent) {
    if (carried) {
      carried = false;
      return;
    }
    // Trifft der Klick Milo selbst, kichert er (und die Insel klappt trotzdem um).
    stage?.poke(e.clientX, e.clientY);
    if (open && !peeking) closePanel(true);
    else openPanel(true);
  }

  /** Antwort kam, waehrend die Insel zu war: kurz aufklappen, ohne den Fokus zu nehmen. */
  function showAnswer() {
    if (open && !peeking) return;
    // Ein erledigter Befehl („öffne YouTube“) klappt die Insel nicht auf: die Pille zeigt kurz einen
    // Haken. Aufgeklappt wird nur, wenn es etwas zu lesen gibt oder etwas schiefging.
    const last = app.messages.at(-1);
    if (!open && last && isDoneCommand(last)) {
      const asked = app.messages.at(-2);
      app.pushCard({
        id: "done-cmd",
        kind: "info",
        title: last.text.trim().replace(/\.$/, "") || t("Erledigt", "Done"),
        sub: asked?.role === "user" ? `„${asked.text.trim().slice(0, 60)}“` : t("Erledigt", "Done"),
        until: Date.now() + 6000,
      });
      return;
    }
    openPanel(false);
    schedulePeekClose();
  }

  /** Eine Aktion ohne Lesestoff: alle Werkzeuge erfolgreich, keine Karte (Wetter, Dateien …), kurze Antwort. */
  function isDoneCommand(m: (typeof app.messages)[number]): boolean {
    return m.role === "milo" && !m.error && m.tools.length > 0 && m.tools.every((tool) => tool.status === "ok" && (!tool.card || tool.card.type === "link") && !tool.links?.length) && m.text.trim().length <= 120 && !m.text.includes("\n");
  }

  function schedulePeekClose() {
    clearTimeout(peekTimer);
    // Solange eine Rueckfrage offen ist oder ein Timer klingelt, bleibt die Insel offen.
    if (!peeking || hovered || app.needsAttention || tour.active) return;
    const last = app.messages.at(-1);
    const cards = last?.tools.filter((t) => t.card).length ?? 0;
    // Fehler: Grund lesen, nach 12 s klappt sie wieder zu. Sonst Lesezeit von 5 s, bei langen
    // Antworten und Karten bis zu 22 s.
    const failed = !!last && (last.error || last.tools.some((tool) => tool.status === "error"));
    const ms = failed ? 12000 : Math.min(22000, 5000 + (last?.text.length ?? 0) * 45 + cards * 5000);
    peekTimer = window.setTimeout(() => closePanel(false), ms);
  }

  // Rueckfrage oder Freigabe: von selbst aufklappen, ohne den Fokus zu nehmen. Ein klingelnder Timer
  // klappt die Insel nicht mehr auf: dafuer waechst die Pille (Live-Karte mit +1 min, +5 min, Stopp).
  // Klappt der Nutzer sie trotzdem zu, bleibt sie zu, bis etwas Neues dazukommt.
  let attention = 0;
  $effect(() => {
    const count = app.questions.length + Number(app.wiping) + Number(!!app.claude.permission);
    untrack(() => {
      if (count > attention) {
        if (!open) openPanel(false);
        else clearTimeout(peekTimer);
      } else if (count === 0) {
        schedulePeekClose();
      }
      attention = count;
    });
  });

  // Vollbild: die Insel zieht sich nach oben zurueck und kommt danach von selbst wieder.
  let wasHidden = false;
  $effect(() => {
    const hidden = app.hidden;
    untrack(() => {
      if (hidden === wasHidden) return;
      wasHidden = hidden;
      if (hidden) {
        if (open) closePanel(false);
        reportHitRect(0, 0);
      } else {
        reportHitRect(open ? PANEL.w : CLOSED.w, open ? PANEL.h : CLOSED.h);
        stage?.stir();
        stage?.play("perk");
      }
    });
  });

  // Claude braucht eine Freigabe: gleich die Claude-Ansicht zeigen.
  let lastPermission = "";
  $effect(() => {
    const id = app.claude.permission?.id ?? "";
    untrack(() => {
      if (id && id !== lastPermission) app.tab = "claude";
      lastPermission = id;
    });
  });

  // Sobald etwas los ist, wird Milo wach.
  $effect(() => {
    if (app.mood !== "idle") untrack(() => stage?.stir());
  });

  /** Milo sagt kurz etwas in der geschlossenen Pille. */
  function say(text: string, seconds = 5) {
    clearTimeout(sayTimer);
    said = text;
    sayTimer = window.setTimeout(() => (said = null), seconds * 1000);
  }

  function greeting(): string {
    const hour = new Date().getHours();
    const who = app.settings.userName.trim();
    const to = who ? `, ${who}` : "";
    if (hour < 5) return t(`Noch wach${to}?`, `Still up${to}?`);
    if (hour < 11) return t(`Guten Morgen${to}!`, `Good morning${to}!`);
    if (hour < 18) return t(`Hallo${to}!`, `Hello${to}!`);
    return t(`Guten Abend${to}!`, `Good evening${to}!`);
  }

  // Erster Start: die Insel klappt fuer den Willkommens-Assistenten von selbst auf.
  let welcomed = false;
  $effect(() => {
    if (!app.settingsLoaded || app.settings.onboarded) {
      welcomed = false;
      return;
    }
    if (welcomed) return;
    welcomed = true;
    untrack(() => {
      openPanel(true);
      peeking = false;
    });
  });

  /** Fehlt der Gemini-Schluessel, zeigt ein kleines Ausrufezeichen, was zu tun ist. */
  function openKeySettings() {
    if (inTauri) invoke("settings_open", { section: "ai" }).catch(() => (app.tab = "settings"));
    else app.tab = "settings";
  }

  onMount(() => {
    // Vorschau des Willkommens-Assistenten im Browser: http://127.0.0.1:1420/?welcome
    if (!inTauri && new URLSearchParams(location.search).has("welcome")) {
      app.settings = { ...app.settings, onboarded: false };
      app.settingsLoaded = true;
    }
    reportHitRect(PILL.w, PILL.h);
    app.gesture = (kind) => stage?.play(kind);
    app.onAnswer = showAnswer;
    app.onToken = () => stage?.talk();
    // Text-Werkzeuge: aufklappen und offen lassen, bis der Nutzer gewaehlt hat.
    app.onTextWork = () => {
      openPanel(false);
      peeking = false;
      clearTimeout(peekTimer);
    };
    app.onNotice = (text) => {
      stage?.play("shrug");
      say(text, 5);
    };
    // Modell-Download der lokalen KI: Fortschritt in der Pille, am Ende eine Karte.
    localAi.init();
    localAi.onFinish = (p) => {
      if (p.phase === "done") {
        stage?.play("nod");
        app.pushCard({ id: "local-ai", kind: "info", title: t("Lokale KI ist bereit", "Local AI is ready"), sub: t(`${p.model} ist mit ${app.settings.name} verbunden`, `${p.model} is connected to ${app.settings.name}`), until: Date.now() + 15_000 });
      } else {
        app.pushCard({ id: "local-ai", kind: "info", title: t("Download fehlgeschlagen", "Download failed"), sub: p.error || p.model, until: Date.now() + 20_000, failed: true });
      }
    };
    // Nur im Entwicklungsmodus: Zugriff fuer die Pruefskripte.
    if (import.meta.env.DEV) Object.assign(window, { __stage: stage, __localAi: localAi, __app: app });
    app.init();
    // Nur im Entwicklungsmodus: ?open=media&demo=1 klappt die Insel mit Beispieldaten auf (fuer Bildschirmfotos).
    if (import.meta.env.DEV && !inTauri) {
      const params = new URLSearchParams(location.search);
      if (params.has("demo")) {
        app.media = { title: "Windows down, the city’s glowing", artist: "Nova Lane", album: "Golden Hour", subtitle: "", app: "Spotify", playing: true, positionMs: 42000, durationMs: 195000, atMs: Date.now(), canNext: true, canPrevious: true, canSeek: true, cover: "" };
        app.weather = { place: "Berlin", region: "", now: { temp: 6, feels: 3, code: 3, text: "bewölkt", wind: 12, humidity: 80, isDay: true }, hours: Array.from({ length: 14 }, (_, i) => ({ hour: `${(18 + i) % 24}:00`, temp: 6 - i * 0.4, code: i % 5 === 0 ? 61 : 3, rain: i % 5 === 0 ? 60 : 10, isDay: false })), days: ["Mi", "Do", "Fr", "Sa", "So", "Mo", "Di"].map((label, i) => ({ date: `2026-11-${11 + i}`, label, min: 1 + (i % 3), max: 7 + (i % 4), code: [3, 61, 2, 0, 71, 3, 1][i], text: "", rain: [10, 70, 20, 0, 40, 10, 5][i], wind: 12, sunrise: "2026-11-11T07:32", sunset: "2026-11-11T16:28" })) };
        if (params.get("bg") !== "0") document.body.style.background = "linear-gradient(120deg,#6a2a3a,#2a3a6a)";
      }
      const tab = params.get("open");
      if (tab) {
        window.setTimeout(() => openPanel(false), 200);
        window.setTimeout(() => (app.tab = tab as TabId), 500);
      }
    }

    // Begruessung beim Start und ein kurzer Hinweis zur vollen Stunde.
    const hello = window.setTimeout(() => {
      stage?.play("wave");
      say(greeting(), 4);
    }, 900);
    let lastHour = new Date().getHours();
    const hourly = window.setInterval(() => {
      const now = new Date();
      // Ab und zu ein Ausflug auf die Taskleiste, wenn gerade nichts los ist.
      const chance = [0.03, 0.06, 0.12][app.settings.petFrequency] ?? 0.06;
      if (app.settings.petMode && !petAway && !app.focus && Date.now() - lastTrip > 180_000 && Math.random() < chance) goOnTrip();
      if (now.getHours() !== lastHour) {
        hour = now.getHours();
        lastHour = now.getHours();
        if (!open && app.mood === "idle" && !app.focus) {
          stage?.play("perk");
          say(t(`Es ist ${lastHour} Uhr`, `It's ${lastHour} o'clock`), 6);
        }
      }
    }, 20000);

    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !open) return;
      if (menu || library) {
        menu = library = false;
        return;
      }
      closePanel(true);
    };
    window.addEventListener("keydown", onKey);

    let unlisten = () => {};
    let undrop = () => {};
    let unpet = () => {};
    let unwidget = () => {};
    let unhome = () => {};
    let unadopt = () => {};
    if (inTauri) {
      // Zurueck vom Ausflug: auf der Insel wieder auftauchen.
      // Vom Desktop zurueck in die Insel geworfen oder geholt.
      listen("milo://milo-home", () => {
        app.miloOnDesk = false;
        stage?.stir();
        stage?.play("appear");
      }).then((fn) => (unhome = fn));
      listen("milo://pet-back", () => {
        if (!petAway) return;
        petAway = false;
        stage?.stir();
        stage?.play("appear");
      }).then((fn) => (unpet = fn));
      // Unterwegs angefasst (gegriffen oder Rechtsklick): Milo bleibt auf dem Desktop, bis man ihn holt.
      listen("milo://pet-adopted", () => {
        petAway = false;
        app.miloOnDesk = true;
      }).then((fn) => (unadopt = fn));
      // Rechtsklick auf ein Widget: Editor in der Insel oeffnen.
      listen<string>("milo://widget-edit", ({ payload }) => {
        app.editWidget = payload;
        app.tab = "widgets";
        if (!open) openPanel(true);
      }).then((fn) => (unwidget = fn));
      // Die Pflanze waechst ab dem ersten Start.
      window.setTimeout(() => {
        if (!app.settings.plantBornMs) app.saveSettings({ plantBornMs: Date.now() });
      }, 3000);
      // Dateien auf die Insel ziehen: Milo streckt die Arme aus und nimmt sie beim Loslassen an.
      getCurrentWebview()
        .onDragDropEvent(({ payload }) => {
          // Liegt die Maus ueber einem Ordner-Knopf (Dateien-Ansicht), wird dorthin verschoben.
          const folderAt = (position: { x: number; y: number }) => {
            const dpr = window.devicePixelRatio;
            const element = document.elementFromPoint(position.x / dpr, position.y / dpr);
            return element?.closest<HTMLElement>("[data-drop-folder]")?.dataset.dropFolder ?? null;
          };
          if (payload.type === "enter" || payload.type === "over") {
            app.dropFolder = folderAt(payload.position);
            // Eigene Dateien, die gerade aus Milo gezogen werden, sind kein „Datei abgeben“.
            if (app.handing) return;
            if (!app.dragging) stage?.stir();
            app.dragging = true;
          } else {
            app.dragging = false;
            const target = payload.type === "drop" ? folderAt(payload.position) : null;
            app.dropFolder = null;
            if (payload.type !== "drop") return;
            if (target) {
              // Aus Milo gezogen: verschieben; von draussen hereingezogen: kopieren.
              app.transferFiles(payload.paths, target, !app.handing).catch((err) => app.onNotice(String(err)));
            } else if (!app.handing) {
              app.importFiles(payload.paths);
            }
          }
        })
        .then((fn) => (undrop = fn));
      // Klickt der Nutzer in ein anderes Fenster, klappt die Insel wieder zu (abschaltbar).
      getCurrentWindow()
        .onFocusChanged(({ payload: focused }) => {
          // Waehrend der Tour bleibt die Insel offen, auch wenn z. B. das Einstellungsfenster vorne ist.
          if (!focused && app.settings.closeOnBlur !== false && open && !peeking && !app.needsAttention && app.settings.onboarded && !tour.active) closePanel(false);
        })
        .then((fn) => (unlisten = fn));
    }
    return () => {
      window.removeEventListener("keydown", onKey);
      clearTimeout(peekTimer);
      clearTimeout(sayTimer);
      clearTimeout(hello);
      clearInterval(hourly);
      unlisten();
      undrop();
      unpet();
      unwidget();
      unhome();
      unadopt();
    };
  });

  /** Umfang des kleinen Timer-Rings (r = 8). */
  const RING = 2 * Math.PI * 8;

  // Aussehen aus den Einstellungen: Akzentfarbe, Glasstaerke, weniger Bewegung.
  $effect(() => {
    const root = document.documentElement;
    root.style.setProperty("--accent", app.settings.accent || "#64d2ff");
    root.style.setProperty("--focus", app.settings.accent || "#64d2ff");
    root.dataset.glass = String(app.settings.glass ?? 1);
    root.classList.toggle("reduce-motion", !!app.settings.reduceMotion);
  });

  // ---------- Kopfzeile und Wuerfelregal der offenen Insel ----------

  /** Menue und Deko-Bibliothek. */
  let menu = $state(false);
  let library = $state(false);
  $effect(() => {
    if (!open) {
      menu = false;
      library = false;
    }
  });

  /** Einstellungen bekommen ein eigenes Fenster; im Browser (Entwicklung) bleiben sie in der Insel. */
  function openSettings() {
    menu = false;
    if (inTauri) invoke("settings_open").catch(() => (app.tab = "settings"));
    else app.tab = "settings";
  }

  // ---------- Milos Landeplatz in der offenen Insel ----------
  // Offen schwebt Milo nicht in der Kopfzeile, sondern springt auf eine echte Flaeche: auf das, was
  // die Ansicht mit data-perch anbietet (Vorschlaege, Eingabefeld, eine Karte ...), sonst auf die
  // Glasleiste ueber dem aktiven Reiter. Erst wenn die Insel ganz aufgeklappt ist, sonst huepft er
  // dem wachsenden Rahmen hinterher.
  let islandEl = $state<HTMLElement>();
  let perch = $state<{ x: number; y: number } | null>(null);
  /** Flaeche, die die aktuelle Ansicht anbietet (data-perch); null = keine sichtbare. */
  function viewPerch(): { x: number; y: number } | null {
    // Waehrend des Willkommens-Assistenten bleibt Milo oben stehen statt hinter der Karte.
    if (!islandEl || islandEl.querySelector(".welcome")) return null;
    const base = islandEl.getBoundingClientRect();
    const offset = (base.width - STAGE.w) / 2;
    for (const spot of islandEl.querySelectorAll<HTMLElement>(".pane [data-perch]")) {
      const r = spot.getBoundingClientRect();
      const top = r.top - base.top;
      // Nur sichtbare Flaechen mit Platz darueber (Milo ist gut 40 Pixel hoch).
      if (r.width < 30 || top < HEAD.open + 36 || top > base.height - BAR - 10) continue;
      const side = spot.dataset.perch;
      const x = side === "left" ? r.left + 28 : side === "right" ? r.right - 28 : r.left + r.width / 2;
      return { x: x - base.left - offset, y: top };
    }
    return null;
  }
  /** Oberkante des Wuerfels der aktiven Ansicht. */
  function cubePerch(): { x: number; y: number } | null {
    if (!islandEl || !nav) return null;
    const cube = nav.querySelector<HTMLElement>("button.active");
    if (!cube) return null;
    const base = islandEl.getBoundingClientRect();
    const t = cube.getBoundingClientRect();
    return { x: t.left + t.width / 2 - base.left - (base.width - STAGE.w) / 2, y: t.top - base.top };
  }
  const settled = $derived(open && Math.abs(box.current.h - PANEL.h) < 3 && Math.abs(box.current.w - PANEL.w) < 3);
  /**
   * Nach einem Ansichtswechsel steht Milo zuerst auf dem Wuerfel (Rueckmeldung: hier bist du).
   * Nach 5 bis 8 Sekunden springt er einmal weg: auf die Flaeche, die die Ansicht anbietet, oder,
   * gibt es keine, zurueck in die Kopfzeile. Dort bleibt er bis zum naechsten Wechsel.
   */
  let onCube = $state(true);
  $effect(() => {
    void app.tab;
    onCube = true;
    // Waehrend der Tour bleibt Milo auf dem Wuerfel, damit die Markierung ihn (und den Buddy) zeigt.
    if (tour.active) return;
    const leave = window.setTimeout(() => (onCube = false), 5000 + Math.random() * 3000);
    return () => clearTimeout(leave);
  });
  $effect(() => {
    if (!settled) {
      perch = null;
      return;
    }
    void app.tab;
    const cube = onCube;
    const update = () => {
      const next = cube ? (cubePerch() ?? viewPerch()) : viewPerch();
      if (!next) {
        perch = null;
        return;
      }
      if (!perch || Math.abs(next.x - perch.x) > 2 || Math.abs(next.y - perch.y) > 2) perch = next;
    };
    // Nach dem Wechsel kurz warten, bis die neue Ansicht steht; dann regelmaessig nachsehen.
    const first = window.setTimeout(update, 260);
    const timer = window.setInterval(update, 400);
    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  });


  let nav = $state<HTMLElement>();

  /** Tour, letzter Schritt: Milo winkt und huepft, damit man ihn als Ziel bemerkt. */
  $effect(() => {
    if (tour.demo !== "milo") return;
    const moves = ["wave", "hop", "giggle"] as const;
    let i = 0;
    const first = window.setTimeout(() => stage?.play("wave"), 500);
    const timer = window.setInterval(() => stage?.play(moves[++i % moves.length]), 2600);
    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  });

  /** Tour, Buddy-Schritt: der Claude-Wuerfel spielt vor, was er kann (arbeiten, Frage, fertig). */
  let buddyDemo = $state<"working" | "question" | "done">("working");
  $effect(() => {
    if (tour.demo !== "buddy") return;
    const states = ["working", "question", "done"] as const;
    let i = 0;
    buddyDemo = "working";
    const timer = window.setInterval(() => {
      i = (i + 1) % states.length;
      buddyDemo = states[i];
    }, 3200);
    return () => clearInterval(timer);
  });
</script>

<GlassDefs />
<svelte:window onkeydown={(e) => { if (powerFinal && e.key === "Escape") { e.preventDefault(); abortPower(); } }} />
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="dock"
  class:away={app.hidden && !powerFinal}
  style:transform="translate(calc(-50% + {open ? 0 : push.side * push.amount * 5}px), {app.hidden && !powerFinal ? '-112%' : '0'})"
  onmouseenter={() => { hovered = true; clearTimeout(peekTimer); clearTimeout(popTimer); }}
  onmouseleave={() => { hovered = false; schedulePeekClose(); popTimer = window.setTimeout(() => (decorPop = null), 1400); }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="island" class:open class:urgent={powerFinal} bind:this={islandEl} onpointerdowncapture={onIslandDown} style:width="{box.current.w + (open ? 0 : push.amount * 10)}px" style:height="{box.current.h}px" style:--bar="{BAR}px">
    <!-- Lichtebene: ruhige Farbfelder im Inselkoerper. Sie geben dem Glas darueber etwas zu brechen. -->
    {#if open}<span class="ambient" aria-hidden="true" transition:fade={{ duration: 600 }}></span>{/if}
    <header style:height="{headHeight}px">
      <button class="head" onclick={onHeadClick} onpointerdown={onHeadDown} oncontextmenu={onHeadContext} onwheel={onPillWheel} aria-label={t(`${app.settings.name} auf- oder zuklappen`, `Open or close ${app.settings.name}`)} aria-expanded={open}></button>

      <!-- Ein anderes Programm benutzt Mikrofon oder Kamera: kleine Punkte oben rechts. -->
      {#if app.settings.privacyDots && (app.privacy.mic.length || app.privacy.cam.length)}
        <span class="privacy" style:right="{open ? 128 : 16}px" title={[app.privacy.mic.length ? t(`Mikrofon: ${app.privacy.mic.join(", ")}`, `Microphone: ${app.privacy.mic.join(", ")}`) : "", app.privacy.cam.length ? t(`Kamera: ${app.privacy.cam.join(", ")}`, `Camera: ${app.privacy.cam.join(", ")}`) : ""].filter(Boolean).join(" · ")}>
          {#if app.privacy.cam.length}<i class="cam"></i>{/if}
          {#if app.privacy.mic.length}<i class="mic"></i>{/if}
        </span>
      {/if}

      <!-- Das Mikrofon ist offen: immer sichtbar, in jedem Zustand der Insel. -->
      {#if app.voice !== "off"}
        <span
          class="micdot" class:hot={app.voice === "listening"}
          style:left="{open ? HOME + 26 : 18}px" style:top="{open ? 14 : 12}px"
          title={app.voice === "wake" ? t(`Mikrofon ist an und wartet auf „Hey ${app.settings.name}“`, `Microphone is on, waiting for "Hey ${app.settings.name}"`) : t("Mikrofon ist an", "Microphone is on")}
        ></span>
      {/if}

      <div class="stage">
        <Stage
          bind:this={stage}
          width={STAGE.w} height={STAGE.h} {layout}
          mood={app.mood} roaming={!open} hovered={hovered && !open}
          armStyle={app.settings.armStyle} urgency={app.urgency}
          buddy={tour.demo === "buddy" ? buddyDemo : app.claude.present ? app.claude.activity : null}
          helper={app.helper}
          {scenery}
          {perch}
          decor={app.settings.decor}
          decorPositions={app.settings.decorPositions}
          decorLocked={app.settings.decorLocked}
          away={petAway || app.miloOnDesk}
          level={app.voiceLevel}
          liveliness={app.settings.liveliness}
          paused={stagePaused}
          onpush={(side, amount) => (push = { side, amount })}
        />
      </div>

      {#if open}
        <!-- Links: Milos Name mit Statuslicht und was er gerade tut. Datum und Uhrzeit zeigt Windows schon. -->
        <div class="meta left" in:fly={{ x: -8, duration: 320, delay: 140 }}>
          <span class="who"><i class="lamp" class:busy={app.mood !== "idle"} class:alert={app.needsAttention}></i>{app.settings.name}{#if app.settingsLoaded && app.settings.onboarded && !app.cloud.hasKey && !(app.settings.aiMode !== "cloud" && app.settings.localModel)}<button class="needs-key" onclick={openKeySettings} aria-label={t("Gemini-Schlüssel fehlt", "Gemini key missing")}><span aria-hidden="true">!</span><span class="tip" role="tooltip">{t("Noch kein Gemini-Schlüssel: Ich kann nur Schnellbefehle wie Timer oder Lautstärke. Klick hier, um den kostenlosen Schlüssel unter Einstellungen → KI einzutragen.", "No Gemini key yet: I can only do quick commands like timers or volume. Click here to add your free key under Settings → AI.")}</span></button>{/if}</span>
          <span class="status" class:active={app.mood !== "idle"}>{app.status}</span>
        </div>
        <!-- Rechts: Wetter als kleiner Chip, daneben das Menue. -->
        <div class="meta right" bind:clientWidth={buttonsWidth} in:fly={{ x: 8, duration: 320, delay: 140 }}>
          {#if app.weather}
            <button class="chip glass" onclick={() => (app.tab = "weather")} title="{app.weather.place}: {app.weather.now.text}">
              <WeatherIcon code={app.weather.now.code} isDay={app.weather.now.isDay} size={17} />
              <b>{Math.round(app.weather.now.temp)}°</b>
            </button>
          {/if}
          <button class="menu-btn glass" onclick={() => closePanel(true)} aria-label={t("Insel zuklappen", "Close the island")} title={t("Zuklappen (Esc)", "Close (Esc)")}>
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M3.8 9.8L8 5.8l4.2 4" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
          </button>
          <button class="menu-btn glass" class:on={menu} onclick={() => { menu = !menu; library = false; }} aria-label={t("Menü", "Menu")} aria-expanded={menu} title={t("Menü", "Menu")}>
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M3 5h10M3 8h10M3 11h6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
          </button>
        </div>
      {:else}
        <!-- Anzeige rechts in der Pille: Zustand, Timer oder was Milo gerade sagt. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="live" bind:clientWidth={liveWidth} oncontextmenu={onLiveContext}>
          {#key live?.kind}
            {#if live}
              <div class="item {live.kind}" in:fly={{ x: 14, duration: 300 }}>
                {#if live.kind === "timer" && app.nextTimer}
                  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
                    <circle cx="10" cy="10" r="8" fill="none" stroke="#2c2c2e" stroke-width="2.6" />
                    <circle
                      class="arc" cx="10" cy="10" r="8" fill="none" stroke="#64d2ff" stroke-width="2.6" stroke-linecap="round"
                      stroke-dasharray={RING} stroke-dashoffset={RING * (1 - app.timerLeft)} transform="rotate(-90 10 10)"
                    />
                  </svg>
                  <span class="label">{live.text}</span>
                  <span class="time">{clock(app.nextTimer.dueMs - app.now)}</span>
                {:else if live.kind === "media"}
                  {#if app.media?.cover}<img class="thumb" src={app.media.cover} alt="" />{/if}
                  <span class="label">{live.text}</span>
                  {#if hovered}
                    <span class="media-keys" in:fly={{ x: 6, duration: 180 }}>
                      <button onclick={() => app.mediaCommand("previous")} aria-label={t("Zurück", "Previous")} title={t("Zurück", "Previous")}><svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"><path d="M4 3v10M13 3.5v9L6 8Z" fill="currentColor" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /></svg></button>
                      <button onclick={() => app.mediaCommand("toggle")} aria-label={app.media?.playing ? t("Pause", "Pause") : t("Abspielen", "Play")} title={app.media?.playing ? t("Pause", "Pause") : t("Abspielen", "Play")}>
                        {#if app.media?.playing}<svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"><path d="M5 3v10M11 3v10" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" /></svg>{:else}<svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"><path d="M4.5 3v10l8.5-5Z" fill="currentColor" /></svg>{/if}
                      </button>
                      <button onclick={() => app.mediaCommand("next")} aria-label={t("Weiter", "Next")} title={t("Weiter", "Next")}><svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"><path d="M12 3v10M3 3.5v9L10 8Z" fill="currentColor" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /></svg></button>
                    </span>
                  {:else}
                    <span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>
                  {/if}
                {:else if live.kind === "voice"}
                  <span class="label strong">{live.text}</span>
                  <span class="meter" aria-hidden="true">
                    {#each [0.55, 1, 0.75, 0.4] as weight}
                      <i style:height="{4 + Math.min(1, app.voiceLevel * 1.6) * weight * 14}px"></i>
                    {/each}
                  </span>
                {:else if live.kind === "focus" && app.focus}
                  {@const left = Math.max(0, app.focus.endMs - app.clockNow)}
                  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
                    <circle cx="10" cy="10" r="8" fill="none" stroke="#2c2c2e" stroke-width="2.6" />
                    <circle
                      cx="10" cy="10" r="8" fill="none" stroke={app.focus.phase === "focus" ? "#ff9f0a" : "#30d158"} stroke-width="2.6" stroke-linecap="round"
                      stroke-dasharray={RING} stroke-dashoffset={RING * (1 - left / Math.max(1, app.focus.totalMs))} transform="rotate(-90 10 10)"
                    />
                  </svg>
                  <span class="label">{live.text}</span>
                  <span class="time">{clock(left)}</span>
                {:else if live.kind === "pull" && localAi.pull}
                  {@const p = localAi.pull}
                  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true" class:spin={p.phase !== "download"}>
                    <circle cx="10" cy="10" r="8" fill="none" stroke="#2c2c2e" stroke-width="2.6" />
                    <circle
                      class="arc" cx="10" cy="10" r="8" fill="none" stroke="#30d158" stroke-width="2.6" stroke-linecap="round"
                      stroke-dasharray={RING} stroke-dashoffset={RING * (1 - (p.phase === "download" ? p.percent / 100 : 0.25))} transform="rotate(-90 10 10)"
                    />
                  </svg>
                  <span class="label">{p.phase === "check" ? t("Richtet KI ein", "Setting up AI") : p.phase === "start" ? t("Bereite Download vor", "Preparing download") : live.text}</span>
                  {#if p.phase === "download"}
                    <span class="time">{Math.floor(p.percent)} %{p.etaSecs !== null ? ` · ${etaShort(p.etaSecs)}` : ""}</span>
                  {/if}
                {:else if live.kind === "event"}
                  <span class="cal" aria-hidden="true"></span>
                  <span class="label">{live.text}</span>
                  <span class="time">{untilText(live.event)}</span>
                {:else if live.kind === "volume"}
                  <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M2.5 6h2.2L8 3.2v9.6L4.7 10H2.5Z" fill="currentColor" /><path d="M10.4 5.6a3.4 3.4 0 0 1 0 4.8M12.2 4a5.8 5.8 0 0 1 0 8" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
                  <span class="vol" aria-hidden="true"><i style:width="{volume ?? 0}%"></i></span>
                  <span class="time">{live.text}</span>
                {:else if live.kind === "ringing"}
                  <span class="bell"></span>
                  <span class="label strong">{live.text}</span>
                {:else if live.kind === "activity"}
                  <span class="label">{live.text}</span>
                  <span class="dots"><i></i><i></i><i></i></span>
                {:else}
                  <span class="label">{live.text}</span>
                {/if}
              </div>
            {:else if !card && (app.weather || app.settings.pillClock)}
              <!-- Nichts los: Uhrzeit und das zuletzt abgefragte Wetter als ruhige Randnotiz. -->
              <div class="item ambient" in:fly={{ x: 14, duration: 300 }}>
                {#if app.settings.pillClock}<span class="time clock">{clockText}</span>{/if}
                {#if app.weather}
                  <WeatherIcon code={app.weather.now.code} isDay={app.weather.now.isDay} size={20} />
                  <span class="time">{Math.round(app.weather.now.temp)}°</span>
                {/if}
              </div>
            {/if}
          {/key}
        </div>
      {/if}
    </header>

    {#if !open && card}
      <!-- Aufgeklappte Pille: Timer fertig, Termin beginnt, Claude fertig. -->
      <div class="card-row {card.kind}" in:fly={{ y: -8, duration: 300, delay: 120 }}>
        {#if card.kind === "power"}
          {@const secs = Math.ceil(powerLeft / 1000)}
          <span class="badge-big power" aria-hidden="true">{secs}</span>
          <span class="card-text" role="alert">
            <b>{t(`${powerLabel} in ${secs} s`, `${powerLabel} in ${secs} s`)}</b>
            <small>{app.powerPlan?.action === "sleep" ? t("Gleich geht der PC in den Energiesparmodus.", "The PC is about to go to sleep.") : t("Jetzt noch speichern – oder abbrechen (auch mit Esc).", "Save your work now – or cancel (Esc works too).")}</small>
          </span>
          <button class="card-btn primary stop" onclick={abortPower}>{t("Abbrechen", "Cancel")}</button>
        {:else if card.kind === "done"}
          <span class="badge-big done" aria-hidden="true">✓</span>
          <span class="card-text"><b>{card.timer.label}</b><small>{t("Fertig!", "Done!")}{app.ringing.length > 1 ? t(` · noch ${app.ringing.length - 1} weitere`, ` · ${app.ringing.length - 1} more`) : ""}</small></span>
          <button class="card-btn" onclick={() => app.extendTimer(card.timer, 60)}>+1 min</button>
          <button class="card-btn" onclick={() => app.extendTimer(card.timer, 300)}>+5 min</button>
          <button class="card-btn primary" onclick={() => app.dismissTimer(card.timer)}>{t("Stopp", "Stop")}</button>
        {:else if card.kind === "event"}
          <span class="badge-big event" aria-hidden="true"></span>
          <span class="card-text"><b>{card.event.title}</b><small>{t("Beginnt jetzt", "Starting now")}{card.event.location ? ` · ${card.event.location}` : ""}</small></span>
          <button class="card-btn" onclick={() => { app.tab = "agenda"; openPanel(false); peeking = false; }}>{t("Agenda", "Agenda")}</button>
          <button class="card-btn primary" onclick={() => app.hideEvent(card.event)}>OK</button>
        {:else if card.kind === "generic"}
          {@const item = card.item}
          {#if item.kind === "note"}
            <span class="badge-big note" aria-hidden="true">✎</span>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="note-input" bind:this={noteInput} bind:value={noteText} maxlength="500" spellcheck="true" autofocus
              placeholder={t("Notiz eintippen, Enter speichert, Esc bricht ab", "Type a note, Enter saves, Esc cancels")}
              onkeydown={(e) => { if (e.key === "Enter") saveQuickNote(); else if (e.key === "Escape") { e.stopPropagation(); cancelQuickNote(); } }}
            />
            <button class="card-btn primary" onclick={saveQuickNote}>{t("Speichern", "Save")}</button>
          {:else}
            {#if item.thumb}
              <img class="card-thumb" src={item.thumb} alt="" />
            {:else}
              <span class="badge-big {item.kind}" class:failed={item.failed} aria-hidden="true">{item.failed ? "!" : item.kind === "download" ? "↓" : item.kind === "battery" ? "▮" : item.kind === "rain" ? "☂" : item.kind === "focusDone" || item.kind === "info" ? "✓" : item.kind === "breakDone" ? "↻" : item.kind === "review" ? "☾" : "•"}</span>
            {/if}
            <span class="card-text"><b>{item.title}</b><small title={item.sub}>{item.sub}</small></span>
            {#if item.kind === "download"}
              <button class="card-btn" disabled={cardBusy} onclick={() => cardAction(item, "folder")}>{t("Ordner", "Folder")}</button>
              <button class="card-btn primary" disabled={cardBusy} onclick={() => cardAction(item, "open")}>{t("Öffnen", "Open")}</button>
            {:else if item.kind === "screenshot"}
              <button class="card-btn" disabled={cardBusy} onclick={() => cardAction(item, "ocr")}>{t("Text lesen", "Read text")}</button>
              <button class="card-btn" disabled={cardBusy} onclick={() => cardAction(item, "ask")}>{t("Milo fragen", "Ask Milo")}</button>
              <button class="card-btn primary" disabled={cardBusy} onclick={() => cardAction(item, "saveShot")}>{t("Speichern", "Save")}</button>
            {:else if item.kind === "rain"}
              <button class="card-btn" onclick={() => cardAction(item, "weather")}>{t("Wetter", "Weather")}</button>
            {:else if item.kind === "focusDone"}
              <button class="card-btn" onclick={() => cardAction(item, "again")}>{t("Noch eine Runde", "Another round")}</button>
              <button class="card-btn primary" onclick={() => cardAction(item, "break")}>{t("5 Min Pause", "5 min break")}</button>
            {:else if item.kind === "breakDone"}
              <button class="card-btn" onclick={() => cardAction(item, "stopFocus")}>{t("Fertig", "Done")}</button>
              <button class="card-btn primary" onclick={() => cardAction(item, "again")}>{t("Nächste Runde", "Next round")}</button>
            {:else if item.kind === "review"}
              <button class="card-btn" onclick={() => app.dropCard(item.id)}>{t("Später", "Later")}</button>
              <button class="card-btn primary" onclick={() => cardAction(item, "review")}>{t("Erzählen", "Tell Milo")}</button>
            {/if}
            {#if item.kind !== "focusDone" && item.kind !== "breakDone" && item.kind !== "review"}
              <button class="card-x" onclick={() => app.dropCard(item.id)} aria-label={t("Schließen", "Close")} title={t("Schließen", "Close")}><svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg></button>
            {/if}
          {/if}
        {:else}
          <span class="badge-big claude" aria-hidden="true">✓</span>
          <span class="card-text"><b>{t("Claude ist fertig", "Claude is done")}</b><small>{app.claude.project || t("Deine Aufgabe ist erledigt", "Your task is finished")}</small></span>
          <button class="card-btn primary" onclick={() => { claudeFlash = false; app.tab = "claude"; openPanel(false); peeking = false; }}>{t("Ansehen", "View")}</button>
        {/if}
      </div>
    {/if}

    {#if open}
      <div class="body" class:bleed={app.tab === "media" || app.tab === "agenda"}>
        {#if app.settingsLoaded && !app.settings.onboarded}<Welcome />{/if}
        {#if app.wiping}<WipeDialog />{/if}
        {#if app.textOffer || app.textError}<TextWork />{/if}
        <Notices />
        {#key app.tab}
          <div class="pane" in:paneIn>
            {#if app.tab === "home"}
              <HomeView />
            {:else if app.tab === "media"}
              <MediaView />
            {:else if app.tab === "agenda"}
              <AgendaView />
            {:else if app.tab === "timer"}
              <TimerView />
            {:else if app.tab === "weather"}
              <WeatherView />
            {:else if app.tab === "files"}
              <FilesView />
            {:else if app.tab === "notes"}
              <NotesView />
            {:else if app.tab === "system"}
              <SystemView />
            {:else if app.tab === "claude"}
              <ClaudeView />
            {:else if app.tab === "usage"}
              <UsageView />
            {:else if app.tab === "history"}
              <HistoryView />
            {:else if app.tab === "widgets"}
              <WidgetsView />
            {:else if app.tab === "settings"}
              <SettingsView />
            {:else}
              <Chat bind:this={chat} />
            {/if}
          </div>
        {/key}
      </div>

      <!-- Wuerfelregal: jede Ansicht ist ein kleiner Wuerfel auf einem Glasbrett. Der aktive hebt sich,
           Milo springt auf ihn. Links ein neues Gespraech, rechts die Deko-Bibliothek. -->
      <!-- Waehrend der Einrichtung ist die Leiste abgedunkelt, damit nichts ablenkt. -->
      <div class="shelf-area" class:veiled={app.settingsLoaded && !app.settings.onboarded}>
        <div class="shelf glass" use:lens={{ selector: ".cubes button.active", grow: 2 }}>
          <span class="lens-thumb" aria-hidden="true"></span>
          <button class="cube util" onclick={() => { app.clear(); app.tab = "chat"; tick().then(() => chat?.focus()); }} title={t("Neues Gespräch", "New conversation")} aria-label={t("Neues Gespräch", "New conversation")}>
            <span class="cube-body"><svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M8 3.5v9M3.5 8h9" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg></span>
          </button>
          <span class="shelf-sep" aria-hidden="true"></span>
          <nav class="cubes" aria-label={t("Ansichten", "Views")} bind:this={nav}>
            {#each tabs as tab (tab.id)}
              <button class:active={app.tab === tab.id} class:alert={tab.alert} onclick={() => (app.tab = tab.id)} title={tab.label} aria-label={tab.label} aria-current={app.tab === tab.id ? "page" : undefined}>
                <span class="cube-body">
                  <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                    <path d={ICONS[tab.id]} fill="none" stroke="currentColor" stroke-width="1.45" stroke-linecap="round" stroke-linejoin="round" />
                  </svg>
                </span>
                {#if tab.badge && app.tab !== tab.id}<span class="badge" class:pulse={tab.alert}></span>{/if}
                <span class="cube-name">{tab.label}</span>
              </button>
            {/each}
          </nav>
          <span class="shelf-sep" aria-hidden="true"></span>
          <button class="cube util" class:on={library} onclick={() => { library = !library; menu = false; }} title={t("Deko-Bibliothek", "Decor library")} aria-label={t("Deko-Bibliothek", "Decor library")} aria-expanded={library}>
            <span class="cube-body"><svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
              <path d="M5 13.5h6M6 13.5l-.6-4h5.2l-.6 4M8 9.5V7M8 7c-.2-1.8-1.4-2.8-3-2.9.1 1.7 1.2 2.8 3 2.9ZM8 7c.2-2.2 1.6-3.4 3.4-3.5-.1 2-1.4 3.4-3.4 3.5Z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
            </svg></span>
          </button>
        </div>
      </div>

      {#if menu || library}
        <!-- Schleier: nimmt den Inhalt zurueck, solange ein Menue darueber liegt; ein Klick schliesst es. -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="scrim" transition:fade={{ duration: 220 }} onclick={() => { menu = false; library = false; }}></div>
      {/if}
      {#if menu}
        <div class="popover glass menu" transition:pop>
          <div class="pop-head">
            <strong>{t("Menü", "Menu")}</strong>
            <button class="pop-x" onclick={() => (menu = false)} aria-label={t("Menü schließen", "Close menu")} title={t("Schließen", "Close")}><svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg></button>
          </div>
          <button onclick={() => { menu = false; app.tab = "widgets"; }}>
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><rect x="2.2" y="2.2" width="5" height="5" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.4" /><circle cx="11.3" cy="4.7" r="2.5" fill="none" stroke="currentColor" stroke-width="1.4" /><rect x="2.2" y="9.2" width="5" height="4.6" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M11.3 9.4v4.4M9.1 11.6h4.4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
            {t("Widgets bauen", "Build widgets")}
          </button>
          <button onclick={openSettings}>
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M2.8 4.6h5.4M11.4 4.6h1.8M2.8 11.4h1.8M7.8 11.4h5.4" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /><circle cx="9.8" cy="4.6" r="1.7" fill="none" stroke="currentColor" stroke-width="1.4" /><circle cx="6.2" cy="11.4" r="1.7" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
            {t("Einstellungen", "Settings")}
          </button>
          <button onclick={() => { menu = false; library = true; }}>
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M5 13.5h6M6 13.5l-.6-4h5.2l-.6 4M8 9.5V7M8 7c-.2-1.8-1.4-2.8-3-2.9.1 1.7 1.2 2.8 3 2.9ZM8 7c.2-2.2 1.6-3.4 3.4-3.5-.1 2-1.4 3.4-3.4 3.5Z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" /></svg>
            {t("Deko-Bibliothek", "Decor library")}
          </button>
          <span class="sep"></span>
          <button onclick={() => closePanel(true)}>
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M3.8 9.8L8 5.8l4.2 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
            {t("Zuklappen", "Close")} <kbd>Esc</kbd>
          </button>
        </div>
      {/if}
      {#if library}
        <div class="popover glass library-pop" transition:pop>
          <DecorLibrary onclose={() => (library = false)} />
        </div>
      {/if}
    {/if}
    {#if open && tour.active && app.settings.onboarded}<Tour island={islandEl} />{/if}
  </div>
  <!-- Erklaerkasten der Tour: unterhalb der Insel, damit in ihr nichts verdeckt wird. -->
  {#if open && tour.active && app.settings.onboarded}<TourCard />{/if}
  {#if decorPop && !open}
    {@const info = DECOR.find((d) => d.kind === decorPop?.kind)}
    {@const kind = decorPop.kind}
    {@const locked = app.settings.decorLocked.includes(kind)}
    <div class="popover glass decor-pop" style:left="{decorPop.left}px" style:top="{PILL.h + 8}px" transition:pop role="menu" aria-label="{info?.label ?? kind}">
      <div class="pop-head">
        <strong>{info?.label ?? kind}</strong>
        <button class="pop-x" onclick={() => (decorPop = null)} aria-label={t("Schließen", "Close")} title={t("Schließen", "Close")}><svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg></button>
      </div>
      <button class="pop-row" role="menuitemcheckbox" aria-checked={locked} onclick={() => toggleDecorLock(kind)}>
        <span class="pop-text"><b>{t("Nur in der Insel", "Island only")}</b><small>{locked ? t("Bleibt fest an seinem Platz", "Stays put in its spot") : t("Lässt sich verschieben und herausziehen", "Can be moved and dragged out")}</small></span>
        <span class="switch" class:on={locked} aria-hidden="true"><i></i></span>
      </button>
      <button class="pop-row" role="menuitem" disabled={locked} onclick={(e) => decorToDesk(kind, e.clientX)}>
        <span class="pop-text"><b>{t("Auf den Desktop", "To the desktop")}</b><small>{t("Fällt auf die Taskleiste", "Drops onto the taskbar")}</small></span>
      </button>
      <button class="pop-row" role="menuitem" onclick={() => decorRemove(kind)}>
        <span class="pop-text"><b>{t("Aus der Insel nehmen", "Remove from the island")}</b></span>
      </button>
      <button class="pop-row more" role="menuitem" onclick={() => openLibraryFor(kind)}>
        <span class="pop-text"><b>{t("Alle Deko …", "All decor …")}</b></span>
      </button>
    </div>
  {/if}
  {#if eventPop && !open}
    {@const ev = eventPop.event}
    <div class="popover glass decor-pop" style:left="{eventPop.left}px" style:top="{CLOSED.h + 8}px" transition:pop role="menu" aria-label={ev.title}>
      <div class="pop-head">
        <strong>{ev.title}</strong>
        <button class="pop-x" onclick={() => (eventPop = null)} aria-label={t("Schließen", "Close")} title={t("Schließen", "Close")}><svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg></button>
      </div>
      <button class="pop-row" role="menuitem" onclick={() => { app.hideEvent(ev); eventPop = null; }}>
        <span class="pop-text"><b>{t("Diesen Termin ausblenden", "Hide this event")}</b><small>{t("Erscheint nicht mehr in der Pille", "Won't show in the pill any more")}</small></span>
      </button>
      <button class="pop-row" role="menuitem" onclick={() => { app.saveSettings({ eventLead: 0 }); eventPop = null; }}>
        <span class="pop-text"><b>{t("Termin-Hinweise ausschalten", "Turn off event reminders")}</b><small>{t("Wieder einschalten unter Einstellungen → Allgemein", "Turn back on under Settings → General")}</small></span>
      </button>
      <button class="pop-row more" role="menuitem" onclick={() => { eventPop = null; app.tab = "agenda"; openPanel(false); peeking = false; }}>
        <span class="pop-text"><b>{t("Agenda öffnen", "Open agenda")}</b></span>
      </button>
    </div>
  {/if}
</div>

<style>
  .dock {
    position: absolute;
    top: 0;
    left: 50%;
    /* Zurueckkommen mit leichtem Nachfedern, Wegziehen zuegig. */
    transition: transform var(--t-spring) var(--ease-spring);
  }
  .dock.away {
    transition: transform var(--t-slow) cubic-bezier(0.5, 0, 0.8, 0.4);
  }
  /* Kleine Hohlkehlen, mit denen die Insel weich in den Bildschirmrand uebergeht. */
  .dock::before,
  .dock::after {
    content: "";
    position: absolute;
    top: 0;
    width: 12px;
    height: 12px;
    pointer-events: none;
  }
  .dock::before {
    left: -12px;
    background: radial-gradient(circle at 0 100%, transparent 11.5px, #000 12px);
  }
  .dock::after {
    right: -12px;
    background: radial-gradient(circle at 100% 100%, transparent 11.5px, #000 12px);
  }
  .island {
    position: relative;
    isolation: isolate;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    /* Dunkler Koerper wie eine Scheibe aus schwarzem Glas: oben tiefschwarz (geht in den
       Bildschirmrand ueber), nach unten eine Spur heller. */
    background: linear-gradient(180deg, #000 0%, var(--ink) 34%, #0a0a0d 100%);
    border-radius: 0 0 30px 30px;
    /* clip statt hidden: ein fokussiertes Feld darf die Insel nicht nach oben scrollen
       (sonst sind Kopfzeile, Laterne und Milo oben angeschnitten). */
    overflow: clip;
    /* Kante: unten ein feiner Lichtstrich (Glaskante), seitlich Haarlinien, darunter drei Schatten. */
    box-shadow:
      inset 0 -1px 0 rgba(255, 255, 255, 0.16),
      inset 0 -2px 3px -1px rgba(255, 255, 255, 0.07),
      inset 1px 0 0 rgba(255, 255, 255, 0.05),
      inset -1px 0 0 rgba(255, 255, 255, 0.05),
      var(--lift-3);
  }
  /* Lichtebene: zwei weiche Farbfelder (Akzent und ein kuehler Nachbar), die langsam wandern. */
  .ambient {
    position: absolute;
    inset: 0;
    z-index: -1;
    pointer-events: none;
    overflow: hidden;
  }
  .ambient::before,
  .ambient::after {
    content: "";
    position: absolute;
    bottom: -45%;
    width: 70%;
    height: 90%;
    border-radius: 50%;
    will-change: transform;
  }
  .ambient::before {
    left: -8%;
    background: radial-gradient(closest-side, color-mix(in oklab, var(--accent) 20%, transparent), transparent);
    animation: drift-a 26s ease-in-out infinite alternate;
  }
  .ambient::after {
    right: -10%;
    background: radial-gradient(closest-side, color-mix(in oklab, color-mix(in oklch, var(--accent), #8a5cff 70%) 17%, transparent), transparent);
    animation: drift-b 32s ease-in-out infinite alternate;
  }
  @keyframes drift-a {
    from { transform: translate(-6%, 4%) scale(0.95); }
    to { transform: translate(14%, -6%) scale(1.08); }
  }
  @keyframes drift-b {
    from { transform: translate(8%, -4%) scale(1.06); }
    to { transform: translate(-12%, 6%) scale(0.94); }
  }
  /* Schleier unter Menues. */
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 7;
    background: rgba(0, 0, 0, 0.46);
  }
  .island.open {
    border-radius: 0 0 38px 38px;
  }

  header {
    position: relative;
    flex: none;
    /* Ueber Inhalt und Glasleiste: Milo steht auf ihnen. */
    z-index: 6;
  }
  .head {
    all: unset;
    position: absolute;
    inset: 0;
    z-index: 1;
    cursor: pointer;
  }
  .stage {
    position: absolute;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
    z-index: 2;
    pointer-events: none;
  }

  /* Kopfzeile der offenen Insel: links Datum und Zustand, rechts Uhrzeit und Menue. */
  .meta {
    position: absolute;
    top: 14px;
    display: flex;
    pointer-events: none;
    font-variant-numeric: tabular-nums;
  }
  .meta.left {
    left: 30px;
    flex-direction: column;
    gap: 2px;
  }
  /* ---------- Aufgeklappte Pille ---------- */
  .card-row {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 60px;
    padding: 0 18px 4px;
  }
  .badge-big {
    flex: none;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    font: var(--fw-bold) var(--fs-headline)/1 var(--font-sans);
    color: #000;
  }
  .badge-big.done {
    background: var(--ok);
    box-shadow: 0 0 18px rgba(48, 209, 88, 0.55);
    animation: card-pop var(--t-spring) cubic-bezier(0.34, 1.6, 0.64, 1) both, ring-glow 1.2s ease-in-out infinite var(--t-spring);
  }
  .badge-big.claude {
    background: #d97757;
    box-shadow: 0 0 18px rgba(217, 119, 87, 0.55);
    animation: card-pop var(--t-spring) cubic-bezier(0.34, 1.6, 0.64, 1) both;
  }
  .badge-big.event {
    background: #bf5af2;
    box-shadow: 0 0 18px rgba(191, 90, 242, 0.55);
    animation: card-pop var(--t-spring) cubic-bezier(0.34, 1.6, 0.64, 1) both, ring-glow 1.6s ease-in-out infinite var(--t-spring);
  }
  .badge-big.event::before {
    content: "";
    width: 14px;
    height: 13px;
    border-radius: 3px;
    border: 2px solid #000;
    border-top-width: 4px;
  }
  .card-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.2;
  }
  .card-text b {
    font-size: var(--fs-headline);
    font-weight: var(--fw-bold);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-text small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-btn {
    flex: none;
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.12);
    color: var(--text);
    font: var(--fw-semibold) var(--fs-footnote) var(--font-sans);
    cursor: pointer;
    transition: background var(--t-fast) ease, transform var(--t-fast) ease;
  }
  .card-btn:hover {
    background: rgba(255, 255, 255, 0.2);
  }
  .card-btn:active {
    transform: scale(0.95);
  }
  .card-btn.primary {
    background: var(--text);
    color: #000;
  }
  /* Letzter Countdown: dringend rot, pulsierend, nicht zu uebersehen. */
  .island.urgent {
    box-shadow:
      inset 0 0 0 2px rgba(255, 69, 58, 0.85),
      0 0 34px rgba(255, 69, 58, 0.55),
      0 18px 40px rgba(0, 0, 0, 0.5);
    animation: urgent-glow 1s ease-in-out infinite;
  }
  @keyframes urgent-glow {
    0%, 100% { box-shadow: inset 0 0 0 2px rgba(255, 69, 58, 0.85), 0 0 22px rgba(255, 69, 58, 0.4), 0 18px 40px rgba(0, 0, 0, 0.5); }
    50% { box-shadow: inset 0 0 0 2px rgba(255, 99, 88, 1), 0 0 44px rgba(255, 69, 58, 0.75), 0 18px 40px rgba(0, 0, 0, 0.5); }
  }
  .card-row.power .card-text b {
    font-size: var(--fs-headline);
    color: #ff6961;
    font-variant-numeric: tabular-nums;
  }
  .badge-big.power {
    width: 44px;
    height: 44px;
    background: var(--bad);
    color: #fff;
    font-size: var(--fs-headline);
    font-variant-numeric: tabular-nums;
    box-shadow: 0 0 20px rgba(255, 69, 58, 0.7);
    animation: card-pop var(--t-spring) cubic-bezier(0.34, 1.6, 0.64, 1) both, ring-glow 1s ease-in-out infinite var(--t-spring);
  }
  .card-btn.primary.stop {
    height: 36px;
    padding: 0 18px;
    background: var(--bad);
    color: #fff;
    font-size: var(--fs-callout);
    font-weight: var(--fw-bold);
  }
  .card-btn.primary.stop:hover {
    background: #ff6961;
  }
  @media (prefers-reduced-motion: reduce) {
    .island.urgent,
    .badge-big.power {
      animation: none;
    }
  }
  @keyframes card-pop {
    from { transform: scale(0.3); opacity: 0; }
    to { transform: scale(1); opacity: 1; }
  }
  @keyframes ring-glow {
    0%, 100% { transform: scale(1); }
    50% { transform: scale(1.08); }
  }
  .badge-big.download { background: #64d2ff; box-shadow: 0 0 16px rgba(100, 210, 255, 0.5); animation: card-pop 520ms cubic-bezier(0.34, 1.6, 0.64, 1) both; }
  .badge-big.battery { background: #ff453a; box-shadow: 0 0 16px rgba(255, 69, 58, 0.5); animation: card-pop 520ms cubic-bezier(0.34, 1.6, 0.64, 1) both, ring-glow 1.4s ease-in-out infinite 520ms; }
  .badge-big.rain { background: #5e9bff; box-shadow: 0 0 16px rgba(94, 155, 255, 0.5); animation: card-pop 520ms cubic-bezier(0.34, 1.6, 0.64, 1) both; }
  .badge-big.focusDone { background: #ff9f0a; box-shadow: 0 0 16px rgba(255, 159, 10, 0.5); animation: card-pop 520ms cubic-bezier(0.34, 1.6, 0.64, 1) both; }
  .badge-big.breakDone,
  .badge-big.info { background: #30d158; box-shadow: 0 0 16px rgba(48, 209, 88, 0.45); animation: card-pop 520ms cubic-bezier(0.34, 1.6, 0.64, 1) both; }
  .badge-big.review { background: #ffd60a; box-shadow: 0 0 16px rgba(255, 214, 10, 0.45); animation: card-pop 520ms cubic-bezier(0.34, 1.6, 0.64, 1) both; }
  .badge-big.note { background: #f5f5f7; animation: card-pop 520ms cubic-bezier(0.34, 1.6, 0.64, 1) both; }
  .badge-big.failed { background: #ff453a; box-shadow: 0 0 16px rgba(255, 69, 58, 0.5); }
  .card-thumb {
    flex: none;
    height: 44px;
    max-width: 80px;
    border-radius: var(--r-sm);
    object-fit: cover;
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.18);
    animation: card-pop var(--t-slow) cubic-bezier(0.34, 1.6, 0.64, 1) both;
  }
  .card-x {
    flex: none;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-2);
    cursor: pointer;
  }
  .card-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .note-input {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.08);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14);
    color: var(--text);
    font: var(--fw-medium) var(--fs-callout) var(--font-sans);
    outline: none;
  }
  .note-input:focus {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .privacy {
    position: absolute;
    top: 7px;
    z-index: 3;
    display: flex;
    gap: 4px;
  }
  .privacy i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .privacy .mic {
    background: #ff9f0a;
    box-shadow: 0 0 6px rgba(255, 159, 10, 0.9);
  }
  .privacy .cam {
    background: var(--ok);
    box-shadow: 0 0 6px rgba(48, 209, 88, 0.9);
  }
  .media-keys {
    display: flex;
    gap: 2px;
    pointer-events: auto;
    position: relative;
    z-index: 3;
  }
  .media-keys button {
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text);
    cursor: pointer;
  }
  .media-keys button:hover {
    background: rgba(255, 255, 255, 0.22);
  }
  .cal {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #bf5af2;
    box-shadow: 0 0 8px rgba(191, 90, 242, 0.8);
  }
  .vol {
    position: relative;
    width: 70px;
    height: 5px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.16);
    overflow: hidden;
  }
  .vol i {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 3px;
    background: var(--text);
    transition: width var(--t-fast) ease;
  }
  .needs-key {
    position: relative;
    width: 16px;
    height: 16px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--warn);
    color: #000;
    font: var(--fw-bold) var(--fs-caption)/1 var(--font-sans);
    cursor: pointer;
    animation: key-pulse 2.4s ease-in-out infinite;
  }
  .needs-key .tip {
    position: absolute;
    top: calc(100% + 8px);
    left: -10px;
    z-index: 40;
    width: 250px;
    padding: 8px 12px;
    border-radius: var(--r-md);
    background: rgba(28, 28, 32, 0.97);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12), 0 10px 26px rgba(0, 0, 0, 0.5);
    color: var(--text-2);
    font: var(--fw-medium) var(--fs-footnote)/1.45 var(--font-sans);
    text-align: left;
    opacity: 0;
    transform: translateY(-4px);
    pointer-events: none;
    transition: opacity var(--t-fast) ease, transform var(--t-fast) ease;
  }
  .needs-key:hover .tip,
  .needs-key:focus-visible .tip {
    opacity: 1;
    transform: none;
  }
  @keyframes key-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(255, 214, 10, 0.5); }
    50% { box-shadow: 0 0 0 5px rgba(255, 214, 10, 0); }
  }
  .who {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .lamp {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 0 3px rgba(48, 209, 88, 0.18);
    transition: background var(--t-slow) ease, box-shadow var(--t-slow) ease;
  }
  .lamp.busy {
    background: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent);
    animation: blink 1.4s ease-in-out infinite;
  }
  .lamp.alert {
    background: var(--warn);
    box-shadow: 0 0 0 3px rgba(255, 214, 10, 0.25);
  }
  .status {
    font-size: var(--fs-footnote);
    color: var(--text-4);
    white-space: nowrap;
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color var(--t-base) ease;
  }
  .status.active {
    color: var(--text-2);
  }
  .meta.right {
    right: 18px;
    top: 12px;
    align-items: center;
    gap: 6px;
    z-index: 3;
  }
  .chip,
  .menu-btn {
    pointer-events: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 34px;
    border: 0;
    border-radius: var(--r-full);
    --glass-lift: var(--lift-1);
    color: var(--text);
    font: inherit;
    cursor: pointer;
    transition: transform var(--t-slow) var(--ease-spring);
  }
  .chip {
    padding: 0 12px 0 8px;
    font-size: var(--fs-body);
  }
  .chip b {
    font-weight: var(--fw-semibold);
  }
  .menu-btn {
    width: 34px;
  }
  .chip:active,
  .menu-btn:active {
    transform: scale(0.93);
  }
  .chip:focus-visible,
  .menu-btn:focus-visible,
  .popover button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .live {
    position: absolute;
    right: 24px;
    top: 0;
    height: 64px;
    display: flex;
    align-items: center;
    pointer-events: none;
    white-space: nowrap;
    font-size: var(--fs-callout);
    font-variant-numeric: tabular-nums;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .label {
    max-width: 170px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-2);
  }
  .label.strong {
    max-width: 240px;
    color: var(--text);
    font-weight: var(--fw-semibold);
  }
  .item:global(.claude) .label {
    max-width: 250px;
    color: #f5b08a;
    font-weight: var(--fw-semibold);
  }
  .item:global(.said) .label {
    max-width: 240px;
    color: var(--text);
  }
  .time {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.01em;
  }
  .ambient {
    gap: 6px;
    opacity: 0.85;
  }
  .ambient .time {
    font-weight: var(--fw-medium);
  }
  .thumb {
    width: 26px;
    height: 26px;
    border-radius: var(--r-sm);
    object-fit: cover;
    outline: 1px solid rgba(255, 255, 255, 0.1);
    outline-offset: -1px;
  }
  .eq {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
  }
  .eq i {
    width: 2.5px;
    height: 100%;
    border-radius: 2px;
    background: var(--ok);
    transform-origin: bottom;
    animation: eq 0.9s ease-in-out infinite;
  }
  .eq i:nth-child(2) { animation-delay: -0.3s; animation-duration: 0.7s; }
  .eq i:nth-child(3) { animation-delay: -0.55s; animation-duration: 1.1s; }
  @keyframes eq {
    0%, 100% { transform: scaleY(0.3); }
    50% { transform: scaleY(1); }
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 2.5px;
    height: 20px;
  }
  .meter i {
    width: 3px;
    border-radius: 2px;
    background: #ff6961;
    transition: height var(--t-fast) linear;
  }
  .micdot {
    position: absolute;
    z-index: 4;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #ff9f0a;
    box-shadow: 0 0 0 2px #000;
    pointer-events: auto;
    transition: left var(--t-slow) ease, top var(--t-slow) ease, background var(--t-base) ease;
    animation: fade var(--t-slow) ease backwards;
  }
  .micdot.hot {
    background: var(--bad);
    animation: blink 1.2s ease-in-out infinite;
  }
  .arc {
    transition: stroke-dashoffset var(--t-base) linear;
  }
  .live svg.spin {
    animation: pull-spin 1.1s linear infinite;
  }
  @keyframes pull-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .bell {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warn);
    animation: blink 1.2s ease-in-out infinite;
  }
  .dots {
    display: flex;
    gap: 4px;
  }
  .dots i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--text-3);
    animation: hop 1.1s ease-in-out infinite;
  }
  .dots i:nth-child(2) { animation-delay: 0.15s; }
  .dots i:nth-child(3) { animation-delay: 0.3s; }

  /* Inhalt: fuellt alles unter der Kopfzeile. Die Glasleiste schwebt darueber; volle Ansichten
     (Musik, Agenda) laufen unter ihr durch, damit das Glas etwas zum Brechen hat. */
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding-bottom: calc(var(--bar) + 6px);
    /* backwards statt both: eine stehende Deckkraft-Animation naehme dem Glas darin den Hintergrund. */
    animation: fade var(--t-slow) ease 120ms backwards;
  }
  .body.bleed {
    padding-bottom: 0;
  }
  .pane {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* ---------- Glasschale mit Wuerfeln ---------- */
  .shelf-area.veiled {
    opacity: 0.08;
    filter: blur(2px);
    pointer-events: none;
    transition: opacity var(--t-slow) ease, filter var(--t-slow) ease;
  }
  .shelf-area {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 14px;
    z-index: 5;
    display: flex;
    justify-content: center;
    pointer-events: none;
    /* Nur Verschiebung, keine Deckkraft: sonst saehe das Glas den Inhalt dahinter nicht. */
    animation: rise var(--t-spring) var(--ease-spring) 80ms backwards;
  }
  /* Glaskapsel: eine einzige klare Flaeche, die Symbole liegen flach darin. */
  .shelf {
    --glass-lift: var(--lift-3);
    display: flex;
    align-items: center;
    gap: var(--s-1);
    padding: var(--s-1);
    border-radius: var(--r-full);
    pointer-events: auto;
  }
  .shelf-sep {
    flex: none;
    width: 1px;
    height: 22px;
    background: rgba(255, 255, 255, 0.12);
  }
  /* Linse: das eine Stueck Glas, das zur gewaehlten Ansicht gleitet. Leicht in der Akzentfarbe
     getoent, mit eigener Glanzkante; sie liegt UNTER den Symbolen. */
  .shelf > :global(.lens-thumb) {
    z-index: 0;
    background:
      radial-gradient(120% 90% at 20% -10%, rgba(255, 255, 255, 0.34), transparent 55%),
      linear-gradient(180deg, color-mix(in srgb, var(--accent) 46%, rgba(255, 255, 255, 0.1)), color-mix(in srgb, var(--accent) 20%, rgba(255, 255, 255, 0.03)));
    box-shadow:
      inset 0 1px 0.5px rgba(255, 255, 255, 0.6),
      inset 0 -1px 0.5px rgba(255, 255, 255, 0.16),
      inset 0 0 0 0.5px rgba(255, 255, 255, 0.22),
      inset 0 -8px 12px -8px rgba(0, 0, 0, 0.3),
      0 6px 16px -4px color-mix(in srgb, var(--accent) 40%, transparent),
      0 2px 6px rgba(0, 0, 0, 0.35);
  }
  .cubes {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
  }
  .cube,
  .cubes button {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    border: 0;
    padding: 0;
    border-radius: var(--r-full);
    background: none;
    font: inherit;
    cursor: pointer;
  }
  /* Symbol: flach, ohne eigenen Kasten. */
  .cube-body {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--r-full);
    color: var(--text-2);
    transition: transform var(--t-slow) var(--ease-spring), background var(--t-base) ease, color var(--t-base) ease;
  }
  .cube-body :global(svg) {
    width: 20px;
    height: 20px;
  }
  .cubes button:hover .cube-body,
  .cube:hover .cube-body {
    color: #fff;
    background: rgba(255, 255, 255, 0.1);
  }
  .cubes button:active .cube-body,
  .cube:active .cube-body {
    transform: scale(0.88);
  }
  /* Gewaehlt: das Symbol rueckt nach oben und macht dem Namen Platz; den Grund liefert die Linse. */
  .cubes button.active .cube-body {
    color: #fff;
    background: none;
    transform: translateY(-7px);
  }
  .cubes button.active:active .cube-body {
    transform: translateY(-7px) scale(0.9);
  }
  .cube-name {
    position: absolute;
    left: 50%;
    bottom: 7px;
    transform: translate(-50%, 5px);
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    letter-spacing: var(--tr-caption);
    line-height: 1;
    white-space: nowrap;
    color: #fff;
    opacity: 0;
    transition: opacity var(--t-base) ease, transform var(--t-slow) var(--ease-spring);
    pointer-events: none;
  }
  .cubes button.active .cube-name {
    opacity: 1;
    transform: translate(-50%, 0);
  }
  .cube.util {
    width: 44px;
  }
  .cube.util.on .cube-body {
    color: #fff;
    background: rgba(255, 255, 255, 0.18);
  }
  .cube:focus-visible,
  .cubes button:focus-visible {
    outline: none;
  }
  .cube:focus-visible .cube-body,
  .cubes button:focus-visible .cube-body {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .badge {
    position: absolute;
    top: 9px;
    right: 9px;
    z-index: 2;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px rgba(10, 10, 14, 0.9);
  }
  .badge.pulse {
    background: var(--warn);
    animation: blink 1.2s ease-in-out infinite;
  }

  .popover {
    --glass-lift: var(--lift-3);
    position: absolute;
    z-index: 8;
    /* Aussen = Zeilenradius + Innenabstand, damit die Rundungen ineinander passen. */
    border-radius: calc(var(--r-md) + 6px);
    padding: 6px;
  }
  .menu {
    top: 54px;
    right: 14px;
    width: 220px;
    display: flex;
    flex-direction: column;
    transform-origin: top right;
  }
  .menu button {
    all: unset;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--r-md);
    font-size: var(--fs-callout);
    color: var(--text);
    cursor: pointer;
  }
  .menu button svg {
    color: var(--text-3);
  }
  .menu button:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .menu kbd {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .sep {
    height: 1px;
    margin: 4px 10px;
    background: rgba(255, 255, 255, 0.1);
  }
  .pop-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 4px 4px 12px;
  }
  .pop-head strong {
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  .popover .pop-x {
    all: unset;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-2);
    cursor: pointer;
  }
  .popover .pop-x:hover {
    background: rgba(255, 255, 255, 0.2);
    color: #fff;
  }
  .popover .pop-x:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  /* Liegt ausserhalb der Insel ueber beliebigen Fenstern: deutlich deckender als das Glas darin. */
  .popover.decor-pop {
    width: 230px;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0.03)), rgba(20, 20, 26, 0.94);
    display: flex;
    flex-direction: column;
    transform-origin: top center;
  }
  .pop-row {
    all: unset;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    padding: 4px 12px;
    border-radius: var(--r-md);
    color: var(--text);
    cursor: pointer;
  }
  .pop-row:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
  }
  .pop-row:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .pop-row:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .pop-row.more b {
    color: var(--accent);
  }
  .pop-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .pop-text b {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .pop-text small {
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .switch {
    position: relative;
    width: 34px;
    height: 20px;
    flex: none;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.16);
    transition: background var(--t-base) ease;
  }
  .switch i {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    transition: transform var(--t-base) var(--ease-spring);
  }
  .switch.on {
    background: var(--accent);
  }
  .switch.on i {
    transform: translateX(14px);
  }
  .library-pop {
    right: 14px;
    bottom: 84px;
    width: 360px;
    transform-origin: bottom right;
  }

  @keyframes fade {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: none; }
  }
  @keyframes rise {
    from { transform: translateY(96px) scale(0.94); }
    to { transform: none; }
  }
  @keyframes blink {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.35; transform: scale(0.8); }
  }
  @keyframes hop {
    0%, 100% { opacity: 0.35; transform: translateY(0); }
    50% { opacity: 1; transform: translateY(-2px); }
  }
  @media (prefers-reduced-motion: reduce) {
    .body, .bell, .dots i, .micdot { animation: none; }
    .shelf-area, .ambient::before, .ambient::after { animation: none; }
  }
</style>
