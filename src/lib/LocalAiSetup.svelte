<script lang="ts">
  // Einrichtung der lokalen KI: erkennt Ollama/LM Studio, prueft auf Wunsch den Rechner, empfiehlt
  // ein Modell und laedt es per Klick herunter (mit Prozent und Restzeit). Danach ist es sofort mit
  // Milo verbunden. Wird im Willkommens-Assistenten (compact) und in den Einstellungen genutzt.
  import { onMount } from "svelte";
  import { app, inTauri } from "./app.svelte";
  import { t } from "./i18n";
  import { localAi, etaText, gb, speedText } from "./localAi.svelte";

  let { compact = false }: { compact?: boolean } = $props();

  onMount(() => {
    localAi.init();
    localAi.refresh();
    // Startet jemand Ollama oder ist die Installation fertig, erscheint es von selbst.
    const timer = window.setInterval(() => localAi.refresh(), 5000);
    return () => clearInterval(timer);
  });

  async function openOllamaSite() {
    const url = "https://ollama.com/download/windows";
    if (!inTauri) return void window.open(url, "_blank");
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    openUrl(url);
  }

  const status = $derived(localAi.status);
  const pull = $derived(localAi.pull);
  const rec = $derived(localAi.check?.recommendation);
  const hw = $derived(localAi.check?.hardware);
  const canPull = $derived(status?.server === "ollama" || !!status?.ollamaInstalled);
  const connected = (model: string) => app.settings.localModel === model && app.settings.aiMode !== "cloud";

  // Neue Empfehlung oder neuer Download-Stand: ins Bild holen (der Assistent in der Insel ist niedrig).
  let recEl = $state<HTMLElement | null>(null);
  let pullEl = $state<HTMLElement | null>(null);
  const reveal = (el: HTMLElement | null) => requestAnimationFrame(() => el?.scrollIntoView({ block: "nearest", behavior: "smooth" }));
  $effect(() => {
    if (localAi.check) reveal(recEl);
  });
  $effect(() => {
    void pull?.phase;
    if (pull) reveal(pullEl);
  });
  /** Vorhandene Modelle (im Assistenten als Auswahl, in den Einstellungen gibt es die Liste schon). */
  const found = $derived(compact ? (status?.models ?? []).slice(0, 4) : []);
</script>

