<script lang="ts" module>
  export type SkyKind = "day" | "night" | "cloudy" | "rain" | "snow" | "storm" | "fog";

  /** Grobe Himmelsart aus WMO-Wettercode und Tageszeit (wie open-meteo). */
  export function skyOf(code: number | undefined, isDay: boolean | undefined): SkyKind {
    if (code === undefined) return "day";
    if (code >= 95) return "storm";
    if ((code >= 71 && code <= 77) || code === 85 || code === 86) return "snow";
    if (code >= 51) return "rain";
    if (code === 45 || code === 48) return "fog";
    if (!isDay) return "night";
    if (code >= 2) return "cloudy";
    return "day";
  }
</script>

<script lang="ts">
  // Animierter Himmel als Hintergrund der Wetterkarte und der Wetteransicht, auf einer Canvas
  // gezeichnet: Sonne mit langsam drehenden Strahlen, Mond und funkelnde Sterne mit gelegentlicher
  // Sternschnuppe, weiche ziehende Wolken, Regen in drei Tiefenebenen mit Spritzern, Gewitter mit
  // Blitz und Aufleuchten, schaukelnde Schneeflocken, Nebelschwaden. Die Animation laeuft nur,
  // solange der Himmel zu sehen ist; bei "Weniger Bewegung" steht ein ruhiges Bild.
  import { onMount } from "svelte";
  import { sky as kindOf, skyGradient } from "./weather";

  let { code = 0, isDay = true }: { code?: number; isDay?: boolean } = $props();

  const kind = $derived(kindOf(code ?? 0));
  const background = $derived(skyGradient(code ?? 0, isDay ?? true));

  let host: HTMLDivElement;
  let canvas: HTMLCanvasElement;

  type Drop = { x: number; y: number; z: number; len: number; speed: number };
  type Flake = { x: number; y: number; z: number; r: number; speed: number; phase: number };
  type Star = { x: number; y: number; r: number; phase: number; speed: number };
  type Cloud = { sprite: HTMLCanvasElement; x: number; y: number; w: number; h: number; speed: number; alpha: number };
  type Splash = { x: number; y: number; age: number };
  type Bolt = { pts: [number, number][]; branch: [number, number][]; life: number };

  let W = 0;
  let H = 0;
  let dpr = 1;
  let drops: Drop[] = [];
  let flakes: Flake[] = [];
  let stars: Star[] = [];
  let clouds: Cloud[] = [];
  let splashes: Splash[] = [];
  let shooting: { x: number; y: number; vx: number; vy: number; life: number } | null = null;
  let nextShooting = 4;
  let bolt: Bolt | null = null;
  let flash = 0;
  let nextBolt = 2;
  let scene = { kind: "clear", night: false };

  const rand = (a: number, b: number) => a + Math.random() * (b - a);
  const rainy = (k: string) => k === "rain" || k === "drizzle" || k === "storm";

  /** Wolke als vorgerenderte Grafik: mehrere weiche Ballen, unten flacher. */
  function cloudSprite(w: number, h: number, tone: [number, number, number], soft: number): HTMLCanvasElement {
    const c = document.createElement("canvas");
    c.width = Math.max(1, Math.round(w * dpr));
    c.height = Math.max(1, Math.round(h * dpr));
    const g = c.getContext("2d")!;
    g.scale(dpr, dpr);
    const puffs = 7;
    for (let i = 0; i < puffs; i++) {
      const cx = w * (0.18 + (0.64 * i) / (puffs - 1)) + rand(-w * 0.04, w * 0.04);
      const top = 1 - Math.abs(i / (puffs - 1) - 0.5) * 1.5;
      const r = h * (0.26 + 0.2 * top) * rand(0.9, 1.1);
      const cy = h * (0.62 - 0.14 * top);
      const grad = g.createRadialGradient(cx, cy - r * 0.2, r * 0.1, cx, cy, r);
      const [R, G, B] = tone;
      grad.addColorStop(0, `rgba(${R},${G},${B},${0.95 * soft})`);
      grad.addColorStop(0.55, `rgba(${R},${G},${B},${0.55 * soft})`);
      grad.addColorStop(1, `rgba(${R},${G},${B},0)`);
      g.fillStyle = grad;
      g.beginPath();
      g.arc(cx, cy, r, 0, Math.PI * 2);
      g.fill();
    }
    return c;
  }

  /** Alles fuer die aktuelle Wetterlage und Groesse neu anlegen. */
  function build() {
    const k = scene.kind;
    const night = scene.night;
    const area = W * H;
    drops = [];
    flakes = [];
    stars = [];
    clouds = [];
    splashes = [];
    bolt = null;
    flash = 0;
    if (!W || !H) return;

    if (night && (k === "clear" || k === "partly")) {
      const count = Math.min(170, Math.round(area / 1300));
      stars = Array.from({ length: count }, () => ({ x: rand(0, W), y: rand(0, H * 0.85), r: Math.random() < 0.12 ? rand(1.1, 1.7) : rand(0.4, 1), phase: rand(0, 6.3), speed: rand(0.8, 2.6) }));
    }

    const cloudCount = { clear: 0, partly: 3, cloudy: 6, fog: 0, drizzle: 5, rain: 6, snow: 5, storm: 7 }[k] ?? 0;
    const tone: [number, number, number] = night
      ? (k === "storm" ? [52, 56, 78] : [86, 96, 124])
      : k === "storm" ? [92, 98, 120] : rainy(k) ? [150, 162, 180] : k === "snow" ? [226, 232, 242] : k === "cloudy" ? [222, 228, 238] : [255, 255, 255];
    for (let i = 0; i < cloudCount; i++) {
      const heavy = k !== "partly";
      const w = W * rand(heavy ? 0.45 : 0.32, heavy ? 0.8 : 0.55);
      const h = w * 0.42;
      const depth = i / Math.max(1, cloudCount - 1);
      clouds.push({
        sprite: cloudSprite(w, h, tone, heavy ? 0.9 : 0.85),
        x: rand(-w * 0.5, W),
        y: heavy ? H * rand(-0.18, 0.28) : H * rand(0.02, 0.42),
        w,
        h,
        speed: rand(5, 11) * (0.5 + depth),
        alpha: heavy ? rand(0.55, 0.95) : rand(0.75, 0.95),
      });
    }

    if (rainy(k)) {
      const per = k === "storm" ? 520 : k === "drizzle" ? 2000 : 820;
      const count = Math.min(k === "storm" ? 380 : 260, Math.round(area / per));
      drops = Array.from({ length: count }, () => newDrop(true));
    }
    if (k === "snow") {
      const count = Math.min(230, Math.round(area / 1000));
      flakes = Array.from({ length: count }, () => {
        const z = Math.random();
        return { x: rand(0, W), y: rand(-H, H), z, r: 0.8 + z * 2.4, speed: 18 + z * 46, phase: rand(0, 6.3) };
      });
    }
  }

  function newDrop(anywhere: boolean): Drop {
    const z = Math.random();
    const drizzle = scene.kind === "drizzle";
    const len = (drizzle ? 5 : 10) + z * (drizzle ? 6 : 16);
    return { x: rand(-H * 0.2, W + 10), y: anywhere ? rand(-H, H) : rand(-H * 0.4, -len), z, len, speed: (drizzle ? 360 : 620) + z * 520 };
  }

  function newBolt(): Bolt {
    const pts: [number, number][] = [];
    let x = rand(W * 0.2, W * 0.8);
    let y = 0;
    const end = H * rand(0.55, 0.92);
    while (y < end) {
      pts.push([x, y]);
      y += H * rand(0.06, 0.12);
      x += rand(-1, 1) * W * 0.05;
    }
    pts.push([x, end]);
    const from = pts[Math.floor(pts.length * 0.4)] ?? pts[0];
    const branch: [number, number][] = [from];
    let [bx, by] = from;
    const dir = Math.random() < 0.5 ? -1 : 1;
    for (let i = 0; i < 4; i++) {
      bx += dir * W * rand(0.03, 0.07);
      by += H * rand(0.05, 0.1);
      branch.push([bx, by]);
    }
    return { pts, branch, life: 0.42 };
  }

  const WIND = 0.16;

  function draw(ctx: CanvasRenderingContext2D, t: number, dt: number) {
    const k = scene.kind;
    const night = scene.night;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    const base = Math.min(W, H);

    // Sterne und Sternschnuppe
    if (stars.length) {
      for (const s of stars) {
        const a = 0.3 + 0.7 * (0.5 + 0.5 * Math.sin(t * s.speed + s.phase));
        ctx.globalAlpha = a;
        ctx.fillStyle = "#fff";
        ctx.beginPath();
        ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
        ctx.fill();
        if (s.r > 1.1) {
          ctx.globalAlpha = a * 0.35;
          ctx.fillRect(s.x - s.r * 3, s.y - 0.4, s.r * 6, 0.8);
          ctx.fillRect(s.x - 0.4, s.y - s.r * 3, 0.8, s.r * 6);
        }
      }
      ctx.globalAlpha = 1;
      if (!shooting && t > nextShooting) {
        shooting = { x: rand(W * 0.3, W * 1.05), y: rand(0, H * 0.35), vx: -rand(260, 380), vy: rand(90, 150), life: 1 };
      }
      if (shooting) {
        shooting.x += shooting.vx * dt;
        shooting.y += shooting.vy * dt;
        shooting.life -= dt * 1.2;
        const tail = 0.22;
        const grad = ctx.createLinearGradient(shooting.x, shooting.y, shooting.x - shooting.vx * tail, shooting.y - shooting.vy * tail);
        grad.addColorStop(0, `rgba(255,255,255,${Math.max(0, shooting.life)})`);
        grad.addColorStop(1, "rgba(255,255,255,0)");
        ctx.strokeStyle = grad;
        ctx.lineWidth = 1.4;
        ctx.beginPath();
        ctx.moveTo(shooting.x, shooting.y);
        ctx.lineTo(shooting.x - shooting.vx * tail, shooting.y - shooting.vy * tail);
        ctx.stroke();
        if (shooting.life <= 0) {
          shooting = null;
          nextShooting = t + rand(6, 14);
        }
      }
    }

    // Sonne oder Mond
    if (k === "clear" || k === "partly") {
      const cx = W * 0.8;
      const cy = H * 0.24;
      if (!night) {
        const R = Math.max(18, Math.min(50, base * 0.11)) * (1 + 0.02 * Math.sin(t * 0.8));
        const glow = ctx.createRadialGradient(cx, cy, R * 0.6, cx, cy, R * 5.5);
        glow.addColorStop(0, "rgba(255,226,150,0.45)");
        glow.addColorStop(0.4, "rgba(255,210,120,0.12)");
        glow.addColorStop(1, "rgba(255,210,120,0)");
        ctx.fillStyle = glow;
        ctx.fillRect(0, 0, W, H);
        ctx.save();
        ctx.translate(cx, cy);
        ctx.rotate(t * 0.03);
        ctx.globalCompositeOperation = "lighter";
        for (let i = 0; i < 12; i++) {
          ctx.rotate((Math.PI * 2) / 12);
          const ray = ctx.createLinearGradient(R, 0, R * 4.6, 0);
          ray.addColorStop(0, "rgba(255,236,180,0.16)");
          ray.addColorStop(1, "rgba(255,236,180,0)");
          ctx.fillStyle = ray;
          ctx.beginPath();
          ctx.moveTo(R * 0.9, -R * 0.12);
          ctx.lineTo(R * 4.6, -R * (i % 2 ? 0.34 : 0.22));
          ctx.lineTo(R * 4.6, R * (i % 2 ? 0.34 : 0.22));
          ctx.lineTo(R * 0.9, R * 0.12);
          ctx.fill();
        }
        ctx.restore();
        const disc = ctx.createRadialGradient(cx - R * 0.3, cy - R * 0.3, R * 0.1, cx, cy, R);
        disc.addColorStop(0, "#fffbe8");
        disc.addColorStop(0.55, "#ffe27a");
        disc.addColorStop(1, "#ffbf3a");
        ctx.fillStyle = disc;
        ctx.beginPath();
        ctx.arc(cx, cy, R, 0, Math.PI * 2);
        ctx.fill();
      } else {
        const R = Math.max(15, Math.min(38, base * 0.09));
        const glow = ctx.createRadialGradient(cx, cy, R * 0.8, cx, cy, R * 4.5);
        glow.addColorStop(0, "rgba(214,226,255,0.32)");
        glow.addColorStop(1, "rgba(214,226,255,0)");
        ctx.fillStyle = glow;
        ctx.fillRect(0, 0, W, H);
        const disc = ctx.createRadialGradient(cx - R * 0.35, cy - R * 0.35, R * 0.1, cx, cy, R);
        disc.addColorStop(0, "#fffef6");
        disc.addColorStop(1, "#e3dfca");
        ctx.fillStyle = disc;
        ctx.beginPath();
        ctx.arc(cx, cy, R, 0, Math.PI * 2);
        ctx.fill();
        ctx.fillStyle = "rgba(150,145,120,0.16)";
        for (const [dx, dy, r] of [[-0.3, -0.1, 0.22], [0.25, 0.3, 0.16], [0.1, -0.38, 0.1], [-0.05, 0.42, 0.08]]) {
          ctx.beginPath();
          ctx.arc(cx + dx * R, cy + dy * R, r * R, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    }

    // Wolken
    for (const c of clouds) {
      c.x += c.speed * dt;
      if (c.x > W + 4) c.x = -c.w - rand(0, W * 0.2);
      ctx.globalAlpha = c.alpha;
      ctx.drawImage(c.sprite, c.x, c.y, c.w, c.h);
    }
    ctx.globalAlpha = 1;

    // Nebel
    if (k === "fog") {
      for (let i = 0; i < 4; i++) {
        const y = H * (0.25 + i * 0.2);
        const x = W * 0.5 + Math.sin(t * (0.05 + i * 0.02) + i) * W * 0.25;
        ctx.save();
        ctx.translate(x, y);
        ctx.scale(1, 0.22);
        const r = W * 0.9;
        const grad = ctx.createRadialGradient(0, 0, 0, 0, 0, r);
        grad.addColorStop(0, `rgba(235,238,242,${0.32 - i * 0.04})`);
        grad.addColorStop(1, "rgba(235,238,242,0)");
        ctx.fillStyle = grad;
        ctx.fillRect(-r, -r, r * 2, r * 2);
        ctx.restore();
      }
    }

    // Regen in drei Tiefen, vorn kraeftiger
    if (drops.length) {
      const layers = [
        { min: 0, max: 0.4, width: 0.7, alpha: 0.22 },
        { min: 0.4, max: 0.75, width: 1, alpha: 0.36 },
        { min: 0.75, max: 1.01, width: 1.4, alpha: 0.55 },
      ];
      ctx.lineCap = "round";
      for (const layer of layers) {
        ctx.strokeStyle = `rgba(214,228,255,${night ? layer.alpha * 0.8 : layer.alpha})`;
        ctx.lineWidth = layer.width;
        ctx.beginPath();
        for (const d of drops) {
          if (d.z < layer.min || d.z >= layer.max) continue;
          ctx.moveTo(d.x, d.y);
          ctx.lineTo(d.x - d.len * WIND, d.y - d.len);
        }
        ctx.stroke();
      }
      for (let i = 0; i < drops.length; i++) {
        const d = drops[i];
        d.y += d.speed * dt;
        d.x += d.speed * WIND * dt;
        if (d.y > H) {
          if (d.z > 0.55 && splashes.length < 60) splashes.push({ x: d.x, y: H - rand(1, 6), age: 0 });
          drops[i] = newDrop(false);
        }
      }
      for (const s of splashes) {
        s.age += dt;
        const p = s.age / 0.28;
        ctx.strokeStyle = `rgba(214,228,255,${0.45 * (1 - p)})`;
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.ellipse(s.x, s.y, 1 + p * 5, 0.6 + p * 1.4, 0, Math.PI, Math.PI * 2);
        ctx.stroke();
      }
      splashes = splashes.filter((s) => s.age < 0.28);
    }

    // Gewitter: Blitz mit Verzweigung, der ganze Himmel leuchtet kurz auf
    if (k === "storm") {
      if (!bolt && t > nextBolt) {
        bolt = newBolt();
        flash = 1;
      }
      if (flash > 0) {
        ctx.fillStyle = `rgba(225,232,255,${flash * 0.32})`;
        ctx.fillRect(0, 0, W, H);
        flash = Math.max(0, flash - dt * 2.6);
      }
      if (bolt) {
        bolt.life -= dt;
        const on = Math.sin(t * 70) > -0.3 ? 1 : 0.45;
        const a = Math.max(0, bolt.life / 0.42) * on;
        ctx.save();
        ctx.shadowColor = "rgba(190,200,255,0.9)";
        ctx.shadowBlur = 14;
        ctx.strokeStyle = `rgba(255,255,255,${a})`;
        for (const [line, width] of [[bolt.pts, 2], [bolt.branch, 1.1]] as const) {
          ctx.lineWidth = width;
          ctx.beginPath();
          line.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
          ctx.stroke();
        }
        ctx.restore();
        if (bolt.life <= 0) {
          bolt = null;
          nextBolt = t + rand(3.5, 8);
        }
      }
    }

    // Schnee, schaukelnd, vorn groesser und schneller
    if (flakes.length) {
      for (const f of flakes) {
        f.y += f.speed * dt;
        f.x += (Math.sin(t * 0.9 + f.phase) * 14 * (0.4 + f.z) + 6) * dt;
        if (f.y > H + 4) {
          f.y = -4;
          f.x = rand(-10, W);
        }
        if (f.x > W + 4) f.x = -4;
        if (f.z > 0.7) {
          ctx.fillStyle = "rgba(255,255,255,0.14)";
          ctx.beginPath();
          ctx.arc(f.x, f.y, f.r * 2.1, 0, Math.PI * 2);
          ctx.fill();
        }
        ctx.fillStyle = `rgba(255,255,255,${0.45 + 0.5 * f.z})`;
        ctx.beginPath();
        ctx.arc(f.x, f.y, f.r, 0, Math.PI * 2);
        ctx.fill();
      }
      const ground = ctx.createLinearGradient(0, H * 0.82, 0, H);
      ground.addColorStop(0, "rgba(255,255,255,0)");
      ground.addColorStop(1, "rgba(255,255,255,0.16)");
      ctx.fillStyle = ground;
      ctx.fillRect(0, H * 0.82, W, H * 0.18);
    }
  }

  // Szene neu aufbauen, wenn sich die Wetterlage aendert.
  $effect(() => {
    const next = { kind, night: !isDay };
    if (next.kind !== scene.kind || next.night !== scene.night) {
      scene = next;
      build();
    }
  });

  onMount(() => {
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    scene = { kind, night: !isDay };
    const still = window.matchMedia("(prefers-reduced-motion: reduce)");
    let visible = true;
    let raf = 0;
    let last = 0;
    let t = 0;

    const resize = () => {
      const r = host.getBoundingClientRect();
      dpr = Math.min(2, window.devicePixelRatio || 1);
      W = Math.max(1, Math.round(r.width));
      H = Math.max(1, Math.round(r.height));
      canvas.width = Math.round(W * dpr);
      canvas.height = Math.round(H * dpr);
      build();
      if (still.matches || !visible) draw(ctx, t, 0);
    };
    const loop = (now: number) => {
      raf = 0;
      if (!visible || still.matches || document.hidden) return;
      const dt = last ? Math.min(0.05, (now - last) / 1000) : 0.016;
      // Etwa 45 Bilder pro Sekunde reichen fuer weiche Bewegung und sparen Rechenzeit.
      if (now - last >= 21) {
        last = now;
        t += dt;
        draw(ctx, t, dt);
      }
      raf = requestAnimationFrame(loop);
    };
    const start = () => {
      if (!raf && visible && !still.matches && !document.hidden) {
        last = 0;
        raf = requestAnimationFrame(loop);
      }
    };

    const ro = new ResizeObserver(resize);
    ro.observe(host);
    const io = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      if (visible) start();
    });
    io.observe(host);
    const onVisibility = () => start();
    document.addEventListener("visibilitychange", onVisibility);
    const onMotion = () => (still.matches ? draw(ctx, t, 0) : start());
    still.addEventListener("change", onMotion);
    resize();
    // Ruhiges Bild: einmal ein paar Sekunden "vorspulen", damit Regen und Schnee verteilt sind.
    if (still.matches) draw(ctx, 2, 0);
    start();
    return () => {
      if (raf) cancelAnimationFrame(raf);
      ro.disconnect();
      io.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
      still.removeEventListener("change", onMotion);
    };
  });
</script>

<div class="sky" class:night={!isDay} bind:this={host} style:background aria-hidden="true">
  <span class="haze"></span>
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .sky {
    position: absolute;
    inset: 0;
    z-index: -1;
    overflow: hidden;
    transition: background 1.2s ease;
  }
  /* Weiches Licht von oben rechts (Sonne/Mond) und ein dunklerer Grund unten fuer lesbaren Text. */
  .haze {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(120% 90% at 85% 0%, rgba(255, 255, 255, 0.14), transparent 55%),
      linear-gradient(180deg, transparent 45%, rgba(0, 0, 0, 0.28));
  }
  .night .haze {
    background:
      radial-gradient(120% 90% at 85% 0%, rgba(160, 180, 255, 0.12), transparent 55%),
      linear-gradient(180deg, transparent 40%, rgba(0, 0, 10, 0.4));
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
