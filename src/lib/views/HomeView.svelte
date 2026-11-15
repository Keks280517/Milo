<script lang="ts" module>
  import { locale, t } from "../i18n";
  /** Karten, die die Uebersicht zeigen kann (wie HOME_CARDS in settings.rs). Farbe = Erkennungsfarbe der Karte. */
  export const HOME_CARDS: { id: string; label: string; color: string }[] = [
    { id: "media", label: t("Musik", "Music"), color: "#ff6fae" },
    { id: "agenda", label: t("Heute", "Today"), color: "#7d7aff" },
    { id: "weather", label: t("Wetter", "Weather"), color: "#5ac8fa" },
    { id: "timer", label: t("Timer", "Timer"), color: "#ff9f0a" },
    { id: "system", label: t("System", "System"), color: "#30d158" },
    { id: "claude", label: "Claude", color: "#f5a46f" },
    { id: "notes", label: t("Notizen", "Notes"), color: "#ffd60a" },
    { id: "usage", label: t("Verbrauch", "Usage"), color: "#bf5af2" },
    { id: "files", label: t("Ablage", "Shelf"), color: "#64d2ff" },
  ];
  /** Ab Werk: drei grosse Karten, die ohne Zusatzdienste nuetzlich sind. */
  export const HOME_DEFAULT = ["media", "agenda", "weather"];
</script>

