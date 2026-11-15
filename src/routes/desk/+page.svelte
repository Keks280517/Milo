<script lang="ts">
  import "$lib/theme.css";
  import { t, lang } from "$lib/i18n";
  // Desktop-Ebene: Deko-Objekte und Milo liegen auf der Oberkante der Taskleiste. Man greift sie
  // mit der Maus, traegt sie herum (sie pendeln am Zeiger), wirft sie, und sie fallen mit Schwerkraft
  // zurueck. Ueber der Insel losgelassen, kehren sie dorthin zurueck. Waehrend des Tragens meldet
  // der Rust-Kern die Mausposition (milo://desk-pointer), auch wenn der Zeiger ueber anderen Fenstern ist.
  // Rechtsklick auf Milo oeffnet die Minispiele (Pong, Basketball, Tennis); Milo ist dabei der Ball.
  // Escape (vom Kern gemeldet) beendet ein Spiel mit einer Puff-Wolke, Milo faellt zurueck.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { Character } from "$lib/character/engine";
  import { decorInfo, drawDecor, type DecorKind } from "$lib/character/decor";
  import { GAMES, LEVELS, createGame, type Game, type GameEvent, type GameKind, type Level, type Pointer, type Result } from "$lib/desk/games";

  /** Groesse auf dem Desktop (Milos Koerper; Deko im selben Massstab). */
  const SIZE = 38;
  const GRAVITY = 2600;

  interface Body {
    kind: string;
    x: number;
    /** Unterkante (Fuesse) */
    y: number;
    vx: number;
    vy: number;
    rot: number;
    vrot: number;
    carried: boolean;
    resting: boolean;
    /** Wackeln nach Stupser oder Aufprall */
    wobble: number;
    /** Kehrt gerade in die Insel zurueck (schrumpft nach oben weg). */
    leaving: number;
    /** Schon als liegend gespeichert. */
    saved: boolean;
    /** Gesperrt: bleibt liegen, laesst sich nicht greifen. */
    locked: boolean;
  }

  let canvas: HTMLCanvasElement;
  /** Rechtsklick-Menue eines Objekts auf dem Desktop. */
  let menu = $state<{ x: number; y: number; locked: boolean; label: string } | null>(null);
  let menuEl = $state<HTMLElement>();
  let menuTarget: { kind: string; x: number; locked: boolean; leaving: number } | null = null;
  let act: (action: "lock" | "unlock" | "home" | "remove") => void = () => {};
  /** Rechtsklick auf Milo: Spiele-Menue. */
  let miloMenu = $state<{ x: number; y: number } | null>(null);
  /** Schwierigkeit der Spiele (gemerkt), Standard Mittel. */
  let level = $state<Level>(readLevel());
  function readLevel(): Level {
    try {
      const v = localStorage.getItem("milo.level");
      return v === "easy" || v === "hard" ? v : "normal";
    } catch {
      return "normal";
    }
  }
  function setLevel(next: Level) {
    level = next;
    try {
      localStorage.setItem("milo.level", next);
    } catch {
      /* ohne Speicher eben nur fuer diese Sitzung */
    }
  }
  /** Anzeige des laufenden Spiels (Spielstand, Countdown, Ergebnis). */
  let hud = $state<{
    kind: GameKind;
    label: string;
    aLabel: string;
    a: string;
    bLabel: string;
    b: string;
    note: string;
    count: string;
    result: Result | null;
  } | null>(null);
  let play: (kind: GameKind) => void = () => {};
  let quit: () => void = () => {};
  let sendHome: () => void = () => {};

  onMount(() => {
    const g = canvas.getContext("2d")!;
    let width = window.innerWidth;
    let height = window.innerHeight;
    const ground = () => height - 1;
    let bodies: Body[] = [];
    const milo = new Character();
    milo.idleGestures = false;
    /** Milos eigener Ablauf auf dem Desktop. */
    let goal: number | null = null;
    let rest = 1.5;
    let clock = 0;
    let pointer = { x: width / 2, y: height / 2, vx: 0, vy: 0, t: performance.now() };
    let frame = 0;
    let last = performance.now();
    let idleTimer = 0;
    const dpr = () => window.devicePixelRatio || 1;
    const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

    // ---------- Minispiele ----------
    let game: Game | null = null;
    /** Milo, solange er der Ball ist (nicht in der Koerperliste). */
    let parked: Body | null = null;
    let countdown = 0;
    /** Ausblenden nach Esc: 0 = aus, steigt bis 1. */
    let fading = 0;
    let puffs: { x: number; y: number; r: number; t: number; seed: number }[] = [];
    const gp: Pointer = { x: width / 2, y: height / 2, vx: 0, vy: 0 };
    let gpT = performance.now();
    let accent = "#64d2ff";
    invoke<{ accent?: string }>("get_settings").then((s) => s.accent && (accent = s.accent)).catch(() => {});
    /** Bestwerte gelten je Spiel und Schwierigkeit. */
    let bestKey = "";
    const bestOf = (k: string) => {
      try {
        return Number(localStorage.getItem("milo.best." + k)) || 0;
      } catch {
        return 0;
      }
    };
    const saveBest = (k: string, v: number) => {
      try {
        localStorage.setItem("milo.best." + k, String(v));
      } catch {
        /* ohne Speicher eben ohne Bestwert */
      }
    };

    // ---------- Milos Alltag auf der Taskleiste ----------
    type Activity = { kind: "stroll" | "dash" | "visit" | "push" | "sit" | "doze" | "peek"; until: number; target?: Body };
    let activity: Activity | null = null;
    /** Letzte Zeigerposition (vom Kern, auch ueber anderen Fenstern) und wann sie sich zuletzt bewegt hat. */
    let hover = { x: -1e4, y: -1e4, t: 0 };
    let waveCooldown = 8;

    // ---------- Ausflug: Milo kommt von selbst aus der Insel ----------
    /** Laeuft ein Ausflug, endet er zu dieser Zeit (Uhr der Seite); null = Milo bleibt, bis man ihn holt. */
    let tripEnds: number | null = null;

    function visit(seconds: number, from: number) {
      if (bodies.some((b) => b.kind === "milo") || parked) return;
      spawn("milo", clamp(from * width, SIZE, width - SIZE), ground(), false);
      tripEnds = clock + seconds;
      rest = 1.2;
    }

    /** Ausflug vorbei: Milo verabschiedet sich und taucht in der Insel wieder auf. */
    function endTrip() {
      if (tripEnds === null) return;
      tripEnds = null;
      const k = bodies.find((b) => b.kind === "milo");
      if (k) {
        goal = null;
        activity = null;
        milo.play("vanish");
        k.leaving = 0.001;
      }
      invoke("pet_home").catch(() => {});
      wake();
    }

    /** Der Nutzer fasst Milo unterwegs an: aus dem Ausflug wird ein normaler Desktop-Besuch. */
    function adopt() {
      if (tripEnds === null) return;
      tripEnds = null;
      invoke("pet_adopt").catch(() => {});
    }

    const boxOf = (b: Body) => {
      if (b.kind === "milo") return { w: SIZE * 1.3, h: SIZE * 1.3 };
      const info = decorInfo(b.kind);
      return { w: SIZE * (info?.w ?? 0.7) + 8, h: SIZE * (info?.h ?? 1) + 6 };
    };

    function spawn(kind: string, x: number, y: number, carried: boolean, locked = false): Body {
      const body: Body = { kind, x, y, vx: 0, vy: 0, rot: 0, vrot: 0, carried, resting: !carried && y >= ground() - 0.5, wobble: 0, leaving: 0, saved: !carried, locked };
      bodies.push(body);
      if (kind === "milo") {
        milo.play(carried ? "startle" : "appear");
      }
      wake();
      return body;
    }

    function carried(): Body | undefined {
      return bodies.find((b) => b.carried);
    }

    // ---------- Physik ----------

    function step(dt: number) {
      clock += dt;
      for (const b of bodies) {
        b.wobble = Math.max(0, b.wobble - dt * 1.6);
        if (b.leaving > 0) {
          b.leaving += dt;
          b.y -= dt * 900;
          continue;
        }
        if (b.carried) {
          // Haengt am Zeiger und pendelt mit der Bewegung.
          const h = boxOf(b).h;
          b.x = pointer.x;
          b.y = pointer.y + h * 0.82;
          const target = Math.max(-0.7, Math.min(0.7, -pointer.vx * 0.0011));
          b.vrot += ((target - b.rot) * 60 - b.vrot * 7) * dt;
          b.rot += b.vrot * dt;
          continue;
        }
        if (!b.resting) {
          b.vy += GRAVITY * dt;
          b.x += b.vx * dt;
          b.y += b.vy * dt;
          b.rot += b.vrot * dt;
          b.vrot *= Math.pow(0.4, dt);
          const half = boxOf(b).w / 2;
          if (b.x < half) { b.x = half; b.vx = Math.abs(b.vx) * 0.5; }
          if (b.x > width - half) { b.x = width - half; b.vx = -Math.abs(b.vx) * 0.5; }
          if (b.y >= ground()) {
            b.y = ground();
            if (b.vy > 260) {
              // Aufprall: zurueckfedern, Drehung abbauen, wackeln.
              b.vy = -b.vy * (b.kind === "duck" ? 0.45 : 0.3);
              b.vx *= 0.7;
              b.vrot *= -0.4;
              b.wobble = Math.min(1, b.wobble + 0.6);
              if (b.kind === "milo") milo.play("land");
            } else {
              b.vy = 0;
              b.vx *= Math.pow(0.02, dt);
              if (Math.abs(b.vx) < 8) {
                b.vx = 0;
                b.resting = true;
                settle(b);
              }
            }
          }
        } else {
          // Liegt: Schraeglage richtet sich weich auf.
          b.rot += (0 - b.rot) * Math.min(1, dt * 10);
        }
        if (b.resting && b.kind !== "milo") b.rot *= 1 - Math.min(1, dt * 10);
      }
      stepGame(dt);
      // Milo laeuft auf der Taskleiste herum, sobald er steht.
      const k = bodies.find((b) => b.kind === "milo");
      if (tripEnds !== null && clock >= tripEnds && k && k.resting && !k.carried && k.leaving === 0 && !game && !miloMenu) endTrip();
      if (k && k.resting && !k.carried && k.leaving === 0) walk(k, dt);
      else if (k && !k.resting) milo.setMood("idle");
      milo.update(dt);
      // Weg sind, die in die Insel zurueckgekehrt sind.
      bodies = bodies.filter((b) => b.leaving < 0.6);
    }

    function walk(k: Body, dt: number) {
      // Der Zeiger in der Naehe zieht Milos Blick an; bleibt er kurz stehen, winkt Milo.
      const hx = hover.x - k.x, hy = hover.y - (k.y - SIZE * 0.6);
      const near = Math.hypot(hx, hy) < 320;
      milo.gaze = near ? { x: clamp(hx / 220, -1, 1), y: clamp(hy / 220, -1, 1) } : null;
      waveCooldown -= dt;
      if (near && clock - hover.t > 1.2 && waveCooldown <= 0 && !milo.busy && goal === null) {
        if (activity?.kind === "doze" || activity?.kind === "sit") activity = null;
        milo.play("wave");
        waveCooldown = 25;
      }
      if (goal !== null) {
        const dx = goal - k.x;
        const dir = Math.sign(dx) || 1;
        const dash = activity?.kind === "dash";
        const moved = Math.min(Math.abs(dx), (dash ? 150 : 55) * dt) * dir;
        k.x += moved;
        milo.dir = dir;
        milo.step += (Math.PI * Math.abs(moved)) / (SIZE * 0.36);
        milo.setMood(dash ? "dash" : "walk");
        if (Math.abs(dx) < 1) {
          goal = null;
          arrive(k);
        }
        return;
      }
      if (activity && clock < activity.until) {
        const mood = { sit: "sit", doze: near ? "idle" : "doze", push: "push" } as const;
        milo.setMood(mood[activity.kind as keyof typeof mood] ?? "idle");
        return;
      }
      activity = null;
      milo.setMood("idle");
      rest -= dt;
      if (rest > 0 || milo.busy) return;
      choose(k);
    }

    /** Was macht Milo als Naechstes? Spazieren, flitzen, Deko besuchen, an den Rand schauen, huepfen ... */
    function choose(k: Body) {
      const decor = bodies.filter((b) => b.kind !== "milo" && b.resting && !b.carried && b.leaving === 0);
      const hour = new Date().getHours();
      const sleepy = hour >= 23 || hour < 6;
      const away = (lo: number, hi: number) => clamp(k.x + (Math.random() < 0.5 ? -1 : 1) * (lo + Math.random() * (hi - lo)), SIZE, width - SIZE);
      const r = Math.random();
      rest = 1 + Math.random() * 2;
      if (sleepy && r < 0.25) {
        activity = { kind: "doze", until: clock + 10 + Math.random() * 10 };
      } else if (r < 0.26) {
        activity = { kind: "stroll", until: clock + 60 };
        goal = away(80, 380);
      } else if (r < 0.36) {
        activity = { kind: "dash", until: clock + 60 };
        goal = away(300, 800);
        milo.play("hop");
      } else if (r < 0.54 && decor.length) {
        const target = decor[Math.floor(Math.random() * decor.length)];
        const side = target.x > k.x ? -1 : 1;
        activity = { kind: "visit", until: clock + 60, target };
        goal = clamp(target.x + side * (boxOf(target).w / 2 + SIZE * 0.55), SIZE, width - SIZE);
      } else if (r < 0.6) {
        activity = { kind: "peek", until: clock + 60 };
        goal = k.x < width / 2 ? SIZE * 0.7 : width - SIZE * 0.7;
      } else if (r < 0.7) {
        // Echter Luftsprung mit Landung.
        k.resting = false;
        k.vy = -600 - Math.random() * 260;
        k.vx = (Math.random() - 0.5) * 160;
        milo.play("jump");
        rest = 2;
      } else if (r < 0.86) {
        const gestures = ["lookAround", "wave", "tilt", "stretch", "hop", "shrug", "kick", "nod"] as const;
        milo.play(gestures[Math.floor(Math.random() * gestures.length)]);
        rest = 2 + Math.random() * 2;
      } else if (r < 0.95) {
        activity = { kind: "sit", until: clock + 5 + Math.random() * 6 };
      } else {
        activity = { kind: "doze", until: clock + 8 + Math.random() * 8 };
      }
    }

    /** Angekommen: Deko anstupsen oder anschauen, am Rand hinausspaehen. */
    function arrive(k: Body) {
      const a = activity;
      if (a?.kind === "visit" && a.target && bodies.includes(a.target)) {
        const dir = Math.sign(a.target.x - k.x) || 1;
        milo.dir = dir;
        if (a.target.locked || Math.random() < 0.35) {
          a.target.wobble = 0.7;
          milo.play(Math.random() < 0.5 ? "tilt" : "lookAround");
          activity = null;
        } else {
          milo.play("kick");
          nudge(a.target, dir);
          activity = { kind: "push", until: clock + 0.7 };
        }
      } else if (a?.kind === "peek") {
        milo.dir = k.x < width / 2 ? -1 : 1;
        milo.play("lookAround");
        activity = null;
      } else {
        activity = null;
      }
      rest = 1.5 + Math.random() * 2.5;
    }

    /** Milo schubst ein Objekt ein Stueck weiter; der neue Platz wird gespeichert. */
    function nudge(b: Body, dir: number) {
      if (b.locked) return;
      b.saved = false;
      invoke("desk_item", { kind: b.kind, x: b.x / width, action: "remove" }).catch(() => {});
      b.resting = false;
      b.vx = dir * (150 + Math.random() * 130);
      b.vy = -140;
      b.vrot = dir * 2;
      b.wobble = 1;
      wake();
    }

    // ---------- Spiel-Ablauf ----------

    function syncHud() {
      if (!game) return;
      const h = game.hud();
      const label = GAMES.find((x) => x.kind === game!.kind)?.label ?? "";
      const count = countdown > 0.6 ? String(Math.ceil(countdown - 0.6)) : countdown > 0 ? t("Los!", "Go!") : "";
      const next = { kind: game.kind, label, ...h, count, result: game.result };
      if (JSON.stringify(next) !== JSON.stringify(hud)) hud = next;
    }

    function react(events: GameEvent[]) {
      for (const e of events) {
        if (e === "hit") milo.play(Math.random() < 0.5 ? "giggle" : "startle");
        else if (e === "score" || e === "win") milo.play("success");
        else if (e === "lose" || e === "over") milo.play("shrug");
        else if (e === "rim") milo.play("tilt");
      }
    }

    function stepGame(dt: number) {
      for (const p of puffs) p.t += dt;
      puffs = puffs.filter((p) => p.t < 0.75);
      if (!game) return;
      if (fading > 0) {
        fading += dt / 0.45;
        if (fading >= 1) {
          game = null;
          fading = 0;
        }
        return;
      }
      // Ohne neue Mausbewegung klingt die Zeigergeschwindigkeit ab.
      if (performance.now() - gpT > 40) {
        gp.vx *= Math.pow(0.001, dt);
        gp.vy *= Math.pow(0.001, dt);
      }
      if (countdown > 0) {
        countdown = Math.max(0, countdown - dt);
      } else {
        const hadResult = !!game.result;
        react(game.update(dt, gp));
        if (game.result && !hadResult) saveBest(bestKey, game.result.best);
      }
      game.ball.squash = Math.max(0, game.ball.squash - dt * 4);
      syncHud();
    }

    play = (kind: GameKind) => {
      miloMenu = null;
      menu = null;
      const k = parked ?? bodies.find((b) => b.kind === "milo");
      if (!k) return;
      if (!parked) {
        parked = k;
        bodies = bodies.filter((b) => b !== k);
      }
      goal = null;
      activity = null;
      milo.gaze = null;
      milo.setMood("idle");
      // Mittel behaelt die alten Bestwerte; Leicht und Schwer zaehlen getrennt.
      bestKey = level === "normal" ? kind : `${kind}.${level}`;
      game = createGame(kind, { width, height, ground: ground(), accent }, bestOf(bestKey), level);
      countdown = 3.6;
      fading = 0;
      document.body.dataset.game = kind;
      invoke("desk_game", { on: true }).catch(() => {});
      milo.play("hop");
      syncHud();
      wake();
    };

    /** Esc oder Beenden: Puff-Wolken, das Spielfeld verschwindet, Milo faellt auf die Taskleiste. */
    quit = () => {
      if (!game || fading > 0) return;
      fading = 0.001;
      for (const piece of game.pieces()) puffs.push({ ...piece, t: 0, seed: Math.random() * 6 });
      const b = game.ball;
      puffs.push({ x: b.x, y: b.y, r: 46, t: 0, seed: 1 });
      const k = parked;
      if (k) {
        k.x = clamp(b.x, SIZE, width - SIZE);
        k.y = Math.min(ground(), b.y + SIZE / 2);
        k.vx = clamp(b.vx * 0.2, -300, 300);
        k.vy = -280;
        k.rot = 0;
        k.vrot = 0;
        k.resting = false;
        k.carried = false;
        bodies.push(k);
        parked = null;
      }
      milo.play("startle");
      hud = null;
      delete document.body.dataset.game;
      invoke("desk_game", { on: false }).catch(() => {});
      rest = 2;
      wake();
    };

    sendHome = () => {
      miloMenu = null;
      const k = bodies.find((b) => b.kind === "milo");
      if (k) {
        milo.play("vanish");
        k.leaving = 0.001;
      }
      invoke("desk_drop_home", { kind: "milo", x: 0, home: true }).catch(() => {});
      wake();
    };

    function drawPuffs() {
      for (const p of puffs) {
        const u = p.t / 0.75;
        const ease = 1 - Math.pow(1 - u, 3);
        g.save();
        g.globalAlpha = Math.max(0, 1 - u) * 0.8;
        g.fillStyle = "#e9e9f2";
        for (let i = 0; i < 7; i++) {
          const ang = (i / 7) * Math.PI * 2 + p.seed;
          const dist = p.r * 0.45 * (0.3 + ease);
          const rad = p.r * 0.32 * (0.6 + ease * 0.7) * (0.8 + ((i * 37) % 10) / 25);
          g.beginPath();
          g.arc(p.x + Math.cos(ang) * dist, p.y + Math.sin(ang) * dist * 0.8, rad, 0, Math.PI * 2);
          g.fill();
        }
        // Funken fliegen hinaus
        g.fillStyle = "#ffd60a";
        for (let i = 0; i < 5; i++) {
          const ang = (i / 5) * Math.PI * 2 + p.seed * 2;
          const dist = p.r * (0.5 + ease * 0.9);
          g.beginPath();
          g.arc(p.x + Math.cos(ang) * dist, p.y + Math.sin(ang) * dist, 3 * (1 - u) + 0.5, 0, Math.PI * 2);
          g.fill();
        }
        g.restore();
      }
    }

    function drawGame() {
      if (!game) return;
      game.draw(g, 1 - fading);
      if (fading > 0) return;
      // Milo als Ball: gedreht und beim Aufprall gestaucht.
      const b = game.ball;
      g.save();
      g.translate(b.x, b.y);
      g.rotate(b.rot);
      g.scale(1 + b.squash * 0.2, 1 - b.squash * 0.2);
      milo.draw(g, 0, SIZE / 2, SIZE);
      g.restore();
    }


    /** Ein Objekt liegt: Platz speichern (Milo nicht, er laeuft ohnehin weiter). */
    function settle(b: Body) {
      if (b.kind === "milo" || b.saved) return;
      b.saved = true;
      invoke("desk_drop_home", { kind: b.kind, x: b.x / width, home: false }).catch(() => {});
    }

    // ---------- Zeichnen ----------

    function draw() {
      const ratio = dpr();
      if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
        canvas.width = Math.round(width * ratio);
        canvas.height = Math.round(height * ratio);
      }
      g.setTransform(ratio, 0, 0, ratio, 0, 0);
      g.clearRect(0, 0, width, height);
      const hour = new Date().getHours();
      for (const b of bodies) {
        const fade = b.leaving > 0 ? Math.max(0, 1 - b.leaving / 0.5) : 1;
        g.save();
        g.globalAlpha = fade;
        // Weicher Schatten auf der Taskleiste, kleiner, je hoeher das Objekt ist.
        const lift = Math.max(0, ground() - b.y);
        const shadow = Math.max(0, 1 - lift / 300);
        if (shadow > 0.02) {
          g.fillStyle = `rgba(0,0,0,${0.28 * shadow})`;
          g.beginPath();
          g.ellipse(b.x, ground() - 1, boxOf(b).w * 0.42 * (0.5 + shadow * 0.5), 3, 0, 0, Math.PI * 2);
          g.fill();
        }
        const h = boxOf(b).h;
        // Getragen dreht es sich um den Griffpunkt oben, sonst um die Unterkante.
        const pivotY = b.carried ? b.y - h * 0.82 : b.y;
        g.translate(b.x, pivotY);
        g.rotate(b.rot);
        g.translate(-b.x, -pivotY);
        if (b.leaving > 0) {
          const s = Math.max(0.05, 1 - b.leaving * 1.6);
          g.translate(b.x, b.y);
          g.scale(s, s);
          g.translate(-b.x, -b.y);
        }
        if (b.kind === "milo") milo.draw(g, b.x, b.y, SIZE);
        else drawDecor(g, b.kind as DecorKind, b.x, b.y, SIZE, { clock, hour, mail: false, growth: 4, perk: 0, wobble: b.wobble });
        g.restore();
      }
      drawGame();
      drawPuffs();
    }

    // ---------- Schleife: nur so oft wie noetig ----------

    function busy() {
      return !!game || puffs.length > 0 || bodies.some((b) => b.carried || !b.resting || b.leaving > 0 || b.wobble > 0) || bodies.some((b) => b.kind === "milo");
    }

    function loop(now: number) {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      step(dt);
      draw();
      reportHits();
      if (busy()) {
        const k = bodies.find((b) => b.kind === "milo");
        // Fallen und Tragen fluessig, Milo beim Spazieren mit 30 Bildern, sonst ruhig.
        const fast = !!game || puffs.length > 0 || bodies.some((b) => b.carried || !b.resting || b.leaving > 0 || b.wobble > 0);
        frame = fast ? requestAnimationFrame(loop) : window.setTimeout(() => (frame = requestAnimationFrame(loop)), k && (goal !== null || milo.lively) ? 30 : 90) as unknown as number;
      } else {
        frame = 0;
        // Ruhende Deko (Laterne, Lavalampe) nur selten neu zeichnen.
        clearTimeout(idleTimer);
        idleTimer = window.setTimeout(() => wake(), 700);
      }
    }

    function wake() {
      clearTimeout(idleTimer);
      if (frame) return;
      last = performance.now();
      frame = requestAnimationFrame(loop);
    }

    let lastHits = "";
    function reportHits() {
      const ratio = dpr();
      const rects = bodies
        .filter((b) => !b.carried && b.leaving === 0)
        .map((b) => {
          const { w, h } = boxOf(b);
          return [Math.round((b.x - w / 2) * ratio), Math.round((b.y - h) * ratio), Math.round(w * ratio), Math.round(h * ratio)];
        });
      // Das offene Menue muss klickbar sein.
      if (menuEl && (menu || miloMenu)) {
        const r = menuEl.getBoundingClientRect();
        rects.push([Math.round(r.left * ratio), Math.round(r.top * ratio), Math.round(r.width * ratio), Math.round(r.height * ratio)]);
      }
      const key = JSON.stringify(rects);
      if (key !== lastHits) {
        lastHits = key;
        invoke("desk_hits", { rects }).catch(() => {});
      }
    }

    // ---------- Greifen auf dem Desktop ----------

    function bodyAt(x: number, y: number): Body | undefined {
      return [...bodies].reverse().find((b) => {
        const { w, h } = boxOf(b);
        return !b.carried && b.leaving === 0 && x >= b.x - w / 2 && x <= b.x + w / 2 && y >= b.y - h && y <= b.y + 4;
      });
    }

    /** Letzte Zeigerpositionen: beim Loslassen zaehlt die Bewegung der letzten ~100 ms, nicht ein einzelner Ruck. */
    const trail: { x: number; y: number; t: number }[] = [];
    const onGameMove = (e: PointerEvent) => {
      const now = performance.now();
      trail.push({ x: e.clientX, y: e.clientY, t: now });
      while (trail.length > 2 && now - trail[0].t > 110) trail.shift();
      const dt = Math.max(1, now - gpT) / 1000;
      gpT = now;
      gp.vx = gp.vx * 0.5 + ((e.clientX - gp.x) / dt) * 0.5;
      gp.vy = gp.vy * 0.5 + ((e.clientY - gp.y) / dt) * 0.5;
      gp.x = e.clientX;
      gp.y = e.clientY;
    };
    window.addEventListener("pointermove", onGameMove);
    const onGameUp = () => {
      if (!game?.pointerUp) return;
      const now = performance.now();
      const recent = trail.filter((p) => now - p.t < 110);
      if (recent.length >= 2) {
        const first = recent[0], end = recent[recent.length - 1];
        const span = Math.max(0.03, (end.t - first.t) / 1000);
        gp.vx = (end.x - first.x) / span;
        gp.vy = (end.y - first.y) / span;
      } else {
        gp.vx = gp.vy = 0;
      }
      game.pointerUp(gp);
    };
    window.addEventListener("pointerup", onGameUp);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && quit();
    window.addEventListener("keydown", onKey);

    const onDown = (e: PointerEvent) => {
      if (game) {
        if (e.button === 0 && !(e.target as HTMLElement).closest(".hud, .result")) {
          gp.x = e.clientX;
          gp.y = e.clientY;
          game.pointerDown?.(gp);
        }
        return;
      }
      if (e.button !== 0 || (e.target as HTMLElement).closest(".menu")) return;
      menu = null;
      miloMenu = null;
      const b = bodyAt(e.clientX, e.clientY);
      if (!b) return;
      const start = { x: e.clientX, y: e.clientY };
      const move = (m: PointerEvent) => {
        if (Math.hypot(m.clientX - start.x, m.clientY - start.y) < 5) return;
        // Gesperrt: nur ein kurzes Wackeln, kein Tragen.
        if (b.locked) {
          cleanup();
          b.wobble = 1;
          wake();
          return;
        }
        cleanup();
        pointer = { x: m.clientX, y: m.clientY, vx: 0, vy: 0, t: performance.now() };
        b.carried = true;
        b.resting = false;
        b.saved = false;
        if (b.kind === "milo") {
          adopt();
          goal = null;
          milo.play("startle");
        }
        invoke("desk_carry", { kind: b.kind, fromX: b.kind === "milo" ? null : b.x / width }).catch(() => {});
        wake();
      };
      const up = () => {
        cleanup();
        // Nur angetippt: wackeln bzw. kichern.
        b.wobble = 1;
        if (b.kind === "milo") milo.play("giggle");
        wake();
      };
      const cleanup = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    };
    window.addEventListener("pointerdown", onDown);

    // Rechtsklick: Menue mit Sperren, In die Insel, Entfernen.
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
      if (game) return;
      const b = bodyAt(e.clientX, e.clientY);
      if (b?.kind === "milo") {
        adopt();
        menu = null;
        miloMenu = { x: clamp(e.clientX - 160, 8, width - 328), y: Math.max(8, e.clientY - 250) };
        milo.play("perk");
        wake();
        return;
      }
      miloMenu = null;
      if (!b) {
        menu = null;
        return;
      }
      menuTarget = b;
      menu = { x: Math.min(e.clientX, width - 190), y: Math.max(8, e.clientY - 130), locked: b.locked, label: decorInfo(b.kind)?.label ?? b.kind };
      wake();
    };
    window.addEventListener("contextmenu", onContext);
    act = (action: "lock" | "unlock" | "home" | "remove") => {
      const b = menuTarget;
      menu = null;
      if (!b) return;
      invoke("desk_item", { kind: b.kind, x: b.x / width, action }).catch(() => {});
      if (action === "lock" || action === "unlock") b.locked = action === "lock";
      else b.leaving = 0.001;
      wake();
    };

    // ---------- Ereignisse vom Kern ----------

    const unlisten: (() => void)[] = [];
    const on = <T,>(name: string, handle: (payload: T) => void) => listen<T>(name, ({ payload }) => handle(payload)).then((fn) => unlisten.push(fn));

    on<{ kind: string }>("milo://desk-carry", ({ kind }) => {
      if (carried()) return;
      // Kommt aus der Insel oder der Bibliothek: am Zeiger erscheinen.
      spawn(kind, pointer.x, pointer.y + 30, true);
    });
    on("milo://desk-escape", () => quit());
    on<{ seconds: number; from: number }>("milo://desk-visit", ({ seconds, from }) => {
      invoke("pet_ready").catch(() => {});
      visit(seconds, from);
    });
    on("milo://desk-visit-end", () => endTrip());
    on<[number, number]>("milo://desk-hover", ([px, py]) => {
      const x = px / dpr(), y = py / dpr();
      if (Math.hypot(x - hover.x, y - hover.y) > 3) hover = { x, y, t: clock };
    });
    on<[number, number]>("milo://desk-pointer", ([px, py]) => {
      const now = performance.now();
      const x = px / dpr(), y = py / dpr();
      const dt = Math.max(1, now - pointer.t) / 1000;
      // Geschwindigkeit geglaettet, damit ein Wurf nicht von einem einzelnen Sprung abhaengt.
      pointer = { x, y, vx: pointer.vx * 0.6 + ((x - pointer.x) / dt) * 0.4, vy: pointer.vy * 0.6 + ((y - pointer.y) / dt) * 0.4, t: now };
      wake();
    });
    on<{ kind: string; x: number; y: number; overIsland: boolean }>("milo://desk-drop", ({ x, y, overIsland }) => {
      const b = carried();
      if (!b) return;
      b.carried = false;
      // Ohne frische Zeigerdaten (nur angetippt) zaehlt die echte Position beim Loslassen, ohne Schwung.
      if (performance.now() - pointer.t > 150) {
        b.x = x / dpr();
        b.y = y / dpr() + boxOf(b).h * 0.82;
        pointer.vx = pointer.vy = 0;
      }
      if (overIsland) {
        b.leaving = 0.001;
        invoke("desk_drop_home", { kind: b.kind, x: 0, home: true }).catch(() => {});
      } else {
        b.vx = Math.max(-1600, Math.min(1600, pointer.vx));
        b.vy = Math.max(-1600, Math.min(1200, pointer.vy));
        b.vrot = b.vx * 0.004;
        b.resting = false;
      }
      wake();
    });
    on("milo://desk-recall", () => {
      const k = bodies.find((b) => b.kind === "milo");
      if (k) {
        milo.play("vanish");
        k.leaving = 0.001;
      }
      invoke("desk_drop_home", { kind: "milo", x: 0, home: true }).catch(() => {});
      wake();
    });
    // Abgleich mit der gespeicherten Liste: was fehlt, geht heim; was neu ist (aus dem Mini-Menue
    // der Insel), faellt von oben herein.
    let ready = false;
    on<{ deskItems: { kind: string; x: number; locked?: boolean }[]; accent?: string }>("milo://settings", ({ deskItems, accent: color }) => {
      if (color) accent = color;
      if (!ready) return;
      const left = [...deskItems];
      for (const b of bodies) {
        if (b.kind === "milo" || !b.saved || b.carried || b.leaving > 0) continue;
        const i = left.findIndex((d) => d.kind === b.kind && Math.abs(d.x * width - b.x) < 40);
        if (i >= 0) left.splice(i, 1);
        else b.leaving = 0.001;
      }
      // Gerade getragene oder geschubste Objekte stehen kurz nicht in der Liste: nicht doppelt anlegen.
      const pending = bodies.filter((b) => b.kind !== "milo" && !b.saved);
      for (const item of left) {
        const i = pending.findIndex((b) => b.kind === item.kind);
        if (i >= 0) {
          pending.splice(i, 1);
          continue;
        }
        const b = spawn(item.kind, item.x * width, 90, false, !!item.locked);
        b.resting = false;
        b.saved = true;
        b.vrot = (Math.random() - 0.5) * 3;
      }
      wake();
    });

    invoke<{ items: { kind: string; x: number; locked?: boolean }[]; carrying: string | null; milo: boolean; trip: [number, number] | null }>("desk_ready").then((start) => {
      for (const item of start.items) spawn(item.kind, item.x * width, ground(), false, !!item.locked);
      ready = true;
      if (start.carrying) spawn(start.carrying, pointer.x, pointer.y, true);
      else if (start.trip) visit(start.trip[0], start.trip[1]);
      else if (start.milo && !bodies.some((b) => b.kind === "milo")) spawn("milo", width / 2, ground(), false);
      wake();
    });

    const onResize = () => {
      width = window.innerWidth;
      height = window.innerHeight;
      for (const b of bodies) if (b.resting) b.y = ground();
      wake();
    };
    window.addEventListener("resize", onResize);
    wake();

    return () => {
      unlisten.forEach((fn) => fn());
      window.removeEventListener("resize", onResize);
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("contextmenu", onContext);
      window.removeEventListener("pointermove", onGameMove);
      window.removeEventListener("pointerup", onGameUp);
      window.removeEventListener("keydown", onKey);
      if (game) invoke("desk_game", { on: false }).catch(() => {});
      cancelAnimationFrame(frame);
      clearTimeout(frame);
      clearTimeout(idleTimer);
    };
  });
