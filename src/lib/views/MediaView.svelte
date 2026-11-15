<script lang="ts">
  import { t } from "../i18n";
  // Was gerade laeuft: links das Titelbild, das weich ins Schwarz der Insel auslaeuft, rechts
  // Album und Kuenstler, grosser Titel, Fortschritt, Steuerung und das Ausgabegeraet.
  // Die Daten liefert Windows (dieselbe Quelle wie die Medientasten); hier wird nur die Vorlage gefuellt.
  import EmptyState from "../EmptyState.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { app, clock, inTauri } from "../app.svelte";
  import { pop } from "../motion";

  interface Output {
    id: string;
    name: string;
    default: boolean;
  }

  let now = $state(Date.now());
  let dragging = $state<number | null>(null);
  let error = $state("");
  let outputs = $state<Output[]>([]);
  let picking = $state(false);

  const media = $derived(app.media);
  /** Aktuelle Position: seit der letzten Messung weitergezaehlt, solange es laeuft. */
  const position = $derived.by(() => {
    if (!media) return 0;
    if (dragging !== null) return dragging;
    const moved = media.playing && media.atMs > 0 ? Math.max(0, now - media.atMs) : 0;
    return Math.min(media.durationMs || Infinity, media.positionMs + moved);
  });
  const progress = $derived(media && media.durationMs > 0 ? Math.min(1, position / media.durationMs) : 0);
  const current = $derived(outputs.find((o) => o.default));

  /** Ohne Titelbild: ein ruhiges Farbfeld, dessen Toene aus dem Titel abgeleitet sind. */
  const hue = $derived.by(() => {
    let h = 0;
    for (const c of `${media?.title ?? ""}${media?.artist ?? ""}`) h = (h * 31 + c.charCodeAt(0)) % 360;
    return h;
  });
  /** Kuenstler zuerst, dann das Album. */
  const credit = $derived([media?.artist, media?.album].filter((x) => x && x.trim()).join(" — "));

  async function run(action: "toggle" | "next" | "previous" | "seek" | "skip", positionMs?: number) {
    error = "";
    try {
      await app.mediaCommand(action, positionMs);
    } catch (err) {
      error = String(err);
    }
  }

  function seekFromEvent(e: PointerEvent, commit: boolean) {
    if (!media || !media.canSeek || media.durationMs <= 0) return;
    const bar = e.currentTarget as HTMLElement;
    const box = bar.getBoundingClientRect();
    const target = Math.round(Math.min(1, Math.max(0, (e.clientX - box.left) / box.width)) * media.durationMs);
    if (commit) {
      dragging = null;
      run("seek", target);
    } else {
      dragging = target;
    }
  }

  async function loadOutputs() {
    if (!inTauri) return;
    try {
      outputs = await invoke<Output[]>("audio_outputs");
    } catch {
      outputs = [];
    }
  }

  async function choose(output: Output) {
    picking = false;
    if (output.default) return;
    try {
      outputs = await invoke<Output[]>("audio_set_output", { id: output.id });
    } catch (err) {
      error = t(`Umschalten ging nicht: ${err}`, `Couldn't switch: ${err}`);
    }
  }

  onMount(() => {
    loadOutputs();
    const timer = window.setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(timer);
  });
</script>

