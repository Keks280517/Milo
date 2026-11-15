<script lang="ts">
  import { t } from "../i18n";
  // Wetter als ganze Flaeche: der Himmel fuellt die Ansicht, links Ort und grosse Temperatur mit
  // vier Messwerten, rechts sieben Tage als Temperaturbalken, unten die naechsten Stunden als Glasleiste.
  // Abgerufen wird nur, wenn der Nutzer die Ansicht oeffnet oder es ausdruecklich anstoesst.
  import EmptyState from "../EmptyState.svelte";
  import Sky from "../Sky.svelte";
  import WeatherIcon from "../cards/WeatherIcon.svelte";
  import { onMount, tick } from "svelte";
  import { app } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoWeather } from "../demoData";
  import { rise } from "../motion";

  let place = $state("");
  let editing = $state(false);
  let input = $state<HTMLInputElement>();

  onMount(() => {
    // Beim Oeffnen frisch holen, wenn das letzte Wetter aelter als 15 Minuten ist.
    if (app.settings.weatherPlace && (!app.weather || Date.now() - app.weatherAt > 15 * 60_000)) app.refreshWeather();
  });

  function search() {
    if (!place.trim()) return;
    app.refreshWeather(place);
    place = "";
    editing = false;
  }

  async function edit() {
    editing = true;
    await tick();
    input?.focus();
  }

  // Tour: ohne eingerichteten Ort ein Beispielwetter, damit man sieht, wie es aussieht.
  const w = $derived(tour.demo === "weather" && !app.weather ? demoWeather() : app.weather);
  const today = $derived(w?.days[0]);
  /** Spannweite aller Tage fuer die Temperaturbalken. */
  const range = $derived.by(() => {
    if (!w || w.days.length === 0) return { lo: 0, hi: 1 };
    const lo = Math.min(...w.days.map((d) => d.min));
    const hi = Math.max(...w.days.map((d) => d.max));
    return { lo, hi: Math.max(hi, lo + 1) };
  });
  const pos = (t: number) => ((t - range.lo) / (range.hi - range.lo)) * 100;
  const clockOf = (s: string) => s.match(/(\d{2}:\d{2})/)?.[1] ?? "–";
</script>

