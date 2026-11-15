<script lang="ts">
  import { t } from "./i18n";
  // Mini-Bibliothek der Deko-Objekte: antippen schaltet ein Objekt in der Insel an oder aus,
  // herausziehen legt eine Kopie auf den Desktop, wo sie auf die Taskleiste faellt. Das Schloss
  // sperrt ein Objekt in der Insel gegen Verschieben. Milo selbst kann man hier ebenfalls
  // herausziehen oder vom Desktop zurueckholen.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { app, inTauri } from "./app.svelte";
  import { DECOR, drawDecor, type DecorKind } from "./character/decor";
  import { Character } from "./character/engine";
  import { rise } from "./motion";

  let { onclose }: { onclose: () => void } = $props();
  let hint = $state(t("Antippen: in der Insel an oder aus. Herausziehen: auf den Desktop.", "Tap: on or off in the island. Drag out: onto the desktop."));
  let grid = $state<HTMLElement>();

  onMount(() => {
    // Per Rechtsklick in der Insel gewaehltes Objekt hervorheben und sichtbar machen.
    if (app.decorFocus) {
      const label = DECOR.find((d) => d.kind === app.decorFocus)?.label;
      if (label) hint = t(`${label}: Schloss sperrt es in der Insel, Herausziehen legt es auf den Desktop.`, `${label}: the lock keeps it in place in the island; drag it out to put it on the desktop.`);
      grid?.querySelector<HTMLElement>(`[data-kind="${app.decorFocus}"]`)?.scrollIntoView({ block: "nearest" });
    }
    return () => (app.decorFocus = "");
  });

  function toggle(kind: string) {
    const list = app.settings.decor.includes(kind) ? app.settings.decor.filter((d) => d !== kind) : [...app.settings.decor, kind];
    app.saveSettings({ decor: list });
  }

  function lock(e: MouseEvent, kind: string) {
    e.stopPropagation();
    const locked = app.settings.decorLocked.includes(kind);
    app.saveSettings({ decorLocked: locked ? app.settings.decorLocked.filter((d) => d !== kind) : [...app.settings.decorLocked, kind] });
  }

  /** Druecken und ziehen: ab ein paar Pixeln wird getragen, der Kern fuehrt die Maus weiter. */
  function grab(e: PointerEvent, kind: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".lock")) return;
    const start = { x: e.clientX, y: e.clientY };
    let dragging = false;
    const move = (m: PointerEvent) => {
      if (dragging || Math.hypot(m.clientX - start.x, m.clientY - start.y) < 6) return;
      dragging = true;
      cleanup();
      if (!inTauri) {
        hint = t("Herausziehen geht nur in der App.", "Dragging out only works in the app.");
        return;
      }
      if (kind === "milo") app.miloOnDesk = true;
      invoke("desk_carry", { kind, fromX: null, fromIsland: false }).catch((err) => (hint = String(err)));
      onclose();
    };
    const up = () => {
      cleanup();
      if (!dragging && kind !== "milo") toggle(kind);
    };
    const cleanup = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  /** Kleines Vorschaubild; laeuft leise weiter, damit Flamme, Fisch und Sanduhr sich bewegen. */
  function preview(canvas: HTMLCanvasElement, kind: string) {
    const g = canvas.getContext("2d");
    if (!g) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = 48 * dpr;
    canvas.height = 48 * dpr;
    const milo = kind === "milo" ? new Character() : null;
    let frame = 0;
    const start = performance.now();
    const draw = (now: number) => {
      const clock = (now - start) / 1000;
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.clearRect(0, 0, 48, 48);
      if (milo) {
        milo.update(0.05);
        milo.draw(g, 24, 43, 26);
      } else {
        drawDecor(g, kind as DecorKind, 24, 44, 32, { clock, hour: new Date().getHours(), mail: true, growth: 4, perk: 0, music: true });
      }
      frame = window.setTimeout(() => requestAnimationFrame(draw), 66) as unknown as number;
    };
    requestAnimationFrame(draw);
    return { destroy: () => clearTimeout(frame) };
  }
</script>

