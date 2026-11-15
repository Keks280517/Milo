<script lang="ts">
  // Gefuehrte Tour in der offenen Insel: der erklaerte Bereich bleibt hell, alles andere wird
  // abgedunkelt. In vielen Bereichen laufen gespielte Beispiele ab ("Geisterdaten"), die sich ruhig
  // wiederholen; sie sind reine Darstellung und werden nie gespeichert. Der Erklaerkasten sitzt
  // unterhalb der Insel (TourCard.svelte), damit hier nichts verdeckt wird.
  import { onDestroy, tick } from "svelte";
  import { fade } from "svelte/transition";
  import { app, inTauri } from "./app.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { tour, tourSteps } from "./tourState.svelte";

  let { island }: { island: HTMLElement | undefined } = $props();

  const steps = $derived(tourSteps(app.settings.name));
  const step = $derived(steps[Math.min(tour.index, steps.length - 1)]);

  /** Hervorgehobener Bereich relativ zur Insel; null = nichts hervorheben. */
  let spot = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  let size = $state({ w: 0, h: 0 });

  function rectOf(selector: string | undefined) {
    if (!selector || !island) return null;
    const el = island.querySelector(selector) as HTMLElement | null;
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const base = island.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return null;
    return { x: r.left - base.left, y: r.top - base.top, w: r.width, h: r.height };
  }

  /** Seitlich immer etwas Abstand zur Inselkante, damit die Markierung als helle Flaeche im
   *  Grau steht und nicht bis an den Rand geht. */
  const SIDE = 26;
  /** Ansichten enden immer kurz ueber der Leiste (manche reichen bis zum unteren Rand). */
  let shelfTop = 1e9;
  function inset(r: { x: number; y: number; w: number; h: number } | null, aboveShelf = false) {
    if (!r) return null;
    const left = Math.max(r.x, SIDE);
    const right = Math.min(r.x + r.w, size.w - SIDE);
    const bottom = aboveShelf ? Math.min(r.y + r.h, shelfTop - 10) : r.y + r.h;
    return { x: left, y: r.y, w: Math.max(0, right - left), h: Math.max(0, bottom - r.y) };
  }

  type Box = { x: number; y: number; w: number; h: number };
  const round = (r: Box | null): Box | null => (r ? { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.w), h: Math.round(r.h) } : null);
  const same = (a: Box | null, b: Box | null) => (a === null || b === null ? a === b : a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h);

  /** Wo Milo gerade steht: in der offenen Insel auf dem aktiven Wuerfel (bzw. einem Vorschlag im
   *  Chat). Hervorgehoben wird dieser Platz samt dem Raum darueber, in dem Milo und der Buddy stehen. */
  function miloSpot(): Box | null {
    const perch = rectOf("[data-perch]:not([data-perch=''])") ?? rectOf(".cubes button.active");
    if (!perch) return { x: size.w / 2 - 95, y: 2, w: 190, h: 86 };
    const w = Math.max(perch.w + 110, 170);
    return { x: perch.x + perch.w / 2 - w / 2, y: perch.y - 74, w, h: perch.h + 80 };
  }

  function measure() {
    if (!island) return;
    const base = island.getBoundingClientRect();
    const nextSize = { w: Math.round(base.width), h: Math.round(base.height) };
    if (nextSize.w !== size.w || nextSize.h !== size.h) size = nextSize;
    shelfTop = rectOf(".shelf")?.y ?? 1e9;
    const target = step?.target;
    let nextSpot: Box | null;
    if (tour.done || !target) nextSpot = null;
    else if (target === ".stage") nextSpot = miloSpot();
    else nextSpot = inset(rectOf(target), target.includes(".pane"));
    nextSpot = round(nextSpot);
    if (!same(nextSpot, spot)) spot = nextSpot;
    const focus = spot ? Math.round(spot.x + spot.w / 2 - size.w / 2) : 0;
    if (focus !== tour.focusX) tour.focusX = focus;
  }

  // Schritt wechselt: passende Ansicht oeffnen, Vorfuehrung setzen, neu vermessen.
  $effect(() => {
    if (tour.done) {
      tour.demo = "";
      return;
    }
    const current = step;
    if (current?.tab && app.tab !== current.tab) app.tab = current.tab;
    tour.demo = current?.demo ?? "";
    tick().then(measure);
    // Die Ansicht gleitet kurz herein; danach stimmt die Lage endgueltig.
    const late = window.setTimeout(measure, 420);
    return () => clearTimeout(late);
  });

  // Schritt mit eigenem Fenster (Einstellungen): oeffnen und beim Weitergehen wieder schliessen.
  // War das Fenster schon offen (Tour aus den Einstellungen gestartet), bleibt es offen.
  // Wichtig: der Effekt haengt nur an einem Wahrheitswert. `step` ist bei jeder Einstellungs-
  // Aenderung (milo://settings ersetzt app.settings) ein neues Objekt; frueher lief der Effekt
  // dadurch neu, schloss das gerade geoeffnete Fenster sofort wieder, und es war nie zu sehen.
  const wantSettings = $derived(!tour.done && step?.window === "settings");
  $effect(() => {
    if (!wantSettings || !inTauri) return;
    let opened = false;
    let cancelled = false;
    invoke<boolean>("settings_is_open")
      .then((already) => {
        if (already || cancelled) return;
        opened = true;
        // Ueber der Insel (die immer im Vordergrund ist), solange die Tour es zeigt.
        return invoke("settings_open", { section: "look", onTop: true });
      })
      .catch((e) => console.error("Tour: Einstellungsfenster", e));
    return () => {
      cancelled = true;
      if (opened) invoke("settings_close").catch((e) => console.error("Tour: Einstellungen schliessen", e));
    };
  });

  // Bereiche bewegen sich (Insel waechst, Listen laden): regelmaessig nachmessen.
  const follow = window.setInterval(measure, 700);
  onDestroy(() => {
    clearInterval(follow);
    tour.demo = "";
  });

  const PAD = 6;
  const hole = $derived(spot ? { x: spot.x - PAD, y: spot.y - PAD, w: spot.w + PAD * 2, h: spot.h + PAD * 2 } : null);


