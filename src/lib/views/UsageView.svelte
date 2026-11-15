<script lang="ts">
  import { t, locale } from "../i18n";
  // Verbrauch: wie viel vom kostenlosen Gemini-Kontingent heute und in dieser Minute benutzt ist.
  // Milo zaehlt selbst mit und weicht auf das naechste Modell aus, bevor eins voll ist.
  import { onMount } from "svelte";
  import { app, timeOfDay } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoUsage } from "../demoData";

  onMount(() => {
    app.loadUsage();
    const timer = window.setInterval(() => app.loadUsage(), 3000);
    return () => clearInterval(timer);
  });

  const demo = $derived(tour.demo === "usage");
  const u = $derived(demo ? demoUsage() : app.usage);

  function tone(share: number, exhausted: boolean): string {
    if (exhausted || share >= 1) return "#ff453a";
    return share > 0.8 ? "#ffd60a" : "#30d158";
  }

  function tokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toLocaleString(locale, { maximumFractionDigits: 1 })}${t(" Mio.", "M")}`;
    if (n >= 1000) return `${Math.round(n / 1000).toLocaleString(locale)}${t(" Tsd.", "k")}`;
    return String(n);
  }
</script>

<div class="view">
  {#if !app.cloud.hasKey && !demo}
    <p class="hint">{app.settings.aiMode === "local" && app.settings.localModel ? t("Deine KI läuft lokal auf diesem PC: Es gibt kein Kontingent, das sich verbrauchen könnte.", "Your AI runs locally on this PC: there's no quota to use up.") : t("Hinterlege zuerst deinen Gemini-Schlüssel in den Einstellungen.", "Add your Gemini key in Settings first.")}</p>
  {:else if u}
    {#each u.rows as row (row.model)}
      {@const day = row.today / Math.max(1, row.rpd)}
      {@const minute = row.minute / Math.max(1, row.rpm)}
      <section class="tile">
        <div class="head">
          <span class="model">{row.model}</span>
          <span class="role">{row.role}</span>
          {#if row.exhausted}<span class="full">{t("für heute voll", "used up for today")}</span>{/if}
        </div>
        <div class="line">
          <span class="lname">{t("Heute", "Today")}</span>
          <div class="bar"><span style:width="{Math.min(100, day * 100)}%" style:background={tone(day, row.exhausted)}></span></div>
          <span class="num">{row.today} / {row.rpd}</span>
        </div>
        <div class="line">
          <span class="lname">{t("Minute", "Minute")}</span>
          <div class="bar"><span style:width="{Math.min(100, minute * 100)}%" style:background={tone(minute, false)}></span></div>
          <span class="num">{row.minute} / {row.rpm}</span>
        </div>
        <div class="small">{t(`${tokens(row.tokensToday)} Token heute · ${tokens(row.tokensMinute)} von ${tokens(row.tpm)} in dieser Minute`, `${tokens(row.tokensToday)} tokens today · ${tokens(row.tokensMinute)} of ${tokens(row.tpm)} this minute`)}</div>
      </section>
    {/each}
    <p class="foot">{t(`Neues Kontingent um ${timeOfDay(u.resetMs)} Uhr. Schnellbefehle (Timer, Lautstärke, Programme öffnen) zählen nicht mit. Die Grenzen kannst du in den Einstellungen anpassen.`, `New quota at ${timeOfDay(u.resetMs)}. Quick commands (timers, volume, opening apps) don't count. You can adjust the limits in Settings.`)}</p>
  {:else}
    <p class="hint">{t("Lade …", "Loading …")}</p>
  {/if}
</div>

<style>
  .view {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px 12px 12px;
    scrollbar-width: none;
    font-variant-numeric: tabular-nums;
  }
  .tile {
    border-radius: var(--r-md);
    background: var(--card);
    box-shadow: var(--card-edge);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .model {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .role {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .full {
    margin-left: auto;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: #ff6961;
  }
  .line {
    display: grid;
    grid-template-columns: 52px 1fr 70px;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .num {
    text-align: right;
  }
  .bar {
    position: relative;
    height: 6px;
    border-radius: 3px;
    background: var(--chip-strong);
    overflow: hidden;
  }
  .bar span {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 3px;
    transition: width var(--t-slow) ease;
  }
  .small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .foot {
    margin: 4px 6px 0;
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-4);
  }
  .hint {
    margin: auto;
    font-size: var(--fs-body);
    color: var(--text-3);
  }
</style>
