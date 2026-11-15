<script lang="ts">
  import { lens } from "$lib/lens";
  import { t, lang, locale } from "../i18n";
  // Agenda: oben der naechste Termin mit grosser Uhrzeit, darunter der Tag als Zeitleiste
  // (Terminbloecke und Jetzt-Markierung) oder der Monat als Raster. Eigene Termine legt man mit „+“
  // an; abonnierte Kalender (ICS) stehen in den Einstellungen und werden nur gelesen.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { app, inTauri } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoEvents } from "../demoData";
  import { pop, rise } from "../motion";

  interface Event {
    id: string;
    title: string;
    startMs: number;
    endMs: number;
    allDay: boolean;
    location: string;
    source: string;
  }

  const DAY = 86_400_000;
  let mode = $state<"day" | "month">("day");
  /** Gezeigter Tag (Mitternacht, lokale Zeit). */
  let day = $state(midnight(Date.now()));
  let events = $state<Event[]>([]);
  let now = $state(Date.now());
  let selected = $state<string | null>(null);
  let adding = $state(false);
  let error = $state("");
  let form = $state({ title: "", date: "", from: "10:00", to: "11:00", allDay: false });

  function midnight(ms: number): number {
    const d = new Date(ms);
    return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  }
  const two = (n: number) => String(n).padStart(2, "0");
  const hm = (ms: number) => {
    const d = new Date(ms);
    return `${two(d.getHours())}:${two(d.getMinutes())}`;
  };
  const isoDate = (ms: number) => {
    const d = new Date(ms);
    return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
  };

  /** Monatsraster: beginnt am Montag vor dem Ersten, sechs Wochen. */
  const monthStart = $derived.by(() => {
    const d = new Date(day);
    const first = new Date(d.getFullYear(), d.getMonth(), 1);
    const shift = (first.getDay() + 6) % 7;
    return new Date(first.getFullYear(), first.getMonth(), 1 - shift).getTime();
  });
  const range = $derived(mode === "day" ? { from: day - DAY, to: day + 2 * DAY } : { from: monthStart, to: monthStart + 42 * DAY + 3_600_000 });

  async function load() {
    error = "";
    // Tour: Beispieltermine in der echten Ansicht.
    if (tour.demo === "agenda") {
      events = demoEvents();
      return;
    }
    if (!inTauri) {
      events = sample();
      return;
    }
    try {
      events = await invoke<Event[]>("agenda_list", { from: range.from, to: range.to });
    } catch (err) {
      error = String(err);
    }
  }

  /** Beispieltermine fuer die Entwicklung im Browser. */
  function sample(): Event[] {
    const t = midnight(Date.now());
    const at = (h: number, m = 0) => t + (h * 60 + m) * 60_000;
    return [
      { id: "a", title: "Stand-up", startMs: at(9, 30), endMs: at(9, 45), allDay: false, location: "", source: "local" },
      { id: "b", title: "Design review", startMs: at(18, 30), endMs: at(19, 30), allDay: false, location: "Raum 2", source: "Arbeit" },
      { id: "c", title: "Mittag mit Lea", startMs: at(12, 30), endMs: at(13, 30), allDay: false, location: "", source: "local" },
      { id: "d", title: "Sport", startMs: at(17), endMs: at(18), allDay: false, location: "", source: "local" },
    ];
  }

  $effect(() => {
    void range.from;
    void range.to;
    void app.settings.agendaIcs;
    void tour.demo;
    load();
  });

  onMount(() => {
    const tick = window.setInterval(() => (now = Date.now()), 20_000);
    const reload = window.setInterval(load, 5 * 60_000);
    return () => {
      clearInterval(tick);
      clearInterval(reload);
    };
  });

  const dayEvents = $derived(events.filter((e) => !e.allDay && e.endMs > day && e.startMs < day + DAY));
  const allDayEvents = $derived(events.filter((e) => e.allDay && e.endMs > day && e.startMs < day + DAY));
  /** Was oben gross steht: der gewaehlte Termin, sonst der naechste (oder laufende) ab jetzt. */
  const focus = $derived(
    events.find((e) => e.id === selected) ??
      events.filter((e) => !e.allDay && e.endMs > now).sort((a, b) => a.startMs - b.startMs)[0] ??
      null,
  );
  const lead = $derived.by(() => {
    if (!focus) return "";
    if (focus.startMs <= now && focus.endMs > now) {
      const left = Math.max(1, Math.round((focus.endMs - now) / 60_000));
      return t(`Läuft noch ${left} Minuten`, `${left} ${left === 1 ? "minute" : "minutes"} left`);
    }
    const minutes = Math.round((focus.startMs - now) / 60_000);
    if (minutes < 0) return t("Vorbei", "Over");
    if (minutes < 60) return t(`In ${minutes} ${minutes === 1 ? "Minute" : "Minuten"}`, `In ${minutes} ${minutes === 1 ? "minute" : "minutes"}`);
    const days = Math.round((midnight(focus.startMs) - midnight(now)) / DAY);
    if (days === 0) return t(`Heute um ${hm(focus.startMs)}`, `Today at ${hm(focus.startMs)}`);
    if (days === 1) return t(`Morgen um ${hm(focus.startMs)}`, `Tomorrow at ${hm(focus.startMs)}`);
    return t(`In ${days} Tagen`, `In ${days} days`);
  });
  /** Ring rechts oben: laeuft der Termin, wie viel schon vorbei ist; sonst die letzte Stunde davor. */
  const ring = $derived.by(() => {
    if (!focus || focus.allDay) return { share: 0, big: hm(now), small: t("jetzt", "now") };
    if (focus.startMs <= now) {
      const share = (now - focus.startMs) / Math.max(1, focus.endMs - focus.startMs);
      return { share, big: String(Math.max(1, Math.round((focus.endMs - now) / 60_000))), small: t("Min übrig", "min left") };
    }
    const minutes = Math.round((focus.startMs - now) / 60_000);
    if (minutes <= 60) return { share: 1 - minutes / 60, big: String(minutes), small: t("Min", "min") };
    return { share: 0, big: hm(focus.startMs), small: t("Beginn", "starts") };
  });
  const RING = 2 * Math.PI * 27;

  /** Zeitleiste: 08 bis 20 Uhr, erweitert, wenn Termine frueher oder spaeter liegen. */
  const hours = $derived.by(() => {
    let start = 8, end = 20;
    for (const e of dayEvents) {
      start = Math.min(start, Math.floor((Math.max(e.startMs, day) - day) / 3_600_000));
      end = Math.max(end, Math.ceil((Math.min(e.endMs, day + DAY) - day) / 3_600_000));
    }
    return { start, end };
  });
  const pos = (ms: number) => ((ms - day) / 3_600_000 - hours.start) / (hours.end - hours.start);
  const nowPos = $derived(midnight(now) === day ? pos(now) : null);

  /** Ueberlappende Termine bekommen eigene Zeilen. */
  const lanes = $derived.by(() => {
    const ends: number[] = [];
    return dayEvents.map((e) => {
      let lane = ends.findIndex((end) => end <= e.startMs);
      if (lane < 0) lane = ends.length;
      ends[lane] = e.endMs;
      return { e, lane: Math.min(lane, 2) };
    });
  });

  const dayLabel = $derived.by(() => {
    const diff = Math.round((day - midnight(now)) / DAY);
    if (diff === 0) return t("Heute", "Today");
    if (diff === 1) return t("Morgen", "Tomorrow");
    if (diff === -1) return t("Gestern", "Yesterday");
    return new Date(day).toLocaleDateString(locale, { weekday: "short", day: "numeric", month: "short" });
  });
  const monthLabel = $derived(new Date(day).toLocaleDateString(locale, { month: "long", year: "numeric" }));
  const sources = $derived([...new Set(events.map((e) => (e.source === "local" ? t("Eigene", "Mine") : e.source)))]);

  function shift(by: number) {
    selected = null;
    if (mode === "day") day = midnight(day + by * DAY + 3_600_000 * 2);
    else {
      const d = new Date(day);
      day = new Date(d.getFullYear(), d.getMonth() + by, 1).getTime();
    }
  }

  function color(source: string): string {
    if (source === "local") return app.settings.accent || "#64d2ff";
    const palette = ["#ff9f0a", "#30d158", "#bf5af2", "#ff6961", "#ffd60a"];
    let h = 0;
    for (const c of source) h = (h * 31 + c.charCodeAt(0)) % 997;
    return palette[h % palette.length];
  }

  function openForm() {
    const base = new Date(Math.max(now, day));
    const next = new Date(base.getFullYear(), base.getMonth(), base.getDate(), base.getHours() + 1);
    form = { title: "", date: isoDate(day), from: `${two(next.getHours())}:00`, to: `${two((next.getHours() + 1) % 24)}:00`, allDay: false };
    adding = true;
  }

  async function save() {
    error = "";
    const title = form.title.trim();
    if (!title) {
      error = t("Gib dem Termin einen Namen.", "Give the event a name.");
      return;
    }
    const [y, m, d] = form.date.split("-").map(Number);
    const at = (time: string) => {
      const [h, min] = time.split(":").map(Number);
      return new Date(y, m - 1, d, h || 0, min || 0).getTime();
    };
    const start = form.allDay ? new Date(y, m - 1, d).getTime() : at(form.from);
    let end = form.allDay ? start + DAY : at(form.to);
    if (end <= start) end += DAY;
    try {
      if (inTauri) await invoke("agenda_add", { title, startMs: start, endMs: end, allDay: form.allDay, location: null });
      else events = [...events, { id: String(Date.now()), title, startMs: start, endMs: end, allDay: form.allDay, location: "", source: "local" }];
      adding = false;
      void app.loadUpcoming();
      day = midnight(start);
      mode = "day";
      load();
    } catch (err) {
      error = String(err);
    }
  }

  async function remove(event: Event) {
    if (inTauri) await invoke("agenda_delete", { id: event.id });
    void app.loadUpcoming();
    selected = null;
    load();
  }