</script>

<svelte:window onresize={measure} />

<div class="tour" aria-hidden="true" transition:fade={{ duration: 220 }}>
  <!-- Abdunklung mit Loch: der hervorgehobene Bereich bleibt hell und klickbar sieht man durch. -->
  {#if hole && !tour.done}
    <div class="hole" style:left="{hole.x}px" style:top="{hole.y}px" style:width="{hole.w}px" style:height="{hole.h}px"></div>
  {:else}
    <div class="rim"></div>
  {/if}

</div>

<style>
  .tour {
    position: absolute;
    inset: 0;
    z-index: 60;
    color: var(--text);
  }
  /* Begruessung und Abschluss: die ganze Insel bleibt hell, nur zum Rand hin wird es dunkler. */
  .rim {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--accent) 55%, transparent), inset 0 0 70px 26px rgba(0, 0, 0, 0.62);
    pointer-events: none;
  }
  /* Das Loch: riesiger Schatten dunkelt alles ausser dem Bereich ab; die Kante leuchtet. */
  .hole {
    position: absolute;
    border-radius: var(--r-lg);
    box-shadow: 0 0 0 4000px rgba(0, 0, 0, 0.74), 0 0 0 2px color-mix(in srgb, var(--accent) 80%, white);
    transition: left var(--t-slow) var(--ease-spring, ease), top var(--t-slow) var(--ease-spring, ease), width var(--t-slow) var(--ease-spring, ease), height var(--t-slow) var(--ease-spring, ease);
    pointer-events: none;
  }
  .hole::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: 0 0 24px 4px color-mix(in srgb, var(--accent) 50%, transparent);
    animation: glow 2.4s ease-in-out infinite;
    will-change: opacity;
  }
  @keyframes glow {
    0%, 100% { opacity: 0.55; }
    50% { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .hole::after {
      animation: none;
    }
  }
</style>
