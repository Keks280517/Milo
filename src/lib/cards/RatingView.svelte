<script lang="ts">
  import { t } from "../i18n";
  // Debug-Ansicht des Bewertungssystems: Wertung 0 bis 100, Schwelle und jede Teilwertung.
  import type { Rating } from "../app.svelte";

  let { rating, open = false }: { rating: Rating; open?: boolean } = $props();
  let toggled = $state<boolean | null>(null);
  const expanded = $derived(toggled ?? open);

  const KIND = { quick: t("Schnellbefehl, ohne KI", "Quick command, no AI"), easy: t("schnell", "fast"), hard: t("gründlich", "thorough"), report: t("Bericht", "report"), browser: "Browser" };
  const where = $derived(rating.model ? `${KIND[rating.kind]} · ${rating.model}` : KIND[rating.kind]);
</script>

<div class="rating" class:cloud={rating.kind === "hard" || rating.kind === "report"} class:browser={rating.kind === "browser"}>
  <button class="head" onclick={() => (toggled = !expanded)} aria-expanded={expanded}>
    <span class="bar" aria-hidden="true">
      <span class="fill" style:width="{rating.score}%"></span>
      <span class="mark" style:left="{rating.threshold}%"></span>
    </span>
    <span class="score">{rating.score}</span>
    <span class="where">{where}</span>
    <svg class="chevron" class:up={expanded} viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
      <path d="M2.5 4.5L6 8l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </button>
  {#if expanded}
    <div class="parts">
      {#each rating.parts as part}
        <div class="part">
          <span class="points" class:plus={part.points > 0} class:minus={part.points < 0}>
            {part.points > 0 ? "+" : ""}{part.points}
          </span>
          <span>{part.label}</span>
        </div>
      {/each}
      <div class="reason">{rating.reason}</div>
    </div>
  {/if}
</div>

<style>
  .rating {
    margin: 0 0 6px;
    max-width: 94%;
    border-radius: var(--r-md);
    background: var(--card);
    box-shadow: var(--card-edge);
    font-size: var(--fs-footnote);
    color: var(--text-3);
    --tone: var(--ok);
  }
  .rating.cloud {
    --tone: #9fb4fa;
  }
  .rating.browser {
    --tone: var(--accent);
  }
  .head {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    cursor: pointer;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    position: relative;
    flex: none;
    width: 74px;
    height: 5px;
    border-radius: 3px;
    background: var(--chip-strong);
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 3px;
    background: var(--tone);
    transition: width var(--t-spring) cubic-bezier(0.3, 1.2, 0.4, 1);
  }
  .mark {
    position: absolute;
    top: -3px;
    width: 1.5px;
    height: 11px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--text);
  }
  .score {
    min-width: 20px;
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .where {
    flex: 1;
    color: var(--tone);
  }
  .chevron {
    color: #636366;
    transition: transform var(--t-base) ease;
  }
  .chevron.up {
    transform: rotate(180deg);
  }
  .parts {
    padding: 2px 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    animation: unfold var(--t-base) ease both;
  }
  .part {
    display: flex;
    gap: 8px;
    line-height: 1.4;
  }
  .points {
    flex: none;
    width: 30px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
  }
  .points.plus {
    color: #9fb4fa;
  }
  .points.minus {
    color: var(--ok);
  }
  .reason {
    margin-top: 4px;
    padding-top: 6px;
    border-top: 1px solid var(--line);
    color: var(--text-2);
    line-height: 1.4;
  }
  @keyframes unfold {
    from { opacity: 0; transform: translateY(-3px); }
    to { opacity: 1; transform: none; }
  }
</style>
