<script lang="ts">
  import { t, locale } from "../i18n";
  // System-Ansicht: Prozessor, Arbeitsspeicher, Laufwerke, Netzwerk und die Programme, die gerade
  // am meisten brauchen. Gemessen wird nur, solange diese Ansicht offen ist.
  import EmptyState from "../EmptyState.svelte";
  import { onMount, untrack } from "svelte";
  import { app, fileSize } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoSystem } from "../demoData";

  /** Verlauf der Prozessorlast fuer die kleine Kurve (letzte 60 Sekunden). */
  let history = $state<number[]>([]);
  // Tour: Beispielwerte, die Prozessorkurve laeuft jede Sekunde weiter.
  const demo = $derived(tour.demo === "system");
  let demoCpu = $state(23);
  $effect(() => {
    if (!demo) return;
    history = Array.from({ length: 40 }, (_, i) => 18 + Math.sin(i / 3) * 9 + (i % 7) * 1.5);
    const timer = window.setInterval(() => {
      demoCpu = Math.max(6, Math.min(78, demoCpu + (Math.random() - 0.45) * 14));
      history = [...history.slice(-59), demoCpu];
    }, 1000);
    return () => clearInterval(timer);
  });
  $effect(() => {
    if (demo) return;
    const cpu = app.system?.cpu;
    if (cpu === undefined) return;
    history = [...untrack(() => history).slice(-59), cpu];
  });

  onMount(() => {
    app.watchSystem(true);
    return () => app.watchSystem(false);
  });

  const s = $derived(demo ? demoSystem(demoCpu) : app.system);
  const ram = $derived(s ? s.ramUsed / Math.max(1, s.ramTotal) : 0);
  const curve = $derived.by(() => {
    if (history.length < 2) return "";
    const w = 100 / 59;
    const start = 60 - history.length;
    return history.map((v, i) => `${i === 0 ? "M" : "L"}${((start + i) * w).toFixed(2)},${(30 - (v / 100) * 28).toFixed(2)}`).join(" ");
  });

  function rate(bytes: number): string {
    return `${fileSize(bytes)}/s`;
  }

  function uptime(seconds: number): string {
    const d = Math.floor(seconds / 86400), h = Math.floor((seconds % 86400) / 3600), m = Math.floor((seconds % 3600) / 60);
    if (d > 0) return t(`${d} T ${h} Std`, `${d}d ${h}h`);
    if (h > 0) return t(`${h} Std ${m} Min`, `${h} h ${m} min`);
    return t(`${m} Min`, `${m} min`);
  }

  /** Farbe je nach Auslastung. */
  function tone(share: number): string {
    return share > 0.9 ? "#ff453a" : share > 0.7 ? "#ffd60a" : "#30d158";
  }
</script>

<div class="view">
  {#if !s}
    <EmptyState kind="system" title={t("Messe …", "Measuring …")} text={t("Prozessor, Arbeitsspeicher, Laufwerke und Netz kommen gleich.", "Processor, memory, drives and network are on their way.")} />
  {:else}
    <div class="grid">
      <section class="tile">
        <div class="kicker">{t("Prozessor", "Processor")}</div>
        <div class="big" style:color={tone(s.cpu / 100)}>{Math.round(s.cpu)} %</div>
        <svg class="curve" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true">
          <path d={curve} fill="none" stroke={tone(s.cpu / 100)} stroke-width="1.4" vector-effect="non-scaling-stroke" />
        </svg>
        <div class="cores" title={t("Auslastung je Kern", "Load per core")}>
          {#each s.cores as core}
            <i style:height="{Math.max(2, core * 0.22)}px" style:background={tone(core / 100)}></i>
          {/each}
        </div>
        <div class="small" title={s.cpuName}>{s.cpuName}</div>
      </section>

      <section class="tile">
        <div class="kicker">{t("Arbeitsspeicher", "Memory")}</div>
        <div class="big" style:color={tone(ram)}>{Math.round(ram * 100)} %</div>
        <div class="bar"><span style:width="{ram * 100}%" style:background={tone(ram)}></span></div>
        <div class="small">{fileSize(s.ramUsed)} {t("von", "of")} {fileSize(s.ramTotal)}</div>
        <div class="kicker spaced">{t("Netzwerk", "Network")}</div>
        <div class="net">
          <span>↓ {rate(s.netDown)}</span>
          <span>↑ {rate(s.netUp)}</span>
        </div>
        <div class="small">{t("Läuft seit", "Up for")} {uptime(s.uptime)}</div>
      </section>
    </div>

    <section class="tile">
      <div class="kicker">{t("Laufwerke", "Drives")}</div>
      {#each s.disks as disk (disk.mount)}
        {@const used = 1 - disk.free / Math.max(1, disk.total)}
        <div class="disk">
          <span class="drive">{disk.mount.replace(/\\$/, "")}</span>
          <div class="bar"><span style:width="{used * 100}%" style:background={tone(used)}></span></div>
          <span class="free">{fileSize(disk.free)} {t("frei", "free")}</span>
        </div>
      {/each}
    </section>

    <section class="tile">
      <div class="kicker">{t("Braucht gerade am meisten", "Using the most right now")}</div>
      {#each s.top as p (p.name)}
        <div class="proc">
          <span class="pname">{p.name}{#if p.count > 1}<em> ×{p.count}</em>{/if}</span>
          <span class="pcpu">{p.cpu.toLocaleString(locale)} %</span>
          <span class="pmem">{fileSize(p.memory)}</span>
        </div>
      {/each}
    </section>
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
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .tile {
    border-radius: var(--r-md);
    background: var(--card);
    box-shadow: var(--card-edge);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .kicker {
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  .kicker.spaced {
    margin-top: 6px;
  }
  .big {
    font-size: var(--fs-title);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.02em;
  }
  .small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .curve {
    width: 100%;
    height: 34px;
  }
  .cores {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 24px;
  }
  .cores i {
    flex: 1;
    border-radius: 1.5px;
    opacity: 0.85;
    transition: height var(--t-slow) ease;
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
    transition: width var(--t-spring) ease;
  }
  .net {
    display: flex;
    gap: 14px;
    font-size: var(--fs-body);
    color: var(--text-2);
  }
  .disk {
    display: grid;
    grid-template-columns: 34px 1fr 92px;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .drive {
    font-weight: var(--fw-semibold);
  }
  .free {
    text-align: right;
    color: var(--text-3);
  }
  .proc {
    display: grid;
    grid-template-columns: 1fr 64px 76px;
    gap: 8px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
    padding: 2px 0;
  }
  .pname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pname em {
    font-style: normal;
    color: var(--text-4);
  }
  .pcpu,
  .pmem {
    text-align: right;
    color: var(--text-3);
  }
</style>