<div class="weather">
  {#if w}
    <Sky code={w.now.code} isDay={w.now.isDay} />
    <div class="layout">
      <section class="now">
        {#if editing}
          <form class="find glass" onsubmit={(e) => { e.preventDefault(); search(); }}>
            <input bind:this={input} bind:value={place} placeholder={t("Ort, z. B. München", "Place, e.g. London")} maxlength="80" spellcheck="false" onblur={() => !place && (editing = false)} />
            <button type="submit" disabled={!place.trim() || app.weatherBusy}>{t("Anzeigen", "Show")}</button>
          </form>
        {:else}
          <button class="place" onclick={edit} title={t("Anderen Ort wählen", "Choose another place")}>
            {w.place}
            <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"><path d="M3.5 6l4.5 4.5L12.5 6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
          </button>
        {/if}
        <div class="temp">{Math.round(w.now.temp)}°</div>
        <div class="cond">{w.now.text}{today ? ` · ${t("H", "H")} ${Math.round(today.max)}° ${t("T", "L")} ${Math.round(today.min)}°` : ""}</div>
        <div class="facts">
          <span class="fact glass"><small>{t("Gefühlt", "Feels like")}</small><b>{Math.round(w.now.feels)}°</b></span>
          <span class="fact glass"><small>{t("Wind", "Wind")}</small><b>{Math.round(w.now.wind)} km/h</b></span>
          <span class="fact glass"><small>{t("Feuchte", "Humidity")}</small><b>{Math.round(w.now.humidity)} %</b></span>
          {#if today}<span class="fact glass"><small>{t("Sonne", "Sun")}</small><b>{clockOf(today.sunrise)} – {clockOf(today.sunset)}</b></span>{/if}
        </div>
      </section>

      <section class="days glass" aria-label={t("Die nächsten Tage", "The next days")}>
        {#each w.days.slice(0, 6) as d, i (d.date)}
          <div class="dayrow" in:rise={{ delay: 40 + i * 40, y: 8 }}>
            <span class="dname">{i === 0 ? t("Heute", "Today") : d.label}</span>
            <WeatherIcon code={d.code} isDay={true} size={20} />
            <span class="rain">{d.rain >= 20 ? `${Math.round(d.rain)} %` : ""}</span>
            <span class="lo">{Math.round(d.min)}°</span>
            <span class="bar"><i style:left="{pos(d.min)}%" style:right="{100 - pos(d.max)}%"></i></span>
            <span class="hi">{Math.round(d.max)}°</span>
          </div>
        {/each}
      </section>
    </div>

    <div class="hours glass" aria-label={t("Die nächsten Stunden", "The next hours")}>
      {#each w.hours.slice(0, 14) as h, i (h.hour + i)}
        <div class="hour" class:first={i === 0}>
          <small>{i === 0 ? t("Jetzt", "Now") : h.hour}</small>
          <WeatherIcon code={h.code} isDay={h.isDay} size={22} />
          <b>{Math.round(h.temp)}°</b>
          {#if h.rain >= 30}<em>{Math.round(h.rain)} %</em>{/if}
        </div>
      {/each}
    </div>
    {#if app.weatherError}<p class="error">{app.weatherError}</p>{/if}
  {:else}
    <div class="empty">
      {#if app.weatherBusy}
        <EmptyState kind="weather" title={t("Hole das Wetter …", "Getting the weather …")} />
      {:else}
        <EmptyState kind="weather" title={t("Wo bist du gerade?", "Where are you right now?")} text={t("Gib einen Ort ein, Milo merkt ihn sich.", "Enter a place and Milo will remember it.")} />
        <form class="find glass solo" onsubmit={(e) => { e.preventDefault(); search(); }}>
          <input bind:value={place} placeholder={t("Ort, z. B. München", "Place, e.g. London")} maxlength="80" spellcheck="false" />
          <button type="submit" disabled={!place.trim()}>{t("Anzeigen", "Show")}</button>
        </form>
      {/if}
      {#if app.weatherError}<p class="error">{app.weatherError}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .weather {
    position: relative;
    isolation: isolate;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0 14px;
    padding: 16px 18px 12px;
    border-radius: var(--r-lg);
    overflow: hidden;
    color: #fff;
  }
  .layout {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 330px;
    gap: 18px;
  }
  .now {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .place {
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    padding: 2px 0;
    background: none;
    color: #fff;
    font: inherit;
    font-size: var(--fs-headline);
    font-weight: var(--fw-semibold);
    cursor: pointer;
    text-shadow: 0 1px 6px rgba(0, 0, 0, 0.25);
  }
  .temp {
    font-size: 64px;
    font-weight: 200;
    letter-spacing: -0.04em;
    line-height: 1;
    text-shadow: 0 2px 12px rgba(0, 0, 0, 0.2);
  }
  .cond {
    margin-top: 2px;
    font-size: var(--fs-callout);
    font-weight: var(--fw-medium);
    color: rgba(255, 255, 255, 0.9);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 6px;
    margin-top: auto;
  }
  .fact {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 10px;
    border-radius: var(--r-md);
  }
  .fact small {
    position: relative;
    font-size: var(--fs-caption);
    color: rgba(255, 255, 255, 0.75);
  }
  .fact b {
    position: relative;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
  }
  .days {
    align-self: start;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 12px;
    border-radius: var(--r-lg);
  }
  .dayrow {
    position: relative;
    display: grid;
    grid-template-columns: 52px 22px 34px 30px 1fr 30px;
    align-items: center;
    gap: 6px;
    height: 23px;
    font-size: var(--fs-footnote);
  }
  .dname {
    font-weight: var(--fw-semibold);
  }
  .rain {
    font-size: var(--fs-caption);
    color: #9fd8ff;
    font-weight: var(--fw-semibold);
  }
  .lo {
    color: rgba(255, 255, 255, 0.65);
    text-align: right;
  }
  .hi {
    font-weight: var(--fw-semibold);
  }
  .bar {
    position: relative;
    height: 5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.18);
  }
  .bar i {
    position: absolute;
    top: 0;
    bottom: 0;
    border-radius: 3px;
    background: linear-gradient(90deg, #8fd3ff, var(--warn), #ff9f0a);
  }
  .hours {
    display: flex;
    gap: 2px;
    padding: 8px 8px;
    border-radius: var(--r-lg);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .hour {
    position: relative;
    flex: 1 0 52px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 2px 0;
    border-radius: var(--r-md);
  }
  .hour.first {
    background: rgba(255, 255, 255, 0.14);
  }
  .hour small {
    font-size: var(--fs-caption);
    color: rgba(255, 255, 255, 0.8);
  }
  .hour b {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .hour em {
    font-style: normal;
    font-size: var(--fs-caption);
    color: #9fd8ff;
    font-weight: var(--fw-semibold);
  }
  .find {
    display: flex;
    gap: 6px;
    padding: 4px 4px 4px 12px;
    border-radius: var(--r-full);
    max-width: 320px;
  }
  .find.solo {
    margin-top: 10px;
  }
  .find input {
    position: relative;
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    color: #fff;
    font: inherit;
    font-size: var(--fs-body);
  }
  .find input::placeholder {
    color: rgba(255, 255, 255, 0.6);
  }
  .find button {
    position: relative;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-full);
    background: #fff;
    color: #000;
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .find button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text);
  }
  .error {
    margin: 0;
    font-size: var(--fs-footnote);
    color: #ffb4a8;
  }
</style>