</script>

<div class="agenda" style:--glow={app.settings.accent || "#64d2ff"}>
  <div class="top">
    <div class="next" style:--c={focus ? color(focus.source) : "#48484a"}>
      {#if focus}
        <span class="lead">{lead}</span>
        <h2 class="title" title={focus.title}>{focus.title}</h2>
        <span class="where">
          {focus.allDay ? t("Ganztägig", "All day") : `${hm(focus.startMs)} – ${hm(focus.endMs)}`}{focus.location ? ` · ${focus.location}` : ""}{focus.source !== "local" ? ` · ${focus.source}` : ""}
          {#if focus.source === "local" && selected === focus.id}
            <button class="link" onclick={() => remove(focus)}>{t("Löschen", "Delete")}</button>
          {/if}
        </span>
      {:else}
        <span class="lead">{t("Nichts geplant", "Nothing planned")}</span>
        <h2 class="title quiet">{t("Der Tag gehört dir", "The day is yours")}</h2>
        <span class="where">{t("Mit + legst du einen Termin an; Kalender-Abos stehen in den Einstellungen.", "Add an event with +; calendar subscriptions live in Settings.")}</span>
      {/if}
    </div>
    <div class="side">
      <div class="controls">
        <div class="segments glass" use:lens={{ selector: "button.on" }} role="tablist" aria-label={t("Ansicht", "View")}>
          <span class="lens-thumb pick" aria-hidden="true"></span>
          <button role="tab" aria-selected={mode === "day"} class:on={mode === "day"} onclick={() => (mode = "day")}>{t("Tag", "Day")}</button>
          <button role="tab" aria-selected={mode === "month"} class:on={mode === "month"} onclick={() => (mode = "month")}>{t("Monat", "Month")}</button>
        </div>
        <button class="plus" onclick={openForm} aria-label={t("Termin anlegen", "Add event")} title={t("Termin anlegen", "Add event")}>
          <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
        </button>
      </div>
      <div class="ring" aria-hidden="true">
        <svg viewBox="0 0 64 64" width="64" height="64">
          <circle cx="32" cy="32" r="27" fill="none" stroke="rgba(255,255,255,0.12)" stroke-width="5" />
          <circle cx="32" cy="32" r="27" fill="none" stroke="var(--accent)" stroke-width="5" stroke-linecap="round" stroke-dasharray={RING} stroke-dashoffset={RING * (1 - Math.min(1, Math.max(0, ring.share)))} transform="rotate(-90 32 32)" />
        </svg>
        <span class="rbig">{ring.big}</span>
        <span class="rsmall">{ring.small}</span>
      </div>
    </div>
  </div>

  <div class="nav">
    <button class="arrow" onclick={() => shift(-1)} aria-label={t("Zurück", "Back")}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M10 3.5L5.5 8l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
    <button class="label" onclick={() => { day = midnight(now); selected = null; }} title={t("Zu heute", "Go to today")}>{mode === "day" ? dayLabel : monthLabel}</button>
    <button class="arrow" onclick={() => shift(1)} aria-label={t("Weiter", "Forward")}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M6 3.5L10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
    {#if mode === "day"}
      {#each allDayEvents as e (e.id)}
        <button class="allday" style:--c={color(e.source)} onclick={() => (selected = e.id)}>{e.title}</button>
      {/each}
    {/if}
  </div>

  {#if mode === "day"}
    <div class="timeline">
      <div class="track glass">
      <div class="lanes" style:--lanes={Math.max(1, ...lanes.map((l) => l.lane + 1))}>
        {#each lanes as { e, lane }, i (e.id)}
          {@const left = Math.max(0, pos(e.startMs))}
          {@const right = Math.min(1, pos(e.endMs))}
          <button
            class="block" class:past={e.endMs < now} class:sel={focus?.id === e.id}
            style:left="{left * 100}%" style:width="max(26px, {(right - left) * 100}%)" style:--lane={lane} style:--c={color(e.source)}
            onclick={() => (selected = selected === e.id ? null : e.id)}
            title="{e.title} · {hm(e.startMs)}–{hm(e.endMs)}"
            data-perch={focus?.id === e.id ? "center" : undefined}
            in:rise={{ delay: 60 + i * 50, y: 10 }}
          >
            {#if (right - left) * 760 > 70}<span>{e.title}</span>{/if}
          </button>
        {/each}
        {#if nowPos !== null && nowPos >= 0 && nowPos <= 1}
          <span class="now" style:left="{nowPos * 100}%"><i>{t("jetzt", "now")}</i></span>
        {/if}
      </div>
      </div>
      <div class="ruler">
        {#each Array.from({ length: hours.end - hours.start + 1 }, (_, i) => hours.start + i) as h}
          <span class="tick" class:major={(h - hours.start) % 2 === 0} style:left="{((h - hours.start) / (hours.end - hours.start)) * 100}%">
            {#if (h - hours.start) % 2 === 0}<b>{t(`${h % 24} Uhr`, `${h % 24}:00`)}</b>{/if}
          </span>
        {/each}
      </div>
    </div>
  {:else}
    <div class="month">
      {#each lang === "en" ? ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] : ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"] as w}<span class="wd">{w}</span>{/each}
      {#each Array.from({ length: 42 }, (_, i) => monthStart + i * DAY + 3_600_000 * 2) as cell, i}
        {@const d = midnight(cell)}
        {@const list = events.filter((e) => e.endMs > d && e.startMs < d + DAY)}
        <button
          class="cell" class:other={new Date(d).getMonth() !== new Date(day).getMonth()} class:today={d === midnight(now)}
          onclick={() => { day = d; mode = "day"; }}
          in:rise={{ delay: i * 8, y: 6, duration: 360 }}
        >
          <span class="num">{new Date(d).getDate()}</span>
          <span class="dots">{#each list.slice(0, 3) as e}<i style:background={color(e.source)}></i>{/each}</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="foot">
    <span>{sources.length ? sources.join(" · ") : t("Eigene Termine", "Your events")}</span>
    {#if error}<span class="error">{error}</span>{/if}
  </div>

  {#if adding}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div class="scrim" onclick={() => (adding = false)}></div>
    <form class="sheet glass" transition:pop onsubmit={(e) => { e.preventDefault(); save(); }}>
      <div class="sheet-title">{t("Neuer Termin", "New event")}</div>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" bind:value={form.title} placeholder={t("Was steht an?", "What's happening?")} maxlength="120" autofocus />
      <div class="row">
        <input class="field" type="date" bind:value={form.date} />
        {#if !form.allDay}
          <input class="field time" type="time" bind:value={form.from} aria-label={t("Von", "From")} />
          <span class="dash">–</span>
          <input class="field time" type="time" bind:value={form.to} aria-label={t("Bis", "To")} />
        {/if}
      </div>
      <label class="check"><input type="checkbox" bind:checked={form.allDay} /> {t("Ganztägig", "All day")}</label>
      <div class="row end">
        <button type="button" class="ghost" onclick={() => (adding = false)}>{t("Abbrechen", "Cancel")}</button>
        <button type="submit" class="primary">{t("Eintragen", "Add")}</button>
      </div>
    </form>
  {/if}
</div>

<style>
  .agenda {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 4px 30px calc(var(--bar, 62px) + 18px);
    overflow: hidden;
    /* Polarlicht aus der Akzentfarbe: oben rechts hell, nach unten auslaufend. */
    background:
      radial-gradient(45% 70% at 92% -10%, color-mix(in srgb, var(--glow) 34%, transparent), transparent 70%),
      radial-gradient(60% 60% at 10% 120%, color-mix(in srgb, var(--glow) 22%, #5e5ce6 10%), transparent 70%);
  }
  .top {
    display: flex;
    justify-content: space-between;
    gap: 20px;
  }
  .next {
    position: relative;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 14px;
  }
  /* Farbstreifen des Kalenders, aus dem der Termin stammt. */
  .next::before {
    content: "";
    position: absolute;
    left: 0;
    top: 3px;
    bottom: 3px;
    width: 4px;
    border-radius: 2px;
    background: var(--c);
  }
  .lead {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    color: var(--accent);
  }
  .title {
    margin: 0;
    font-size: var(--fs-display);
    line-height: 1.1;
    font-weight: var(--fw-bold);
    letter-spacing: -0.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 480px;
  }
  .title.quiet {
    color: var(--text-2);
  }
  .where {
    font-size: var(--fs-body);
    color: var(--text-3);
    display: flex;
    gap: 10px;
    align-items: baseline;
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    font: inherit;
    color: #ff6961;
    cursor: pointer;
  }
  .side {
    display: flex;
    flex-direction: row-reverse;
    align-items: center;
    gap: 14px;
  }
  .ring {
    position: relative;
    width: 64px;
    height: 64px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }
  .ring svg {
    position: absolute;
    inset: 0;
  }
  .ring circle:last-child {
    transition: stroke-dashoffset 900ms var(--ease-spring);
  }
  .rbig {
    font-size: var(--fs-headline);
    font-weight: var(--fw-bold);
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .rsmall {
    font-size: 9.5px;
    color: var(--text-3);
    margin-top: 2px;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .segments {
    display: flex;
    padding: 4px;
    border-radius: var(--r-full);
  }
  .segments button {
    position: relative;
    z-index: 1;
    width: 64px;
    height: 28px;
    border: 0;
    background: none;
    font: inherit;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    color: var(--text-2);
    cursor: pointer;
    transition: color var(--t-base) ease;
  }
  .segments button.on {
    color: #000;
  }
  .plus {
    width: 34px;
    height: 34px;
    border: 0;
    border-radius: 50%;
    background: var(--text);
    color: #000;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: transform var(--t-slow) var(--ease-spring);
  }
  .plus:active {
    transform: scale(0.9);
  }

  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 10px;
  }
  .arrow,
  .label {
    border: 0;
    background: none;
    color: var(--text-2);
    font: inherit;
    cursor: pointer;
    border-radius: var(--r-sm);
  }
  .arrow {
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
  }
  .label {
    height: 26px;
    padding: 0 6px;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .arrow:hover,
  .label:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #fff;
  }
  .allday {
    margin-left: 8px;
    height: 22px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-sm);
    font: inherit;
    font-size: var(--fs-footnote);
    background: color-mix(in srgb, var(--c) 28%, transparent);
    color: var(--text);
    cursor: pointer;
  }

  .timeline {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 8px;
  }
  /* Glasbahn, in der die Termine als Pillen liegen. */
  .track {
    padding: 10px 12px;
    border-radius: var(--r-lg);
  }
  .lanes {
    position: relative;
    height: calc(var(--lanes) * 40px - 6px);
  }
  .block {
    position: absolute;
    top: calc(var(--lane) * 40px);
    height: 34px;
    border: 0;
    border-radius: var(--r-lg);
    padding: 0 10px;
    box-sizing: border-box;
    background: linear-gradient(180deg, color-mix(in srgb, var(--c) 92%, #fff), var(--c));
    color: #000;
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.45), 0 4px 14px color-mix(in srgb, var(--c) 30%, transparent);
    transition: transform var(--t-slow) var(--ease-spring), filter var(--t-base) ease, opacity var(--t-base) ease;
  }
  .block span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .block.past {
    opacity: 0.4;
    filter: saturate(0.4);
  }
  .block:hover {
    transform: translateY(-2px);
  }
  .block.sel {
    outline: 2px solid #fff;
    outline-offset: 2px;
  }
  .block:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .now {
    position: absolute;
    top: -6px;
    bottom: -6px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--accent);
    box-shadow: 0 0 12px var(--accent);
    pointer-events: none;
  }
  .now i {
    position: absolute;
    bottom: calc(100% + 4px);
    left: 50%;
    transform: translateX(-50%);
    padding: 1px 6px;
    border-radius: var(--r-xs);
    background: var(--accent);
    color: #000;
    font-style: normal;
    font-size: var(--fs-caption);
    font-weight: var(--fw-bold);
  }
  .ruler {
    position: relative;
    height: 26px;
    margin: 0 12px;
  }
  .tick {
    position: absolute;
    top: 2px;
    width: 3px;
    height: 3px;
    margin-left: -1.5px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.25);
  }
  .tick.major {
    background: rgba(255, 255, 255, 0.55);
  }
  .tick b {
    position: absolute;
    top: 12px;
    left: 0;
    transform: translateX(-50%);
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    color: var(--text-3);
    white-space: nowrap;
  }
  .tick:first-child b {
    transform: none;
  }
  .tick:last-child b {
    transform: translateX(-100%);
  }

  .month {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    grid-template-rows: auto repeat(6, 1fr);
    gap: 4px;
    margin-top: 6px;
  }
  .wd {
    font-size: var(--fs-caption);
    color: var(--text-4);
    text-align: center;
    padding-bottom: 2px;
  }
  .cell {
    border: 0;
    border-radius: var(--r-sm);
    background: rgba(255, 255, 255, 0.035);
    color: var(--text-2);
    font: inherit;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    cursor: pointer;
    min-height: 0;
    transition: background var(--t-fast) ease;
  }
  .cell:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .cell.other {
    color: #48484a;
  }
  .cell.today {
    background: var(--text);
    color: #000;
    font-weight: var(--fw-bold);
  }
  .num {
    font-size: var(--fs-footnote);
    font-variant-numeric: tabular-nums;
  }
  .dots {
    display: flex;
    gap: 2px;
  }
  .dots i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
  }

  .foot {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-top: 8px;
    font-size: var(--fs-footnote);
    color: var(--text-4);
  }
  .error {
    color: #ff9f8a;
    font-family: var(--font-sans);
  }

  .scrim {
    position: absolute;
    inset: 0;
    z-index: 6;
    background: rgba(0, 0, 0, 0.35);
  }
  .sheet {
    position: absolute;
    z-index: 7;
    top: 50px;
    right: 30px;
    width: 360px;
    padding: 14px;
    border-radius: var(--r-lg);
    display: flex;
    flex-direction: column;
    gap: 10px;
    transform-origin: top right;
  }
  .sheet-title {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .row.end {
    justify-content: flex-end;
  }
  .field {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(0, 0, 0, 0.35);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    color-scheme: dark;
  }
  .field:focus {
    outline: 2px solid var(--focus);
    outline-offset: -1px;
  }
  .field.time {
    flex: 0 0 92px;
  }
  .dash {
    color: var(--text-3);
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: var(--fs-body);
    color: var(--text-2);
  }
  .ghost,
  .primary {
    height: 32px;
    padding: 0 14px;
    border: 0;
    border-radius: var(--r-lg);
    font: inherit;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .ghost {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text);
  }
  .primary {
    background: var(--text);
    color: #000;
  }
</style>
