<script lang="ts">
  // Zeichenflaeche der Buehne. Kuemmert sich um Bildrate, Pausieren und den Mauszeiger;
  // was auf der Buehne passiert, steckt in stage.ts und engine.ts.
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { Stage, type BuddyState, type HelperPhase, type Scenery, type StageLayout } from "./stage";
  import type { ArmStyle, Mood, OneShot } from "./engine";

  interface Props {
    /** Groesse der Zeichenflaeche in CSS-Pixeln. */
    width: number;
    height: number;
    layout: StageLayout;
    mood?: Mood;
    /** Darf Milo von selbst herumlaufen und spielen? */
    roaming?: boolean;
    hovered?: boolean;
    armStyle?: ArmStyle;
    level?: number;
    bpm?: number;
    urgency?: number;
    liveliness?: number;
    /** Animation anhalten (z. B. Spielemodus oder verdecktes Fenster). */
    paused?: boolean;
    /** Claude ist offen und tut gerade das; null = Claude ist nicht da. */
    buddy?: BuddyState | null;
    /** Der Helfer (staerkeres Modell) arbeitet gerade mit; null = er wird nicht gebraucht. */
    helper?: HelperPhase | null;
    /** Kulisse: Tageszeit, Wetter, Post, Pflanze. */
    scenery?: Scenery;
    /** Deko-Objekte in der Insel (Reihenfolge = Platz), eigene Plaetze und gesperrte. */
    decor?: string[];
    decorPositions?: Record<string, number>;
    decorLocked?: string[];
    /** Offene Insel: Flaeche, auf die Milo springt (Zeichenkoordinaten); null = Boden der Kopfzeile. */
    perch?: { x: number; y: number } | null;
    /** Milo ist auf Ausflug auf dem Desktop. */
    away?: boolean;
    onpush?: (side: -1 | 1, amount: number) => void;
  }

  let {
    width, height, layout, mood = "idle", roaming = true, hovered = false, armStyle = "capsule",
    level = 0, bpm = 112, urgency = 0, liveliness = 1, paused = false, buddy = null, helper = null, scenery, decor, decorPositions = {}, decorLocked = [], perch = null, away = false, onpush,
  }: Props = $props();

  let canvas: HTMLCanvasElement;
  let g: CanvasRenderingContext2D | null = null;
  const stage = new Stage();

  /** Spielt eine einmalige Geste und liefert ihre Dauer in Millisekunden. */
  export function play(kind: OneShot): number {
    return stage.play(kind) * 1000;
  }

  /** Ein Wort wurde ausgegeben; laesst den Koerper beim Antworten im Sprechtakt pulsieren. */
  export function talk() {
    stage.milo.talk();
  }

  /** Laesst Milo sofort etwas Bestimmtes unternehmen (Labor und Pruefskripte). */
  export function perform(what: Parameters<Stage["perform"]>[0]) {
    stage.perform(what);
  }

  /** Es ist etwas los: Milo wird wach und aufmerksam. */
  export function stir() {
    stage.stir();
  }

  /** Klick an dieser Bildschirmposition; liefert, ob Milo getroffen wurde. */
  export function poke(clientX: number, clientY: number): boolean {
    const box = canvas.getBoundingClientRect();
    return stage.poke(clientX - box.left, clientY - box.top);
  }

  /** Deko-Objekt an dieser Bildschirmposition (nur geschlossene Pille), sonst null. */
  export function decorAt(clientX: number, clientY: number): string | null {
    const box = canvas.getBoundingClientRect();
    return stage.decorAt(clientX - box.left, clientY - box.top);
  }

  /** Deko live ziehen (null beendet) und liefert den Platz als Anteil der Breite. */
  export function dragDecor(kind: string | null, clientX = 0): number {
    const box = canvas.getBoundingClientRect();
    const x = clientX - box.left;
    stage.decorDrag = kind ? { kind, x: Math.min(stage.layout.right - 10, Math.max(stage.layout.left + 10, x)) } : null;
    return stage.decorRatio(x);
  }

  /** Liegt diese Bildschirmposition auf Milo? (ohne dass er reagiert) */
  export function hits(clientX: number, clientY: number): boolean {
    const box = canvas.getBoundingClientRect();
    return stage.hits(clientX - box.left, clientY - box.top);
  }

  /** Rechnet die Animation um `seconds` vor, ohne zu zeichnen (fuer Standbilder im Labor). */
  export function advance(seconds: number) {
    for (let left = seconds; left > 0; left -= 1 / 60) stage.update(Math.min(left, 1 / 60));
  }

  /** Zeichnet sofort ein Bild, auch wenn die Animation angehalten ist. */
  export function redraw() {
    if (!g) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
    }
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, height);
    stage.draw(g);
  }

  $effect(() => {
    stage.layout = layout;
    stage.mood = mood;
    stage.roaming = roaming;
    stage.hovered = hovered;
    stage.liveliness = liveliness;
    stage.milo.armStyle = armStyle;
    stage.milo.level = level;
    stage.milo.bpm = bpm;
    stage.milo.urgency = urgency;
    stage.setBuddy(buddy);
    stage.setHelper(helper);
    if (scenery) stage.scenery = scenery;
    if (decor) stage.decor = [...decor];
    stage.decorPositions = { ...decorPositions };
    stage.decorLocked = [...decorLocked];
    stage.perch = perch ? { x: perch.x, y: perch.y } : null;
    stage.away = away;
    stage.onPush = (side, amount) => onpush?.(side, amount);
  });

  onMount(() => {
    g = canvas.getContext("2d");
    let frame = 0;
    let last = performance.now();

    let pause = 0;
    let running = false;

    // Bildrate bewusst begrenzen: Bewegung mit 60 Bildern pro Sekunde (auch auf schnelleren
    // Monitoren), ruhiges Stehen und Atmen mit 15. Im ruhigen Fall wartet ein Zeitgeber statt
    // einer durchlaufenden Bildschleife, damit der Rechner dazwischen wirklich nichts tut.
    // Abstand bis zum naechsten Bild in Millisekunden. Jedes gezeichnete Bild kostet vor allem beim
    // Zusammensetzen des durchsichtigen Fensters Rechenzeit (gemessen ca. 0,3 % eines Kerns je Bild pro
    // Sekunde), daher so wenige Bilder wie fuer eine fluessige Wirkung noetig:
    // 60 beim Benutzen der Insel, 30 beim Herumlaufen in der geschlossenen Insel, 24 bei Musik, 12 in Ruhe.
    const interval = () => {
      if (!stage.lively) return 83;
      if (mood === "music" && !stage.busyScene) return 41;
      return roaming && !hovered ? 33 : 16.6;
    };
    // Bis kurz vor dem naechsten Bild wirklich schlafen und erst dann auf den Bildwechsel des Monitors warten.
    // Frueher lief die Schleife mit der vollen Monitorrate (240 Hz) und verwarf die meisten Durchlaeufe.
    const wakeAt = (elapsed: number) => {
      pause = window.setTimeout(() => (frame = requestAnimationFrame(render)), Math.max(0, interval() - elapsed - 4));
    };
    const next = () => {
      if (running) wakeAt(performance.now() - last);
    };
    const render = (now: number) => {
      // Zu frueh dran (schneller Monitor): nochmals schlafen statt zu zeichnen.
      if (now - last < interval() - 2) {
        wakeAt(now - last);
        return;
      }
      stage.update((now - last) / 1000);
      last = now;
      redraw();
      next();
    };

    const start = () => {
      if (running || paused || document.hidden) return;
      running = true;
      last = performance.now();
      frame = requestAnimationFrame(render);
    };
    const stop = () => {
      running = false;
      cancelAnimationFrame(frame);
      clearTimeout(pause);
    };
    const onVisibility = () => (document.hidden ? stop() : start());
    document.addEventListener("visibilitychange", onVisibility);

    // Mauszeiger: in der App meldet ihn der Rust-Kern (auch ausserhalb der Insel),
    // im Browser zum Entwickeln genuegt die Mausbewegung auf der Seite.
    const point = (clientX: number, clientY: number) => {
      const box = canvas.getBoundingClientRect();
      stage.cursor = { x: clientX - box.left, y: clientY - box.top };
    };
    let unlisten = () => {};
    const onMove = (e: MouseEvent) => point(e.clientX, e.clientY);
    const onLeave = () => (stage.cursor = null);
    if ("__TAURI_INTERNALS__" in window) {
      listen<[number, number] | null>("milo://cursor", ({ payload }) => {
        if (!payload) stage.cursor = null;
        else point(payload[0] / window.devicePixelRatio, payload[1] / window.devicePixelRatio);
      }).then((fn) => (unlisten = fn));
    } else {
      window.addEventListener("mousemove", onMove);
      document.addEventListener("mouseleave", onLeave);
    }

    $effect(() => {
      if (paused) stop();
      else start();
    });
    redraw();

    return () => {
      stop();
      unlisten();
      window.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseleave", onLeave);
      document.removeEventListener("visibilitychange", onVisibility);
      g = null;
    };
  });
</script>

<canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>

<style>
  canvas {
    display: block;
    flex: none;
    pointer-events: none;
  }
</style>
