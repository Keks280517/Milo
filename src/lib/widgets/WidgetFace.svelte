<script lang="ts">
  // Das sichtbare Widget: Form (rund, Pille, Karte), Optik (haengend wie die Insel oder frei
  // stehend) und die Bausteine. Jeder Baustein liefert Beschriftung, Wert und, wo es passt,
  // einen Prozentwert. Runde Widgets zeigen Prozentwerte als Ringe ineinander.
  // Wird im Widget-Fenster und als Vorschau im Editor benutzt.
  import WeatherIcon from "../cards/WeatherIcon.svelte";
  import type { Feed } from "./feed.svelte";
  import { viewOf } from "./blocks";
  import { faceSize, type Widget } from "./types";

  interface Props {
    widget: Widget;
    feed: Feed;
  }
  let { widget, feed }: Props = $props();

  const size = $derived(faceSize(widget.shape, widget.size));
  const scale = $derived(widget.size === "s" ? 0.85 : widget.size === "l" ? 1.2 : 1);
  const hanging = $derived(widget.style === "hanging");

  const views = $derived(widget.blocks.map((b) => viewOf(b, feed)));
  /** Rund und alles Anteile: als Ringe ineinander zeichnen. */
  // Rund: Anteile werden Ringe (hoechstens zwei); andere Werte stehen klein in der Mitte.
  const ringViews = $derived(views.filter((v) => v.pct !== null).slice(0, 2));
  const extras = $derived(views.filter((v) => !ringViews.includes(v)));
  const rings = $derived(widget.shape === "round" && ringViews.length > 0);

  function tone(value: number): string {
    return value >= 90 ? "#ff453a" : value >= 75 ? "#ffd60a" : widget.accent;
  }

  /** Zweiter Ring in einem verwandten Ton. */
  function ringColor(index: number, value: number): string {
    if (value >= 75) return tone(value);
    return index === 0 ? widget.accent : `color-mix(in srgb, ${widget.accent} 45%, #f5f5f7)`;
  }

  const RING = [15.5, 11];

  /** Kurve als SVG-Pfad in einem 100x30-Feld. */
  function sparkPath(values: number[]): string {
    if (values.length < 2) return "";
    const step = 100 / (values.length - 1);
    return values.map((v, i) => `${i === 0 ? "M" : "L"}${(i * step).toFixed(1)} ${(28 - (Math.min(100, Math.max(0, v)) / 100) * 26).toFixed(1)}`).join(" ");
  }
  const circumference = (r: number) => 2 * Math.PI * r;
</script>

<div
  class="face {widget.shape}"
  class:hanging
  class:floating={!hanging}
  style:width="{size.w}px"
  style:height="{size.h}px"
  style:--h="{size.h}px"
  style:--accent={widget.accent}
  style:--scale={scale}
