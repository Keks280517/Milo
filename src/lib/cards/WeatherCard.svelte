<script lang="ts">
  import { t, lang } from "../i18n";
  // Wetter-Vorlage: Der Kern liefert nur die Daten, hier entsteht daraus die Ansicht.
  import type { Weather } from "../app.svelte";
  import { round, skyGradient, tempColor } from "../weather";
  import WeatherIcon from "./WeatherIcon.svelte";

  interface Props {
    weather: Weather;
    /** Kompakte Fassung fuer den Gespraechsverlauf: ohne Tagesliste, weniger Stunden. */
    compact?: boolean;
  }
  let { weather, compact = false }: Props = $props();

  const hours = $derived(weather.hours.slice(0, compact ? 9 : 12));
  const days = $derived(compact ? [] : weather.days.slice(0, 5));
  // Gemeinsame Skala fuer die Temperaturbalken aller Tage.
  const low = $derived(Math.min(...days.map((d) => d.min)));
  const high = $derived(Math.max(...days.map((d) => d.max)));
  const scale = (t: number) => (high > low ? ((t - low) / (high - low)) * 100 : 50);
  const today = $derived(weather.days[0]);
  const shortDay = (label: string) => (["Heute", "Morgen", "Today", "Tomorrow"].includes(label) ? label : label.slice(0, lang === "en" ? 3 : 2));
  const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);
</script>

<article class="weather" class:compact style:background={skyGradient(weather.now.code, weather.now.isDay)}>
  <div class="top">
    <div class="main">
      <div class="where">
        <div>
          <div class="place">{weather.place}</div>
          {#if weather.region}<div class="region">{weather.region}</div>{/if}
        </div>
        <WeatherIcon code={weather.now.code} isDay={weather.now.isDay} size={compact ? 44 : 48} animated />
      </div>
      <div class="now">
        <div class="temp">{round(weather.now.temp)}<span>°</span></div>
        <div class="facts">
          <div class="text">{capital(weather.now.text)}</div>
          {#if today}<div class="range-text">{round(today.min)}° {t("bis", "to")} {round(today.max)}°</div>{/if}
        </div>
      </div>
      <div class="more">
        {t("Gefühlt", "Feels like")} {round(weather.now.feels)}° · {t("Wind", "Wind")} {round(weather.now.wind)} km/h · {t("Feuchte", "Humidity")} {round(weather.now.humidity)} %
      </div>
    </div>

    {#if days.length > 0}
      <div class="days">
        {#each days as day}
          <div class="day">
            <span class="name">{shortDay(day.label)}</span>
            <WeatherIcon code={day.code} size={20} />
            <span class="rain" class:none={day.rain < 20}>{round(day.rain)} %</span>
            <span class="min">{round(day.min)}°</span>
            <span class="track">
              <span
                class="range"
                style:left="{scale(day.min)}%"
                style:right="{100 - scale(day.max)}%"
                style:background="linear-gradient(90deg, {tempColor(day.min)}, {tempColor(day.max)})"
              ></span>
            </span>
            <span class="max">{round(day.max)}°</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  {#if hours.length > 0}
    <div class="hours">
      {#each hours as hour, i}
        <div class="hour">
          <span class="when">{i === 0 ? t("Jetzt", "Now") : hour.hour}</span>
          <WeatherIcon code={hour.code} isDay={hour.isDay} size={22} />
          <span class="deg">{round(hour.temp)}°</span>
          <span class="rain" class:none={hour.rain < 20}>{round(hour.rain)} %</span>
        </div>
      {/each}
    </div>
  {/if}

  {#if !compact && today?.sunrise}
    <footer>
      <span>{t("Sonnenaufgang", "Sunrise")} {today.sunrise}</span>
      <span>{t("Sonnenuntergang", "Sunset")} {today.sunset}</span>
      <span class="source">{t("Quelle", "Source")}: Open-Meteo</span>
    </footer>
  {/if}
</article>

<style>
  .weather {
    box-sizing: border-box;
    width: 100%;
    border-radius: var(--r-lg);
    padding: 14px 14px 12px 16px;
    color: #fff;
    font-variant-numeric: tabular-nums;
    animation: rise var(--t-slow) cubic-bezier(0.3, 1.2, 0.4, 1) both;
  }
  .top {
    display: flex;
    gap: 14px;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .where {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
  }
  .place {
    font-size: var(--fs-headline);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.01em;
  }
  .region {
    font-size: var(--fs-footnote);
    opacity: 0.72;
    margin-top: 1px;
  }
  .now {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 2px;
  }
  .temp {
    font-size: 52px;
    font-weight: 300;
    line-height: 1.05;
    letter-spacing: -0.04em;
  }
  .compact .temp {
    font-size: 44px;
  }
  .temp span {
    font-weight: 200;
    opacity: 0.85;
  }
  .text {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
  }
  .range-text {
    font-size: var(--fs-footnote);
    opacity: 0.8;
    margin-top: 1px;
  }
  .more {
    font-size: var(--fs-footnote);
    opacity: 0.78;
    margin-top: auto;
    padding-top: 4px;
  }

  .days {
    flex: none;
    width: 262px;
    padding: 4px 12px;
    border-radius: var(--r-lg);
    background: rgba(0, 0, 0, 0.16);
  }
  .day {
    display: grid;
    grid-template-columns: 50px 22px 34px 26px 1fr 26px;
    align-items: center;
    gap: 6px;
    height: 27px;
    font-size: var(--fs-footnote);
  }
  .day + .day {
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }
  .name {
    font-weight: var(--fw-semibold);
  }
  .min {
    text-align: right;
    opacity: 0.72;
  }
  .max {
    font-weight: var(--fw-semibold);
  }
  .track {
    position: relative;
    height: 5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.22);
  }
  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 6px;
    border-radius: 3px;
  }

  .hours {
    display: flex;
    justify-content: space-between;
    gap: 2px;
    margin-top: 10px;
    padding: 8px 6px 8px;
    border-radius: var(--r-lg);
    background: rgba(0, 0, 0, 0.16);
  }
  .hour {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .when {
    font-size: var(--fs-caption);
    opacity: 0.78;
  }
  .deg {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .rain {
    font-size: var(--fs-caption);
    color: #a8e3ff;
  }
  .rain.none {
    visibility: hidden;
  }
  /* Regnet es in keiner der gezeigten Stunden, faellt die Zeile ganz weg. */
  .hours:not(:has(.rain:not(.none))) .rain {
    display: none;
  }

  footer {
    display: flex;
    gap: 16px;
    margin-top: 8px;
    padding: 0 4px;
    font-size: var(--fs-footnote);
    opacity: 0.8;
  }
  .source {
    margin-left: auto;
    opacity: 0.8;
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(8px) scale(0.98); }
    to { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .weather { animation: none; }
  }
</style>