{#if media}
  <div class="media" class:playing={media.playing} style:--hue={hue}>
    <!-- Das ganze Feld traegt die Farben des Titelbilds; davon lebt auch das Glas der Leiste. -->
    <div class="backdrop" aria-hidden="true">
      {#if media.cover}<img src={media.cover} alt="" />{/if}
    </div>

    <div class="stage">
      <!-- Huelle mit Schallplatte: beim Abspielen gleitet die Platte heraus und dreht sich. -->
      <div class="sleeve">
        <div class="record" aria-hidden="true">
          <div class="disc">
            <span class="label">{#if media.cover}<img src={media.cover} alt="" />{/if}<i class="hole"></i></span>
          </div>
        </div>
        <div class="cover">
          {#if media.cover}
            <img src={media.cover} alt="" />
          {:else}
            <div class="generated" aria-hidden="true"><span class="orb"></span><span class="wave"></span></div>
          {/if}
        </div>
      </div>

      <div class="info">
        <span class="source"><i class="live"></i>{media.app || t("Wiedergabe", "Playback")} · {media.playing ? t("spielt", "playing") : t("pausiert", "paused")}</span>
        <h2 class="title" title={media.title}>{media.title}</h2>
        {#if credit}<div class="credit" title={credit}>{credit}</div>{/if}
        {#if media.subtitle && media.subtitle !== media.album}<div class="subtitle">{media.subtitle}</div>{/if}

        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="bar" class:seekable={media.canSeek && media.durationMs > 0}
          onpointerdown={(e) => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); seekFromEvent(e, false); }}
          onpointermove={(e) => dragging !== null && seekFromEvent(e, false)}
          onpointerup={(e) => dragging !== null && seekFromEvent(e, true)}
        >
          <div class="track"><div class="fill" style:width="{progress * 100}%"></div></div>
          {#if media.canSeek && media.durationMs > 0}<div class="knob" style:left="{progress * 100}%"></div>{/if}
        </div>
        <div class="times">
          <span>{media.durationMs > 0 ? clock(position) : ""}</span>
          <span>{media.durationMs > 0 ? clock(media.durationMs) : ""}</span>
        </div>

        <div class="row">
          <div class="capsule glass">
            <button onclick={() => run("skip", -10000)} disabled={!media.canSeek} aria-label={t("10 Sekunden zurück", "Back 10 seconds")} title={t("10 Sekunden zurück", "Back 10 seconds")}>
              <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
                <path d="M5.2 8.6A8 8 0 1 1 4 12.2" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" />
                <path d="M4.6 4.4v4.4H9" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" />
                <text x="12.4" y="15.6" text-anchor="middle" font-size="7.4" font-weight="700" fill="currentColor">10</text>
              </svg>
            </button>
            <button onclick={() => run("previous")} disabled={!media.canPrevious} aria-label={t("Zurück", "Previous")} title={t("Zurück", "Previous")}>
              <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path d="M6 5.5v13" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" /><path d="M19 6.2v11.6c0 .8-.9 1.3-1.6.8L9.6 13a1.2 1.2 0 0 1 0-2l7.8-5.6c.7-.5 1.6 0 1.6.8Z" fill="currentColor" /></svg>
            </button>
            <button class="main" onclick={() => run("toggle")} aria-label={media.playing ? t("Pause", "Pause") : t("Abspielen", "Play")} title={media.playing ? t("Pause", "Pause") : t("Abspielen", "Play")}>
              {#if media.playing}
                <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><rect x="6.2" y="4.8" width="4" height="14.4" rx="1.4" fill="currentColor" /><rect x="13.8" y="4.8" width="4" height="14.4" rx="1.4" fill="currentColor" /></svg>
              {:else}
                <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path d="M8 5.6v12.8c0 .9 1 1.4 1.7.9l9.4-6.4a1.1 1.1 0 0 0 0-1.8L9.7 4.7C9 4.2 8 4.7 8 5.6Z" fill="currentColor" /></svg>
              {/if}
            </button>
            <button onclick={() => run("next")} disabled={!media.canNext} aria-label={t("Weiter", "Next")} title={t("Weiter", "Next")}>
              <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path d="M18 5.5v13" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" /><path d="M5 6.2v11.6c0 .8.9 1.3 1.6.8l7.8-5.6a1.2 1.2 0 0 0 0-2L6.6 5.4C5.9 4.9 5 5.4 5 6.2Z" fill="currentColor" /></svg>
            </button>
            <button onclick={() => run("skip", 10000)} disabled={!media.canSeek} aria-label={t("10 Sekunden vor", "Forward 10 seconds")} title={t("10 Sekunden vor", "Forward 10 seconds")}>
              <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
                <path d="M18.8 8.6A8 8 0 1 0 20 12.2" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" />
                <path d="M19.4 4.4v4.4H15" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" />
                <text x="11.6" y="15.6" text-anchor="middle" font-size="7.4" font-weight="700" fill="currentColor">10</text>
              </svg>
            </button>
          </div>

          <div class="output">
            <button class="device glass" class:on={picking} onclick={() => { picking = !picking; if (picking) loadOutputs(); }} aria-label={t("Ausgabegerät wählen", "Choose output device")} aria-expanded={picking} title={t("Ausgabegerät wählen", "Choose output device")}>
              <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true">
                <path d="M4 9.5v5h3.5l4.5 4V5.5L7.5 9.5Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />
                <path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" />
              </svg>
              <span class="dlabel">{current?.name ?? t("Ausgabe", "Output")}</span>
            </button>
            {#if picking}
              <div class="devices glass" transition:pop>
                <div class="dhead">{t("Wiedergabe über", "Play through")}</div>
                {#each outputs as output (output.id)}
                  <button class:on={output.default} onclick={() => choose(output)}>
                    <span class="dname">{output.name}</span>
                    {#if output.default}
                      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M3.5 8.4l3 3 6-6.4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
                    {/if}
                  </button>
                {:else}
                  <p class="none">{inTauri ? t("Keine Ausgabegeräte gefunden.", "No output devices found.") : t("Nur in der App verfügbar.", "Only available in the app.")}</p>
                {/each}
              </div>
            {/if}
          </div>
        </div>
        {#if error}<div class="error">{error}</div>{/if}
      </div>
    </div>
  </div>
{:else}
  <div class="empty">
    <EmptyState kind="music" title={t("Gerade ist es still", "It's quiet right now")} text={t("Starte Musik oder ein Video, egal in welchem Programm. Dann erscheint es hier mit Cover und Steuerung.", "Start music or a video in any app and it shows up here with its cover and controls.")} />
  </div>
{/if}

<style>
  .media {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background:
      radial-gradient(70% 120% at 15% 40%, hsl(var(--hue) 65% 42% / 0.55), transparent 70%),
      radial-gradient(60% 100% at 85% 110%, hsl(calc(var(--hue) + 50) 60% 35% / 0.4), transparent 70%);
  }
  .backdrop img {
    position: absolute;
    inset: -20%;
    width: 140%;
    height: 140%;
    object-fit: cover;
    filter: blur(70px) saturate(1.7) brightness(0.75);
    opacity: 0.75;
  }
  .backdrop::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.55), rgba(0, 0, 0, 0.15) 45%, rgba(0, 0, 0, 0.45));
  }

  .stage {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    gap: 56px;
    padding: 0 40px calc(var(--bar, 62px) + 14px) 44px;
  }

  /* Huelle und Platte */
  .sleeve {
    position: relative;
    flex: none;
    width: 196px;
    height: 196px;
  }
  .cover {
    position: absolute;
    inset: 0;
    z-index: 1;
    border-radius: var(--r-lg);
    overflow: hidden;
    box-shadow: 0 18px 40px rgba(0, 0, 0, 0.5), 0 2px 6px rgba(0, 0, 0, 0.4);
    transform: rotate(-2deg);
    transition: transform 700ms var(--ease-spring);
  }
  .playing .cover {
    transform: rotate(0deg) translateX(-6px);
  }
  /* Lichtkante wie bei einer glaenzenden Huelle. */
  .cover::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35), inset 0 0 0 1px rgba(255, 255, 255, 0.1);
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.18), transparent 40%);
    pointer-events: none;
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .generated {
    position: absolute;
    inset: 0;
    background: linear-gradient(150deg, hsl(var(--hue) 70% 52%), hsl(calc(var(--hue) + 60) 55% 24%));
  }
  .generated .orb {
    position: absolute;
    right: 16%;
    top: 16%;
    width: 38%;
    aspect-ratio: 1;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 30%, #fff, hsl(var(--hue) 80% 80%) 55%, transparent 72%);
    opacity: 0.9;
  }
  .generated .wave {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 45%;
    background:
      radial-gradient(60% 50% at 30% 100%, hsl(calc(var(--hue) + 30) 50% 15% / 0.9), transparent 70%),
      radial-gradient(60% 60% at 80% 100%, hsl(calc(var(--hue) + 80) 50% 20% / 0.8), transparent 70%);
  }
  .record {
    position: absolute;
    top: 8px;
    left: 8px;
    width: 180px;
    height: 180px;
    border-radius: 50%;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.6), inset 0 0 0 1px rgba(255, 255, 255, 0.05);
    transition: transform 900ms var(--ease-spring);
  }
  /* Die Scheibe selbst dreht sich; ungleichmaessige Rillen und helle Segmente machen die Drehung sichtbar. */
  .disc {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background:
      conic-gradient(from 0deg, rgba(255, 255, 255, 0.07) 0 7%, transparent 7% 34%, rgba(255, 255, 255, 0.05) 34% 39%, transparent 39% 66%, rgba(255, 255, 255, 0.08) 66% 70%, transparent 70%),
      radial-gradient(circle, transparent 0 19%, rgba(255, 255, 255, 0.07) 19.5%, transparent 20.5% 44%, rgba(255, 255, 255, 0.05) 44.5%, transparent 45.5% 71%, rgba(255, 255, 255, 0.05) 71.5%, transparent 72.5%),
      repeating-radial-gradient(circle, #111 0 1.6px, #1b1b1d 1.6px 3.2px),
      #0c0c0d;
    animation: spin 2.4s linear infinite;
    animation-play-state: paused;
  }
  .playing .disc {
    animation-play-state: running;
  }
  .hole {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 10%;
    height: 10%;
    margin: -5% 0 0 -5%;
    border-radius: 50%;
    background: #0c0c0d;
  }
  .record::after {
    z-index: 1;
    /* Glanzstreifen, der beim Drehen stehen bleibt. */
    content: "";
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: conic-gradient(from 30deg, transparent 0 10%, rgba(255, 255, 255, 0.08) 14%, transparent 20% 60%, rgba(255, 255, 255, 0.06) 64%, transparent 70%);
  }
  .label {
    position: absolute;
    inset: 33%;
    border-radius: 50%;
    overflow: hidden;
    background: radial-gradient(circle at 30% 30%, hsl(var(--hue) 70% 62%), hsl(calc(var(--hue) + 40) 60% 38%));
  }
  .label img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .playing .record {
    transform: translateX(64px);
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .info {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .source {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .live {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .playing .live {
    background: var(--ok);
    box-shadow: 0 0 0 3px rgba(48, 209, 88, 0.2);
  }
  .title {
    margin: 8px 0 0;
    font-size: var(--fs-large);
    line-height: 1.1;
    font-weight: var(--fw-bold);
    letter-spacing: -0.02em;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    text-wrap: balance;
  }
  .credit {
    margin-top: 6px;
    font-size: var(--fs-callout);
    font-weight: var(--fw-medium);
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .subtitle {
    margin-top: 4px;
    font-size: var(--fs-body);
    color: rgba(235, 235, 245, 0.6);
  }
  .bar {
    position: relative;
    height: 18px;
    margin-top: 18px;
    display: flex;
    align-items: center;
  }
  .bar.seekable {
    cursor: pointer;
  }
  .track {
    width: 100%;
    height: 6px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.18);
    overflow: hidden;
    transition: height var(--t-base) var(--ease-spring);
  }
  .bar.seekable:hover .track {
    height: 9px;
  }
  .fill {
    height: 100%;
    border-radius: 3px;
    background: linear-gradient(90deg, rgba(255, 255, 255, 0.75), #fff);
  }
  .knob {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    margin: -8px 0 0 -8px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
    opacity: 0;
    transform: scale(0.5);
    transition: opacity var(--t-fast) ease, transform var(--t-base) var(--ease-spring);
  }
  .bar:hover .knob {
    opacity: 1;
    transform: none;
  }
  .times {
    display: flex;
    justify-content: space-between;
    margin-top: 2px;
    font-size: var(--fs-footnote);
    color: rgba(235, 235, 245, 0.6);
    font-variant-numeric: tabular-nums;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 14px;
  }
  .capsule {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 54px;
    padding: 0 6px;
    border-radius: var(--r-full);
  }
  .capsule button {
    position: relative;
    width: 42px;
    height: 42px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: transform var(--t-slow) var(--ease-spring), background var(--t-fast) ease, opacity var(--t-fast) ease;
  }
  .capsule button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
  }
  .capsule button:active:not(:disabled) {
    transform: scale(0.88);
  }
  .capsule button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .capsule button.main {
    width: 46px;
    height: 46px;
    background: var(--accent);
    color: #000;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35), inset 0 1px 0 rgba(255, 255, 255, 0.5);
  }
  .capsule button.main:hover {
    background: var(--accent);
    filter: brightness(1.08);
  }
  .capsule button:focus-visible,
  .device:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .output {
    position: relative;
    min-width: 0;
  }
  .device {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 210px;
    height: 40px;
    padding: 0 14px 0 12px;
    border: 0;
    border-radius: var(--r-lg);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .dlabel {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .devices {
    position: absolute;
    left: 0;
    bottom: 48px;
    z-index: 4;
    width: 260px;
    padding: 6px;
    border-radius: var(--r-lg);
    transform-origin: bottom left;
    display: flex;
    flex-direction: column;
  }
  .dhead {
    padding: 6px 10px 4px;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  .devices button {
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r-md);
    cursor: pointer;
    text-align: left;
  }
  .devices button:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .devices button.on {
    color: var(--accent);
  }
  .dname {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .none {
    margin: 4px 10px 8px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .error {
    margin-top: 10px;
    font-size: var(--fs-footnote);
    color: #ff9f8a;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    padding-bottom: calc(var(--bar, 62px) + 10px);
  }
  @media (prefers-reduced-motion: reduce) {
    .disc { animation: none; }
  }
</style>
