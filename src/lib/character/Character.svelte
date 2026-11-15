<script lang="ts">
  // Zeichenflaeche fuer einen einzelnen Charakter (Labor, Einstellungen). In der Insel lebt
  // Milo auf der Buehne (Stage.svelte); die eigentliche Animation steckt in engine.ts.
  import { onMount } from "svelte";
  import { Character, type ArmStyle, type Mood, type OneShot } from "./engine";

  interface Props {
    mood?: Mood;
    /** Koerpergroesse in Pixeln. */
    size?: number;
    /** Groesse der Zeichenflaeche; sie braucht Platz fuer Spruenge, Arme und Partikel. */
    width?: number;
    height?: number;
    armStyle?: ArmStyle;
    accent?: [string, string] | null;
    level?: number;
    bpm?: number;
    facing?: number;
    urgency?: number;
    gaze?: { x: number; y: number } | null;
    idleGestures?: boolean;
    /** Animation anhalten (z. B. Spielemodus oder verdecktes Fenster). */
    paused?: boolean;
  }

  let {
    mood = "idle", size = 30, width = 72, height = 52, armStyle = "capsule", accent = null,
    level = 0, bpm = 112, facing = 1, urgency = 0, gaze = null, idleGestures = true, paused = false,
  }: Props = $props();

  let canvas: HTMLCanvasElement;
  let g: CanvasRenderingContext2D | null = null;
  const character = new Character();
  const moving = $derived(mood === "walk" || mood === "dash");

  /** Spielt eine einmalige Geste und liefert ihre Dauer in Millisekunden. */
  export function play(kind: OneShot): number {
    return character.play(kind) * 1000;
  }

  /** Ein Wort wurde ausgegeben; laesst den Koerper beim Antworten im Sprechtakt pulsieren. */
  export function talk() {
    character.talk();
  }

  function tick(dt: number) {
    // Auf der Stelle gehen: die Schrittphase laeuft so, als ginge er mit normalem Tempo.
    if (moving) character.step += (Math.PI * 32 * dt) / (34 * 0.36);
    character.update(dt);
  }

  /** Rechnet die Animation um `seconds` vor, ohne zu zeichnen (fuer Standbilder im Labor). */
  export function advance(seconds: number) {
    for (let left = seconds; left > 0; left -= 1 / 60) tick(Math.min(left, 1 / 60));
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
    // Der Boden liegt im unteren Drittel, darueber bleibt Platz fuer Spruenge.
    character.draw(g, width / 2, height * 0.74, size);
  }

  $effect(() => character.setMood(mood));
  $effect(() => {
    character.armStyle = armStyle;
    character.setAccent(accent);
    character.level = level;
    character.bpm = bpm;
    character.facing = facing;
    character.dir = facing;
    character.urgency = urgency;
    character.gaze = gaze;
    character.idleGestures = idleGestures;
  });

  onMount(() => {
    g = canvas.getContext("2d");
    let frame = 0;
    let last = performance.now();
    let skip = false;

    const render = (now: number) => {
      frame = requestAnimationFrame(render);
      // Ruhige Zustaende laufen mit halber Bildrate, das halbiert die Last im Leerlauf.
      skip = !skip;
      if (!character.lively && skip) return;
      tick((now - last) / 1000);
      last = now;
      redraw();
    };

    const start = () => {
      if (frame || paused || document.hidden) return;
      last = performance.now();
      frame = requestAnimationFrame(render);
    };
    const stop = () => {
      cancelAnimationFrame(frame);
      frame = 0;
    };
    const onVisibility = () => (document.hidden ? stop() : start());
    document.addEventListener("visibilitychange", onVisibility);

    $effect(() => {
      if (paused) stop();
      else start();
    });
    redraw();

    return () => {
      stop();
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
  }
</style>
