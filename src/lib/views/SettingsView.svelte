<script lang="ts">
  import { lens } from "$lib/lens";
  import { t } from "../i18n";
  // Einstellungen in der Insel. Der Gemini-Schluessel wird hier nur eingegeben und sofort an den
  // Kern uebergeben; er wird nie wieder angezeigt und liegt in der Windows-Anmeldeinformationsverwaltung.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import LocalAiSetup from "../LocalAiSetup.svelte";
  import { gb, localAi } from "../localAi.svelte";
  import { app, inTauri, niceDate, type Action, type ModelLimit, type Rating, type Settings, type VoiceInfo } from "../app.svelte";
  import RatingView from "../cards/RatingView.svelte";
  import ClaudeGuide from "./ClaudeGuide.svelte";
  import { tour } from "../tourState.svelte";
  import { SHELF_ICONS, SHELF_TABS, shelfOrder } from "../shelf";

  /** Nur diese Bereiche zeigen (Einstellungsfenster); leer = alle (Insel). */
  let { only = [] }: { only?: string[] } = $props();
  const show = (id: string) => only.length === 0 || only.includes(id);
  import { DECOR, drawDecor, type DecorKind } from "../character/decor";
  import { HOME_CARDS } from "./HomeView.svelte";
  import { flip } from "svelte/animate";
  import { layout, sizeInfo, SIZES, usedCells, type CardSize } from "../homeLayout";

  // ---------- Browser-Erweiterung: Stand regelmaessig nachsehen, solange die Einstellungen offen sind ----------

  let extStatus = $state<{ connected: boolean; browser: string; folder: string }>({ connected: false, browser: "", folder: "" });
  /** Schritt-fuer-Schritt-Anleitung je Browser: Adresse der Erweiterungsseite und wo die Schalter sitzen. */
  const EXT_BROWSERS = [
    {
      id: "chrome",
      label: "Chrome",
      page: "chrome://extensions",
      dev: t("Oben rechts auf der Seite steht „Entwicklermodus“ mit einem Schalter. Klick ihn an, sodass er blau wird.", "At the top right of the page there's \"Developer mode\" with a switch. Click it so it turns blue."),
      load: t("Jetzt erscheint oben links eine neue Leiste. Der erste Knopf darin heißt „Entpackte Erweiterung laden“.", "A new bar now appears at the top left. The first button in it is \"Load unpacked\"."),
    },
    {
      id: "edge",
      label: "Edge",
      page: "edge://extensions",
      dev: t("Links in der Seitenleiste, etwas weiter unten, steht „Entwicklermodus“ mit einem Schalter. Klick ihn an, sodass er blau wird. (Ist das Fenster schmal, findest du die Seitenleiste über die drei Striche oben links.)", "In the sidebar on the left, a bit further down, there's \"Developer mode\" with a switch. Click it so it turns blue. (If the window is narrow, open the sidebar with the three lines at the top left.)"),
      load: t("Oben in der Mitte erscheinen neue Knöpfe. Klick auf „Entpackt laden“.", "New buttons appear at the top center. Click \"Load unpacked\"."),
    },
    {
      id: "brave",
      label: "Brave",
      page: "brave://extensions",
      dev: t("Oben rechts auf der Seite steht „Entwicklermodus“ mit einem Schalter. Klick ihn an, sodass er eingeschaltet ist.", "At the top right of the page there's \"Developer mode\" with a switch. Click it so it's turned on."),
      load: t("Jetzt erscheint oben links eine neue Leiste. Der erste Knopf darin heißt „Entpackte Erweiterung laden“.", "A new bar now appears at the top left. The first button in it is \"Load unpacked\"."),
    },
  ] as const;
  let extBrowser = $state<string>("chrome");
  const extInfo = $derived(EXT_BROWSERS.find((b) => b.id === extBrowser) ?? EXT_BROWSERS[0]);
  /** Was gerade in die Zwischenablage kopiert wurde (fuer das kurze "Kopiert ✓"). */
  let copied = $state("");
  function copy(text: string, what: string) {
    navigator.clipboard.writeText(text).then(() => {
      copied = what;
      window.setTimeout(() => (copied = copied === what ? "" : copied), 1800);
    }).catch(() => {});
  }
  $effect(() => {
    if (!inTauri || !show("browserext")) return;
    const poll = () => invoke<typeof extStatus>("browser_extension_status").then((s) => (extStatus = s)).catch(() => {});
    poll();
    const timer = window.setInterval(poll, 3000);
    return () => clearInterval(timer);
  });

  // ---------- Leiste unten: Reihenfolge und Auswahl der Wuerfel ----------

  /** Reihenfolge waehrend des Ziehens (erst beim Loslassen gespeichert). */
  let shelfDragOrder = $state<string[] | null>(null);
  let shelfDrag = $state<{ id: string; dy: number } | null>(null);
  const shelfList = $derived(shelfDragOrder ?? shelfOrder(app.settings.shelfOrder ?? []));
  const shelfLabel = (id: string) => SHELF_TABS.find((s) => s.id === id)?.label ?? id;

  /** Wuerfel am Griff packen und senkrecht ziehen: die Zeile folgt dem Zeiger, die anderen
   *  ruecken nach, sobald die Mitte der Nachbarzeile ueberquert ist. */
  function shelfGrab(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    e.preventDefault();
    const row = (e.currentTarget as HTMLElement).closest("li");
    if (!row) return;
    const list = row.parentElement!;
    const gap = parseFloat(getComputedStyle(list).rowGap || "0") || 0;
    const step = row.getBoundingClientRect().height + gap;
    let order = [...shelfList];
    let origin = e.clientY;
    shelfDragOrder = order;
    shelfDrag = { id, dy: 0 };
    const move = (m: PointerEvent) => {
      let dy = m.clientY - origin;
      const from = order.indexOf(id);
      const to = Math.max(0, Math.min(order.length - 1, from + Math.round(dy / step)));
      if (to !== from) {
        order = order.filter((x) => x !== id);
        order.splice(to, 0, id);
        shelfDragOrder = order;
        // Die Zeile steht jetzt an ihrem neuen Platz: den Versatz entsprechend verkleinern.
        origin += (to - from) * step;
        dy = m.clientY - origin;
      }
      shelfDrag = { id, dy };
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      const changed = order.join() !== shelfOrder(app.settings.shelfOrder ?? []).join();
      if (changed) app.saveSettings({ shelfOrder: order });
      shelfDrag = null;
      shelfDragOrder = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }
  function shelfMove(id: string, by: number) {
    const list = [...shelfList];
    const i = list.indexOf(id), j = i + by;
    if (i < 0 || j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    app.saveSettings({ shelfOrder: list });
  }
  function shelfToggle(id: string, visible: boolean) {
    if (id === "chat") return;
    const hidden = app.settings.shelfHidden.filter((h) => h !== id);
    app.saveSettings({ shelfHidden: visible ? hidden : [...hidden, id] });
  }

  // ---------- Uebersicht gestalten: Schattenvorschau mit Ziehen, Groessen und + ----------

  let homeSel = $state<string | null>(null);
  let addOpen = $state(false);
  /** Reihenfolge waehrend des Ziehens (erst beim Loslassen gespeichert). */
  let dragOrder = $state<string[] | null>(null);
  let dragId = $state<string | null>(null);
  let justDragged = false;
  /** Groessen waehrend des Ziehens eingefroren: sonst wechseln Karten beim Tauschen ihre Groesse,
   *  das Raster springt, und der Zeiger landet wieder auf der eben getauschten Karte (Zittern). */
  let frozenSizes: Record<string, CardSize> | null = null;
  /** Schwebende Kopie der gezogenen Karte, folgt dem Zeiger. */
  let ghost = $state<{ x: number; y: number; w: number; h: number; id: string } | null>(null);
  const homeList = $derived(dragOrder ?? app.settings.homeCards);
  const homeItems = $derived(
    dragOrder && frozenSizes ? dragOrder.map((id) => ({ id, size: frozenSizes![id] ?? ("s" as CardSize) })) : layout(homeList, app.settings.homeSizes),
  );
  const homeRemaining = $derived(HOME_CARDS.filter((c) => !app.settings.homeCards.includes(c.id)));
  const homeOverflow = $derived(usedCells(homeItems) > 24);
  const cardLabel = (id: string) => HOME_CARDS.find((c) => c.id === id)?.label ?? id;

  function homeAdd(id: string) {
    app.saveSettings({ homeCards: [...app.settings.homeCards, id] });
    homeSel = id;
    if (homeRemaining.length <= 1) addOpen = false;
  }
  function homeRemove(id: string) {
    if (app.settings.homeCards.length <= 1) return;
    const sizes = { ...app.settings.homeSizes };
    delete sizes[id];
    app.saveSettings({ homeCards: app.settings.homeCards.filter((c) => c !== id), homeSizes: sizes });
    homeSel = null;
  }
  function homeSize(id: string, size: CardSize | null) {
    const sizes = { ...app.settings.homeSizes };
    if (size) sizes[id] = size;
    else delete sizes[id];
    app.saveSettings({ homeSizes: sizes });
  }
  function homeShift(id: string, by: number) {
    const list = [...app.settings.homeCards];
    const i = list.indexOf(id), j = i + by;
    if (i < 0 || j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    app.saveSettings({ homeCards: list });
  }

  /** Dauer der Ausweich-Animation; solange wird nicht erneut getauscht (sonst Pendeln). */
  const HOME_FLIP_MS = 220;

  /** Karte greifen: ab ein paar Pixeln schwebt eine Kopie unter dem Zeiger, die anderen weichen aus.
   *  Gegen Zittern: Groessen bleiben fest, nach jedem Tausch kurze Ruhe, und mit der gerade
   *  getauschten Karte wird erst wieder getauscht, wenn der Zeiger sie einmal verlassen hat. */
  function homeGrab(e: PointerEvent, id: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".hl-del")) return;
    const card = e.currentTarget as HTMLElement;
    const frame_el = card.closest<HTMLElement>(".hl-frame");
    const rect = card.getBoundingClientRect();
    const offset = { x: e.clientX - rect.left, y: e.clientY - rect.top };
    const start = { x: e.clientX, y: e.clientY };
    let moved = false;
    let lastSwap: string | null = null;
    let calmUntil = 0;
    let frame = 0;
    let retry = 0;
    let pointer = { x: e.clientX, y: e.clientY };

    const step = () => {
      frame = 0;
      if (!dragOrder) return;
      // Relativ zum Rahmen: ein Vorfahr mit Glas-Filter wuerde "position: fixed" verschieben.
      const box = frame_el?.getBoundingClientRect();
      ghost = { x: pointer.x - offset.x - (box?.left ?? 0), y: pointer.y - offset.y - (box?.top ?? 0), w: rect.width, h: rect.height, id };
      const wait = calmUntil - performance.now();
      if (wait > 0) {
        // Nach der Ruhezeit noch einmal pruefen, auch wenn der Zeiger stillsteht.
        if (!retry) retry = window.setTimeout(() => { retry = 0; if (!frame) frame = requestAnimationFrame(step); }, wait);
        return;
      }
      const over = (document.elementFromPoint(pointer.x, pointer.y) as HTMLElement | null)?.closest<HTMLElement>("[data-card]")?.dataset.card;
      if (!over || over === id) {
        lastSwap = null;
        return;
      }
      if (over === lastSwap) return;
      const from = dragOrder.indexOf(id);
      const to = dragOrder.indexOf(over);
      if (from < 0 || to < 0) return;
      const list = dragOrder.filter((c) => c !== id);
      list.splice(to, 0, id);
      dragOrder = list;
      lastSwap = over;
      calmUntil = performance.now() + HOME_FLIP_MS + 40;
    };
    const move = (m: PointerEvent) => {
      pointer = { x: m.clientX, y: m.clientY };
      if (!moved && Math.hypot(m.clientX - start.x, m.clientY - start.y) < 6) return;
      if (!moved) {
        moved = true;
        frozenSizes = Object.fromEntries(homeItems.map((item) => [item.id, item.size]));
        dragOrder = [...app.settings.homeCards];
        dragId = id;
        homeSel = null;
      }
      if (!frame) frame = requestAnimationFrame(step);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      if (frame) cancelAnimationFrame(frame);
      if (retry) clearTimeout(retry);
      // Losgelassen ueber einer Karte: dort ablegen, auch wenn gerade noch Ruhezeit war.
      if (moved && dragOrder) {
        calmUntil = 0;
        lastSwap = null;
        step();
      }
      if (moved && dragOrder) {
        app.saveSettings({ homeCards: dragOrder });
        justDragged = true;
        window.setTimeout(() => (justDragged = false), 0);
      }
      dragOrder = null;
      frozenSizes = null;
      dragId = null;
      ghost = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }

  const ACCENT_CHOICES = ["#64d2ff", "#30d158", "#ffd60a", "#ff9f0a", "#ff6961", "#bf5af2", "#f5b08a", "#f5f5f7"];

  function toggleDecor(kind: string) {
    const list = app.settings.decor.includes(kind) ? app.settings.decor.filter((d) => d !== kind) : [...app.settings.decor, kind];
    app.saveSettings({ decor: list });
  }

  /** Zeichnet ein Deko-Objekt als kleines Vorschaubild. */
  function decorPreview(canvas: HTMLCanvasElement, kind: string) {
    const g = canvas.getContext("2d");
    if (!g) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = 56 * dpr;
    canvas.height = 56 * dpr;
    g.scale(dpr, dpr);
    drawDecor(g, kind as DecorKind, 28, 50, 40, { clock: 0, hour: 20, mail: true, growth: 4, perk: 0 });
  }


  let key = $state("");
  let keyBusy = $state(false);
  let keyError = $state("");
  let sample = $state("");
  let sampleRating = $state<Rating | null>(null);
  let log = $state<Action[]>([]);
  let rateTimer = 0;
  let voice = $state<VoiceInfo | null>(null);
  let hotkeyError = $state("");
  let newGame = $state("");

  function addGame() {
    const name = newGame.trim().toLowerCase();
    if (!name) return;
    newGame = "";
    app.saveSettings({
      gamesAlways: [...app.settings.gamesAlways.filter((g) => g !== name), name],
      gamesNever: app.settings.gamesNever.filter((g) => g !== name),
    });
  }

  async function setHotkey(field: "hotkey" | "textHotkey" | "noteHotkey", value: string) {
    hotkeyError = "";
    try {
      await app.saveSettings({ [field]: value } as Partial<Settings>);
    } catch (err) {
      hotkeyError = String(err);
      app.settings = await (await import("@tauri-apps/api/core")).invoke("get_settings");
    }
  }

  /** Modelle, die der Schluessel kann (fuer die Auswahl). */
  let models = $state<string[]>([]);
  async function loadModels() {
    if (!inTauri || !app.cloud.hasKey) return;
    models = (await app.geminiModels().catch(() => [] as string[])).filter((m) => m.startsWith("gemini-")).sort();
  }
  /** Auswahl enthaelt immer das eingestellte Modell, auch wenn die Liste noch fehlt. */
  function choices(current: string): string[] {
    return models.includes(current) ? models : [current, ...models];
  }

  /** Grenzen eines Modells aendern; fehlt es in der Tabelle, wird es angelegt. */
  function setLimit(model: string, field: "rpm" | "tpm" | "rpd", value: number) {
    if (!Number.isFinite(value) || value < 1) return;
    const list: ModelLimit[] = app.settings.limits.map((l) => ({ ...l }));
    const entry = list.find((l) => l.model === model);
    if (entry) entry[field] = Math.round(value);
    else list.push({ model, rpm: 5, tpm: 250000, rpd: 20, [field]: Math.round(value) });
    app.saveSettings({ limits: list });
  }
  function limitOf(model: string): ModelLimit {
    return app.settings.limits.find((l) => l.model === model) ?? { model, rpm: 5, tpm: 250000, rpd: 20 };
  }
  const usedModels = $derived([...new Set([app.settings.modelEasy, app.settings.modelHard, app.settings.modelHard2])]);

  // ---------- Groq ----------
  let groqKey = $state("");
  let groqBusy = $state(false);
  let groqError = $state("");
  let groq = $state({ hasKey: false, verified: false, message: "" });
  let groqModels = $state<string[]>([]);

  async function loadGroq() {
    if (!inTauri) return;
    const { invoke } = await import("@tauri-apps/api/core");
    groq = await invoke("groq_status");
    if (groq.hasKey) groqModels = await invoke<string[]>("groq_models").catch(() => []);
  }

  async function saveGroqKey() {
    if (!groqKey.trim() || groqBusy) return;
    groqBusy = true;
    groqError = "";
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      groq = await invoke("set_groq_key", { key: groqKey });
      groqKey = "";
      app.settings = await invoke("get_settings");
      loadGroq();
    } catch (err) {
      groqError = String(err);
    } finally {
      groqBusy = false;
    }
  }

  async function removeGroqKey() {
    const { invoke } = await import("@tauri-apps/api/core");
    groq = await invoke("delete_groq_key");
    groqModels = [];
  }

  let claudeBusy = $state(false);
  let claudeError = $state("");
  async function connectClaude(enable: boolean) {
    claudeBusy = true;
    claudeError = "";
    try {
      await app.connectClaude(enable);
    } catch (err) {
      claudeError = String(err);
    } finally {
      claudeBusy = false;
    }
  }

  async function saveKey() {
    if (!key.trim() || keyBusy) return;
    keyBusy = true;
    keyError = "";
    try {
      await app.setCloudKey(key);
      key = "";
      loadModels();
    } catch (err) {
      keyError = String(err);
    } finally {
      // Das Eingabefeld behaelt den Schluessel nach einem Fehler, damit man ihn korrigieren kann.
      keyBusy = false;
      log = await app.cloudLog();
    }
  }

  async function removeKey() {
    keyError = "";
    await app.deleteCloudKey();
    log = await app.cloudLog();
  }

  // Bewertung beim Tippen, leicht verzoegert.
  function onSample() {
    clearTimeout(rateTimer);
    rateTimer = window.setTimeout(rateSample, 180);
  }
  async function rateSample() {
    sampleRating = sample.trim() && inTauri ? await app.rate(sample) : null;
  }
  // Aendert sich die Schwelle oder ein Schalter, stimmt die Probe-Bewertung sofort wieder.
  $effect(() => {
    void app.settings.cloudThreshold;
    void app.cloud.hasKey;
    rateSample();
  });

  // ---------- Lokale KI: Erkennung, Pruefung und Download stecken in LocalAiSetup ----------

  const localModels = $derived(localAi.models);
  function setMode(mode: string) {
    app.saveSettings({ aiMode: mode });
  }

  onMount(async () => {
    log = await app.cloudLog();
    if (inTauri) voice = await app.voiceInfo();
    loadModels();
    loadGroq();
  });
</script>

<div class="settings">
  {#if show("gemini")}
  <section id="s-gemini">
    <h2><span class="gem"></span>Gemini</h2>
    <p class="lead">
      {t(`${app.settings.name} denkt mit Google Gemini, über deinen eigenen kostenlosen Schlüssel. Alltagsbefehle wie Timer, Lautstärke oder Programme öffnen erledigt er sofort selbst, ganz ohne KI.`, `${app.settings.name} thinks with Google Gemini, using your own free API key. Everyday commands like timers, volume or opening apps are handled instantly, no AI needed.`)}
    </p>

    <div class="status" class:ok={app.cloud.hasKey}>
      <span class="lamp"></span>
      {#if app.cloud.hasKey}
        {app.cloud.verified ? t("Schlüssel hinterlegt und geprüft", "Key saved and verified") : t("Schlüssel hinterlegt", "Key saved")}
        <button class="link danger" onclick={removeKey}>{t("Löschen", "Remove")}</button>
      {:else}
        {t("Kein Schlüssel hinterlegt: nur Schnellbefehle", "No key yet: quick commands only")}
      {/if}
    </div>
    {#if app.cloud.message}<div class="hint">{app.cloud.message}</div>{/if}

    <form class="keyrow" onsubmit={(e) => { e.preventDefault(); saveKey(); }}>
      <input
        type="password" bind:value={key} placeholder={app.cloud.hasKey ? t("Neuen Schlüssel eintragen", "Enter a new key") : t("Gemini-API-Schlüssel", "Gemini API key")}
        autocomplete="off" spellcheck="false"
      />
      <button class="primary" type="submit" disabled={!key.trim() || keyBusy}>{keyBusy ? t("Prüfe …", "Checking …") : t("Speichern", "Save")}</button>
    </form>
    {#if keyError}<div class="hint error">{keyError}</div>{/if}
    <div class="hint">
      {t("Den Schlüssel bekommst du kostenlos bei", "Get a free key at")}
      <button class="link" onclick={() => inTauri && openUrl("https://aistudio.google.com/apikey")}>Google AI Studio</button>.
      {t("Er wird nur in der Windows-Anmeldeinformationsverwaltung gespeichert.", "It is stored only in Windows Credential Manager.")}
    </div>
    <div class="warn">
      {t(`An Google gehen deine Frage, der Gesprächsverlauf und was ${app.settings.name} dafür liest (z. B. eine Datei oder ein Bildschirmfoto, wenn du darum bittest). In der EU nutzt Google das auch im kostenlosen Kontingent nicht zum Trainieren und speichert es nur kurz zur Missbrauchserkennung. Passwörter, Kontonummern oder Gesundheitsdaten trotzdem nicht eingeben.`, `Google receives your question, the conversation so far and whatever ${app.settings.name} reads to answer it (for example a file or a screenshot, if you ask for one). In the EU, Google does not use this for training, even on the free tier, and keeps it only briefly for abuse detection. Still, don't enter passwords, account numbers or health data.`)}
    </div>

    <div class="row">
      <span>
        <strong>{t("Für leichte Anfragen", "For simple requests")}</strong>
        <small>{t("Schnell, großes Tageskontingent", "Fast, large daily quota")}</small>
      </span>
      <select value={app.settings.modelEasy} onchange={(e) => app.saveSettings({ modelEasy: e.currentTarget.value })}>
        {#each choices(app.settings.modelEasy) as m}<option value={m}>{m}</option>{/each}
      </select>
    </div>
    <div class="row">
      <span>
        <strong>{t("Für schwere Anfragen", "For hard requests")}</strong>
        <small>{t("Klüger, kleines Tageskontingent", "Smarter, small daily quota")}</small>
      </span>
      <select value={app.settings.modelHard} onchange={(e) => app.saveSettings({ modelHard: e.currentTarget.value })}>
        {#each choices(app.settings.modelHard) as m}<option value={m}>{m}</option>{/each}
      </select>
    </div>
    <div class="row">
      <span>
        <strong>{t("Zweites Modell für schwere", "Backup model for hard requests")}</strong>
        <small>{t("Hat ein eigenes Kontingent; springt ein, wenn das erste voll ist", "Has its own quota and steps in when the first one runs out")}</small>
      </span>
      <select value={app.settings.modelHard2} onchange={(e) => app.saveSettings({ modelHard2: e.currentTarget.value })}>
        {#each choices(app.settings.modelHard2) as m}<option value={m}>{m}</option>{/each}
      </select>
    </div>

    <div class="limits">
      <div class="lhead"><span>{t("Kostenloses Kontingent", "Free quota")}</span><span>{t("pro Minute", "per minute")}</span><span>{t("Token/Min", "tokens/min")}</span><span>{t("pro Tag", "per day")}</span></div>
      {#each usedModels as model (model)}
        {@const limit = limitOf(model)}
        <div class="lrow">
          <span class="lmodel" title={model}>{model}</span>
          <input type="number" min="1" value={limit.rpm} onchange={(e) => setLimit(model, "rpm", Number(e.currentTarget.value))} />
          <input type="number" min="1000" step="1000" value={limit.tpm} onchange={(e) => setLimit(model, "tpm", Number(e.currentTarget.value))} />
          <input type="number" min="1" value={limit.rpd} onchange={(e) => setLimit(model, "rpd", Number(e.currentTarget.value))} />
        </div>
      {/each}
      <div class="hint">{t(`Werte aus AI Studio („Rate limits“). ${app.settings.name} zählt mit und nimmt rechtzeitig das nächste Modell, statt gegen die Grenze zu laufen.`, `Values from AI Studio ("Rate limits"). ${app.settings.name} keeps count and switches to the next model in time instead of hitting the limit.`)}</div>
    </div>

    <label class="row">
      <span>
        <strong>{t("Vor jedem Senden fragen", "Ask before sending")}</strong>
        <small>{t("Zeigt vor jeder Anfrage an Gemini, was gesendet würde", "Shows what would be sent before every request to Gemini")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.confirmCloud} onchange={(e) => app.saveSettings({ confirmCloud: e.currentTarget.checked })} />
    </label>
  </section>
  {/if}

  {#if show("groq")}
  <section id="s-groq">
    <h2><span class="gem groq"></span>Groq</h2>
    <p class="lead">
      {t("Groq ist ein zweiter Anbieter mit sehr schnellen offenen Modellen. Leichte Fragen beantwortet dann Groq (das erste Wort kommt deutlich schneller), bei schweren Aufgaben denkt Groq in Sekunden vor und Gemini arbeitet es gründlich aus. Fällt einer aus, springt der andere ein.", "Groq is a second provider with very fast open models. Simple questions are then answered by Groq (the first word arrives much sooner); for hard tasks Groq drafts a plan in seconds and Gemini works it out thoroughly. If one of them fails, the other takes over.")}
    </p>
    <div class="status" class:ok={groq.hasKey}>
      <span class="lamp"></span>
      {#if groq.hasKey}
        {groq.verified ? t("Schlüssel hinterlegt und geprüft", "Key saved and verified") : t("Schlüssel hinterlegt", "Key saved")}
        <button class="link danger" onclick={removeGroqKey}>{t("Löschen", "Remove")}</button>
      {:else}
        {t("Kein Groq-Schlüssel: alles läuft über Gemini", "No Groq key: everything runs through Gemini")}
      {/if}
    </div>
    {#if groq.message}<div class="hint">{groq.message}</div>{/if}
    <form class="keyrow" onsubmit={(e) => { e.preventDefault(); saveGroqKey(); }}>
      <input type="password" bind:value={groqKey} placeholder={groq.hasKey ? t("Neuen Schlüssel eintragen", "Enter a new key") : t("Groq-API-Schlüssel", "Groq API key")} autocomplete="off" spellcheck="false" />
      <button class="primary" type="submit" disabled={!groqKey.trim() || groqBusy}>{groqBusy ? t("Prüfe …", "Checking …") : t("Speichern", "Save")}</button>
    </form>
    {#if groqError}<div class="hint error">{groqError}</div>{/if}
    <div class="hint">
      {t("Den Schlüssel bekommst du kostenlos bei", "Get a free key at")}
      <button class="link" onclick={() => inTauri && openUrl("https://console.groq.com/keys")}>console.groq.com</button>.
      {t("Er wird nur in der Windows-Anmeldeinformationsverwaltung gespeichert. Das Gratis-Kontingent ist klein (je nach Modell etwa 30 Anfragen pro Minute und wenige tausend Token pro Minute); ist es voll, übernimmt Gemini.", "It is stored only in Windows Credential Manager. The free quota is small (depending on the model, about 30 requests and a few thousand tokens per minute); once it is used up, Gemini takes over.")}
    </div>
    <div class="warn">
      {t(`An Groq (USA) gehen deine Frage, der Gesprächsverlauf und Werkzeugergebnisse der Anfrage. Beim ersten Mal fragt ${app.settings.name} nach deiner Zustimmung.`, `Groq (USA) receives your question, the conversation so far and the tool results for the request. ${app.settings.name} asks for your consent the first time.`)}
    </div>
    <label class="row">
      <span>
        <strong>{t("Groq mitbenutzen", "Use Groq too")}</strong>
        <small>{t("Aus: alles läuft wie bisher nur über Gemini", "Off: everything runs through Gemini only")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.groqEnabled} onchange={(e) => app.saveSettings({ groqEnabled: e.currentTarget.checked })} />
    </label>
    <div class="row">
      <span>
        <strong>{t("Groq-Modell", "Groq model")}</strong>
        <small>{t("Für schnelle Antworten und Vorüberlegungen", "For quick answers and first drafts")}</small>
      </span>
      <select value={app.settings.groqModel} onchange={(e) => app.saveSettings({ groqModel: e.currentTarget.value })}>
        {#each groqModels.includes(app.settings.groqModel) ? groqModels : [app.settings.groqModel, ...groqModels] as m}<option value={m}>{m}</option>{/each}
      </select>
    </div>
  </section>
  {/if}

  {#if show("local")}
  <section id="s-local">
    <h2><span class="gem local"></span>{t("Lokale KI", "Local AI")}</h2>
    <p class="lead">
      {t(`Eine KI direkt auf deinem PC, zum Beispiel mit Ollama oder LM Studio. Dann verlässt nichts deinen Rechner. Du kannst sie allein nutzen oder mit Gemini mischen: leichte Fragen beantwortet sie lokal, schwere gehen in die Cloud. ${app.settings.name} erkennt einen laufenden Server von selbst.`, `An AI right on your PC, for example with Ollama or LM Studio. Then nothing leaves your computer. Use it on its own or mix it with Gemini: simple questions are answered locally, hard ones go to the cloud. ${app.settings.name} detects a running server by itself.`)}
    </p>

    <div class="row">
      <span>
        <strong>{t("Wo denkt ", "Where ")}{app.settings.name}{t("?", " thinks")}</strong>
        <small>
          {#if app.settings.aiMode === "local"}{t("Nur auf diesem PC. Nichts geht ins Internet (außer Wetter und Recherche, wenn du sie nutzt).", "Only on this PC. Nothing goes to the internet (except weather and research when you use them).")}
          {:else if app.settings.aiMode === "mixed"}{t("Leichte Fragen lokal, schwere mit Gemini. Fällt eins aus, springt das andere ein.", "Simple questions locally, hard ones with Gemini. If one fails, the other steps in.")}
          {:else}{t("Gemini und Groq in der Cloud.", "Gemini and Groq in the cloud.")}{/if}
        </small>
      </span>
      <div class="seg" use:lens={{ selector: "button.on" }}><span class="lens-thumb pick" aria-hidden="true"></span>
        <button class:on={app.settings.aiMode === "cloud"} onclick={() => setMode("cloud")}>{t("Cloud", "Cloud")}</button>
        <button class:on={app.settings.aiMode === "mixed"} onclick={() => setMode("mixed")}>{t("Gemischt", "Mixed")}</button>
        <button class:on={app.settings.aiMode === "local"} onclick={() => setMode("local")}>{t("Nur lokal", "Local only")}</button>
      </div>
    </div>
    {#if app.settings.aiMode !== "cloud" && !app.settings.localModel}
      <div class="warn">{t("Noch kein lokales Modell gewählt. Prüf unten deinen Rechner, dann schlägt dir Milo ein passendes vor.", "No local model chosen yet. Check your computer below and Milo suggests a fitting one.")}</div>
    {/if}

    <div class="row">
      <span>
        <strong>{t("Lokales Modell", "Local model")}</strong>
        <small>{localAi.status?.server ? t("Aus den Modellen, die auf deinem PC liegen", "From the models on your PC") : t("Erscheint, sobald eine lokale KI läuft", "Appears once a local AI is running")}</small>
      </span>
      <select value={app.settings.localModel} disabled={!localAi.status?.server && !app.settings.localModel} onchange={(e) => app.saveSettings({ localModel: e.currentTarget.value })}>
        {#if !app.settings.localModel}<option value="">{t("– keins –", "– none –")}</option>{/if}
        {#each localModels.includes(app.settings.localModel) || !app.settings.localModel ? localModels : [app.settings.localModel, ...localModels] as m}
          {@const info = localAi.status?.models.find((x) => x.name === m)}
          <option value={m}>{m}{info?.size ? ` · ${gb(info.size)} GB` : ""}</option>
        {/each}
      </select>
    </div>

    <LocalAiSetup />
  </section>
  {/if}

  {#if show("rating")}
  <section id="s-rating">
    <h2>{t("Bewertung", "Rating")}</h2>
    <p class="lead">
      {t(`Jede Anfrage bekommt eine Wertung von 0 bis 100. Darunter antwortet das schnelle Modell, ab der Schwelle das klügere. Berichte mit Recherche erkennt ${app.settings.name} von selbst.`, `Every request gets a score from 0 to 100. Below the threshold the fast model answers, above it the smarter one. ${app.settings.name} recognizes research reports on its own.`)}
    </p>
    <div class="row">
      <span>
        <strong>{t("Schwelle", "Threshold")}: {app.settings.cloudThreshold}</strong>
        <small>{t("Niedriger = öfter das kluge Modell (verbraucht sein Kontingent schneller)", "Lower = the smart model more often (uses up its quota faster)")}</small>
      </span>
      <input
        class="slider" type="range" min="20" max="95" step="5" value={app.settings.cloudThreshold}
        oninput={(e) => (app.settings.cloudThreshold = Number(e.currentTarget.value))}
        onchange={(e) => app.saveSettings({ cloudThreshold: Number(e.currentTarget.value) })}
      />
    </div>
    <label class="row">
      <span>
        <strong>{t("Bewertung bei jeder Antwort zeigen", "Show the score with every answer")}</strong>
        <small>{t("Sonst erscheint sie nur bei schweren Anfragen und Berichten", "Otherwise it only appears for hard requests and reports")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.showRating} onchange={(e) => app.saveSettings({ showRating: e.currentTarget.checked })} />
    </label>
    <input class="sample" bind:value={sample} oninput={onSample} placeholder={t("Zum Ausprobieren: eine Anfrage eintippen", "Try it: type a request")} spellcheck="false" />
    {#if sampleRating}
      <RatingView rating={sampleRating} open />
    {/if}
  </section>
  {/if}

  {#if show("daily")}
  <section id="s-daily">
    <h2>{t("Tagesablauf", "Daily routine")}</h2>
    <label class="row">
      <span>
        <strong>{t("Morgen-Briefing", "Morning briefing")}</strong>
        <small>{t("Beim ersten Öffnen am Tag: Datum, Wetter, Timer und was du gestern notiert hast", "The first time you open it each day: date, weather, timers and what you noted yesterday")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.briefing} onchange={(e) => app.saveSettings({ briefing: e.currentTarget.checked })} />
    </label>
    <label class="row">
      <span>
        <strong>{t("Abends nach dem Tag fragen", "Ask about your day in the evening")}</strong>
        <small>{t("Deine Antwort wird als Notiz gespeichert und morgens erwähnt; „Feierabend“ startet es jederzeit", "Your answer is saved as a note and mentioned the next morning; say \"done for today\" to start it any time")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.eveningReview} onchange={(e) => app.saveSettings({ eveningReview: e.currentTarget.checked })} />
    </label>
    {#if app.settings.eveningReview}
      <div class="row">
        <span>
          <strong>{t("Ab wann", "Starting at")}</strong>
          <small>{t("Beim ersten Öffnen nach dieser Uhrzeit", "The first time you open it after this hour")}</small>
        </span>
        <select value={app.settings.eveningHour} onchange={(e) => app.saveSettings({ eveningHour: Number(e.currentTarget.value) })}>
          {#each [17, 18, 19, 20, 21, 22, 23] as hour}<option value={hour}>{t(`${hour} Uhr`, `${hour}:00`)}</option>{/each}
        </select>
      </div>
    {/if}
  </section>
  {/if}

  {#if show("textwork")}
  <section id="s-textwork">
    <h2>{t("Text-Werkzeuge", "Text tools")}</h2>
    <p class="lead">
      {t(`Markiere in einem beliebigen Programm Text und drück die Tastenkombination: ${app.settings.name} korrigiert, formuliert um, kürzt oder übersetzt ihn und setzt das Ergebnis an derselben Stelle ein. Ohne Markierung nimmt er die Zwischenablage.`, `Select text in any app and press the shortcut: ${app.settings.name} fixes, rewrites, shortens or translates it and puts the result right back in place. With nothing selected, it uses the clipboard.`)}
    </p>
    <div class="row">
      <span>
        <strong>{t("Tastenkombination", "Shortcut")}</strong>
        <small>{t("Öffnet das Menü der Text-Werkzeuge", "Opens the text tools menu")}</small>
      </span>
      <select value={app.settings.textHotkey} onchange={(e) => setHotkey("textHotkey", e.currentTarget.value)}>
        <option value="Ctrl+Alt+T">{t("Strg + Alt + T", "Ctrl + Alt + T")}</option>
        <option value="Ctrl+Alt+R">{t("Strg + Alt + R", "Ctrl + Alt + R")}</option>
        <option value="Ctrl+Shift+Y">{t("Strg + Umschalt + Y", "Ctrl + Shift + Y")}</option>
        <option value="F8">F8</option>
        <option value="">{t("keine", "none")}</option>
      </select>
    </div>
  </section>
  {/if}

  {#if show("claude")}
  <section id="s-claude">
    <h2>Claude Code</h2>
    <div class="status" class:ok={app.claude.connected}>
      <span class="lamp"></span>
      {app.claude.connected ? t("Verbunden: Live-Ansicht, Limits und Freigaben im Claude-Tab", "Connected: live view, limits and approvals in the Claude tab") : t("Nicht verbunden", "Not connected")}
      <button class="link" class:danger={app.claude.connected} disabled={claudeBusy} onclick={() => connectClaude(!app.claude.connected)}>
        {app.claude.connected ? t("Trennen", "Disconnect") : t("Verbinden", "Connect")}
      </button>
    </div>
    {#if claudeError}<div class="hint error">{claudeError}</div>{/if}
    <ClaudeGuide open={!app.claude.connected} />
    <label class="row">
      <span>
        <strong>{t("Ansagen", "Announcements")}</strong>
        <small>{t("„Claude ist fertig“ und „Claude braucht dich“ sagen", "Say \"Claude is done\" and \"Claude needs you\"")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.claudeVoice} onchange={(e) => app.saveSettings({ claudeVoice: e.currentTarget.checked })} />
    </label>
    <label class="row">
      <span>
        <strong>{t(`Freigaben in ${app.settings.name}`, `Approvals in ${app.settings.name}`)}</strong>
        <small>{t("Erlauben oder Ablehnen direkt in der Insel; ohne Antwort fragt Claude nach 25 Sekunden wie gewohnt selbst", "Allow or deny right in the island; without an answer, Claude asks on its own after 25 seconds as usual")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.claudeApprovals} onchange={(e) => app.saveSettings({ claudeApprovals: e.currentTarget.checked })} />
    </label>
  </section>
  {/if}

  {#if show("browserext")}
  <section id="s-browserext">
    <h2>{t("Browser-Erweiterung", "Browser extension")}</h2>
    <p class="lead">{t(`Mit der Erweiterung „Milo für den Browser“ sieht ${app.settings.name} Webseiten von innen: alle Titel und Knöpfe, auch unter großen Vorschauen wie bei Netflix, und klickt, ohne die Maus zu bewegen. Sie spricht nur mit ${app.settings.name} auf diesem PC. Ohne Erweiterung geht es wie bisher, nur weniger zuverlässig.`, `With the "Milo für den Browser" extension, ${app.settings.name} sees web pages from the inside: every title and button, even under big previews like on Netflix, and clicks without moving the mouse. It only talks to ${app.settings.name} on this PC. Without it things work as before, just less reliably.`)}</p>
    <div class="status" class:ok={extStatus.connected}>
      <span class="lamp"></span>
      {extStatus.connected ? t(`Verbunden mit ${extStatus.browser || "dem Browser"}`, `Connected to ${extStatus.browser || "the browser"}`) : t("Nicht verbunden", "Not connected")}
      <button class="link" onclick={() => inTauri && invoke("browser_extension_folder").catch(() => {})}>{t("Ordner öffnen", "Open folder")}</button>
    </div>
    {#if !extStatus.connected}
      <div class="ext-guide">
        <div class="ext-pick" role="tablist" aria-label={t("Dein Browser", "Your browser")}>
          <span>{t("Welchen Browser nutzt du?", "Which browser do you use?")}</span>
          {#each EXT_BROWSERS as b (b.id)}
            <button role="tab" aria-selected={extBrowser === b.id} class:on={extBrowser === b.id} onclick={() => (extBrowser = b.id)}>{b.label}</button>
          {/each}
        </div>
        <ol class="ext-steps">
          <li>
            <b>{t("Erweiterungsseite öffnen.", "Open the extensions page.")}</b>
            {t(`Öffne ${extInfo.label}. Klick oben in die Adresszeile (dort, wo sonst „google.de“ steht), füge die Adresse ein (Strg + V) und drück Enter:`, `Open ${extInfo.label}. Click into the address bar at the top (where you'd normally type a website), paste the address (Ctrl + V) and press Enter:`)}
            <span class="copy-row"><code>{extInfo.page}</code><button class="copy" onclick={() => copy(extInfo.page, "page")}>{copied === "page" ? t("Kopiert ✓", "Copied ✓") : t("Kopieren", "Copy")}</button></span>
          </li>
          <li>
            <b>{t("Entwicklermodus einschalten.", "Turn on developer mode.")}</b>
            {extInfo.dev}
          </li>
          <li>
            <b>{t("„Entpackte Erweiterung laden“ klicken.", "Click \"Load unpacked\".")}</b>
            {extInfo.load}
          </li>
          <li>
            <b>{t(`Den Milo-Ordner auswählen.`, `Select the Milo folder.`)}</b>
            {t("Es öffnet sich ein Fenster zur Ordnerauswahl. Klick dort oben in die Adresszeile, füge diesen Pfad ein (Strg + V), drück Enter und dann unten auf „Ordner auswählen“:", "A folder picker opens. Click into its address bar at the top, paste this path (Ctrl + V), press Enter and then click \"Select Folder\" at the bottom:")}
            <span class="copy-row"><code class="path">{extStatus.folder}</code><button class="copy" onclick={() => copy(extStatus.folder, "folder")}>{copied === "folder" ? t("Kopiert ✓", "Copied ✓") : t("Kopieren", "Copy")}</button></span>
            <span class="ext-note">{t("Wichtig: Der Ordner sieht in diesem Fenster leer aus. Das ist richtig so, denn der Browser zeigt beim Auswählen nur Ordner und keine Dateien. Die Erweiterung ist trotzdem drin. Einfach auf „Ordner auswählen“ klicken.", "Note: the folder looks empty in this window. That's expected, the browser only shows folders while picking, not files. The extension is in there. Just click \"Select Folder\".")}</span>
          </li>
          <li>
            <b>{t("Fertig!", "Done!")}</b>
            {t(`In der Liste erscheint „Milo für den Browser“. Nach ein paar Sekunden springt die Lampe hier oben auf grün und es steht „Verbunden“. ${app.settings.name} muss dafür laufen. Die Meldung des Browsers über Entwicklermodus-Erweiterungen kannst du einfach schließen.`, `"Milo für den Browser" appears in the list. After a few seconds the light up here turns green and says "Connected". ${app.settings.name} has to be running. You can simply close the browser's notice about developer mode extensions.`)}
          </li>
        </ol>
      </div>
    {/if}
  </section>
  {/if}

  {#if show("gamemode")}
  <section id="s-gamemode">
    <h2>{t("Spielemodus", "Game mode")}</h2>
    <p class="lead">
      {t(`Solange ein Spiel läuft, schläft ${app.settings.name} vollständig: Fenster geschlossen, Mikrofon zu. Es zählt, ob das Spiel läuft, nicht ob es gerade im Vordergrund ist. Über das Symbol in der Taskleiste kannst du ${app.settings.name} trotzdem wecken.`, `While a game is running, ${app.settings.name} sleeps completely: windows closed, microphone off. What counts is whether the game is running, not whether it is in the foreground. You can still wake ${app.settings.name} from its icon in the taskbar.`)}
    </p>
    <label class="row">
      <span>
        <strong>{t("Bei Spielen schlafen", "Sleep while gaming")}</strong>
        <small>{t("Erkennt Spiele von Steam, Epic, Riot, Xbox, EA, Ubisoft, GOG, Battle.net, VR und Minecraft (auch mit Mods). Dazu automatisch jedes Programm, das im Vollbild läuft oder sehr viel Arbeitsspeicher braucht – außer Browsern, Videoplayern und Arbeitsprogrammen. Liegt sie mal falsch: im Taskleisten-Symbol „… ist kein Spiel“ wählen.", "Detects games from Steam, Epic, Riot, Xbox, EA, Ubisoft, GOG, Battle.net, VR and Minecraft (modded too). Plus, automatically, any program running full screen or using a lot of memory – except browsers, video players and work apps. If it gets one wrong, choose “… isn't a game” from the taskbar icon.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.gameMode} onchange={(e) => app.saveSettings({ gameMode: e.currentTarget.checked })} />
    </label>
    <form class="keyrow" onsubmit={(e) => { e.preventDefault(); addGame(); }}>
      <input class="sample" bind:value={newGame} placeholder={t("Weiteres Programm, z. B. blender.exe", "Another program, e.g. blender.exe")} spellcheck="false" />
      <button class="primary" type="submit" disabled={!newGame.trim()}>{t("Hinzufügen", "Add")}</button>
    </form>
    {#if app.settings.gamesAlways.length + app.settings.gamesNever.length > 0}
      <div class="chips">
        {#each app.settings.gamesAlways as name}
          <button class="chip" title={t("Entfernen", "Remove")} onclick={() => app.saveSettings({ gamesAlways: app.settings.gamesAlways.filter((g) => g !== name) })}>
            {name} · {t("gilt als Spiel", "counts as a game")} ✕
          </button>
        {/each}
        {#each app.settings.gamesNever as name}
          <button class="chip off" title={t("Entfernen", "Remove")} onclick={() => app.saveSettings({ gamesNever: app.settings.gamesNever.filter((g) => g !== name) })}>
            {name} · {t("kein Spiel", "not a game")} ✕
          </button>
        {/each}
      </div>
    {/if}
  </section>
  {/if}

  {#if show("voice")}
  <section id="s-voice">
    <h2>{t("Sprache", "Voice")}</h2>
    <p class="lead">
      {t("Die Spracherkennung läuft vollständig auf deinem PC; es wird nichts aufgezeichnet und nichts verschickt. Solange das Mikrofon offen ist, zeigt die Insel einen Punkt (orange = wartet auf das Aktivierungswort, rot = nimmt auf).", "Speech recognition runs entirely on your PC; nothing is recorded or sent anywhere. While the microphone is on, the island shows a dot (orange = waiting for the wake word, red = recording).")}
    </p>
    {#if voice && !voice.parakeet && !voice.whisper}
      <div class="warn">{t("Es ist kein Spracherkennungs-Modell vorhanden. Erwartet wird es in:", "No speech recognition model found. Expected location:")} {voice.folder}</div>
    {/if}
    <div class="row">
      <span>
        <strong>{t("Tastenkombination", "Shortcut")}</strong>
        <small>{t("Startet das Zuhören; nochmal drücken beendet die Aufnahme sofort", "Starts listening; press again to stop recording right away")}</small>
      </span>
      <select value={app.settings.hotkey} onchange={(e) => setHotkey("hotkey", e.currentTarget.value)}>
        <option value="Ctrl+Alt+K">{t("Strg + Alt + K", "Ctrl + Alt + K")}</option>
        <option value="Ctrl+Shift+Space">{t("Strg + Umschalt + Leertaste", "Ctrl + Shift + Space")}</option>
        <option value="Ctrl+Alt+Space">{t("Strg + Alt + Leertaste", "Ctrl + Alt + Space")}</option>
        <option value="Alt+K">Alt + K</option>
        <option value="F9">F9</option>
        <option value="">{t("keine", "none")}</option>
      </select>
    </div>
    {#if hotkeyError}<div class="hint error">{hotkeyError}</div>{/if}
    <label class="row">
      <span>
        <strong>{t(`Auf „Hey ${app.settings.name}“ hören`, `Listen for "Hey ${app.settings.name}"`)}</strong>
        <small>
          {#if voice && !voice.wake}{t("Das Modell dafür ist noch nicht vorhanden.", "The model for this isn't installed yet.")}{:else}{t("Das Mikrofon bleibt dafür dauerhaft offen; ein winziges Modell sucht nur nach dem Aktivierungswort", "The microphone stays on for this; a tiny model listens only for the wake word")}{/if}
        </small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.wakeWord} disabled={voice !== null && !voice.wake} onchange={(e) => app.saveSettings({ wakeWord: e.currentTarget.checked })} />
    </label>
    {#if app.settings.wakeWord}
      <div class="row">
        <span>
          <strong>{t("Empfindlichkeit", "Sensitivity")}</strong>
          <small>{t("Streng löst seltener versehentlich aus, überhört dich aber eher", "Strict triggers by accident less often but may miss you more")}</small>
        </span>
        <select value={app.settings.wakeLevel} onchange={(e) => app.saveSettings({ wakeLevel: Number(e.currentTarget.value) })}>
          <option value={1}>{t("streng", "strict")}</option>
          <option value={2}>{t("normal", "normal")}</option>
          <option value={3}>{t("empfindlich", "sensitive")}</option>
        </select>
      </div>
    {/if}
    <div class="row">
      <span>
        <strong>{t("Erkenner", "Recognizer")}</strong>
        <small>{t("Parakeet antwortet fast sofort; Whisper ist genauer, braucht aber rund 1,5 Sekunden und mehr Arbeitsspeicher", "Parakeet answers almost instantly; Whisper is more accurate but takes about 1.5 seconds and more memory")}</small>
      </span>
      <select value={app.settings.sttModel} onchange={(e) => app.saveSettings({ sttModel: e.currentTarget.value as "parakeet" | "whisper" })}>
        <option value="parakeet" disabled={voice !== null && !voice.parakeet}>{t("Parakeet (schnell)", "Parakeet (fast)")}</option>
        <option value="whisper" disabled={voice !== null && !voice.whisper}>{t("Whisper (genauer)", "Whisper (more accurate)")}</option>
      </select>
    </div>
    <div class="row">
      <span>
        <strong>{t("Pause bis zum Ende", "Pause before stopping")}</strong>
        <small>{t(`So lange darfst du beim Sprechen nachdenken, bevor ${app.settings.name} die Aufnahme beendet`, `How long you can pause while speaking before ${app.settings.name} stops recording`)}</small>
      </span>
      <select value={app.settings.speechPause} onchange={(e) => app.saveSettings({ speechPause: Number(e.currentTarget.value) })}>
        <option value={1}>{t("kurz (1,2 s)", "short (1.2 s)")}</option>
        <option value={2}>{t("normal (1,8 s)", "normal (1.8 s)")}</option>
        <option value={3}>{t("lang (2,6 s)", "long (2.6 s)")}</option>
      </select>
    </div>
    <label class="row">
      <span>
        <strong>{t("Antworten vorlesen", "Read answers aloud")}</strong>
        <small>{t("Nur bei gesprochenen Anfragen, mit der in Windows eingebauten Stimme", "Only for spoken requests, using the voice built into Windows")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.speakAnswers} onchange={(e) => app.saveSettings({ speakAnswers: e.currentTarget.checked })} />
    </label>
  </section>
  {/if}

  {#if show("cloudlog")}
  <section id="s-cloudlog">
    <h2>{t("Was an Gemini ging", "What was sent to Gemini")}</h2>
    {#if log.length === 0}
      <p class="lead">{t("Bisher nichts.", "Nothing yet.")}</p>
    {:else}
      <div class="log">
        {#each log.slice(0, 30) as entry (entry.id)}
          <div class="entry" class:failed={!entry.ok}>
            <span class="when">{niceDate(entry.at)}</span>
            <span>{entry.summary}</span>
          </div>
        {/each}
      </div>
    {/if}
  </section>
  {/if}

  {#if show("pill")}
  <section id="s-pill">
    <h2>{t("Pille & Hinweise", "Pill & alerts")}</h2>
    <p class="lead">{t("Die Pille oben zeigt kurz, was gerade passiert, und wächst dafür ein Stück. Hier legst du fest, was sie melden darf.", "The pill at the top briefly shows what's happening and grows a little to do so. Choose here what it may tell you.")}</p>
    <div class="row">
      <span>
        <strong>{t("Verlauf der Zwischenablage", "Clipboard history")}</strong>
        <small>{t("Die letzten 20 kopierten Texte stehen in den Notizen unter „Zuletzt kopiert“. Nur im Arbeitsspeicher; Passwörter und Schlüssel werden nie aufgenommen.", "The last 20 copied texts appear in Notes under \"Recently copied\". Kept in memory only; passwords and keys are never recorded.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.clipboardHistory} onchange={(e) => app.saveSettings({ clipboardHistory: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Bildschirmfoto-Karte", "Screenshot card")}</strong>
        <small>{t("Nach Win + Umschalt + S zeigt die Pille die Vorschau mit Speichern, Milo fragen und Text lesen (Texterkennung von Windows, alles auf dem PC).", "After Win + Shift + S the pill shows a preview with Save, Ask Milo and Read text (Windows text recognition, all on this PC).")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.screenshotCard} onchange={(e) => app.saveSettings({ screenshotCard: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Download fertig", "Download finished")}</strong>
        <small>{t("Eine neue, fertige Datei im Downloads-Ordner erscheint kurz mit Öffnen und Ordner.", "A new, finished file in your Downloads folder shows up briefly with Open and Folder.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.downloadCard} onchange={(e) => app.saveSettings({ downloadCard: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Akku-Hinweis", "Battery alert")}</strong>
        <small>{t("Laptop-Akku oder Bluetooth-Geräte (Kopfhörer, Maus) unter 15 %.", "Laptop battery or Bluetooth devices (headphones, mouse) below 15 %.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.batteryAlert} onchange={(e) => app.saveSettings({ batteryAlert: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Mikrofon- und Kamera-Punkt", "Microphone and camera dot")}</strong>
        <small>{t("Orange: ein Programm benutzt das Mikrofon. Grün: die Kamera. Milo selbst zählt nicht.", "Orange: an app is using the microphone. Green: the camera. Milo itself doesn't count.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.privacyDots} onchange={(e) => app.saveSettings({ privacyDots: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Regenwarnung", "Rain alert")}</strong>
        <small>{t("Regnet es am Wetter-Ort in der nächsten Stunde, sagt die Pille kurz Bescheid.", "If it's going to rain at your weather place within the hour, the pill lets you know.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.rainAlert} onchange={(e) => app.saveSettings({ rainAlert: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Schnellnotiz", "Quick note")}</strong>
        <small>{t("Tastenkombination öffnet ein Eingabefeld direkt in der Pille; Enter speichert in die Notizen.", "The shortcut opens an input right in the pill; Enter saves it to your notes.")}</small>
      </span>
      <select value={app.settings.noteHotkey} onchange={(e) => setHotkey("noteHotkey", e.currentTarget.value)}>
        <option value="Ctrl+Alt+N">{t("Strg + Alt + N", "Ctrl + Alt + N")}</option>
        <option value="Ctrl+Shift+N">{t("Strg + Umschalt + N", "Ctrl + Shift + N")}</option>
        <option value="F9">F9</option>
        <option value="">{t("keine", "none")}</option>
      </select>
    </div>
    {#if hotkeyError}<div class="hint error">{hotkeyError}</div>{/if}
  </section>
  {/if}

  {#if show("general")}
  <section id="s-general">
    <h2>{t("Allgemein", "General")}</h2>
    <div class="row">
      <span>
        <strong>{t("Sprache", "Language")}</strong>
        <small>{t("Für die ganze App und Milos Antworten. Beim Wechsel laden die Fenster kurz neu.", "For the whole app and Milo's answers. The windows reload briefly when you switch.")}</small>
      </span>
      <select value={app.settings.language} onchange={(e) => app.saveSettings({ language: e.currentTarget.value })}>
        <option value="auto">{t("Wie Windows", "Same as Windows")}</option>
        <option value="de">Deutsch</option>
        <option value="en">English</option>
      </select>
    </div>
    <div class="row">
      <span>
        <strong>{t("Dein Name", "Your name")}</strong>
        <small>{t(`So schreibt ${app.settings.name} ihn richtig, z. B. beim Netflix-Profil`, `So ${app.settings.name} spells it right, e.g. for your Netflix profile`)}</small>
      </span>
      <input
        class="sample name" value={app.settings.userName} placeholder={t("z. B. Alex", "e.g. Alex")} spellcheck="false"
        onchange={(e) => app.saveSettings({ userName: e.currentTarget.value })}
      />
    </div>
    <div class="row">
      <span>
        <strong>{t("Termine in der Pille", "Events in the pill")}</strong>
        <small>{t("So lange vorher erscheint ein Termin mit Countdown oben in der Pille; bei Beginn klingelt es kurz. Rechtsklick darauf blendet ihn aus.", "How long before an event it shows up with a countdown in the pill; it chimes briefly when it starts. Right-click it to hide it.")}</small>
      </span>
      <select value={app.settings.eventLead} onchange={(e) => app.saveSettings({ eventLead: Number(e.currentTarget.value) })}>
        <option value={0}>{t("aus", "off")}</option>
        {#each [5, 10, 15, 30, 60, 120] as minutes}
          <option value={minutes}>{minutes < 60 ? t(`${minutes} Min vorher`, `${minutes} min before`) : t(`${minutes / 60} Std vorher`, `${minutes / 60} h before`)}</option>
        {/each}
      </select>
    </div>
    <div class="row">
      <span>
        <strong>{t("Uhrzeit in der Pille", "Clock in the pill")}</strong>
        <small>{t("Zeigt die Uhrzeit neben dem Wetter, wenn gerade nichts anderes los ist. Mausrad über der Pille ändert die Lautstärke.", "Shows the time next to the weather when nothing else is going on. Scrolling over the pill changes the volume.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.pillClock} onchange={(e) => app.saveSettings({ pillClock: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Zuklappen beim Klick daneben", "Close when clicking elsewhere")}</strong>
        <small>{t("Die offene Insel klappt zu, sobald du irgendwo anders hinklickst. Aus: sie bleibt offen, bis du den Pfeil oben rechts drückst oder Esc.", "The open island closes as soon as you click anywhere else. Off: it stays open until you press the arrow at the top right or Esc.")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.closeOnBlur !== false} onchange={(e) => app.saveSettings({ closeOnBlur: e.currentTarget.checked })} />
    </div>
    <div class="row">
      <span>
        <strong>{t("Lebhaftigkeit", "Liveliness")}</strong>
        <small>{t(`Wie oft ${app.settings.name} von selbst herumläuft, spielt oder Besuch bekommt`, `How often ${app.settings.name} wanders around, plays or gets visitors on its own`)}</small>
      </span>
      <select value={app.settings.liveliness} onchange={(e) => app.saveSettings({ liveliness: Number(e.currentTarget.value) })}>
        <option value={0}>{t("ruhig", "calm")}</option>
        <option value={1}>{t("normal", "normal")}</option>
        <option value={2}>{t("verspielt", "playful")}</option>
      </select>
    </div>
    <label class="row">
      <span>
        <strong>{t("Im Vollbild verstecken", "Hide in full screen")}</strong>
        <small>{t("Bei Filmen, Serien und Spielen im Vollbild zieht sich die Insel zurück; klingelnde Timer und Rückfragen zeigt sie trotzdem", "The island tucks away during full-screen movies, shows and games; it still shows ringing timers and questions")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.hideInFullscreen} onchange={(e) => app.saveSettings({ hideInFullscreen: e.currentTarget.checked })} />
    </label>
    <label class="row">
      <span>
        <strong>{t("Timer klingeln hörbar", "Timers ring out loud")}</strong>
        <small>{t("Sichtbar klingeln sie immer", "They always ring visually")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.timerSound} onchange={(e) => app.saveSettings({ timerSound: e.currentTarget.checked })} />
    </label>
    <label class="row">
      <span>
        <strong>{t("Ausflüge auf den Desktop", "Trips to the desktop")}</strong>
        <small>{t(`${app.settings.name} spaziert ab und zu auf die Taskleiste und kommt zurück, sobald du ihn brauchst`, `${app.settings.name} strolls onto the taskbar now and then and comes back as soon as you need it`)}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.petMode} onchange={(e) => app.saveSettings({ petMode: e.currentTarget.checked })} />
    </label>
    {#if app.settings.petMode}
      <div class="row">
        <span>
          <strong>{t("Wie oft", "How often")}</strong>
          <small>{t("Nur wenn gerade nichts los ist", "Only when nothing else is going on")}</small>
        </span>
        <select value={app.settings.petFrequency} onchange={(e) => app.saveSettings({ petFrequency: Number(e.currentTarget.value) })}>
          <option value={0}>{t("selten", "rarely")}</option>
          <option value={1}>{t("normal", "normal")}</option>
          <option value={2}>{t("oft", "often")}</option>
        </select>
      </div>
    {/if}
  </section>
  {/if}
  {#if show("look")}
  <section id="s-look">
    <h2>{t("Aussehen", "Appearance")}</h2>
    <div class="row">
      <span>
        <strong>{t("Akzentfarbe", "Accent color")}</strong>
        <small>{t("Für aktive Elemente, Fokusrahmen und eigene Termine", "For active elements, focus rings and your own events")}</small>
      </span>
      <div class="swatches">
        {#each ACCENT_CHOICES as color}
          <button class="swatch" class:on={app.settings.accent === color} style:background={color} onclick={() => app.saveSettings({ accent: color })} aria-label="{t('Akzentfarbe', 'Accent color')} {color}"></button>
        {/each}
      </div>
    </div>
    <div class="row">
      <span>
        <strong>{t("Größe der geöffneten Insel", "Size of the open island")}</strong>
        <small>{t("Breiter heißt flacher: mehr Platz nebeneinander, weniger nach unten", "Wider means shorter: more room side by side, less downward")}</small>
      </span>
      <div class="seg" use:lens={{ selector: "button.on" }}><span class="lens-thumb pick" aria-hidden="true"></span>
        {#each [t("Kompakt", "Compact"), t("Normal", "Normal"), t("Breit", "Wide")] as label, i}
          <button class:on={app.settings.islandSize === i} onclick={() => app.saveSettings({ islandSize: i })}>{label}</button>
        {/each}
      </div>
    </div>
    <div class="row">
      <span>
        <strong>{t("Glas", "Glass")}</strong>
        <small>{t("Wie stark Leiste, Menüs und Knöpfe den Inhalt dahinter durchscheinen lassen", "How much the bar, menus and buttons let the content behind them show through")}</small>
      </span>
      <div class="seg" use:lens={{ selector: "button.on" }}><span class="lens-thumb pick" aria-hidden="true"></span>
        {#each [t("Matt", "Frosted"), t("Normal", "Normal"), t("Klar", "Clear")] as label, i}
          <button class:on={app.settings.glass === i} onclick={() => app.saveSettings({ glass: i })}>{label}</button>
        {/each}
      </div>
    </div>
    <div class="row">
      <span>
        <strong>{t(`${app.settings.name}s Arme`, `${app.settings.name}'s arms`)}</strong>
        <small>{t("Kapsel-Arme, schwebende Kugelhände oder Federlinie", "Capsule arms, floating ball hands or a springy line")}</small>
      </span>
      <select value={app.settings.armStyle} onchange={(e) => app.saveSettings({ armStyle: e.currentTarget.value as Settings["armStyle"] })}>
        <option value="capsule">{t("Kapsel", "Capsule")}</option>
        <option value="hands">{t("Kugelhände", "Ball hands")}</option>
        <option value="spring">{t("Federlinie", "Spring line")}</option>
      </select>
    </div>
    <label class="row">
      <span>
        <strong>{t("Weniger Bewegung", "Reduce motion")}</strong>
        <small>{t("Kurze Überblendungen statt federnder Animationen", "Short fades instead of springy animations")}</small>
      </span>
      <input class="switch" type="checkbox" checked={app.settings.reduceMotion} onchange={(e) => app.saveSettings({ reduceMotion: e.currentTarget.checked })} />
    </label>
  </section>
  {/if}

  {#if show("home")}
  <section id="s-home">
    <h2>{t("Übersicht", "Home")}</h2>
    <p class="lead">{t(`So sieht deine Übersicht aus. Zieh Karten an einen anderen Platz, tipp eine an, um ihre Größe zu ändern, und füge mit + weitere hinzu. Ohne eigene Größe ordnet ${app.settings.name} die Karten je nach Anzahl selbst an.`, `This is what your home screen looks like. Drag cards to a new spot, tap one to change its size and add more with +. Without a size of their own, ${app.settings.name} arranges the cards based on how many there are.`)}</p>
    <div class="hl-frame" class:overflow={homeOverflow}>
      <div class="hl-grid">
        {#each homeItems as item (item.id)}
          {@const size = sizeInfo(item.size)}
          {@const card = HOME_CARDS.find((c) => c.id === item.id)}
          <div
            class="hl-slot" data-card={item.id}
            style:grid-column="span {size.cols}" style:grid-row="span {size.rows}" style:--c={card?.color}
            animate:flip={{ duration: HOME_FLIP_MS }}
          >
            <button
              class="hl-card" class:sel={homeSel === item.id} class:dragging={dragId === item.id}
              aria-pressed={homeSel === item.id}
              aria-label="{cardLabel(item.id)}, {size.label}. {t('Pfeiltasten verschieben, Entf entfernt.', 'Arrow keys move it, Delete removes it.')}"
              onpointerdown={(e) => homeGrab(e, item.id)}
              onclick={() => { if (!justDragged) homeSel = homeSel === item.id ? null : item.id; }}
              onkeydown={(e) => {
                if (e.key === "ArrowLeft" || e.key === "ArrowUp") { e.preventDefault(); homeShift(item.id, -1); }
                if (e.key === "ArrowRight" || e.key === "ArrowDown") { e.preventDefault(); homeShift(item.id, 1); }
                if (e.key === "Delete") homeRemove(item.id);
              }}
            >
              <span class="hl-name">{card?.label}</span>
              <span class="hl-size">{size.label}{app.settings.homeSizes[item.id] ? "" : t(" · automatisch", " · automatic")}</span>
            </button>
            {#if homeItems.length > 1 && !dragId}
              <button class="hl-del" onclick={() => homeRemove(item.id)} aria-label={t(`${cardLabel(item.id)} entfernen`, `Remove ${cardLabel(item.id)}`)} title={t("Entfernen", "Remove")}>
                <svg viewBox="0 0 16 16" width="9" height="9" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="2" stroke-linecap="round" /></svg>
              </button>
            {/if}
          </div>
        {/each}
        {#if homeRemaining.length}
          <button class="hl-add" class:on={addOpen} onclick={() => (addOpen = !addOpen)} aria-expanded={addOpen} aria-label={t("Karte hinzufügen", "Add card")}>
            <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
          </button>
        {/if}
      </div>
      {#if ghost}
        {@const gcard = HOME_CARDS.find((c) => c.id === ghost!.id)}
        <div class="hl-ghost" style:left="{ghost.x}px" style:top="{ghost.y}px" style:width="{ghost.w}px" style:height="{ghost.h}px" style:--c={gcard?.color} aria-hidden="true">
          <span class="hl-name">{gcard?.label}</span>
        </div>
      {/if}
    </div>
    {#if homeOverflow}
      <p class="hl-note">{t("Unter der gestrichelten Linie scrollt die Übersicht. Kleinere Karten passen alle auf einen Blick.", "Below the dashed line, the home screen scrolls. Smaller cards all fit at a glance.")}</p>
    {/if}
    {#if addOpen && homeRemaining.length}
      <div class="chips hl-pick">
        {#each homeRemaining as card (card.id)}
          <button class="chip" style:--c={card.color} onclick={() => homeAdd(card.id)}>+ {card.label}</button>
        {/each}
      </div>
    {/if}
    {#if homeSel && app.settings.homeCards.includes(homeSel)}
      {@const sel = homeSel}
      <div class="hl-edit">
        <div class="hl-edit-head">
          <strong>{cardLabel(sel)}</strong>
          <span class="hl-moves">
            <button onclick={() => homeShift(sel, -1)} aria-label={t("Nach vorn", "Move earlier")}>
              <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M10 3.5L5.5 8l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
            </button>
            <button onclick={() => homeShift(sel, 1)} aria-label={t("Nach hinten", "Move later")}>
              <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M6 3.5L10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
            </button>
            <button class="danger" disabled={app.settings.homeCards.length <= 1} onclick={() => homeRemove(sel)}>{t("Entfernen", "Remove")}</button>
            <button class="hl-x" onclick={() => (homeSel = null)} aria-label={t("Auswahl schließen", "Close selection")}>
              <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
            </button>
          </span>
        </div>
        <div class="seg hl-sizes" use:lens={{ selector: "button.on" }}><span class="lens-thumb pick" aria-hidden="true"></span>
          <button class:on={!app.settings.homeSizes[sel]} onclick={() => homeSize(sel, null)}>{t("Automatisch", "Automatic")}</button>
          {#each SIZES as size (size.id)}
            <button class:on={app.settings.homeSizes[sel] === size.id} onclick={() => homeSize(sel, size.id)}>
              <i class="hl-glyph" style:--w={size.cols} style:--h={size.rows} aria-hidden="true"></i>{size.label}
            </button>
          {/each}
        </div>
      </div>
    {/if}
    <div class="row">
      <span>
        <strong>{t("Automatisch anordnen", "Arrange automatically")}</strong>
        <small>{t("Setzt alle eigenen Größen zurück; die Reihenfolge bleibt", "Resets all custom sizes; the order stays")}</small>
      </span>
      <button class="hl-reset" disabled={Object.keys(app.settings.homeSizes).length === 0} onclick={() => app.saveSettings({ homeSizes: {} })}>{t("Zurücksetzen", "Reset")}</button>
    </div>
  </section>
  {/if}

  {#if show("shelf")}
  <section id="s-shelf">
    <h2>{t("Leiste unten", "Bottom bar")}</h2>
    <p class="lead">{t("Welche Würfel in der Leiste unten in der Insel stehen und in welcher Reihenfolge. Zum Umsortieren am Griff links packen und ziehen. Ein ausgeblendeter Würfel taucht nur kurz auf, wenn er etwas Dringendes meldet, z. B. eine Freigabe für Claude.", "Which cubes sit in the bar at the bottom of the island, and in what order. Grab the handle on the left and drag to reorder. A hidden cube only shows up briefly when it has something urgent, e.g. an approval for Claude.")}</p>
    <ul class="shelf-list" class:sorting={shelfDrag}>
      {#each shelfList as id (id)}
        {@const hidden = app.settings.shelfHidden.includes(id)}
        {@const lifted = shelfDrag?.id === id}
        <li class:hidden class:lifted style:transform={lifted ? `translateY(${shelfDrag!.dy}px)` : undefined} animate:flip={{ duration: lifted ? 0 : 200 }}>
          <button
            class="shelf-grip"
            onpointerdown={(e) => shelfGrab(e, id)}
            onkeydown={(e) => {
              if (e.key === "ArrowUp") { e.preventDefault(); shelfMove(id, -1); }
              if (e.key === "ArrowDown") { e.preventDefault(); shelfMove(id, 1); }
            }}
            aria-label={t(`${shelfLabel(id)} verschieben (ziehen oder Pfeiltasten)`, `Move ${shelfLabel(id)} (drag or arrow keys)`)}
          >
            <svg viewBox="0 0 12 16" width="9" height="13" aria-hidden="true"><g fill="currentColor"><circle cx="3.5" cy="3" r="1.3" /><circle cx="8.5" cy="3" r="1.3" /><circle cx="3.5" cy="8" r="1.3" /><circle cx="8.5" cy="8" r="1.3" /><circle cx="3.5" cy="13" r="1.3" /><circle cx="8.5" cy="13" r="1.3" /></g></svg>
          </button>
          <span class="shelf-icon" aria-hidden="true">
            <svg viewBox="0 0 16 16" width="15" height="15"><path d={SHELF_ICONS[id]} fill="none" stroke="currentColor" stroke-width="1.45" stroke-linecap="round" stroke-linejoin="round" /></svg>
          </span>
          <span class="shelf-name">{shelfLabel(id)}{#if id === "chat"}<small>{t("immer da", "always there")}</small>{/if}</span>
          <input class="switch" type="checkbox" checked={!hidden} disabled={id === "chat"} onchange={(e) => shelfToggle(id, e.currentTarget.checked)} aria-label={t(`${shelfLabel(id)} anzeigen`, `Show ${shelfLabel(id)}`)} />
        </li>
      {/each}
    </ul>
    <div class="row">
      <span>
        <strong>{t("Standard wiederherstellen", "Restore default")}</strong>
        <small>{t("Alle Würfel in der ursprünglichen Reihenfolge", "All cubes in the original order")}</small>
      </span>
      <button class="hl-reset" disabled={app.settings.shelfOrder.length === 0 && app.settings.shelfHidden.length === 0} onclick={() => app.saveSettings({ shelfOrder: [], shelfHidden: [] })}>{t("Zurücksetzen", "Reset")}</button>
    </div>
  </section>
  {/if}

  {#if show("decor")}
  <section id="s-decor">
    <h2>{t("Deko in der Insel", "Decor in the island")}</h2>
    <p class="lead">{t(`Was in der geschlossenen Insel neben ${app.settings.name} steht. Tipp ein Objekt in der Insel an: ein kleines Menü sperrt es, legt es auf den Desktop oder nimmt es heraus. Auf dem Desktop fällt es auf die Taskleiste.`, `What stands next to ${app.settings.name} in the closed island. Tap an object in the island: a small menu locks it, puts it on the desktop or removes it. On the desktop it drops onto the taskbar.`)}</p>
    <div class="decor-grid">
      {#each DECOR as item (item.kind)}
        {@const on = app.settings.decor.includes(item.kind)}
        <button class="decor-item" class:on onclick={() => toggleDecor(item.kind)} aria-pressed={on}>
          <canvas use:decorPreview={item.kind} width="56" height="56" aria-hidden="true"></canvas>
          <span class="dname">{item.label}</span>
          <span class="dhint">{item.hint}</span>
          <span class="dstate">{on ? t("In der Insel", "In the island") : t("Aus", "Off")}</span>
        </button>
      {/each}
    </div>
    {#if app.settings.deskItems.length > 0}
      <div class="row">
        <span>
          <strong>{t("Auf dem Desktop", "On the desktop")}</strong>
          <small>{app.settings.deskItems.map((d) => DECOR.find((x) => x.kind === d.kind)?.label ?? d.kind).join(", ")}</small>
        </span>
        <button class="primary" onclick={() => app.saveSettings({ deskItems: [] })}>{t("Alle zurückholen", "Bring all back")}</button>
      </div>
    {/if}
  </section>
  {/if}

  {#if show("calendar")}
  <section id="s-calendar">
    <h2>{t("Kalender", "Calendar")}</h2>
    <p class="lead">
      {t(`Die Agenda zeigt eigene Termine und, wenn du willst, abonnierte Kalender. Bei Google Kalender findest du die Adresse unter Einstellungen › dein Kalender › „Privatadresse im iCal-Format“; Outlook nennt sie „ICS-Link veröffentlichen“. ${app.settings.name} liest die Kalender nur, höchstens alle zehn Minuten, und ändert nichts daran.`, `The agenda shows your own events and, if you like, subscribed calendars. In Google Calendar you'll find the address under Settings › your calendar › "Secret address in iCal format"; Outlook calls it "Publish ICS link". ${app.settings.name} only reads calendars, at most every ten minutes, and never changes them.`)}
    </p>
    <textarea
      class="ics" rows="3" spellcheck="false" placeholder={t("https://calendar.google.com/calendar/ical/…/basic.ics (eine Adresse pro Zeile)", "https://calendar.google.com/calendar/ical/…/basic.ics (one address per line)")}
      value={app.settings.agendaIcs}
      onchange={(e) => app.saveSettings({ agendaIcs: e.currentTarget.value })}
    ></textarea>
    <span class="hint">{t("Nur https- und webcal-Adressen. Die Adresse ist wie ein Passwort: wer sie hat, kann den Kalender lesen.", "Only https and webcal addresses. Treat the address like a password: anyone who has it can read the calendar.")}</span>
  </section>
  {/if}

  {#if show("about")}
  <section id="s-about">
    <h2>{t(`Über ${app.settings.name}`, `About ${app.settings.name}`)}</h2>
    <p class="lead">{t("Version 0.4.0 · Alles Persönliche (Notizen, Verlauf, Termine) bleibt auf diesem PC. An Gemini oder Groq geht nur die einzelne Anfrage, die du stellst.", "Version 0.4.0 · Everything personal (notes, history, events) stays on this PC. Gemini or Groq only receive the single request you make.")}</p>
    <div class="row">
      <span>
        <strong>{t("Willkommen erneut zeigen", "Show the welcome again")}</strong>
        <small>{t("Startet den Willkommens-Assistenten mit Sprache, Name, Schlüsseln und Tour noch einmal. Deine Schlüssel und Einstellungen bleiben erhalten.", "Runs the welcome wizard with language, name, keys and tour once more. Your keys and settings stay as they are.")}</small>
      </span>
      <button class="primary" onclick={() => { try { localStorage.removeItem("milo.welcomeStep"); } catch {} app.saveSettings({ onboarded: false }); }}>{t("Zeigen", "Show")}</button>
    </div>
    <div class="row">
      <span>
        <strong>{t("Tour erneut ansehen", "Watch the tour again")}</strong>
        <small>{t("Geht noch einmal mit Beispielen durch alle Bereiche der Insel.", "Walks through every area of the island once more, with examples.")}</small>
      </span>
      <button class="primary" onclick={() => { tour.start(); app.tab = "home"; }}>{t("Starten", "Start")}</button>
    </div>
  </section>
  {/if}
</div>

<style>
  .swatches {
    display: flex;
    gap: 6px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 0;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.15);
    transition: transform var(--t-base) var(--ease-spring);
  }
  .swatch.on {
    box-shadow: 0 0 0 2px #000, 0 0 0 4px currentColor, inset 0 0 0 1px rgba(255, 255, 255, 0.2);
    color: var(--text);
    transform: scale(1.08);
  }
  .seg {
    display: flex;
    padding: 2px;
    border-radius: var(--r-full);
    background: var(--well);
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.08), inset 0 1px 2px rgba(0, 0, 0, 0.35);
  }
  .seg button {
    position: relative;
    z-index: 1;
    height: 28px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-full);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
    transition: color var(--t-base) ease;
  }
  .seg button.on {
    color: #000;
    font-weight: var(--fw-semibold);
  }
  .decor-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
  }
  .decor-item {
    display: grid;
    grid-template-columns: 56px 1fr;
    grid-template-rows: auto auto auto;
    column-gap: 8px;
    align-items: center;
    padding: 8px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.03);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background var(--t-fast) ease, border-color var(--t-fast) ease, transform var(--t-slow) var(--ease-spring);
  }
  .decor-item canvas {
    grid-row: 1 / 4;
    width: 56px;
    height: 56px;
  }
  .decor-item:hover {
    background: rgba(255, 255, 255, 0.07);
  }
  .decor-item:active {
    transform: scale(0.97);
  }
  .decor-item.on {
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .dname {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .dhint {
    font-size: var(--fs-caption);
    color: var(--text-3);
    line-height: 1.3;
  }
  .dstate {
    font-size: var(--fs-caption);
    color: var(--text-4);
  }
  .decor-item.on .dstate {
    color: var(--accent);
  }
  .ics {
    resize: vertical;
    min-height: 64px;
    padding: 8px 12px;
    border-radius: var(--r-md);
    border: 0;
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.12), inset 0 1px 2px rgba(0, 0, 0, 0.4);
    background: var(--well);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--fs-footnote);
    outline: 0;
  }
  .ics:focus {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .settings {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 18px;
    scrollbar-width: thin;
    scrollbar-color: #2c2c2e transparent;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h2 {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-headline);
    font-weight: var(--fw-semibold);
    letter-spacing: var(--tr-headline);
  }
  .gem {
    width: 11px;
    height: 11px;
    border-radius: 3.5px;
    background: linear-gradient(135deg, #7fdcff, #b98cf5);
  }
  .gem.groq {
    background: linear-gradient(135deg, #ff9f5a, #f55036);
  }
  .gem.local {
    background: linear-gradient(135deg, #7ee8a2, #2fbf71);
  }
  .lead {
    margin: 0;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-3);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-body);
    color: var(--text-2);
  }
  .lamp {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #636366;
  }
  .status.ok .lamp {
    background: var(--ok);
  }
  .ext-steps {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-3);
  }
  .ext-steps code {
    padding: 1px 6px;
    border-radius: var(--r-xs);
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-2);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: var(--fs-caption);
  }
  .ext-steps .path {
    word-break: break-all;
  }
  .ext-guide {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .ext-pick {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .ext-pick span {
    margin-right: 4px;
  }
  .ext-pick button {
    height: 26px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.07);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .ext-pick button.on {
    background: var(--accent);
    color: #000;
    font-weight: var(--fw-semibold);
  }
  .ext-steps li b {
    display: block;
    color: var(--text);
    font-weight: var(--fw-semibold);
  }
  .ext-note {
    display: block;
    margin-top: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    background: rgba(255, 214, 10, 0.12);
    border: 1px solid rgba(255, 214, 10, 0.3);
    font-size: 12px;
    line-height: 1.4;
  }
  .copy-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }
  .copy-row code {
    flex: 1;
    min-width: 0;
    padding: 6px 8px;
  }
  .copy {
    flex: none;
    height: 26px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-sm);
    background: rgba(255, 255, 255, 0.1);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .copy:hover {
    background: rgba(255, 255, 255, 0.18);
  }
  .ext-pick button:focus-visible,
  .copy:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .keyrow {
    display: flex;
    gap: 8px;
  }
  input[type="password"],
  .sample,
  select {
    height: 32px;
    padding: 0 12px;
    box-sizing: border-box;
    border-radius: var(--r-lg);
    border: 0;
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.12), inset 0 1px 2px rgba(0, 0, 0, 0.4);
    outline: 0;
    background: var(--well);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    transition: box-shadow var(--t-fast) ease;
  }
  input[type="password"] {
    flex: 1;
    min-width: 0;
  }
  input[type="password"]:focus,
  .sample:focus {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  /* Auswahl: eine Kapsel mit eigenem Pfeil statt des Standard-Kaestchens. */
  select {
    appearance: none;
    padding: 0 var(--s-3);
    border-radius: var(--r-full);
    background-color: var(--chip);
    cursor: pointer;
  }
  select:hover {
    background-color: var(--chip-strong);
  }
  .primary {
    height: 32px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--r-lg);
    background: var(--text);
    color: #000;
    font: inherit;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    cursor: pointer;
    transition: opacity var(--t-fast) ease;
  }
  .primary:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .link {
    all: unset;
    color: var(--accent);
    cursor: pointer;
  }
  .link.danger {
    margin-left: auto;
    color: #ff8a80;
    font-size: var(--fs-footnote);
  }
  .hint {
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-3);
  }
  .hint.error {
    color: #ff8a80;
  }
  .warn {
    padding: 8px 12px;
    border-radius: var(--r-sm);
    background: #17140a;
    color: #d9c98a;
    font-size: var(--fs-footnote);
    line-height: 1.45;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 40px;
  }
  .row > span {
    display: flex;
    flex-direction: column;
    line-height: 1.35;
  }
  .row strong {
    font-size: var(--fs-body);
    font-weight: var(--fw-medium);
  }
  .row small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  label.row {
    cursor: pointer;
  }
  .switch {
    appearance: none;
    flex: none;
    width: 44px;
    height: 26px;
    margin: 0;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.16);
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.45), inset 0 0 0 0.5px rgba(255, 255, 255, 0.08);
    position: relative;
    cursor: pointer;
    transition: background var(--t-base) ease;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 22px;
    height: 22px;
    border-radius: var(--r-full);
    background: linear-gradient(180deg, #fff, #e9e9ee);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.35), 0 3px 8px -2px rgba(0, 0, 0, 0.4);
    transition: transform var(--t-slow) var(--ease-spring), width var(--t-base) var(--ease-out), left var(--t-base) var(--ease-out);
  }
  .switch:active::after {
    width: 27px;
  }
  .switch:checked:active::after {
    left: -3px;
  }
  .sample.name {
    width: 170px;
  }
  .keyrow .sample {
    flex: 1;
    min-width: 0;
  }
  /* Schattenvorschau der Uebersicht: dasselbe 12er-Raster wie die echte Startseite, verkleinert. */
  .hl-frame {
    position: relative;
    margin: 6px 0 4px;
    padding: 10px;
    border-radius: var(--r-lg);
    background: rgba(0, 0, 0, 0.28);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  /* Sichtbare Flaeche der Insel: zwei Reihen. Darunter wird gescrollt. */
  .hl-frame.overflow::after {
    content: "";
    position: absolute;
    left: 6px;
    right: 6px;
    top: calc(10px + 2 * 58px + 8px + 4px);
    border-top: 1.5px dashed rgba(255, 255, 255, 0.28);
    pointer-events: none;
  }
  .hl-grid {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    grid-auto-rows: 58px;
    grid-auto-flow: row;
    gap: 8px;
  }
  .hl-slot {
    position: relative;
    display: flex;
    min-width: 0;
  }
  .hl-slot > .hl-card {
    flex: 1;
  }
  .hl-del {
    position: absolute;
    top: 5px;
    right: 5px;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.45);
    color: var(--text-2);
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--t-fast) ease, background var(--t-fast) ease, color var(--t-fast) ease;
  }
  .hl-slot:hover .hl-del,
  .hl-slot:focus-within .hl-del,
  .hl-del:focus-visible {
    opacity: 1;
  }
  .hl-del:hover {
    background: #ff5f57;
    color: #fff;
  }
  .hl-del:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  @media (hover: none) {
    .hl-del {
      opacity: 1;
    }
  }
  .hl-ghost {
    position: absolute;
    z-index: 50;
    display: flex;
    align-items: flex-end;
    padding: 8px 10px;
    border-radius: var(--r-md);
    background: linear-gradient(160deg, color-mix(in srgb, var(--c) 45%, #1c1c22), color-mix(in srgb, var(--c) 22%, #1c1c22));
    box-shadow: inset 0 0 0 1.5px var(--c), 0 14px 30px rgba(0, 0, 0, 0.45);
    color: var(--text);
    pointer-events: none;
    transform: scale(1.03) rotate(-1deg);
    cursor: grabbing;
  }
  .hl-card {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: flex-start;
    gap: 1px;
    min-width: 0;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--r-md);
    background: linear-gradient(160deg, color-mix(in srgb, var(--c) 30%, transparent), color-mix(in srgb, var(--c) 10%, transparent));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 45%, transparent), inset 0 1px 0 rgba(255, 255, 255, 0.12);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: grab;
    touch-action: none;
    transition: box-shadow var(--t-fast) ease, opacity var(--t-fast) ease, transform var(--t-base) var(--ease-spring);
  }
  .hl-card:hover {
    transform: translateY(-1px);
  }
  .hl-card.sel {
    box-shadow: inset 0 0 0 2px var(--c), 0 0 0 3px color-mix(in srgb, var(--c) 30%, transparent);
  }
  /* Platzhalter, wo die gezogene Karte landen wird. */
  .hl-card.dragging {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--c) 60%, transparent);
    outline: 1.5px dashed color-mix(in srgb, var(--c) 70%, transparent);
    outline-offset: -4px;
    cursor: grabbing;
    transform: none;
  }
  .hl-card.dragging > span {
    opacity: 0;
  }
  .hl-card:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .hl-name {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .hl-size {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-caption);
    color: rgba(255, 255, 255, 0.6);
  }
  .hl-add {
    grid-column: span 2;
    display: grid;
    place-items: center;
    border: 1.5px dashed rgba(255, 255, 255, 0.25);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    transition: background var(--t-fast) ease, color var(--t-fast) ease, border-color var(--t-fast) ease;
  }
  .hl-add:hover,
  .hl-add.on {
    border-color: var(--accent);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .hl-add:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .hl-note {
    margin: 2px 0 0;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .hl-pick .chip {
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 50%, transparent);
  }
  .hl-edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 10px;
    padding: 12px;
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.07);
  }
  .hl-edit-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .hl-moves {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hl-moves button,
  .hl-reset {
    height: 28px;
    min-width: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-sm);
    display: inline-grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    cursor: pointer;
  }
  .hl-moves button:hover:not(:disabled),
  .hl-reset:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.18);
  }
  .hl-moves button:disabled,
  .hl-reset:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .hl-moves .danger {
    color: #ff8a80;
  }
  .hl-moves .hl-x {
    padding: 0;
    border-radius: 50%;
  }
  .hl-sizes {
    flex-wrap: wrap;
  }
  .hl-sizes button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  /* Mini-Form der Groesse: Breite und Hoehe im Verhaeltnis zum Raster. */
  .hl-glyph {
    display: inline-block;
    width: calc(var(--w) * 1.6px);
    height: calc(var(--h) * 6px);
    border-radius: 2px;
    background: currentColor;
    opacity: 0.55;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .shelf-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .shelf-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 6px 6px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
    transition: opacity var(--t-base) ease;
  }
  .shelf-list li.hidden {
    opacity: 0.5;
  }
  .shelf-icon {
    flex: none;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: var(--r-sm);
    background: rgba(255, 255, 255, 0.07);
    color: var(--text-2);
  }
  .shelf-name {
    flex: 1;
    min-width: 0;
    font-size: var(--fs-body);
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .shelf-name small {
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .shelf-list li.lifted {
    position: relative;
    z-index: 2;
    background: rgba(48, 48, 56, 0.96);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.16), 0 10px 24px rgba(0, 0, 0, 0.45);
    transition: none;
  }
  .shelf-list.sorting {
    cursor: grabbing;
    user-select: none;
  }
  .shelf-grip {
    flex: none;
    width: 22px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-3);
    display: grid;
    place-items: center;
    cursor: grab;
    touch-action: none;
  }
  .shelf-grip:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text);
  }
  .shelf-list.sorting .shelf-grip {
    cursor: grabbing;
  }
  .shelf-grip:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .chip {
    all: unset;
    padding: 4px 10px;
    border-radius: var(--r-md);
    background: #1c1c1e;
    color: var(--text-2);
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .chip.off {
    color: var(--text-3);
  }
  .chip:hover {
    background: #2c2c2e;
  }
  .switch:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .switch:checked {
    background: var(--ok);
  }
  .switch:checked::after {
    transform: translateX(18px);
  }
  .slider {
    flex: none;
    width: 170px;
    accent-color: #9fb4fa;
    cursor: pointer;
  }
  .limits {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    border-radius: var(--r-md);
    background: var(--well);
  }
  .lhead,
  .lrow {
    display: grid;
    grid-template-columns: 1fr 70px 90px 70px;
    gap: 6px;
    align-items: center;
    font-size: var(--fs-footnote);
  }
  .lhead {
    color: var(--text-3);
    font-size: var(--fs-caption);
  }
  .lmodel {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }
  .lrow input {
    height: 26px;
    min-width: 0;
    padding: 0 6px;
    border-radius: var(--r-sm);
    border: 0;
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.12), inset 0 1px 2px rgba(0, 0, 0, 0.4);
    background: #0b0b0c;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    font-variant-numeric: tabular-nums;
  }
  .log {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .entry {
    display: flex;
    gap: 10px;
    line-height: 1.4;
  }
  .entry.failed {
    color: var(--text-3);
  }
  .when {
    flex: none;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
</style>
