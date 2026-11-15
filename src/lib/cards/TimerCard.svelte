<script lang="ts">
  import { t } from "../i18n";
  // Timer-Vorlage: Ring mit Restzeit, Bezeichnung, Endzeit und Knoepfe zum Verlaengern und Stoppen.
  import { app, clock, timeOfDay } from "../app.svelte";

  interface Props {
    id: number;
    /** Bezeichnung fuer den Fall, dass der Timer schon vorbei ist. */
    label?: string;
    /** Grosse Fassung fuer die Timer-Ansicht. */
    big?: boolean;
    /** Fertiger Timer (Beispiel in der Tour) statt Suche in den laufenden Timern. */
    timer?: import("../app.svelte").Timer;
  }
  let { id, label = "Timer", big = false, timer: given }: Props = $props();

  const timer = $derived(given ?? app.timers.find((t) => t.id === id));
  const left = $derived(timer ? Math.max(0, timer.dueMs - app.now) : 0);
  const part = $derived(timer && !timer.ringing ? Math.min(1, left / Math.max(1, timer.totalMs)) : 0);
  const size = $derived(big ? 112 : 50);
  const stroke = $derived(big ? 7 : 4.5);
  const radius = $derived((size - stroke) / 2);
  const around = $derived(2 * Math.PI * radius);
</script>

<article class="timer" class:big class:ringing={timer?.ringing} class:over={!timer}>
  <div class="dial" style:width="{size}px" style:height="{size}px">
    <svg viewBox="0 0 {size} {size}" width={size} height={size} aria-hidden="true">
      <circle cx={size / 2} cy={size / 2} r={radius} fill="none" stroke="#242427" stroke-width={stroke} />
      {#if timer}
        <circle
          class="arc"
          cx={size / 2} cy={size / 2} r={radius} fill="none" stroke-width={stroke} stroke-linecap="round"
          stroke-dasharray={around} stroke-dashoffset={timer.ringing ? 0 : around * (1 - part)}
          transform="rotate(-90 {size / 2} {size / 2})"
        />
      {/if}
    </svg>
    {#if big}
      <div class="center">
        <span class="time">{timer ? (timer.ringing ? "0:00" : clock(left)) : "–"}</span>
      </div>
    {:else if timer?.ringing}
      <svg class="bell" viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
        <path d="M10 3.2a4.3 4.3 0 0 0-4.3 4.3v2.6L4.4 12.6h11.2l-1.3-2.5V7.5A4.3 4.3 0 0 0 10 3.2Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
        <path d="M8.3 15a1.8 1.8 0 0 0 3.4 0" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    {/if}
  </div>

  <div class="info">
    <div class="label">{timer?.label ?? label}</div>
    {#if !timer}
      <div class="sub">{t("Beendet", "Finished")}</div>
    {:else if timer.ringing}
      <div class="sub alert">{t("Abgelaufen", "Time's up")}</div>
    {:else}
      {#if !big}<div class="time">{clock(left)}</div>{/if}
      <div class="sub">{t(`Endet um ${timeOfDay(timer.dueMs)} Uhr`, `Ends at ${timeOfDay(timer.dueMs)}`)}</div>
    {/if}
  </div>

  {#if timer}
    <div class="actions">
      <button onclick={() => app.extendTimer(timer, 60)} title={timer.ringing ? t("Noch eine Minute", "One more minute") : t("Eine Minute mehr", "Add a minute")}>+1 min</button>
      {#if timer.ringing}
        <button onclick={() => app.extendTimer(timer, 300)} title={t("In fünf Minuten erneut klingeln", "Ring again in five minutes")}>+5 min</button>
        <button class="primary" onclick={() => app.dismissTimer(timer)}>{t("Stopp", "Stop")}</button>
      {:else}
        <button onclick={() => app.dismissTimer(timer)}>{t("Abbrechen", "Cancel")}</button>
      {/if}
    </div>
  {/if}
</article>

<style>
  .timer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 12px 12px 12px;
    border-radius: var(--r-lg);
    background: var(--card);
    box-shadow: var(--card-edge);
    border: 1px solid var(--line);
    font-variant-numeric: tabular-nums;
    animation: rise var(--t-slow) cubic-bezier(0.3, 1.2, 0.4, 1) both;
  }
  .timer.big {
    gap: 18px;
    padding: 16px 16px 16px 18px;
  }
  .timer.over {
    opacity: 0.6;
  }
  .dial {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
  }
  .dial svg:first-child {
    position: absolute;
    inset: 0;
  }
  .arc {
    stroke: var(--accent);
    transition: stroke-dashoffset var(--t-base) linear, stroke var(--t-slow) ease;
  }
  .ringing .arc {
    stroke: var(--warn);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .bell {
    color: var(--warn);
    transform-origin: 50% 15%;
    animation: swing 1.2s ease-in-out infinite;
  }
  .center {
    position: relative;
  }
  .info {
    flex: 1;
    min-width: 0;
  }
  .label {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .big .label {
    font-size: var(--fs-headline);
    color: var(--text);
  }
  .time {
    font-size: var(--fs-title);
    font-weight: 300;
    letter-spacing: -0.02em;
    line-height: 1.15;
  }
  .big .time {
    font-size: var(--fs-large);
    font-weight: var(--fw-regular);
  }
  .sub {
    font-size: var(--fs-footnote);
    color: var(--text-3);
    margin-top: 1px;
  }
  .sub.alert {
    color: var(--warn);
    font-weight: var(--fw-semibold);
  }
  .actions {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .big .actions {
    flex-direction: column;
    align-items: stretch;
  }
  button {
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-lg);
    background: var(--chip);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
    transition: background var(--t-fast) ease, transform var(--t-fast) ease;
  }
  button:hover {
    background: #2e2e32;
  }
  button:active {
    transform: scale(0.96);
  }
  button.primary {
    background: var(--text);
    color: #000;
  }
  button.primary:hover {
    background: #fff;
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(8px) scale(0.98); }
    to { opacity: 1; transform: none; }
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }
  @keyframes swing {
    0%, 62%, 100% { transform: rotate(0); }
    8% { transform: rotate(16deg); }
    20% { transform: rotate(-14deg); }
    32% { transform: rotate(10deg); }
    44% { transform: rotate(-6deg); }
    54% { transform: rotate(2deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .timer, .arc, .bell { animation: none; }
  }
</style>