<div class="lai" class:compact>
  <div class="state" class:ok={!!status?.server}>
    <span class="lamp" aria-hidden="true"></span>
    <span class="state-text">
      {#if status?.server}
        <b>{localAi.serverName}{status.version ? ` ${status.version}` : ""} {t("gefunden", "found")}</b>
        <small>{status.models.length === 0 ? t("Noch kein Modell geladen", "No model downloaded yet") : status.models.length === 1 ? t("1 Modell auf deinem PC", "1 model on your PC") : t(`${status.models.length} Modelle auf deinem PC`, `${status.models.length} models on your PC`)}</small>
      {:else if status?.ollamaInstalled}
        <b>{t("Ollama ist installiert", "Ollama is installed")}</b>
        <small>{t("Läuft gerade nicht. Milo startet es beim Herunterladen von selbst.", "Not running right now. Milo starts it by itself when downloading.")}</small>
      {:else}
        <b>{t("Noch keine lokale KI gefunden", "No local AI found yet")}</b>
        <small>{t("Milo sucht nach Ollama und LM Studio und erkennt sie von selbst.", "Milo looks for Ollama and LM Studio and detects them by itself.")}</small>
      {/if}
    </span>
    {#if !status?.server && status?.ollamaInstalled}
      <button class="btn ghost" disabled={localAi.starting} onclick={() => localAi.startOllama()}>{localAi.starting ? t("Startet …", "Starting …") : t("Ollama starten", "Start Ollama")}</button>
    {/if}
  </div>

  {#if found.length > 0}
    <div class="found">
      {#each found as m (m.name)}
        <div class="model" class:on={connected(m.name)}>
          <span><b>{m.name}</b>{#if m.size}<small>{gb(m.size)} GB{m.params ? ` · ${m.params}` : ""}</small>{/if}</span>
          {#if connected(m.name)}
            <span class="ok-tag">✓ {t("Verbunden", "Connected")}</span>
          {:else}
            <button class="btn ghost small" onclick={() => localAi.use(m.name)}>{t("Verwenden", "Use")}</button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="card">
    {#if !localAi.check}
      <div class="intro">
        <span class="ico" aria-hidden="true">
          <svg viewBox="0 0 20 20" width="18" height="18"><rect x="4" y="4" width="12" height="12" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.5" /><rect x="7.5" y="7.5" width="5" height="5" rx="1" fill="currentColor" /><path d="M7 1.8v2.2M10 1.8v2.2M13 1.8v2.2M7 16v2.2M10 16v2.2M13 16v2.2M1.8 7h2.2M1.8 10h2.2M1.8 13h2.2M16 7h2.2M16 10h2.2M16 13h2.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
        </span>
        <span class="intro-text">
          <b>{t("Welches Modell passt zu meinem PC?", "Which model fits my PC?")}</b>
          <small>{t("Milo liest dafür Prozessor, Arbeitsspeicher und Grafikkarte aus. Das bleibt auf deinem PC.", "Milo reads your processor, memory and graphics card for this. It stays on your PC.")}</small>
        </span>
        <button class="btn primary" disabled={localAi.checking} onclick={() => localAi.checkHardware()}>{localAi.checking ? t("Prüfe …", "Checking …") : t("Rechner prüfen", "Check my PC")}</button>
      </div>
    {:else if hw && rec}
      <div class="hw">
        <div><small>{t("Prozessor", "Processor")}</small><b title={hw.cpu}>{hw.cpu || "–"}</b><small>{hw.cores} {t("Threads", "threads")}</small></div>
        <div><small>{t("Arbeitsspeicher", "Memory")}</small><b>{hw.ramGb.toFixed(0)} GB</b></div>
        <div><small>{t("Grafikkarte", "Graphics")}</small><b title={hw.gpu}>{hw.gpu || t("keine eigene", "none dedicated")}</b>{#if hw.vramGb > 0}<small>{hw.vramGb.toFixed(0)} GB</small>{/if}</div>
      </div>
      <div class="rec" bind:this={recEl}>
        <div class="rec-head">
          <span class="badge">{t("Empfehlung", "Recommended")}</span>
          <b class="name">{rec.model}</b>
          <small class="needs">{rec.onGpu ? t(`≈ ${rec.needsGb.toFixed(0)} GB Grafikspeicher`, `≈ ${rec.needsGb.toFixed(0)} GB graphics memory`) : t(`≈ ${rec.needsGb.toFixed(0)} GB Arbeitsspeicher`, `≈ ${rec.needsGb.toFixed(0)} GB memory`)}</small>
        </div>
        <p>{rec.reason}{rec.alternative && !canPull ? t(` Alternative: ${rec.alternative}.`, ` Alternative: ${rec.alternative}.`) : ""}</p>
        <div class="actions">
          {#if localAi.has(rec.model)}
            {#if connected(rec.model)}
              <span class="ok-tag">✓ {t("Verbunden mit Milo", "Connected to Milo")}</span>
            {:else}
              <button class="btn primary" onclick={() => localAi.use(rec.model)}>{t("Verwenden", "Use it")}</button>
            {/if}
          {:else if canPull}
            <button class="btn primary" disabled={localAi.running} onclick={() => localAi.download(rec.model)}>
              {localAi.running && pull?.model === rec.model ? t("Lädt …", "Downloading …") : t("Herunterladen und verbinden", "Download and connect")}
            </button>
            {#if rec.alternative && !localAi.has(rec.alternative)}
              <button class="btn ghost" disabled={localAi.running} onclick={() => localAi.download(rec.alternative)}>{t(`Lieber ${rec.alternative}`, `${rec.alternative} instead`)}</button>
            {/if}
          {:else if status?.server === "lmstudio"}
            <small>{t(`In LM Studio nach „${rec.model.split(":")[0]}“ suchen und laden; danach erscheint es hier von selbst.`, `Search for "${rec.model.split(":")[0]}" in LM Studio and load it; then it shows up here by itself.`)}</small>
          {:else}
            <button class="btn primary" onclick={openOllamaSite}>{t("Ollama herunterladen", "Download Ollama")}</button>
            <small>{t("Kostenlos. Sobald es installiert ist, erkennt Milo es und du kannst hier das Modell laden.", "Free. Once it's installed, Milo detects it and you can download the model here.")}</small>
          {/if}
          <button class="btn link" onclick={() => localAi.checkHardware()}>{t("Neu prüfen", "Check again")}</button>
        </div>
      </div>
    {/if}

    {#if pull}
      <div class="pull" bind:this={pullEl} class:error={pull.phase === "error"} class:done={pull.phase === "done"}>
        {#if pull.phase === "error"}
          <span class="pull-line"><b>{t("Download fehlgeschlagen", "Download failed")}</b><small>{pull.error}</small></span>
          <span class="pull-keys">
            <button class="btn ghost small" onclick={() => localAi.download(pull.model)}>{t("Nochmal", "Retry")}</button>
            <button class="btn link" onclick={() => localAi.dismiss()}>{t("Schließen", "Close")}</button>
          </span>
        {:else if pull.phase === "done"}
          <span class="pull-line"><b>✓ {t(`${pull.model} ist bereit`, `${pull.model} is ready`)}</b><small>{t("Heruntergeladen und mit Milo verbunden. Frag einfach los.", "Downloaded and connected to Milo. Just ask away.")}</small></span>
          <button class="btn link" onclick={() => localAi.dismiss()}>{t("Ausblenden", "Hide")}</button>
        {:else}
          <div class="pull-top">
            <b>{pull.model}</b>
            <span class="pct">{pull.phase === "download" ? `${Math.floor(pull.percent)} %` : ""}</span>
          </div>
          <div class="bar" class:busy={pull.phase !== "download"} role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.floor(pull.percent)} aria-label={t("Download-Fortschritt", "Download progress")}>
            <span style:width="{pull.phase === 'download' ? Math.max(1.5, pull.percent) : 100}%"></span>
          </div>
          <small class="pull-meta">
            {#if pull.phase === "start"}{t("Ollama wird vorbereitet …", "Getting Ollama ready …")}
            {:else if pull.phase === "check"}{t("Fast fertig: wird geprüft und eingerichtet …", "Almost done: checking and setting up …")}
            {:else}{gb(pull.completed)} / {gb(pull.total)} GB{pull.bytesPerSec > 0 ? ` · ${speedText(pull.bytesPerSec)}` : ""} · {etaText(pull.etaSecs)}{/if}
          </small>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .lai {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .lamp {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #8e8e93;
    box-shadow: 0 0 0 3px rgba(142, 142, 147, 0.18);
  }
  .state.ok .lamp {
    background: #30d158;
    box-shadow: 0 0 0 3px rgba(48, 209, 88, 0.2), 0 0 10px rgba(48, 209, 88, 0.6);
  }
  .state-text,
  .intro-text,
  .pull-line {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  b {
    font-weight: var(--fw-semibold);
    font-size: var(--fs-body);
    color: var(--text);
  }
  small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
    line-height: 1.35;
  }
  .found {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 6px;
  }
  .model {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.07);
    min-width: 0;
  }
  .model.on {
    box-shadow: inset 0 0 0 1.5px rgba(48, 209, 88, 0.6);
  }
  .model > span {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .model b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: "JetBrains Mono Variable", ui-monospace, monospace;
    font-size: var(--fs-footnote);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border-radius: var(--r-lg);
    background: linear-gradient(160deg, rgba(47, 191, 113, 0.11), rgba(255, 255, 255, 0.03) 70%);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.07);
  }
  .intro {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .ico {
    flex: none;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 10px;
    background: rgba(47, 191, 113, 0.18);
    color: #8ff0b6;
  }
  .hw {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 0.7fr) minmax(0, 1.4fr);
    gap: 10px;
    padding-bottom: 12px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
  }
  .hw div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .hw b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rec {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* Im Assistenten klebt unten die Knopfleiste: beim Hineinscrollen Platz dafuer lassen. */
  .rec,
  .pull {
    scroll-margin: 12px 0 72px;
  }
  .rec-head {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 4px 10px;
  }
  .badge {
    align-self: center;
    padding: 2px 8px;
    border-radius: var(--r-full);
    background: rgba(47, 191, 113, 0.22);
    color: #8ff0b6;
    font-size: var(--fs-caption);
    font-weight: var(--fw-bold);
  }
  .name {
    font-family: "JetBrains Mono Variable", ui-monospace, monospace;
    font-size: var(--fs-headline);
  }
  .rec p {
    margin: 0;
    color: var(--text-2);
    font-size: var(--fs-footnote);
    line-height: 1.45;
  }
  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 10px;
    margin-top: 2px;
  }
  .actions small {
    flex: 1 1 200px;
  }
  .ok-tag {
    color: #30d158;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
  }
  .btn {
    height: 30px;
    padding: 0 14px;
    border: 0;
    border-radius: var(--r-md);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
    cursor: pointer;
    transition: background var(--t-fast, 0.15s) ease, opacity var(--t-fast, 0.15s) ease;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .btn.primary {
    background: var(--text);
    color: #000;
  }
  .btn.ghost {
    background: rgba(255, 255, 255, 0.09);
    color: var(--text);
  }
  .btn.ghost:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.15);
  }
  .btn.small {
    height: 26px;
    padding: 0 10px;
  }
  .btn.link {
    height: auto;
    padding: 0;
    background: none;
    color: var(--accent);
  }
  .btn:focus-visible {
    outline: 2px solid var(--focus, var(--accent));
    outline-offset: 2px;
  }
  .pull {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--r-md);
    background: rgba(0, 0, 0, 0.22);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .pull.error,
  .pull.done {
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }
  .pull.error {
    background: rgba(255, 69, 58, 0.1);
  }
  .pull.error b {
    color: #ff8a80;
  }
  .pull.done {
    background: rgba(48, 209, 88, 0.1);
  }
  .pull.done b {
    color: #7ee29a;
  }
  .pull-keys {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .pull-top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }
  .pull-top b {
    font-family: "JetBrains Mono Variable", ui-monospace, monospace;
    font-size: var(--fs-footnote);
  }
  .pct {
    font-variant-numeric: tabular-nums;
    font-weight: var(--fw-bold);
    font-size: var(--fs-callout);
    color: #8ff0b6;
  }
  .bar {
    height: 6px;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, #7ee8a2, #2fbf71);
    transition: width 0.3s ease;
  }
  .bar.busy span {
    background: linear-gradient(90deg, rgba(126, 232, 162, 0.15), rgba(126, 232, 162, 0.7), rgba(126, 232, 162, 0.15));
    background-size: 50% 100%;
    background-repeat: no-repeat;
    animation: sweep 1.3s ease-in-out infinite;
  }
  @keyframes sweep {
    from {
      background-position: -100% 0;
    }
    to {
      background-position: 200% 0;
    }
  }
  .pull-meta {
    font-variant-numeric: tabular-nums;
  }
  .compact .hw {
    grid-template-columns: minmax(0, 1fr) minmax(0, 0.6fr) minmax(0, 1fr);
  }
  @media (prefers-reduced-motion: reduce) {
    .bar.busy span {
      animation: none;
      background: rgba(126, 232, 162, 0.5);
    }
  }
</style>
