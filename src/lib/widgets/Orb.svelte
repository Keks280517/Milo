<script lang="ts">
  // Die Wasserkugel der KI-Hilfe: ein schimmernder, leicht wabernder Tropfen in Milos Farben.
  // Ruhig: atmet langsam. Zuhoeren: die Oberflaeche schwingt mit der Lautstaerke der Stimme.
  // Nachdenken: das Wasser wirbelt, Lichtpunkte kreisen um den Kern. Fertig: ein Lichtring.
  // Gezeichnet wird nur, solange die Kugel zu sehen ist (eigene Bildschleife, ohne Bilddateien).
  import { onMount } from "svelte";
  import type { OrbState } from "./types";

  interface Props {
    phase?: OrbState;
    /** Lautstaerke der Stimme, 0 bis 1 (nur beim Zuhoeren). */
    level?: number;
    size?: number;
  }
  let { phase = "idle", level = 0, size = 190 }: Props = $props();

  let canvas: HTMLCanvasElement;

  onMount(() => {
    const g = canvas.getContext("2d")!;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    g.scale(dpr, dpr);

    let frame = 0;
    let last = performance.now();
    let t = 0;
    // Weich nachgefuehrte Werte, damit Zustandswechsel nicht springen.
    let wobble = 0.03;
    let swirl = 0.25;
    let voice = 0;
    let glow = 0.5;
    let ring = 0;
    let wasPhase: OrbState = phase;
    const phases = [0, 1.7, 3.1, 4.4, 5.6];

    function blobRadius(angle: number, base: number): number {
      return (
        base *
        (1 +
          wobble * Math.sin(2 * angle + t * 1.3 + phases[0]) +
          wobble * 0.7 * Math.sin(3 * angle - t * 1.7 + phases[1]) +
          (wobble * 0.45 + voice * 0.08) * Math.sin(5 * angle + t * 2.9 + phases[2]) +
          voice * 0.05 * Math.sin(8 * angle - t * 5.3 + phases[3]))
      );
    }

    function draw(now: number) {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const thinking = phase === "thinking";
      const listening = phase === "listening";
      if (phase === "done" && wasPhase !== "done") ring = 1;
      wasPhase = phase;
      const ease = (from: number, to: number, speed: number) => from + (to - from) * Math.min(1, dt * speed);
      wobble = ease(wobble, thinking ? 0.055 : listening ? 0.04 : 0.028, 3);
      swirl = ease(swirl, thinking ? 1.6 : listening ? 0.6 : 0.25, 2);
      voice = ease(voice, listening ? Math.min(1, level * 1.6) : 0, 12);
      glow = ease(glow, thinking ? 1 : listening ? 0.75 + voice * 0.25 : 0.5, 3);
      ring = Math.max(0, ring - dt * 0.9);
      t += dt * (thinking ? 1.6 : 1);

      const c = size / 2;
      // Kugel klein genug, dass Leuchten und kreisende Lichtpunkte nie an den Rand stossen
      // (sonst sieht man eine harte Kante, wo die Zeichenflaeche endet).
      const base = size * 0.24 * (1 + voice * 0.06 + (thinking ? 0.03 * Math.sin(t * 3) : 0.015 * Math.sin(t * 1.2)));
      g.clearRect(0, 0, size, size);

      // Leuchten um die Kugel.
      const halo = g.createRadialGradient(c, c, base * 0.6, c, c, Math.min(base * 1.9, c * 0.98));
      halo.addColorStop(0, `rgba(120, 190, 255, ${0.28 * glow})`);
      halo.addColorStop(0.5, `rgba(185, 140, 245, ${0.14 * glow})`);
      halo.addColorStop(1, "rgba(0, 0, 0, 0)");
      g.fillStyle = halo;
      g.fillRect(0, 0, size, size);

      // Umriss des Tropfens.
      const path = new Path2D();
      const steps = 96;
      for (let i = 0; i <= steps; i++) {
        const a = (i / steps) * Math.PI * 2;
        const r = blobRadius(a, base);
        const x = c + Math.cos(a) * r;
        const y = c + Math.sin(a) * r;
        if (i === 0) path.moveTo(x, y);
        else path.lineTo(x, y);
      }
      path.closePath();

      // Wasserkoerper: tiefes Blau mit hellerem Kern.
      const body = g.createRadialGradient(c - base * 0.3, c - base * 0.35, base * 0.1, c, c, base * 1.1);
      body.addColorStop(0, "#9fe6ff");
      body.addColorStop(0.35, "#4aa8f0");
      body.addColorStop(0.75, "#3b3fb8");
      body.addColorStop(1, "#1a1550");
      g.fillStyle = body;
      g.fill(path);

      // Stroemungen im Inneren: farbige Schwaden, die kreisen (beim Nachdenken schneller).
      g.save();
      g.clip(path);
      g.globalCompositeOperation = "lighter";
      const swirls: [string, number, number, number][] = [
        ["100, 210, 255", 0.55, 0.0, 0.42],
        ["185, 140, 245", 0.5, 2.1, 0.38],
        ["255, 120, 200", 0.42, 4.2, 0.3],
        ["120, 255, 220", 0.36, 1.1, 0.22],
      ];
      for (const [rgb, reach, offset, alpha] of swirls) {
        const angle = t * swirl * (offset % 2 ? 1 : -0.8) + offset;
        const x = c + Math.cos(angle) * base * reach;
        const y = c + Math.sin(angle * 1.3) * base * reach * 0.8;
        const spot = g.createRadialGradient(x, y, 0, x, y, base * 0.85);
        spot.addColorStop(0, `rgba(${rgb}, ${alpha * (0.7 + glow * 0.3)})`);
        spot.addColorStop(1, `rgba(${rgb}, 0)`);
        g.fillStyle = spot;
        g.fillRect(0, 0, size, size);
      }
      // Lichtwellen (Kaustik) quer durch das Wasser.
      g.globalCompositeOperation = "screen";
      g.lineWidth = 1.2;
      for (let k = 0; k < 4; k++) {
        g.beginPath();
        const yBase = c - base * 0.6 + k * base * 0.38;
        for (let x = c - base; x <= c + base; x += 4) {
          const y = yBase + Math.sin(x * 0.045 + t * (1.4 + k * 0.3) + k) * base * (0.06 + voice * 0.08);
          if (x === c - base) g.moveTo(x, y);
          else g.lineTo(x, y);
        }
        g.strokeStyle = `rgba(200, 240, 255, ${0.08 + glow * 0.05})`;
        g.stroke();
      }
      g.restore();

      // Glanzlicht oben links und feiner Rand.
      const shine = g.createRadialGradient(c - base * 0.38, c - base * 0.45, 0, c - base * 0.38, c - base * 0.45, base * 0.5);
      shine.addColorStop(0, "rgba(255, 255, 255, 0.75)");
      shine.addColorStop(0.4, "rgba(255, 255, 255, 0.18)");
      shine.addColorStop(1, "rgba(255, 255, 255, 0)");
      g.fillStyle = shine;
      g.fill(path);
      g.strokeStyle = `rgba(210, 235, 255, ${0.22 + glow * 0.15})`;
      g.lineWidth = 1.5;
      g.stroke(path);

      // Nachdenken: Lichtpunkte kreisen um den Kern.
      if (thinking || swirl > 0.8) {
        const strength = Math.min(1, (swirl - 0.6) / 1);
        for (let i = 0; i < 7; i++) {
          const a = t * (1.4 + (i % 3) * 0.35) + (i * Math.PI * 2) / 7;
          const orbit = base * (1.22 + 0.08 * Math.sin(t * 2 + i));
          const x = c + Math.cos(a) * orbit;
          const y = c + Math.sin(a) * orbit * 0.55;
          const behind = Math.sin(a) < 0;
          g.beginPath();
          g.arc(x, y, behind ? 1.6 : 2.6, 0, Math.PI * 2);
          g.fillStyle = `rgba(${i % 2 ? "185, 140, 245" : "120, 220, 255"}, ${(behind ? 0.35 : 0.9) * strength})`;
          g.fill();
        }
      }

      // Fertig: ein Lichtring breitet sich aus.
      if (ring > 0) {
        g.beginPath();
        g.arc(c, c, base * (1.05 + (1 - ring) * 0.6), 0, Math.PI * 2);
        g.strokeStyle = `rgba(140, 255, 200, ${ring * 0.8})`;
        g.lineWidth = 3 * ring;
        g.stroke();
      }

      frame = requestAnimationFrame(draw);
    }

    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  });
</script>

<canvas bind:this={canvas} style:width="{size}px" style:height="{size}px" aria-hidden="true"></canvas>

<style>
  canvas {
    display: block;
  }
</style>
