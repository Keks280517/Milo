<script lang="ts">
  import { t } from "$lib/i18n";
  // Ein eigenes Widget in seinem Fenster am oberen Bildschirmrand. Ziehen verschiebt es nur
  // waagerecht entlang der Kante (der Kern haelt die Hoehe fest und haelt Abstand zur Insel).
  // Das Schloss sperrt es gegen versehentliches Verschieben; Rechtsklick oeffnet den Editor.
  import "@fontsource-variable/inter/wght.css";
  import "$lib/theme.css";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import WidgetFace from "$lib/widgets/WidgetFace.svelte";
  import { WidgetFeed } from "$lib/widgets/feed.svelte";
  import type { Widget } from "$lib/widgets/types";

  const id = getCurrentWindow().label.replace(/^widget-/, "");
  const feed = new WidgetFeed();
  let widget = $state<Widget | null>(null);
  let dragging = $state(false);
  let hover = $state(false);

  $effect(() => {
    if (widget) feed.use(widget.blocks.map((b) => b.kind));
  });

  onMount(() => {
    invoke<Widget | null>("widget_get", { id }).then((w) => (widget = w));
    const stop = listen<Widget>("milo://widget", ({ payload }) => {
      if (payload.id === id) widget = payload;
    });
    return () => {
      stop.then((fn) => fn());
      feed.stop();
    };
  });

  // ---------- Ziehen entlang der Kante ----------
  let startX = 0;
  let offset = 0;
  let first = true;
  let frame = 0;
  let moved = 0;

  function down(e: PointerEvent) {
    if (e.button !== 0 || !widget || widget.locked || (e.target as HTMLElement).closest("button")) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
    startX = e.screenX;
    offset = 0;
    first = true;
    moved = 0;
  }

  function move(e: PointerEvent) {
    if (!dragging) return;
    // Abstand seit Beginn (physische Pixel); der Kern rechnet daraus die Stelle an der Kante.
    offset = Math.round((e.screenX - startX) * window.devicePixelRatio);
    moved = Math.max(moved, Math.abs(offset));
    // Hoechstens ein Schritt pro Bild.
    if (!frame) {
      frame = requestAnimationFrame(() => {
        frame = 0;
        invoke("widget_drag", { id, offset, start: first }).catch(() => {});
        first = false;
      });
    }
  }

  function up() {
    if (!dragging) return;
    dragging = false;
    if (moved > 2) setTimeout(() => invoke("widget_drop", { id }).catch(() => {}), 40);
  }

  async function toggleLock() {
    if (!widget) return;
    widget = await invoke<Widget>("widget_lock", { id, locked: !widget.locked });
  }
</script>

<svelte:head><title>{t("Milo-Widget", "Milo widget")}</title></svelte:head>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="stage"
  class:hanging={widget?.style === "hanging"}
  class:grab={widget && !widget.locked}
  class:dragging
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onpointerenter={() => (hover = true)}
  onpointerleave={() => (hover = false)}
  oncontextmenu={(e) => { e.preventDefault(); invoke("widget_edit", { id }); }}
  title={widget?.locked ? t("Gesperrt · Rechtsklick: bearbeiten", "Locked · Right-click: edit") : t("Ziehen: entlang der Kante verschieben · Rechtsklick: bearbeiten", "Drag: move along the edge · Right-click: edit")}
>
  {#if widget}
    <div class="holder">
      <WidgetFace {widget} {feed} />
      {#if hover || widget.locked}
        <button class="lock" class:on={widget.locked} class:quiet={!hover} onclick={toggleLock} aria-label={widget.locked ? t("Entsperren", "Unlock") : t("Sperren", "Lock")} title={widget.locked ? t("Entsperren", "Unlock") : t("Sperren (nicht mehr verschiebbar)", "Lock (can't be moved)")}>
          <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
            <rect x="3.5" y="7" width="9" height="6.5" rx="1.5" fill="currentColor" />
            <path d={widget.locked ? "M5.5 7V5a2.5 2.5 0 0 1 5 0v2" : "M5.5 7V5a2.5 2.5 0 0 1 4.8-1"} fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          </svg>
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
    font-family: "Inter Variable", "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
    color: var(--text);
    user-select: none;
    -webkit-font-smoothing: antialiased;
  }
  .stage {
    height: 100%;
    display: flex;
    justify-content: center;
    align-items: flex-start;
  }
  .grab {
    cursor: grab;
  }
  .dragging {
    cursor: grabbing;
  }
  .holder {
    position: relative;
    animation: drop var(--t-slow) cubic-bezier(0.3, 1.35, 0.45, 1) both;
  }
  .lock {
    position: absolute;
    right: -6px;
    bottom: -4px;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: var(--r-sm);
    display: grid;
    place-items: center;
    background: #2c2c2e;
    color: var(--text-2);
    cursor: pointer;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.5);
  }
  .lock.on {
    color: var(--warn);
  }
  .lock.quiet {
    opacity: 0;
  }
  @keyframes drop {
    from { transform: translateY(-100%); }
    to { transform: none; }
  }
</style>