<script lang="ts">
  import { layout, sizeInfo } from "$lib/homeLayout";
  // Uebersicht: jede Karte hat ihre eigene Form, passend zu ihrem Inhalt (Cover als Hintergrund,
  // Tagesleiste, Himmel, Ring, Kurven, Zettel ...). Ein Klick oeffnet die volle Ansicht. Drei Karten
  // werden gross, bis zu sechs kompakt. Auswahl in den Einstellungen.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { app, clock, inTauri, fileSize } from "../app.svelte";
  import { WidgetFeed } from "../widgets/feed.svelte";
  import WeatherIcon from "../cards/WeatherIcon.svelte";
  import EmptyState from "../EmptyState.svelte";
  import Sky from "../Sky.svelte";
  import { conditionText } from "../weather";
  import { rise } from "../motion";
  import type { TabId } from "../app.svelte";

  interface Event {
    id: string;
    title: string;
    startMs: number;
    endMs: number;
    allDay: boolean;
  }

  const feed = new WidgetFeed();
  let today = $state<Event[]>([]);
  let now = $state(Date.now());

  onMount(() => {
    feed.use(["cpu", "ram", "cpugraph", "agenda", "weather", "rain", "timer", "note", "quota"]);
    // Nur im Entwicklungsmodus (?demo): Beispielwerte fuer alle Karten, um das Aussehen zu pruefen.
    if (import.meta.env.DEV && !inTauri && new URLSearchParams(location.search).has("demo")) {
      const n = Date.now();
      feed.system = { cpu: 34, ramUsed: 11.2e9, ramTotal: 16e9, disks: [], netDown: 0, netUp: 0, top: [], uptime: 3600 };
      feed.cpuHistory = Array.from({ length: 40 }, (_, i) => 25 + Math.sin(i / 3) * 15 + (i % 7) * 3);
      feed.agenda = { title: t("Zahnarzt mit sehr langem Titel, der nicht passt", "Dentist with a very long title that won't fit"), startMs: n + 42 * 60_000, endMs: n + 102 * 60_000 };
      feed.note = t("Milch, Brot und Kaffee kaufen. Danach das Paket bei der Post abholen und Oma anrufen.", "Buy milk, bread and coffee. Then pick up the parcel at the post office and call grandma.");
      feed.quota = { used: 37, limit: 540 };
      today = [{ id: "a", title: "Standup", startMs: n - 3 * 3_600_000, endMs: n - 2.5 * 3_600_000, allDay: false }, { id: "b", title: "Zahnarzt", startMs: n + 42 * 60_000, endMs: n + 102 * 60_000, allDay: false }];
    }
    const loadDay = async () => {
      if (!inTauri) return;
      const start = new Date();
      start.setHours(0, 0, 0, 0);
      today = await invoke<Event[]>("agenda_list", { from: start.getTime(), to: start.getTime() + 86_400_000 }).catch(() => []);
    };
    loadDay();
    const timer = window.setInterval(() => {
      now = Date.now();
      loadDay();
    }, 60_000);
    return () => {
      feed.stop();
      clearInterval(timer);
    };
  });

  const cards = $derived((app.settings.homeCards?.length ? app.settings.homeCards : HOME_DEFAULT).filter((id) => HOME_CARDS.some((c) => c.id === id)));
  /** Jede Karte mit Groesse im 12er-Raster (eigene Wahl oder Automatik nach Anzahl). */
  const items = $derived(layout(cards, app.settings.homeSizes));
  const info = (id: string) => HOME_CARDS.find((c) => c.id === id)!;

  const cpu = $derived(feed.system ? feed.system.cpu : 0);
  const ram = $derived(feed.system ? (feed.system.ramUsed / Math.max(1, feed.system.ramTotal)) * 100 : 0);
  const timer = $derived(app.nextTimer);
  /** Wetter der Karte: aus der Widget-Quelle, sonst das, was die Insel schon kennt (Chip oben rechts). */
  const wx = $derived.by(() => {
    if (feed.weather) return feed.weather;
    const w = app.weather;
    if (!w) return null;
    const day = w.days[0];
    return { place: w.place, temp: w.now.temp, code: w.now.code, isDay: w.now.isDay, min: day?.min ?? w.now.temp, max: day?.max ?? w.now.temp, rain: day?.rain ?? 0 };
  });
  const CLAUDE: Record<string, string> = { working: t("arbeitet gerade", "is working"), question: t("hat eine Frage", "has a question"), confirm: t("braucht deine Freigabe", "needs your approval"), done: t("ist fertig", "is done"), idle: t("wartet", "is waiting") };
  const RING = 2 * Math.PI * 40;

  /** Symbole der Kartenkoepfe (16er-Raster, Linien). */
  const ICONS: Record<string, string> = {
    media: "M6 12.2V4.4l7-1.6v7.8M6 12.2a1.8 1.8 0 1 1-1.8-1.8A1.8 1.8 0 0 1 6 12.2Zm7-1.6a1.8 1.8 0 1 1-1.8-1.8 1.8 1.8 0 0 1 1.8 1.8ZM6 6.6l7-1.6",
    agenda: "M3 5.2a1.6 1.6 0 0 1 1.6-1.6h6.8A1.6 1.6 0 0 1 13 5.2v6.2a1.6 1.6 0 0 1-1.6 1.6H4.6A1.6 1.6 0 0 1 3 11.4ZM3 7h10M5.8 2.4v2.2M10.2 2.4v2.2",
    weather: "M8 14s-4.2-3.7-4.2-7.2a4.2 4.2 0 1 1 8.4 0C12.2 10.3 8 14 8 14Zm0-5.6a1.6 1.6 0 1 0 0-3.2 1.6 1.6 0 0 0 0 3.2Z",
    timer: "M8 14a5.2 5.2 0 1 0 0-10.4A5.2 5.2 0 0 0 8 14Zm0-7.6v2.6l1.6 1.2M6.4 1.8h3.2",
    system: "M2 9h2.6l1.6-4.4L9 13l1.8-4H14",
    claude: "M3.4 6.2a1.6 1.6 0 0 1 1.6-1.6h6a1.6 1.6 0 0 1 1.6 1.6v4.4a1.6 1.6 0 0 1-1.6 1.6H5a1.6 1.6 0 0 1-1.6-1.6ZM6.4 7.6v1.2M9.6 7.6v1.2M8 2.4v2.2",
    notes: "M4 2.8h8v10.4H4ZM6 6h4M6 8.4h4M6 10.8h2.4",
    usage: "M3 13V9.4M6.4 13V6.6M9.8 13V8.2M13.2 13V3.6",
    files: "M2.6 4.6a1.2 1.2 0 0 1 1.2-1.2h2.6l1.4 1.6h4.4a1.2 1.2 0 0 1 1.2 1.2v6a1.2 1.2 0 0 1-1.2 1.2H3.8a1.2 1.2 0 0 1-1.2-1.2Z",
  };

  const dateLabel = $derived(new Date(now).toLocaleDateString(locale, { weekday: "short", day: "numeric", month: "short" }));
  /** "in 25 Min", "in 2 Std" oder "läuft". */
  function until(ms: number): string {
    const min = Math.round((ms - now) / 60_000);
    if (min <= 0) return t("läuft gerade", "happening now");
    if (min < 60) return t(`in ${min} Min`, `in ${min} min`);
    const h = Math.floor(min / 60);
    return t(`in ${h} Std`, `in ${h} h`);
  }
  const busy = (v: number) => (v > 85 ? "#ff453a" : v > 65 ? "#ff9f0a" : "var(--c)");

  /** Tagesleiste 6 bis 22 Uhr. */
  const dayStart = $derived(new Date(new Date(now).setHours(6, 0, 0, 0)).getTime());
  const span = 16 * 3_600_000;
  const at = (ms: number) => Math.min(1, Math.max(0, (ms - dayStart) / span)) * 100;
  const hm = (ms: number) => new Date(ms).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });

  /** Karte kippt leicht zur Maus, ein Lichtfleck folgt ihr. */
  function tilt(e: PointerEvent) {
    const card = e.currentTarget as HTMLElement;
    const r = card.getBoundingClientRect();
    const x = (e.clientX - r.left) / r.width;
    const y = (e.clientY - r.top) / r.height;
    card.style.setProperty("--ry", `${(x - 0.5) * 7}deg`);
    card.style.setProperty("--rx", `${(0.5 - y) * 7}deg`);
    card.style.setProperty("--lx", `${x * 100}%`);
    card.style.setProperty("--ly", `${y * 100}%`);
  }
  function untilt(e: PointerEvent) {
    const card = e.currentTarget as HTMLElement;
    card.style.setProperty("--rx", "0deg");
    card.style.setProperty("--ry", "0deg");
  }

  function open(id: string) {
    app.tab = id as TabId;
  }
  function stop(e: Event | MouseEvent) {
    (e as MouseEvent).stopPropagation?.();
  }
  function quickTimer(e: MouseEvent, minutes: number) {
    e.stopPropagation();
    app.startTimer(t(`${minutes} Minuten`, `${minutes} minutes`), minutes * 60);
  }
  function ask(e: MouseEvent, text: string) {
    e.stopPropagation();
    app.tab = "chat";
    app.send(text);
  }
  function media(e: MouseEvent, action: "toggle" | "next" | "previous") {
    e.stopPropagation();
    app.mediaCommand(action).catch(() => {});
  }

  function spark(values: number[]): string {
    if (values.length < 2) return "";
    const step = 100 / (values.length - 1);
    return values.map((v, i) => `${i === 0 ? "M" : "L"}${(i * step).toFixed(1)} ${(30 - (Math.min(100, v) / 100) * 28).toFixed(1)}`).join(" ");
  }