<div class="library">
  <div class="head">
    <strong>{t("Deko", "Decor")}</strong>
    <div class="head-actions">
      <button class="more" onclick={() => { onclose(); if (inTauri) invoke("settings_open", { section: "look" }); }}>{t("Einstellungen", "Settings")}</button>
      <button class="close" onclick={onclose} aria-label={t("Deko-Bibliothek schließen", "Close decor library")} title={t("Schließen", "Close")}><svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg></button>
    </div>
  </div>
  <div class="grid" bind:this={grid}>
    <div class="tile milo" role="button" tabindex="0" onpointerdown={(e) => !app.miloOnDesk && grab(e, "milo")} in:rise={{ delay: 40, y: 8 }}>
      <canvas use:preview={"milo"} aria-hidden="true"></canvas>
      <span class="name">{app.settings.name}</span>
      {#if app.miloOnDesk}
        <button class="state on" onclick={() => inTauri && invoke("desk_take")}>{t("Zurückholen", "Bring back")}</button>
      {:else}
        <span class="state">{t("ziehen", "drag")}</span>
      {/if}
    </div>
    {#each DECOR as item, i (item.kind)}
      {@const on = app.settings.decor.includes(item.kind)}
      {@const locked = app.settings.decorLocked.includes(item.kind)}
      <div
        class="tile" class:on class:focus={app.decorFocus === item.kind} role="button" tabindex="0" aria-pressed={on} title={item.hint} data-kind={item.kind}
        onpointerdown={(e) => grab(e, item.kind)}
        onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), toggle(item.kind))}
        in:rise={{ delay: 60 + i * 25, y: 8 }}
      >
        {#if on}
          <button class="lock" class:locked onclick={(e) => lock(e, item.kind)} aria-label={locked ? t(`${item.label} entsperren`, `Unlock ${item.label}`) : t(`${item.label} sperren`, `Lock ${item.label}`)} title={locked ? t("Gesperrt: lässt sich in der Insel nicht verschieben", "Locked: can't be moved in the island") : t("Sperren", "Lock")}>
            <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true">
              <rect x="3.5" y="7" width="9" height="6.5" rx="1.6" fill="currentColor" />
              <path d={locked ? "M5.5 7V5a2.5 2.5 0 0 1 5 0v2" : "M5.5 7V5a2.5 2.5 0 0 1 4.8-1"} fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            </svg>
          </button>
        {/if}
        <canvas use:preview={item.kind} aria-hidden="true"></canvas>
        <span class="name">{item.label}</span>
        <span class="state">{on ? (locked ? t("gesperrt", "locked") : t("in der Insel", "in the island")) : t("aus", "off")}</span>
      </div>
    {/each}
  </div>
  <p class="hint">{hint}</p>
</div>

<style>
  .library {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 4px;
  }
  .head {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 6px 0;
  }
  .head strong {
    font-size: var(--fs-callout);
  }
  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .close {
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-2);
    cursor: pointer;
  }
  .close:hover {
    background: rgba(255, 255, 255, 0.2);
    color: #fff;
  }
  .close:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .more {
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .grid {
    position: relative;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    max-height: 290px;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 6px 2px 6px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05);
    cursor: grab;
    touch-action: none;
    transition: background var(--t-fast) ease, box-shadow var(--t-fast) ease, transform var(--t-slow) var(--ease-spring);
  }
  .tile:hover {
    background: rgba(255, 255, 255, 0.09);
    transform: translateY(-2px);
  }
  .tile:active {
    cursor: grabbing;
    transform: scale(0.96);
  }
  .tile.on {
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--accent) 70%, transparent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .tile.focus {
    box-shadow: inset 0 0 0 2px var(--warn);
  }
  .tile:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  canvas {
    width: 48px;
    height: 48px;
  }
  .name {
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    text-align: center;
  }
  .state {
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .tile.on .state {
    color: var(--accent);
  }
  .lock {
    position: absolute;
    top: 3px;
    right: 3px;
    z-index: 1;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.35);
    color: var(--text-3);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .lock.locked {
    background: var(--warn);
    color: #000;
  }
  button.state {
    border: 0;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    background: var(--accent);
    color: #000;
    font: inherit;
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .hint {
    position: relative;
    margin: 0;
    padding: 0 6px 2px;
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
</style>
