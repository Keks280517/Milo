<script lang="ts">
  import { t } from "../i18n";
  // Kleine Wetterkarte fuer den Chat: eine Zeile mit dem Wichtigsten. Ein Klick oeffnet den Wetter-Reiter.
  import { app, type Weather } from "../app.svelte";
  import { round, skyGradient } from "../weather";
  import WeatherIcon from "./WeatherIcon.svelte";

  let { weather }: { weather: Weather } = $props();

  const today = $derived(weather.days[0]);
  const hours = $derived(weather.hours.slice(1, 6));
  const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

  function openDetails() {
    app.weather = weather;
    app.tab = "weather";
  }
</script>

<button class="mini" style:background={skyGradient(weather.now.code, weather.now.isDay)} onclick={openDetails} title={t("Alle Details im Wetter-Reiter", "All details in the Weather tab")}>
  <WeatherIcon code={weather.now.code} isDay={weather.now.isDay} size={32} animated />
  <span class="temp">{round(weather.now.temp)}°</span>
  <span class="lines">
    <span class="place">{weather.place}</span>
    <span class="text">{capital(weather.now.text)}{#if today}{` · ${round(today.min)}° ${t("bis", "to")} ${round(today.max)}°`}{/if}</span>
  </span>
  <span class="hours">
    {#each hours as hour}
      <span class="hour">
        <span class="when">{hour.hour}</span>
        <WeatherIcon code={hour.code} isDay={hour.isDay} size={16} />
        <span class="deg">{round(hour.temp)}°</span>
      </span>
    {/each}
  </span>
</button>

<style>
  .mini {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-radius: var(--r-lg);
    color: #fff;
    cursor: pointer;
    transition: transform var(--t-fast) ease, filter var(--t-fast) ease;
  }
  .mini:hover {
    filter: brightness(1.08);
  }
  .mini:active {
    transform: scale(0.99);
  }
  .temp {
    font-size: var(--fs-title);
    font-weight: 300;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.3;
  }
  .place {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .text {
    font-size: var(--fs-footnote);
    color: rgba(255, 255, 255, 0.8);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hours {
    display: flex;
    gap: 8px;
  }
  .hour {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    font-size: var(--fs-caption);
    font-variant-numeric: tabular-nums;
  }
  .when {
    color: rgba(255, 255, 255, 0.72);
  }
  .deg {
    font-weight: var(--fw-semibold);
  }
</style>