</script>

{#snippet head(id: string, label: string, meta: string = "", light: boolean = false)}
  <div class="head" class:light>
    <span class="ico" aria-hidden="true"><svg viewBox="0 0 16 16" width="13" height="13"><path d={ICONS[id]} fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg></span>
    <span class="label">{label}</span>
    {#if meta}<span class="meta">{meta}</span>{/if}
  </div>
{/snippet}

<div class="home">
  <div class="grid">
    {#each items as item, i (item.id)}
      {@const id = item.id}
      {@const size = sizeInfo(item.size)}
      <!-- Zwei Reihen hoch: Karte zeigt mehr (Tagesleiste, Erklaertexte, grosses Cover). -->
      {@const big = size.rows === 2}
      <!-- Breit genug fuer Nebeneinander (ab 6 Spalten). -->
      {@const wide = size.cols >= 6}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="card {id}" class:big class:wide style:grid-column="span {size.cols}" style:grid-row="span {size.rows}" role="button" tabindex="0" style:--c={info(id).color} onclick={() => open(id)} onpointermove={tilt} onpointerleave={untilt} onkeydown={(e) => e.key === "Enter" && open(id)} in:rise={{ delay: i * 60, y: 14 }}>

        {#if id === "media"}
          <!-- Musik: das Cover ist die Karte, die Steuerung liegt als Glas darauf. -->
          <div class="bg" aria-hidden="true">{#if app.media?.cover}<img src={app.media.cover} alt="" />{/if}</div>
          {@render head(id, info(id).label, app.media?.app ?? "")}
          {#if app.media}
            <div class="media-body">
              <span class="cover">{#if app.media.cover}<img src={app.media.cover} alt="" />{:else}<svg viewBox="0 0 16 16" width="22" height="22" aria-hidden="true"><path d={ICONS.media} fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg>{/if}</span>
              <span class="col">
                <b class="title">{app.media.title}</b>
                <small>{app.media.artist || app.media.app}</small>
              </span>
            </div>
            <div class="media-foot">
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <div class="mini glass" onclick={stop}>
                <button onclick={(e) => media(e, "previous")} aria-label={t("Zurück", "Previous")}><svg viewBox="0 0 24 24" width="15" height="15"><path d="M6 5.5v13" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" /><path d="M19 6.2v11.6c0 .8-.9 1.3-1.6.8L9.6 13a1.2 1.2 0 0 1 0-2l7.8-5.6c.7-.5 1.6 0 1.6.8Z" fill="currentColor" /></svg></button>
                <button class="play" onclick={(e) => media(e, "toggle")} aria-label={app.media.playing ? t("Pause", "Pause") : t("Abspielen", "Play")}>
                  {#if app.media.playing}<svg viewBox="0 0 24 24" width="15" height="15"><rect x="6.2" y="4.8" width="4" height="14.4" rx="1.4" fill="currentColor" /><rect x="13.8" y="4.8" width="4" height="14.4" rx="1.4" fill="currentColor" /></svg>
                  {:else}<svg viewBox="0 0 24 24" width="15" height="15"><path d="M8 5.6v12.8c0 .9 1 1.4 1.7.9l9.4-6.4a1.1 1.1 0 0 0 0-1.8L9.7 4.7C9 4.2 8 4.7 8 5.6Z" fill="currentColor" /></svg>{/if}
                </button>
                <button onclick={(e) => media(e, "next")} aria-label={t("Weiter", "Next")}><svg viewBox="0 0 24 24" width="15" height="15"><path d="M18 5.5v13" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" /><path d="M5 6.2v11.6c0 .8.9 1.3 1.6.8l7.8-5.6a1.2 1.2 0 0 0 0-2L6.6 5.4C5.9 4.9 5 5.4 5 6.2Z" fill="currentColor" /></svg></button>
              </div>
              <span class="eq" class:on={app.media.playing} aria-hidden="true"><i></i><i></i><i></i><i></i></span>
            </div>
          {:else}
            <div class="quiet">
              <span class="eq idle" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>
              <b class="title">{t("Gerade ist es still", "It's quiet right now")}</b>
              {#if big || wide}<small>{t("Spiel etwas in Spotify, YouTube oder einer anderen App – hier erscheint es mit Steuerung.", "Play something in Spotify, YouTube or any app – it shows up here with controls.")}</small>{/if}
            </div>
          {/if}

        {:else if id === "agenda"}
          <!-- Heute: naechster Termin gross, darunter die Tagesleiste mit Jetzt-Marke. -->
          {@render head(id, info(id).label, dateLabel)}
          <div class="agenda-body">
            {#if feed.agenda}
              <span class="chip">{hm(feed.agenda.startMs)} · {until(feed.agenda.startMs)}</span>
              <b class="title">{feed.agenda.title}</b>
            {:else}
              <b class="title">{t("Frei für heute", "Free for today")}</b>
              <small>{t("Keine weiteren Termine", "No more events")}</small>
            {/if}
          </div>
          <div class="dayline" aria-hidden="true">
            <span class="passed" style:width="{at(now)}%"></span>
            {#each today.filter((e) => !e.allDay) as e (e.id)}
              <span class="slot" class:past={e.endMs < now} style:left="{at(e.startMs)}%" style:width="max(6px, {at(e.endMs) - at(e.startMs)}%)" title="{e.title} · {hm(e.startMs)}"></span>
            {/each}
            <span class="now" style:left="{at(now)}%"></span>
          </div>
          {#if big}<div class="hours"><span>6</span><span>10</span><span>14</span><span>18</span><span>22</span></div>{/if}
          {#if !feed.agenda && big}
            <button class="act" onclick={(e) => { e.stopPropagation(); open("agenda"); }}>{t("Termin anlegen", "Add event")}</button>
          {/if}

        {:else if id === "weather"}
          <!-- Wetter: die Karte ist der Himmel, animiert passend zur Wetterlage. -->
          <Sky code={wx?.code ?? 0} isDay={wx?.isDay ?? true} />
          {@render head(id, wx?.place || info(id).label, "", true)}
          {#if wx}
            <div class="weather-body">
              <b class="temp">{Math.round(wx.temp)}°</b>
              <span class="cond">{conditionText(wx.code, wx.isDay)}</span>
              <span class="wx-row">
                <span>↑ {Math.round(wx.max)}°</span>
                <span>↓ {Math.round(wx.min)}°</span>
                <span class="rain"><svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M8 2.2s4 4.4 4 7.2a4 4 0 0 1-8 0c0-2.8 4-7.2 4-7.2Z" fill="currentColor" /></svg>{Math.round(wx.rain)} %</span>
              </span>
            </div>
          {:else}
            <div class="weather-body">
              <b class="title light">{t("Wo bist du gerade?", "Where are you right now?")}</b>
              <button class="act" onclick={(e) => ask(e, t("Wie wird das Wetter heute?", "What's the weather like today?"))}>{t("Wetter fragen", "Ask about the weather")}</button>
            </div>
          {/if}

        {:else if id === "timer"}
          <!-- Timer: ein grosser Ring, der sich leert. -->
          {@render head(id, info(id).label, timer ? timer.label : "")}
          {#if timer}
            <div class="ring-wrap">
              <svg viewBox="0 0 100 100" class="ring"><circle cx="50" cy="50" r="40" class="track" /><circle cx="50" cy="50" r="40" class="fill" stroke-dasharray={RING} stroke-dashoffset={RING * (1 - app.timerLeft)} /></svg>
              <span class="ring-text"><b>{clock(timer.dueMs - app.now)}</b></span>
            </div>
          {:else}
            <div class="quiet">
              <b class="title">{big || wide ? t("Kurz Pause machen?", "Time for a short break?") : t("Kurze Pause?", "Quick break?")}</b>
              <span class="chips">{#each big || wide ? [5, 10, 25] : [5, 25] as m}<button class="act" onclick={(e) => quickTimer(e, m)}>{m} {t("Min", "min")}</button>{/each}</span>
            </div>
          {/if}

        {:else if id === "system"}
          <!-- System: Ringe fuer Prozessor und Arbeitsspeicher, die Kurve der letzten Minute als Grund. -->
          {@render head(id, info(id).label, feed.system ? t("live", "live") : "")}
          <svg class="spark" viewBox="0 0 100 32" preserveAspectRatio="none" aria-hidden="true">
            {#if feed.cpuHistory.length > 1}<path class="area" d="{spark(feed.cpuHistory)} L100 32 L0 32 Z" /><path class="line" d={spark(feed.cpuHistory)} />{/if}
          </svg>
          <div class="gauges">
            {#each [["CPU", cpu], ["RAM", ram]] as [name, value]}
              <div class="gauge">
                <svg viewBox="0 0 100 100"><circle cx="50" cy="50" r="44" class="disc" /><circle cx="50" cy="50" r="40" class="track" /><circle cx="50" cy="50" r="40" class="fill" style:stroke={busy(Number(value))} stroke-dasharray={RING} stroke-dashoffset={RING * (1 - Number(value) / 100)} /></svg>
                <span><b>{feed.system ? Math.round(Number(value)) : "–"}<em>%</em></b><small>{name}</small></span>
              </div>
            {/each}
          </div>

        {:else if id === "claude"}
          <!-- Claude: das orange Wuerfelchen mit seinem Zustand. -->
          {@render head(id, info(id).label, app.claude.present ? app.claude.project || "Claude Code" : "")}
          {#if app.claude.present}
            <div class="claude-row">
              <span class="buddy" class:busy={app.claude.activity === "working"}><i></i><i></i></span>
              <b class="title">Claude {CLAUDE[app.claude.activity] ?? ""}</b>
            </div>
            {#if app.claude.activity === "working"}<span class="work" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>{/if}
          {:else}
            <EmptyState kind="claude" small title={t("Claude schläft", "Claude is asleep")} text={big ? t("Startet Claude Code, siehst du hier, woran es arbeitet.", "Once Claude Code starts, you'll see what it's working on here.") : ""} />
          {/if}

        {:else if id === "notes"}
          <!-- Notizen: ein Zettel. -->
          <div class="paper">
            <span class="kind dark"><i></i>{info(id).label}</span>
            {#if feed.note}
              <p class="note">{feed.note}</p>
            {:else}
              <p class="note faint">{t("Noch nichts notiert.", "Nothing noted yet.")}</p>
              <button class="act dark" onclick={(e) => ask(e, t("Merk dir: ", "Remember: "))}>{t("Etwas merken", "Remember something")}</button>
            {/if}
          </div>

        {:else if id === "usage"}
          <!-- Verbrauch: Fuellstand des Tageskontingents. -->
          {@const used = feed.quota ? feed.quota.used / Math.max(1, feed.quota.limit) : 0}
          {@render head(id, info(id).label, feed.quota ? `${Math.round(used * 100)} %` : "")}
          <b class="big-num">{feed.quota ? feed.quota.used : "–"}<small> / {feed.quota?.limit ?? "–"}</small></b>
          <small>{t("Gemini-Anfragen heute", "Gemini requests today")}</small>
          <div class="meter"><span style:width="{Math.min(100, used * 100)}%"></span></div>

        {:else if id === "files"}
          <!-- Ablage: ein Stapel der neuesten Dateien. -->
          {@render head(id, info(id).label, app.files.length ? String(app.files.length) : "")}
          {#if app.files.length > 0}
            <div class="stack">
              {#each app.files.slice(0, 3) as f, k}
                <span class="sheet" style:--k={k}><b>{f.name}</b><small>{fileSize(f.size)}</small></span>
              {/each}
            </div>
          {:else}
            <EmptyState kind="files" small title={t("Ablage ist leer", "The shelf is empty")} text={big ? t("Zieh eine Datei auf die Insel.", "Drag a file onto the island.") : ""} />
          {/if}
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .home {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 2px 22px 0;
  }
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    /* 12 Spalten, zwei Reihen fuellen die Flaeche; weitere Reihen lassen sich scrollen. */
    grid-template-columns: repeat(12, minmax(0, 1fr));
    grid-auto-rows: calc(50% - 6px);
    /* Ohne "dense": die Karten stehen genau in der Reihenfolge, die man in den Einstellungen legt. */
    grid-auto-flow: row;
    gap: 12px;
    overflow-y: auto;
    scrollbar-width: none;
  }

  /* ---------- Karte ---------- */
  .card {
    position: relative;
    isolation: isolate;
    min-height: 0;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    gap: 6px;
    padding: 44px 16px 15px;
    border-radius: var(--r-lg);
    /* Glas mit einem Schimmer in der Farbe der Karte oben links. */
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--c) 20%, transparent), transparent 62%),
      linear-gradient(165deg, rgba(255, 255, 255, 0.085), rgba(255, 255, 255, 0.025) 60%, rgba(255, 255, 255, 0.04));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.13),
      inset 0 0 0 0.5px rgba(255, 255, 255, 0.08),
      0 10px 28px rgba(0, 0, 0, 0.28);
    color: var(--text);
    text-align: left;
    overflow: hidden;
    cursor: pointer;
    transition: transform var(--t-slow) var(--ease-spring), box-shadow var(--t-base) ease;
  }
  .card:hover {
    transform: perspective(700px) rotateX(var(--rx, 0deg)) rotateY(var(--ry, 0deg)) translateY(-3px);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.16),
      inset 0 0 0 0.5px color-mix(in srgb, var(--c) 40%, transparent),
      0 16px 36px rgba(0, 0, 0, 0.36);
    transition: transform var(--t-fast) ease-out, box-shadow var(--t-base) ease;
  }
  /* Lichtfleck, der der Maus ueber die Karte folgt. */
  .card::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 3;
    border-radius: inherit;
    pointer-events: none;
    background: radial-gradient(200px circle at var(--lx, 50%) var(--ly, 0%), rgba(255, 255, 255, 0.1), transparent 65%);
    opacity: 0;
    transition: opacity var(--t-base) ease;
  }
  .card:hover::after {
    opacity: 1;
  }
  .card:active {
    transform: scale(0.97);
  }
  .card:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  /* Kopfzeile: Symbol in der Kartenfarbe, Name, rechts eine Nebenangabe. */
  .head {
    position: absolute;
    top: 13px;
    left: 14px;
    right: 14px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--text-2);
  }
  .ico {
    flex: none;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 7px;
    background: color-mix(in srgb, var(--c) 22%, transparent);
    box-shadow: inset 0 0 0 0.5px color-mix(in srgb, var(--c) 45%, transparent);
    color: color-mix(in srgb, var(--c) 80%, #fff);
  }
  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    margin-left: auto;
    flex: none;
    max-width: 50%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: var(--fw-medium, 500);
    color: var(--text-3);
  }
  .head.light {
    color: #fff;
    text-shadow: 0 1px 6px rgba(0, 0, 0, 0.3);
  }
  .head.light .ico {
    background: rgba(255, 255, 255, 0.2);
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.3);
    color: #fff;
  }
  .kind {
    position: absolute;
    top: 13px;
    left: 16px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
  }
  .kind i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
  }
  .kind.dark {
    color: #5b4a12;
  }

  .title {
    position: relative;
    font-size: var(--fs-headline);
    font-weight: var(--fw-bold);
    line-height: 1.22;
    letter-spacing: var(--tr-headline);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .big .title {
    font-size: var(--fs-title);
    letter-spacing: var(--tr-title);
  }
  .title.light {
    color: #fff;
  }
  small {
    position: relative;
    font-size: var(--fs-footnote);
    color: var(--text-3);
    line-height: 1.35;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .col {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 3px;
  }
  .col small {
    white-space: nowrap;
  }
  .quiet {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }
  .quiet small {
    max-width: 34ch;
  }
  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  /* Aktion in einer Kachel: eine ruhige Glaskapsel. */
  .act {
    position: relative;
    align-self: flex-start;
    height: 30px;
    margin-top: var(--s-1);
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.14);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.2), inset 0 0 0 0.5px rgba(255, 255, 255, 0.12);
    color: #fff;
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    cursor: pointer;
    transition: transform var(--t-slow) var(--ease-spring), background var(--t-fast) ease;
  }
  .chips .act {
    margin-top: 0;
  }
  .act:hover {
    background: color-mix(in srgb, var(--c) 40%, rgba(255, 255, 255, 0.12));
  }
  .act:active {
    transform: scale(0.94);
  }

  /* ---------- Musik ---------- */
  .media .bg {
    position: absolute;
    inset: 0;
    z-index: -1;
    background:
      radial-gradient(90% 80% at 85% 110%, rgba(255, 111, 174, 0.28), transparent 60%),
      radial-gradient(70% 70% at 0% 0%, rgba(160, 90, 255, 0.18), transparent 60%),
      linear-gradient(160deg, #24141f, #120c14);
  }
  .media .bg img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(26px) saturate(1.6) brightness(0.5);
    transform: scale(1.35);
  }
  .media-body {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .big .media-body {
    flex-direction: column;
    align-items: flex-start;
  }
  /* Flache Karten (eine Reihe): kleineres Cover und einzeilige Titel, sonst rutscht der Inhalt
     unter die Kopfzeile. In schmalen Karten faellt der Equalizer weg. */
  .card:not(.big) .cover {
    width: 40px;
    height: 40px;
    border-radius: 10px;
  }
  .card:not(.big).media .title,
  .card:not(.big).timer .title {
    -webkit-line-clamp: 1;
    line-clamp: 1;
  }
  .card:not(.big):not(.wide) .media-foot .eq {
    display: none;
  }
  .card:not(.big) .eq.idle {
    height: 26px;
    margin-bottom: 0;
  }
  .cover {
    flex: none;
    width: 54px;
    height: 54px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    overflow: hidden;
    background: linear-gradient(145deg, color-mix(in srgb, var(--c) 55%, #2a1626), #1a0f18);
    color: rgba(255, 255, 255, 0.8);
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.5), inset 0 0 0 0.5px rgba(255, 255, 255, 0.15);
  }
  .big .cover {
    width: 96px;
    height: 96px;
    border-radius: 16px;
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .media-foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 2px;
  }
  .mini {
    display: flex;
    gap: 2px;
    padding: 4px;
    border-radius: var(--r-full);
  }
  .mini button {
    position: relative;
    z-index: 1;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: #fff;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .mini button:hover {
    background: rgba(255, 255, 255, 0.14);
  }
  .mini .play {
    background: #fff;
    color: #000;
  }
  .mini .play:hover {
    background: #fff;
  }
  /* Kleiner Equalizer: tanzt, solange Musik laeuft; im Leerlauf atmet er nur. */
  .eq {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 18px;
  }
  .eq i {
    width: 3px;
    height: 30%;
    border-radius: 2px;
    background: var(--c);
    opacity: 0.6;
  }
  .eq.on i {
    opacity: 1;
    animation: eq 0.9s ease-in-out infinite alternate;
  }
  .eq.on i:nth-child(2) { animation-duration: 0.7s; }
  .eq.on i:nth-child(3) { animation-duration: 1.1s; }
  .eq.on i:nth-child(4) { animation-duration: 0.8s; }
  @keyframes eq {
    from { height: 18%; }
    to { height: 100%; }
  }
  .eq.idle {
    height: 46px;
    margin-bottom: 6px;
    gap: 5px;
  }
  .eq.idle i {
    width: 6px;
    border-radius: 3px;
    animation: breathe 3.2s ease-in-out infinite;
  }
  .eq.idle i:nth-child(1) { --h: 35%; animation-delay: 0s; }
  .eq.idle i:nth-child(2) { --h: 70%; animation-delay: 0.25s; }
  .eq.idle i:nth-child(3) { --h: 50%; animation-delay: 0.5s; }
  .eq.idle i:nth-child(4) { --h: 85%; animation-delay: 0.75s; }
  .eq.idle i:nth-child(5) { --h: 40%; animation-delay: 1s; }
  @keyframes breathe {
    0%, 100% { height: 14%; opacity: 0.35; }
    50% { height: var(--h); opacity: 0.85; }
  }

  /* ---------- Heute ---------- */
  .agenda-body {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 5px;
  }
  .chip {
    padding: 3px 9px;
    border-radius: var(--r-full);
    background: color-mix(in srgb, var(--c) 24%, transparent);
    color: color-mix(in srgb, var(--c) 70%, #fff);
    font-size: var(--fs-caption);
    font-weight: var(--fw-bold);
    font-variant-numeric: tabular-nums;
  }
  .dayline {
    position: relative;
    height: 10px;
    margin-top: 10px;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.07);
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.06);
  }
  .passed {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    border-radius: inherit;
    background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--c) 30%, transparent));
  }
  .slot {
    position: absolute;
    top: 2px;
    bottom: 2px;
    border-radius: var(--r-full);
    background: var(--c);
    box-shadow: 0 0 8px color-mix(in srgb, var(--c) 60%, transparent);
  }
  .slot.past {
    opacity: 0.35;
    box-shadow: none;
  }
  .dayline .now {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 45%, transparent), 0 0 12px rgba(255, 255, 255, 0.6);
  }
  .hours {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-caption);
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
  }

  /* ---------- Wetter ---------- */
  .weather {
    background: none;
  }
  .weather-body {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    color: #fff;
    text-shadow: 0 1px 10px rgba(0, 0, 0, 0.3);
  }
  .temp {
    font-size: 48px;
    font-weight: 250;
    letter-spacing: -0.04em;
    line-height: 0.95;
  }
  .big .temp {
    font-size: 68px;
  }
  .cond {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
  }
  .wx-row {
    display: flex;
    gap: 10px;
    font-size: var(--fs-footnote);
    color: rgba(255, 255, 255, 0.82);
    font-variant-numeric: tabular-nums;
  }
  .wx-row .rain {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: #bfe3ff;
  }

  /* ---------- Timer ---------- */
  .ring-wrap {
    position: relative;
    align-self: center;
    height: 100%;
    max-height: 150px;
    aspect-ratio: 1;
    margin-top: auto;
  }
  .ring,
  .gauge svg {
    width: 100%;
    height: 100%;
    transform: rotate(-90deg);
  }
  .track {
    fill: none;
    stroke: rgba(255, 255, 255, 0.09);
    stroke-width: 8;
  }
  .fill {
    fill: none;
    stroke: var(--c);
    stroke-width: 8;
    stroke-linecap: round;
    filter: drop-shadow(0 0 4px color-mix(in srgb, var(--c) 55%, transparent));
    transition: stroke-dashoffset var(--t-spring) linear, stroke var(--t-base) ease;
  }
  .ring-text {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }
  .ring-text b {
    font-size: var(--fs-title);
    font-weight: var(--fw-bold);
    font-variant-numeric: tabular-nums;
  }

  /* ---------- System ---------- */
  .gauges {
    position: relative;
    display: flex;
    gap: 12px;
  }
  .gauge {
    position: relative;
    width: 66px;
    height: 66px;
  }
  .big .gauge {
    width: 92px;
    height: 92px;
  }
  /* Dunkle Scheibe hinter dem Ring: die Kurve im Hintergrund laeuft nicht durch die Zahl. */
  .disc {
    fill: rgba(14, 14, 18, 0.78);
  }
  .gauge span {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0;
  }
  .gauge b {
    font-size: var(--fs-headline);
    font-weight: var(--fw-bold);
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
  }
  .big .gauge b {
    font-size: var(--fs-title);
  }
  .gauge em {
    font-style: normal;
    font-size: 0.6em;
    color: var(--text-3);
    margin-left: 1px;
  }
  .gauge small {
    font-size: var(--fs-caption);
  }
  .spark {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    height: 58%;
    z-index: -1;
    -webkit-mask-image: linear-gradient(180deg, #000 30%, transparent);
    mask-image: linear-gradient(180deg, #000 30%, transparent);
  }
  .spark .area {
    fill: color-mix(in srgb, var(--c) 18%, transparent);
  }
  .spark .line {
    fill: none;
    stroke: var(--c);
    stroke-width: 1.6;
    vector-effect: non-scaling-stroke;
    opacity: 0.8;
  }

  /* ---------- Claude ---------- */
  .claude-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .buddy {
    flex: none;
    width: 44px;
    height: 38px;
    border-radius: var(--r-md);
    background: linear-gradient(180deg, #f8b892, #e7895a);
    box-shadow: 0 6px 16px rgba(231, 137, 90, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }
  .buddy i {
    width: 5px;
    height: 7px;
    border-radius: 3px;
    background: #3a2216;
  }
  .buddy.busy {
    animation: hop 1s ease-in-out infinite;
  }
  @keyframes hop {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-4px); }
  }
  .work {
    display: flex;
    gap: 4px;
    margin-top: 6px;
  }
  .work i {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--c);
    opacity: 0.25;
    animation: work 1.5s ease-in-out infinite;
  }
  .work i:nth-child(2) { animation-delay: 0.15s; }
  .work i:nth-child(3) { animation-delay: 0.3s; }
  .work i:nth-child(4) { animation-delay: 0.45s; }
  .work i:nth-child(5) { animation-delay: 0.6s; }
  @keyframes work {
    0%, 100% { opacity: 0.2; }
    50% { opacity: 1; }
  }

  /* ---------- Notizen: Zettel ---------- */
  .notes {
    padding: 10px;
  }
  .paper {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    gap: 8px;
    padding: 34px 14px 14px;
    border-radius: var(--r-md);
    background:
      repeating-linear-gradient(180deg, transparent 0 21px, rgba(140, 110, 20, 0.18) 21px 22px),
      linear-gradient(180deg, #fff6c8, #fde98f);
    color: #3f3410;
    transform: rotate(-1.2deg);
    box-shadow: 0 8px 18px rgba(0, 0, 0, 0.35);
  }
  .paper .kind {
    top: 10px;
    left: 14px;
  }
  .note {
    margin: 0;
    font-family: Georgia, "Times New Roman", serif;
    font-size: var(--fs-callout);
    line-height: 22px;
    display: -webkit-box;
    -webkit-line-clamp: 5;
    line-clamp: 5;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .note.faint {
    color: #8a7a3c;
  }
  .act.dark {
    background: #3f3410;
    color: #fff6c8;
  }

  /* ---------- Verbrauch ---------- */
  .big-num {
    font-size: var(--fs-display);
    font-weight: var(--fw-bold);
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .big-num small {
    font-size: var(--fs-callout);
  }
  .meter {
    height: 8px;
    margin-top: 4px;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, color-mix(in srgb, var(--c) 60%, #fff), var(--c));
    box-shadow: 0 0 10px color-mix(in srgb, var(--c) 60%, transparent);
    transition: width var(--t-spring) var(--ease-spring);
  }

  /* ---------- Ablage: Stapel ---------- */
  .stack {
    position: relative;
    height: 74px;
    margin-bottom: 2px;
  }
  .sheet {
    position: absolute;
    left: calc(var(--k) * 10px);
    right: calc((2 - var(--k)) * 10px);
    bottom: calc(var(--k) * 12px);
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    background: linear-gradient(180deg, #2a2a30, #1c1c20);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.1), 0 4px 10px rgba(0, 0, 0, 0.35);
    z-index: calc(3 - var(--k));
  }
  .sheet b {
    font-size: var(--fs-footnote);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sheet small {
    font-size: var(--fs-caption);
  }

  @media (prefers-reduced-motion: reduce) {
    .eq i, .buddy.busy, .work i { animation: none !important; }
  }
</style>