>
  {#if rings}
    <svg class="rings" viewBox="0 0 36 36" aria-hidden="true">
      {#each ringViews as v, i}
        {@const r = ringViews.length === 1 ? 15 : RING[i]}
        <circle cx="18" cy="18" {r} class="track" style:stroke-width={ringViews.length === 1 ? 3.2 : 2.6} />
        <circle
          cx="18" cy="18" {r} class="fill"
          style:stroke={ringColor(i, v.pct ?? 0)}
          style:stroke-width={ringViews.length === 1 ? 3.2 : 2.6}
          style:stroke-dasharray="{((v.pct ?? 0) / 100) * circumference(r)} {circumference(r)}"
        />
      {/each}
    </svg>
    <div class="ring-center" class:two={views.length > 1}>
      <b>{ringViews[0].value}</b>
      <small>{ringViews[0].label}</small>
      {#if ringViews[1]}<small class="second" style:color={ringColor(1, ringViews[1].pct ?? 0)}>{ringViews[1].label} {Math.round(ringViews[1].pct ?? 0)}%</small>{/if}
      {#each extras as v}
        {#if v.lines}
          <small class="second extra down">{v.lines[0]}</small>
          <small class="second extra">{v.lines[1]}</small>
        {:else}
          <small class="second extra">{v.label} {v.value}</small>
        {/if}
      {/each}
    </div>
  {:else}
    <div class="items">
      {#each views as v, i (i)}
        <div class="item" class:big={v.big} class:muted={v.muted} class:first={i === 0}>
          {#if v.weather}<WeatherIcon code={v.weather.code} isDay={v.weather.isDay} size={Math.round(20 * scale)} />{/if}
          {#if v.moon !== undefined}
            {@const lit = Math.cos(v.moon * 2 * Math.PI)}
            <svg class="moon" viewBox="0 0 20 20" aria-hidden="true">
              <circle cx="10" cy="10" r="8" fill="#2c2c30" />
              <path d="M10 2 A8 8 0 0 {v.moon < 0.5 ? 1 : 0} 10 18 A{Math.abs(lit) * 8} 8 0 0 {(v.moon < 0.5) === lit > 0 ? 0 : 1} 10 2Z" fill="#f2efe4" />
            </svg>
          {/if}
          {#if v.dot}<span class="dot {v.dot}"></span>{/if}
          {#if v.label && widget.shape !== "round"}<span class="label">{v.label}</span>{/if}
          {#if v.pct !== null && widget.shape !== "round"}
            <span class="bar"><span style:width="{Math.min(100, v.pct)}%" style:background={tone(v.pct)}></span></span>
          {/if}
          {#if v.analog}
            <svg class="analog" viewBox="0 0 40 40" aria-label={v.value}>
              <circle cx="20" cy="20" r="18" class="dial" />
              {#each Array.from({ length: 12 }, (_, i) => i) as i}
                <line x1="20" y1="4" x2="20" y2={i % 3 === 0 ? 7.5 : 6} transform="rotate({i * 30} 20 20)" class="tick" />
              {/each}
              <line x1="20" y1="20" x2="20" y2="10.5" class="hand h" transform="rotate({(v.analog.h % 12) * 30 + v.analog.m * 0.5} 20 20)" />
              <line x1="20" y1="20" x2="20" y2="6.5" class="hand m" transform="rotate({v.analog.m * 6 + v.analog.s * 0.1} 20 20)" />
              <line x1="20" y1="22.5" x2="20" y2="6" class="hand s" transform="rotate({v.analog.s * 6} 20 20)" />
              <circle cx="20" cy="20" r="1.4" class="pin" />
            </svg>
          {:else if v.bits}
            <span class="bits" title={v.value}>
              {#each v.bits as row}
                <span class="bitrow">{#each row.split("") as bit}<i class:on={bit === "1"}></i>{/each}</span>
              {/each}
            </span>
          {:else if v.spark}
            <svg class="spark" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true">
              {#if v.spark.length > 1}
                <path d="{sparkPath(v.spark)} L100 30 L0 30 Z" class="area" />
                <path d={sparkPath(v.spark)} class="stroke" />
              {/if}
            </svg>
            <span class="value">{v.value}</span>
          {:else if v.lines}
            <span class="lines" title={v.value}><span class="down">{v.lines[0]}</span><span>{v.lines[1]}</span></span>
          {:else}
            <span class="value" title={v.sub ?? v.value}>{v.value}</span>
          {/if}
          {#if widget.shape === "round" && v.label}<span class="label">{v.label}</span>{/if}
          {#if v.sub && widget.shape === "card" && widget.size === "l"}<span class="sub">{v.sub}</span>{/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .moon {
    flex: none;
    width: calc(16px * var(--scale));
    height: calc(16px * var(--scale));
  }
  /* ---------- Neue Darstellungen: Zifferblatt, Binaeruhr, Verlauf ---------- */
  .analog {
    flex: none;
    width: calc(var(--h) * 0.72);
    height: calc(var(--h) * 0.72);
    max-width: 96px;
    max-height: 96px;
  }
  .analog .dial {
    fill: rgba(255, 255, 255, 0.04);
    stroke: rgba(255, 255, 255, 0.14);
    stroke-width: 1;
  }
  .analog .tick {
    stroke: rgba(255, 255, 255, 0.45);
    stroke-width: 1.1;
    stroke-linecap: round;
  }
  .analog .hand {
    stroke-linecap: round;
  }
  .analog .h {
    stroke: var(--text);
    stroke-width: 2.4;
  }
  .analog .m {
    stroke: var(--text);
    stroke-width: 1.6;
  }
  .analog .s {
    stroke: var(--accent);
    stroke-width: 0.8;
  }
  .analog .pin {
    fill: var(--accent);
  }
  .bits {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .bitrow {
    display: flex;
    gap: 4px;
    justify-content: flex-end;
  }
  .bitrow i {
    width: calc(6px * var(--scale));
    height: calc(6px * var(--scale));
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.12);
  }
  .bitrow i.on {
    background: var(--accent);
    box-shadow: 0 0 6px var(--accent);
  }
  .spark {
    flex: 1;
    min-width: 30px;
    height: calc(18px * var(--scale));
  }
  .spark .area {
    fill: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .spark .stroke {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.6;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
  }
  .face {
    position: relative;
    box-sizing: border-box;
    flex: none;
    background: #000;
    color: var(--text);
    font-variant-numeric: tabular-nums;
    font-size: calc(12px * var(--scale));
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px 12px;
  }
  /* Haengend: oben buendig am Bildschirmrand, mit Hohlkehlen wie die Insel. */
  .hanging {
    border-radius: 0 0 18px 18px;
    box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.06);
  }
  .hanging.pill {
    border-radius: 0 0 24px 24px;
  }
  .hanging::before,
  .hanging::after {
    content: "";
    position: absolute;
    top: 0;
    width: 12px;
    height: 12px;
    pointer-events: none;
  }
  .hanging::before {
    left: -12px;
    background: radial-gradient(circle at 0 100%, transparent 11.5px, #000 12px);
  }
  .hanging::after {
    right: -12px;
    background: radial-gradient(circle at 100% 100%, transparent 11.5px, #000 12px);
  }
  /* Haengende Kugel: bleibt rund; ihr oberes Zehntel steckt im Bildschirmrand, die Hohlkehlen
     sitzen genau dort, wo der Kreis den Rand beruehrt (bei 20 % und 80 % der Breite). */
  .hanging.round {
    border-radius: 50%;
    margin-top: calc(var(--h) * -0.1);
  }
  .hanging.round::before,
  .hanging.round::after {
    top: 10%;
  }
  .hanging.round::before {
    left: calc(20% - 11.5px);
  }
  .hanging.round::after {
    right: calc(20% - 11.5px);
  }
  /* Frei stehend: kleiner Abstand zum Rand, rundum gerundet, leichter Schatten. */
  .floating {
    margin-top: 6px;
    border-radius: var(--r-lg);
    background: rgba(12, 12, 14, 0.94);
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.45), inset 0 0 0 1px rgba(255, 255, 255, 0.07);
  }
  .floating.pill {
    border-radius: var(--r-full);
  }
  .floating.round {
    border-radius: 50%;
  }
  .round {
    padding: 0;
  }

  /* ---------- Ringe (rund) ---------- */
  .rings {
    position: absolute;
    inset: 9%;
    width: 82%;
    height: 82%;
    transform: rotate(-90deg);
  }
  .rings circle {
    fill: none;
  }
  .track {
    stroke: #232326;
  }
  .fill {
    stroke-linecap: round;
    transition: stroke-dasharray var(--t-spring) ease;
  }
  .ring-center {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    line-height: 1.1;
  }
  .ring-center b {
    font-size: 1.25em;
    font-weight: var(--fw-bold);
  }
  .ring-center small {
    font-size: 0.72em;
    color: var(--text-3);
    font-weight: var(--fw-semibold);
  }
  .ring-center .second {
    margin-top: 1px;
    font-size: 0.62em;
    white-space: nowrap;
  }
  .ring-center .extra.down {
    color: var(--accent);
  }
  .ring-center .extra {
    margin-top: 0;
    line-height: 1.15;
    max-width: 72px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-2);
  }
  .ring-center.two b {
    font-size: 1.05em;
  }
  .ring-center.two small:not(.second) {
    font-size: 0.62em;
  }
  .lines {
    flex: none;
    display: flex;
    flex-direction: column;
    font-size: 0.82em;
    font-weight: var(--fw-semibold);
    line-height: 1.2;
    white-space: nowrap;
  }
  .lines .down {
    color: var(--accent);
  }
  .card .lines {
    flex: 1;
    flex-direction: row;
    justify-content: flex-end;
    gap: 10px;
    font-size: 0.9em;
  }
  .round .lines {
    align-items: center;
  }

  /* ---------- Eintraege ---------- */
  .items {
    width: 100%;
    min-width: 0;
    display: flex;
    gap: 12px;
    align-items: center;
    justify-content: center;
  }
  .card .items {
    flex-direction: column;
    gap: 6px;
    align-items: stretch;
  }
  .round .items {
    flex-direction: column;
    gap: 4px;
    padding: 0 10px;
  }
  .item {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: center;
  }
  .card .item {
    justify-content: flex-start;
  }
  .round .item {
    flex-direction: column;
    gap: 1px;
  }
  .label {
    flex: none;
    max-width: 90px;
    font-size: 0.85em;
    font-weight: var(--fw-bold);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .round .label {
    font-size: 0.75em;
  }
  .bar {
    flex: 1;
    min-width: 22px;
    height: calc(5px * var(--scale));
    border-radius: 3px;
    background: #2c2c2e;
    overflow: hidden;
    position: relative;
  }
  .bar span {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 3px;
    transition: width var(--t-spring) ease;
  }
  .value {
    flex: none;
    min-width: 0;
    max-width: 100%;
    font-size: 1.05em;
    font-weight: var(--fw-semibold);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card .item:not(:has(.bar)) .value {
    flex: 1;
    text-align: right;
  }
  .card .item.big .value,
  .card .item.first:not(:has(.label)) .value {
    text-align: left;
  }
  .big .value {
    font-size: 1.55em;
    font-weight: var(--fw-semibold);
    letter-spacing: -0.02em;
  }
  .pill .big .value {
    font-size: 1.3em;
  }
  .round .first .value {
    font-size: 1.3em;
  }
  .muted .value {
    color: var(--text-4);
    font-weight: var(--fw-medium);
  }
  .sub {
    font-size: 0.75em;
    color: var(--text-4);
  }
  .emoji {
    font-size: 1.15em;
    line-height: 1;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #48484a;
  }
  .dot.working { background: #f5b08a; animation: pulse 1.4s ease-in-out infinite; }
  .dot.question,
  .dot.confirm { background: #ffd60a; animation: pulse 1s ease-in-out infinite; }
  .dot.done { background: #30d158; }
  .dot.idle { background: #8e8e93; }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }
</style>
