<script lang="ts">
  import { t } from "../i18n";
  // Zeigt zu einem Werkzeug-Ergebnis die passende fertige Ansicht.
  import { openPath, openUrl } from "@tauri-apps/plugin-opener";
  import { app, clock, inTauri, niceDate, type Card } from "../app.svelte";
  import WeatherMini from "./WeatherMini.svelte";
  import TimerCard from "./TimerCard.svelte";
  import FileRow from "./FileRow.svelte";
  import { formatAnswer } from "../format";

  let { card }: { card: Card } = $props();

  function host(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  /** Ruhige, aber unterscheidbare Farbe je Quelle, abgeleitet aus dem Namen. */
  function tint(text: string): string {
    let hash = 0;
    for (const ch of text) hash = (hash * 31 + ch.charCodeAt(0)) % 360;
    return `hsl(${hash} 45% 42%)`;
  }

  function open(url: string) {
    if (inTauri) openUrl(url);
  }

  // Herunterfahren: Restzeit mitzaehlen, bis es so weit ist oder abgebrochen wurde.
  let powerNow = $state(Date.now());
  let abortedHere = $state(false);
  // Der Knopf reagiert erst nach dieser Zeit: Am 16.10. hat ein zweiter Klick direkt nach dem
  // Bestaetigen das Herunterfahren versehentlich wieder abgebrochen.
  const ARM_MS = 2000;
  const shownAt = Date.now();
  /** Dieser Plan gilt laut Kern nicht mehr (per Sprache abgebrochen oder durch einen neuen ersetzt). */
  const gone = $derived(
    card.type === "power" &&
      app.powerPlan !== undefined &&
      (app.powerPlan === null || Math.abs(app.powerPlan.dueMs - card.atMs) > 3000) &&
      card.atMs > powerNow,
  );
  const replaced = $derived(gone && !!app.powerPlan);
  const aborted = $derived(abortedHere || gone);
  const armed = $derived(powerNow - shownAt >= ARM_MS);
  $effect(() => {
    if (card.type !== "power" || aborted) return;
    const timer = setInterval(() => (powerNow = Date.now()), 250);
    return () => clearInterval(timer);
  });

  async function abort() {
    if (!armed) return;
    try {
      await app.abortPower();
    } catch {
      // Nichts mehr abzubrechen (schon abgebrochen oder bereits ausgefuehrt).
    }
    abortedHere = true;
  }
</script>

{#if card.type === "weather"}
  <WeatherMini weather={card} />
{:else if card.type === "timer"}
  <TimerCard id={card.id} label={card.label} />
{:else if card.type === "sources"}
  <article class="card sources">
    <div class="head">{t(`Quellen zu „${card.query}“`, `Sources for "${card.query}"`)}</div>
    {#each card.items.slice(0, 5) as item}
      <button class="source" onclick={() => open(item.url)} title={item.url}>
        <span class="badge" style:background={tint(host(item.url))}>{host(item.url).charAt(0).toUpperCase()}</span>
        <span class="lines">
          <span class="title">{item.title}</span>
          <span class="host">{host(item.url)}</span>
        </span>
        <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
          <path d="M4.2 2.4h5.4v5.4M9.4 2.6 2.6 9.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
    {/each}
  </article>
{:else if card.type === "note"}
  <article class="card row">
    <span class="icon yellow">
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <path d="M4.6 2.6h6.8v10.8L8 11.1l-3.4 2.3Z" fill="currentColor" />
      </svg>
    </span>
    <span class="lines">
      <span class="kicker">{t("Gemerkt", "Remembered")}</span>
      <span class="body">{card.text}</span>
    </span>
    <button class="chip" onclick={() => (app.tab = "notes")}>{t("Notizen", "Notes")}</button>
  </article>
{:else if card.type === "notes"}
  <article class="card">
    <div class="head">{t("Aus dem Gedächtnis", "From memory")}</div>
    {#each card.items.slice(0, 4) as note}
      <div class="note">
        <span class="body">{note.text}</span>
        <span class="host">{niceDate(note.createdAt)}</span>
      </div>
    {/each}
  </article>
{:else if card.type === "files"}
  <article class="card files">
    {#each card.items.slice(0, 5) as file (file.id)}
      <FileRow {file} />
    {/each}
  </article>
{:else if card.type === "app"}
  <article class="card row slim">
    <span class="icon blue">
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <rect x="2.4" y="2.4" width="4.6" height="4.6" rx="1.2" fill="currentColor" />
        <rect x="9" y="2.4" width="4.6" height="4.6" rx="1.2" fill="currentColor" opacity="0.7" />
        <rect x="2.4" y="9" width="4.6" height="4.6" rx="1.2" fill="currentColor" opacity="0.7" />
        <rect x="9" y="9" width="4.6" height="4.6" rx="1.2" fill="currentColor" opacity="0.45" />
      </svg>
    </span>
    <span class="lines">
      <span class="kicker">{t("Geöffnet", "Opened")}</span>
      <span class="body one">{card.name}</span>
    </span>
  </article>
{:else if card.type === "link"}
  <button class="card row slim link" onclick={() => open(card.url)} title={card.url}>
    <span class="icon blue">
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.4" />
        <path d="M2.4 8h11.2M8 2.4c-2 1.6-2 9.6 0 11.2M8 2.4c2 1.6 2 9.6 0 11.2" fill="none" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </span>
    <span class="lines">
      <span class="kicker">{card.title}</span>
      <span class="body one">{host(card.url)}</span>
    </span>
  </button>
{:else if card.type === "power"}
  <article class="card row">
    <span class="icon red">
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <path d="M8 2.2v5.4M4.5 4.4a5 5 0 1 0 7 0" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      </svg>
    </span>
    <span class="lines">
      <span class="kicker">{replaced ? t("Ersetzt", "Replaced") : aborted ? t("Abgebrochen", "Cancelled") : card.title}</span>
      <span class="body one">
        {#if replaced}{t("Es gilt der neue Zeitpunkt.", "The new time applies.")}{:else if aborted}{t("Der PC läuft weiter.", "The PC keeps running.")}{:else if card.atMs > powerNow}{t("in", "in")} {clock(card.atMs - powerNow)}{:else}{t("jetzt", "now")}{/if}
      </span>
    </span>
    {#if !aborted && card.atMs > powerNow}
      <button class="chip strong" class:waiting={!armed} disabled={!armed} onclick={abort} title={armed ? "" : t("Gleich bereit", "Ready in a moment")}>{t("Abbrechen", "Cancel")}</button>
    {/if}
  </article>
{:else if card.type === "document"}
  <button class="card row slim link" onclick={() => inTauri && openPath(card.path)} title={card.path}>
    <span class="icon word">
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <path d="M4 1.8h5.6L12.6 5v9.2H4Z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" />
        <path d="M5.8 7.4l.9 4 1.3-3 1.3 3 .9-4" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </span>
    <span class="lines">
      <span class="kicker">{t("Word-Datei gespeichert", "Word file saved")}</span>
      <span class="body one">{card.title}</span>
    </span>
    <span class="chip">{t("Öffnen", "Open")}</span>
  </button>
{:else if card.type === "image"}
  <button class="card row slim link" onclick={() => inTauri && openPath(card.path)} title={card.path}>
    <span class="icon">
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <rect x="2" y="3" width="12" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.4" />
        <path d="M3.5 11.5l3-3 2 2 1.5-1.5 2.5 2.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" />
        <circle cx="10.5" cy="6" r="1" fill="currentColor" />
      </svg>
    </span>
    <span class="lines">
      <span class="kicker">{t("Bildschirmfoto gespeichert", "Screenshot saved")}</span>
      <span class="body one">{card.title}</span>
    </span>
    <span class="chip">{t("Öffnen", "Open")}</span>
  </button>
{:else if card.type === "sketch"}
  <details class="card sketch">
    <summary class="kicker">{t("Vorüberlegung von Groq", "Groq's first draft")}</summary>
    <div class="sketch-text">{@html formatAnswer(card.text)}</div>
  </details>
{:else if card.type === "script"}
  <article class="card script">
    <span class="kicker">{t("Skript ausgeführt", "Script ran")}</span>
    <pre>{card.code}</pre>
    <span class="kicker">{t("Ausgabe", "Output")}</span>
    <pre class="out">{card.output}</pre>
  </article>
{/if}

<style>
  .sketch {
    padding: 8px 12px;
    border-radius: var(--r-md);
    background: #14121c;
    box-shadow: inset 0 0 0 1px #2a2440;
  }
  .sketch summary {
    cursor: pointer;
    color: #b9a8ff;
    list-style: none;
  }
  .sketch-text {
    margin-top: 6px;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-2);
  }
  .script {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .script pre {
    margin: 0 0 4px;
    padding: 8px 10px;
    max-height: 120px;
    overflow: auto;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
    scrollbar-width: thin;
    scrollbar-color: #3a3a3c transparent;
  }
  .script .out {
    color: var(--text);
  }
  .card {
    box-sizing: border-box;
    width: 100%;
    padding: 10px 12px;
    border-radius: var(--r-lg);
    background: var(--card);
    box-shadow: var(--card-edge);
    border: 1px solid var(--line);
    color: inherit;
    font: inherit;
    text-align: left;
    animation: rise var(--t-slow) cubic-bezier(0.3, 1.2, 0.4, 1) both;
  }
  .files {
    padding: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .slim {
    width: auto;
    max-width: 100%;
    padding: 8px 14px 8px 8px;
  }
  .link {
    cursor: pointer;
    transition: background var(--t-fast) ease, border-color var(--t-fast) ease;
  }
  .link:hover {
    background: #19191c;
    border-color: #34343a;
  }
  .head {
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
    padding: 1px 2px 6px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .icon {
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: var(--r-sm);
    display: grid;
    place-items: center;
  }
  .icon.yellow { background: #3a3110; color: #ffd60a; }
  .icon.blue { background: #10303c; color: #64d2ff; }
  .icon.red { background: #3c1512; color: #ff6961; }
  .icon.word { background: #12233d; color: #7ab4ff; }
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .kicker {
    font-size: var(--fs-footnote);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .body {
    font-size: var(--fs-callout);
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .body.one {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: var(--fw-medium);
  }
  .host {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }

  .source {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 6px;
    border-radius: var(--r-sm);
    cursor: pointer;
    color: var(--text-4);
    transition: background var(--t-fast) ease, color var(--t-fast) ease;
  }
  .source:hover {
    background: #1d1d20;
    color: var(--text-2);
  }
  .badge {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: var(--r-sm);
    display: grid;
    place-items: center;
    color: #fff;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
  }
  .title {
    font-size: var(--fs-body);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 2px;
  }
  .note + .note {
    border-top: 1px solid var(--line);
  }

  .chip {
    flex: none;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--chip);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
    transition: background var(--t-fast) ease;
  }
  .chip:hover {
    background: #2e2e32;
  }
  .chip.strong {
    background: var(--text);
    color: #000;
  }
  .chip.waiting {
    opacity: 0.35;
    cursor: default;
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(8px) scale(0.98); }
    to { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .card { animation: none; }
  }
</style>