</script>

<canvas bind:this={canvas}></canvas>
{#if miloMenu}
  <div class="menu games" bind:this={menuEl} style:left="{miloMenu.x}px" style:top="{miloMenu.y}px" role="menu">
    <div class="menu-head">
      <strong>{t("Mit Milo spielen", "Play with Milo")}</strong>
      <button class="close" onclick={() => (miloMenu = null)} aria-label={t("Menü schließen", "Close menu")} title={t("Schließen", "Close")}>
        <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
      </button>
    </div>
    <div class="tiles">
      {#each GAMES as item (item.kind)}
        <button role="menuitem" class="tile" onclick={() => play(item.kind)} title={item.hint}>
          <svg viewBox="0 0 48 48" width="40" height="40" aria-hidden="true">
            {#if item.kind === "pong"}
              <rect x="6" y="12" width="5" height="24" rx="2.5" fill="#ff6b6b" />
              <rect x="37" y="14" width="5" height="24" rx="2.5" fill="#64d2ff" />
              <rect x="20" y="20" width="9" height="9" rx="3" fill="#f5f5f7" />
              <path d="M24 6v4M24 38v4" stroke="rgba(255,255,255,0.3)" stroke-width="2" stroke-linecap="round" />
            {:else if item.kind === "basket"}
              <rect x="34" y="8" width="4" height="22" rx="1.5" fill="rgba(255,255,255,0.55)" />
              <ellipse cx="25" cy="20" rx="9" ry="2.5" fill="none" stroke="#ff9f0a" stroke-width="2.4" />
              <path d="M17 21l3 9M33 21l-3 9M21 21l1 9M29 21l-1 9" stroke="rgba(255,255,255,0.6)" stroke-width="1.2" />
              <rect x="8" y="30" width="10" height="10" rx="3.5" fill="#f5f5f7" />
            {:else}
              <ellipse cx="17" cy="18" rx="8" ry="10" fill="none" stroke="#64d2ff" stroke-width="2.4" />
              <path d="M21 27l7 9" stroke="rgba(255,255,255,0.6)" stroke-width="3" stroke-linecap="round" />
              <path d="M2 42h44M24 42V30" stroke="rgba(255,255,255,0.5)" stroke-width="1.6" />
              <rect x="33" y="12" width="9" height="9" rx="3" fill="#f5f5f7" />
            {/if}
          </svg>
          <span>{item.label}</span>
        </button>
      {/each}
    </div>
    <div class="levels" role="radiogroup" aria-label={t("Schwierigkeit", "Difficulty")}>
      {#each LEVELS as item (item.id)}
        <button role="radio" aria-checked={level === item.id} class:on={level === item.id} onclick={() => setLevel(item.id)}>{item.label}</button>
      {/each}
    </div>
    <p class="tip">{t("Milo ist der Ball.", "Milo is the ball.")} <kbd>Esc</kbd> {t("beendet ein Spiel.", "ends a game.")}</p>
    <button role="menuitem" onclick={() => sendHome()}>{t("Zurück in die Insel", "Back to the island")}</button>
  </div>
{/if}
{#if hud}
  <div class="hud">
    <div class="board">
      <span class="side"><small>{hud.aLabel}</small><b>{hud.a}</b></span>
      <span class="mid"><b>{hud.label}</b><small>{hud.note}</small></span>
      <span class="side"><small>{hud.bLabel}</small><b>{hud.b}</b></span>
      <button class="x" onclick={() => quit()} aria-label={t("Spiel beenden", "End game")} title={t("Spiel beenden", "End game")}>
        <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
      </button>
    </div>
    <p class="esc">{#if lang === "en"}Press <kbd>Esc</kbd> to quit{:else}<kbd>Esc</kbd> drücken zum Beenden{/if}</p>
  </div>
  {#if hud.count}
    {#key hud.count}
      <div class="count">{hud.count}</div>
    {/key}
  {/if}
  {#if hud.result}
    <div class="result" class:won={hud.result.won}>
      <h2>{hud.result.title}</h2>
      <p>{hud.result.detail}</p>
      <div class="row">
        <button class="primary" onclick={() => hud && play(hud.kind)}>{t("Nochmal", "Play again")}</button>
        <button onclick={() => quit()}>{t("Beenden", "Quit")}</button>
      </div>
    </div>
  {/if}
{/if}
{#if menu}
  <div class="menu" bind:this={menuEl} style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
    <strong>{menu.label}</strong>
    <button role="menuitem" onclick={() => act(menu?.locked ? "unlock" : "lock")}>{menu.locked ? t("Entsperren", "Unlock") : t("Sperren", "Lock")}</button>
    <button role="menuitem" onclick={() => act("home")}>{t("In die Insel", "Into the island")}</button>
    <button role="menuitem" class="danger" onclick={() => act("remove")}>{t("Entfernen", "Remove")}</button>
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  .menu {
    position: fixed;
    z-index: 2;
    width: 180px;
    display: flex;
    flex-direction: column;
    padding: 6px;
    border-radius: var(--r-lg);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.14), rgba(255, 255, 255, 0.05)), rgba(28, 28, 34, 0.92);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.3), 0 12px 30px rgba(0, 0, 0, 0.5);
    font-family: "Inter Variable", "Segoe UI", system-ui, sans-serif;
    color: var(--text);
  }
  .menu strong {
    padding: 4px 10px 6px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .menu button {
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-md);
    background: none;
    color: inherit;
    font: inherit;
    font-size: var(--fs-body);
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover {
    background: rgba(255, 255, 255, 0.12);
  }
  .menu .danger {
    color: #ff8a80;
  }
  .games {
    width: 320px;
    gap: 2px;
    animation: menu-in var(--t-base) cubic-bezier(0.34, 1.4, 0.64, 1);
  }
  @keyframes menu-in {
    from { opacity: 0; transform: translateY(8px) scale(0.94); }
    to { opacity: 1; transform: none; }
  }
  .menu-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 2px;
  }
  .menu .close {
    width: 24px;
    height: 24px;
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-2);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
    padding: 2px 2px 4px;
  }
  .menu .tile {
    height: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 10px 4px 8px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.06);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.12);
    text-align: center;
    font-weight: var(--fw-semibold);
    transition: transform var(--t-base) cubic-bezier(0.34, 1.4, 0.64, 1), background var(--t-fast) ease;
  }
  .menu .tile:hover {
    background: rgba(255, 255, 255, 0.14);
    transform: translateY(-2px);
  }
  .menu .tile:active {
    transform: scale(0.95);
  }
  .levels {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px;
    margin: 2px 2px 4px;
    padding: 4px;
    border-radius: var(--r-md);
    background: rgba(0, 0, 0, 0.35);
  }
  .menu .levels button {
    height: 28px;
    text-align: center;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--text-2);
    border-radius: var(--r-sm);
  }
  .menu .levels button.on {
    background: rgba(255, 255, 255, 0.16);
    color: var(--text);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.2);
  }
  .tip {
    margin: 2px 10px 6px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  kbd {
    padding: 1px 6px;
    border-radius: var(--r-xs);
    background: rgba(255, 255, 255, 0.14);
    box-shadow: inset 0 -1px 0 rgba(0, 0, 0, 0.4);
    font: var(--fw-semibold) var(--fs-caption) "Inter Variable", "Segoe UI", system-ui, sans-serif;
    color: var(--text);
  }
  .menu button:focus-visible,
  .hud button:focus-visible,
  .result button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .hud {
    position: fixed;
    top: 84px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 3;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    font-family: "Inter Variable", "Segoe UI", system-ui, sans-serif;
    color: var(--text);
    cursor: default;
    animation: hud-in var(--t-slow) cubic-bezier(0.34, 1.4, 0.64, 1);
  }
  @keyframes hud-in {
    from { opacity: 0; transform: translate(-50%, -10px); }
    to { opacity: 1; transform: translate(-50%, 0); }
  }
  .board {
    display: flex;
    align-items: center;
    gap: 22px;
    padding: 8px 10px 8px 24px;
    border-radius: var(--r-full);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.16), rgba(255, 255, 255, 0.05)), rgba(22, 22, 28, 0.82);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35), inset 0 0 0 1px rgba(255, 255, 255, 0.08), 0 14px 34px rgba(0, 0, 0, 0.45);
  }
  .side,
  .mid {
    display: flex;
    flex-direction: column;
    align-items: center;
    line-height: 1.1;
  }
  .side b {
    font-size: var(--fs-large);
    font-variant-numeric: tabular-nums;
  }
  .mid b {
    font-size: var(--fs-callout);
  }
  small {
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .x {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text);
    cursor: pointer;
  }
  .x:hover {
    background: rgba(255, 255, 255, 0.2);
  }
  .esc {
    margin: 0;
    padding: 4px 12px;
    border-radius: var(--r-full);
    background: rgba(0, 0, 0, 0.45);
    font-family: "Inter Variable", "Segoe UI", system-ui, sans-serif;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .count {
    position: fixed;
    top: 42%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 3;
    font: 800 120px "Inter Variable", "Segoe UI", system-ui, sans-serif;
    color: var(--text);
    text-shadow: 0 8px 40px rgba(0, 0, 0, 0.6);
    pointer-events: none;
    animation: count-pop var(--t-spring) cubic-bezier(0.34, 1.6, 0.64, 1);
  }
  @keyframes count-pop {
    from { opacity: 0; transform: translate(-50%, -50%) scale(1.8); }
    to { opacity: 1; transform: translate(-50%, -50%) scale(1); }
  }
  .result {
    position: fixed;
    top: 38%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 3;
    width: 340px;
    padding: 22px 22px 18px;
    border-radius: var(--r-xl);
    text-align: center;
    font-family: "Inter Variable", "Segoe UI", system-ui, sans-serif;
    color: var(--text);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.16), rgba(255, 255, 255, 0.05)), rgba(22, 22, 28, 0.9);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35), 0 24px 60px rgba(0, 0, 0, 0.55);
    cursor: default;
  }
  .result h2 {
    margin: 0 0 6px;
    font-size: var(--fs-title);
  }
  .result.won h2 {
    color: var(--warn);
  }
  .result p {
    margin: 0 0 16px;
    font-size: var(--fs-body);
    color: var(--text-2);
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .row button {
    flex: 1;
    height: 38px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.1);
    color: inherit;
    font: var(--fw-semibold) var(--fs-body) "Inter Variable", "Segoe UI", system-ui, sans-serif;
    cursor: pointer;
  }
  .row button:hover {
    background: rgba(255, 255, 255, 0.18);
  }
  .row .primary {
    background: var(--accent);
    color: #000;
  }
  .row .primary:hover {
    background: #8adfff;
  }
  /* Bei Pong und Tennis ist der Schlaeger der Zeiger. */
  :global(body[data-game="pong"]) canvas,
  :global(body[data-game="tennis"]) canvas {
    cursor: none;
  }
  canvas {
    display: block;
    width: 100vw;
    height: 100vh;
    cursor: grab;
  }
</style>
