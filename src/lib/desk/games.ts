import { t } from "../i18n";
// Minispiele auf dem Desktop (Rechtsklick auf Milo, wenn er auf der Taskleiste steht).
// Milo ist in allen drei Spielen der Ball. Jedes Spiel rechnet seine eigene Physik und zeichnet
// sein Feld; Milo selbst zeichnet die Desktop-Seite an der Ballposition (mit Drehung und Stauchung).

export type GameKind = "pong" | "basket" | "tennis";

export const GAMES: { kind: GameKind; label: string; hint: string }[] = [
  { kind: "pong", label: t("Tischtennis", "Table tennis"), hint: t("Dein Schläger rechts fährt mit der Maus hoch und runter. Triffst du mit der Kante, fliegt der Ball steil und schneller – damit schlägst du den Bot. Wer zuerst 7 hat, gewinnt.", "Your paddle on the right moves up and down with the mouse. Hit with the edge for a steep, faster ball – that's how you beat the bot. First to 7 wins.") },
  { kind: "basket", label: "Basketball", hint: t("Wie eine Schleuder: Milo packen und nach hinten ziehen. Pfeil und Linie zeigen genau, wohin er fliegt – wird die Linie grün, geht er in den Korb. Loslassen wirft. Je schwerer, desto kürzer die Vorschau. 60 Sekunden.", "Like a slingshot: grab Milo and pull him back. The arrow and line show exactly where he'll fly – when the line turns green, he's going in. Let go to throw. The harder the level, the shorter the preview. 60 seconds.") },
  { kind: "tennis", label: "Tennis", hint: t("Schläger folgt der Maus. Jeder Treffer geht übers Netz – je kräftiger du Richtung Netz schwingst, desto länger fliegt der Ball. Ganz schnell geschwungen gibt es einen Schmetterball. Zweimal aufkommen verliert den Punkt.", "The racket follows the mouse. Every hit clears the net – the harder you swing towards the net, the deeper the ball goes. Swing really fast for a smash. Two bounces lose the point.") },
];

/** Schwierigkeit der Spiele: wie gut der Bot ist bzw. wie gross der Korb. */
export type Level = "easy" | "normal" | "hard";

export const LEVELS: { id: Level; label: string }[] = [
  { id: "easy", label: t("Leicht", "Easy") },
  { id: "normal", label: t("Mittel", "Normal") },
  { id: "hard", label: t("Schwer", "Hard") },
];

export interface Pointer {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

export interface Arena {
  width: number;
  height: number;
  /** Oberkante der Taskleiste */
  ground: number;
  accent: string;
}

export interface Ball {
  x: number;
  y: number;
  vx: number;
  vy: number;
  rot: number;
  vrot: number;
  /** 0 = rund, >0 = gerade gestaucht (klingt von selbst ab) */
  squash: number;
}

/** Was im Spiel passiert ist, damit Milo passend reagiert. */
export type GameEvent = "hit" | "bounce" | "score" | "lose" | "rim" | "win" | "over";

export interface Hud {
  aLabel: string;
  a: string;
  bLabel: string;
  b: string;
  note: string;
}

export interface Result {
  won: boolean;
  title: string;
  detail: string;
  /** Neuer Bestwert fuer dieses Spiel (Siege bzw. Punkte) */
  best: number;
}

export interface Game {
  kind: GameKind;
  ball: Ball;
  result: Result | null;
  hud(): Hud;
  update(dt: number, p: Pointer): GameEvent[];
  draw(g: CanvasRenderingContext2D, alpha: number): void;
  /** Wo beim Beenden die Puff-Wolken aufgehen. */
  pieces(): { x: number; y: number; r: number }[];
  pointerDown?(p: Pointer): void;
  pointerUp?(p: Pointer): void;
}

export const BALL_R = 21;
const R = BALL_R;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const rand = (lo: number, hi: number) => lo + Math.random() * (hi - lo);

export function createGame(kind: GameKind, arena: Arena, best: number, level: Level = "normal"): Game {
  if (kind === "pong") return new Pong(arena, best, level);
  if (kind === "basket") return new Basket(arena, best, level);
  return new Tennis(arena, best, level);
}

function newBall(x: number, y: number): Ball {
  return { x, y, vx: 0, vy: 0, rot: 0, vrot: 0, squash: 0 };
}

/** Ball prallt an einem festen Punkt ab (Ringkante, Netzkante). Liefert true bei Kontakt. */
function bouncePoint(b: Ball, px: number, py: number, e: number): boolean {
  const dx = b.x - px, dy = b.y - py;
  const d = Math.hypot(dx, dy);
  if (d >= R || d === 0) return false;
  const nx = dx / d, ny = dy / d;
  b.x = px + nx * R;
  b.y = py + ny * R;
  const dot = b.vx * nx + b.vy * ny;
  if (dot < 0) {
    b.vx -= (1 + e) * dot * nx;
    b.vy -= (1 + e) * dot * ny;
  }
  return true;
}

function roundRect(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  g.beginPath();
  g.roundRect(x, y, w, h, r);
}

/** Glasflaeche wie in der Insel: dunkel, durchscheinend, feine helle Kante. */
function glassPanel(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  roundRect(g, x, y, w, h, r);
  g.fillStyle = "rgba(12, 12, 16, 0.88)";
  g.fill();
  const edge = g.createLinearGradient(0, y, 0, y + h);
  edge.addColorStop(0, "rgba(255,255,255,0.28)");
  edge.addColorStop(1, "rgba(255,255,255,0.06)");
  g.strokeStyle = edge;
  g.lineWidth = 1.5;
  g.stroke();
}

/** Schlaeger-Stab mit Glanz (Pong). */
function paddle(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, color: string) {
  const grad = g.createLinearGradient(x - w / 2, 0, x + w / 2, 0);
  grad.addColorStop(0, color);
  grad.addColorStop(0.5, "#ffffff");
  grad.addColorStop(1, color);
  g.save();
  g.shadowColor = color;
  g.shadowBlur = 18;
  roundRect(g, x - w / 2, y - h / 2, w, h, w / 2);
  g.fillStyle = grad;
  g.fill();
  g.restore();
}

// ---------------------------------------------------------------- Pong

class Pong implements Game {
  kind = "pong" as const;
  ball: Ball;
  result: Result | null = null;
  private court: { x: number; y: number; w: number; h: number };
  /** Dein Schlaeger: wie im echten Pong nur hoch und runter. */
  private you = { y: 0, vy: 0 };
  private bot = { y: 0, err: 0 };
  /** Kurz eingeblendet nach einem Kantentreffer. */
  private edgeFlash = 0;
  private lv: { speed: number; miss: number; err: number; react: number; serve: number; cap: number; edge: number };
  private score = { you: 0, bot: 0 };
  private serve = 0.9;
  private serveDir = -1;
  private speed = 580;
  private flash = 0;
  private readonly PW = 16;
  private readonly PH = 150;
  private readonly TARGET = 7;

  constructor(private arena: Arena, private best: number, level: Level) {
    const w = Math.min(1500, arena.width - 160);
    const h = Math.min(820, arena.ground - 120);
    this.court = { x: (arena.width - w) / 2, y: arena.ground - h - 40, w, h };
    this.you.y = this.bot.y = this.court.y + h / 2;
    // speed: wie schnell der Bot-Schlaeger faehrt; miss: Anteil verfehlter Baelle bei geraden Schlaegen;
    // err: Zielstreuung (Anteil der Schlaegerhoehe); react: ab welchem Anteil des Felds (von links)
    // der Bot den Ball verfolgt; serve/cap: Anfangs- und Hoechsttempo; edge: wie stark ein
    // Kantentreffer den Bot zusaetzlich aus dem Tritt bringt (das ist dein Weg zum Sieg).
    this.lv = {
      easy: { speed: 320, miss: 0.26, err: 0.45, react: 0.4, serve: 460, cap: 950, edge: 0.45 },
      normal: { speed: 430, miss: 0.15, err: 0.34, react: 0.6, serve: 520, cap: 1100, edge: 0.4 },
      hard: { speed: 580, miss: 0.07, err: 0.24, react: 0.8, serve: 600, cap: 1350, edge: 0.32 },
    }[level];
    this.speed = this.lv.serve;
    this.ball = newBall(this.court.x + w / 2, this.court.y + h / 2);
  }

  private get youX() {
    return this.court.x + this.court.w - 36;
  }
  private get botX() {
    return this.court.x + 36;
  }

  hud(): Hud {
    return { aLabel: "Bot", a: String(this.score.bot), bLabel: t("Du", "You"), b: String(this.score.you), note: t(`Bis ${this.TARGET}`, `First to ${this.TARGET}`) };
  }

  /** Wo kommt der Ball an der Bot-Linie an? Mit Spiegelung an Decke und Boden des Felds. */
  private predict(): number {
    const b = this.ball;
    if (b.vx >= 0) return this.court.y + this.court.h / 2;
    const time = (b.x - this.botX) / -b.vx;
    const top = this.court.y + R, span = this.court.h - 2 * R;
    let y = b.y + b.vy * time - top;
    y = ((y % (2 * span)) + 2 * span) % (2 * span);
    if (y > span) y = 2 * span - y;
    return top + y;
  }

  update(dt: number, p: Pointer): GameEvent[] {
    const ev: GameEvent[] = [];
    const c = this.court, b = this.ball;
    this.flash = Math.max(0, this.flash - dt * 2);
    this.edgeFlash = Math.max(0, this.edgeFlash - dt * 1.5);
    // Dein Schlaeger folgt der Maus nur senkrecht.
    const py = this.you.y;
    this.you.y = clamp(p.y, c.y + this.PH / 2, c.y + c.h - this.PH / 2);
    this.you.vy = this.you.vy * 0.4 + ((this.you.y - py) / Math.max(dt, 1e-3)) * 0.6;
    // Der Bot reagiert erst, wenn der Ball ein Stueck auf ihn zugeflogen ist; vorher treibt er
    // langsam zur Mitte. Fuehrst du, wird er etwas schneller, fuehrt er, etwas langsamer.
    const botSpeed = clamp(this.lv.speed + (this.score.you - this.score.bot) * 25, this.lv.speed * 0.8, this.lv.speed * 1.25);
    const tracking = b.vx < 0 && b.x < c.x + c.w * this.lv.react;
    const target = tracking ? this.predict() + this.bot.err : c.y + c.h / 2;
    const dy = target - this.bot.y;
    const pace = tracking ? botSpeed : botSpeed * 0.4;
    this.bot.y += clamp(dy, -pace * dt, pace * dt);
    this.bot.y = clamp(this.bot.y, c.y + this.PH / 2, c.y + c.h - this.PH / 2);

    if (this.result) return ev;
    if (this.serve > 0) {
      this.serve -= dt;
      b.x = c.x + c.w / 2;
      b.y = c.y + c.h / 2;
      b.vx = b.vy = 0;
      if (this.serve <= 0) {
        const angle = rand(-0.45, 0.45);
        this.speed = this.lv.serve;
        b.vx = Math.cos(angle) * this.speed * this.serveDir;
        b.vy = Math.sin(angle) * this.speed;
      }
      return ev;
    }
    b.x += b.vx * dt;
    b.y += b.vy * dt;
    b.rot += b.vrot * dt;
    b.vrot *= Math.pow(0.5, dt);
    if (b.y < c.y + R) {
      b.y = c.y + R;
      b.vy = Math.abs(b.vy);
      b.squash = 0.5;
      ev.push("bounce");
    }
    if (b.y > c.y + c.h - R) {
      b.y = c.y + c.h - R;
      b.vy = -Math.abs(b.vy);
      b.squash = 0.5;
      ev.push("bounce");
    }
    // Schlaeger (klassisches Pong): der Auftreffpunkt bestimmt den Winkel. Ein Treffer mit der Kante
    // fliegt steil und etwas schneller; die Auf- oder Abbewegung des Schlaegers gibt etwas Drall.
    // Liefert den Auftreffpunkt (-1 obere Kante … 1 untere Kante) oder null.
    const hitPaddle = (px: number, py: number, dir: number, pvy: number): number | null => {
      const half = this.PW / 2;
      const inReach = Math.abs(b.y - py) < this.PH / 2 + R * 0.7;
      const overlap = b.x + R >= px - half && b.x - R <= px + half;
      const approaching = dir > 0 ? b.vx > 0 && b.x < px : b.vx < 0 && b.x > px;
      if (!inReach || !overlap || !approaching) return null;
      b.x = px - dir * (half + R);
      const offset = clamp((b.y - py) / (this.PH / 2), -1, 1);
      this.speed = Math.min(this.lv.cap, this.speed * 1.045);
      const edge = Math.abs(offset);
      const out = Math.min(this.lv.cap * 1.2, this.speed * (1 + 0.22 * edge * edge));
      const angle = offset * 0.9;
      b.vx = -dir * Math.cos(angle) * out;
      b.vy = Math.sin(angle) * out + clamp(pvy, -1500, 1500) * 0.12;
      b.vrot = (pvy * 0.01 + offset * 6) * -dir;
      b.squash = 0.8;
      return offset;
    };
    const yours = hitPaddle(this.youX, this.you.y, 1, this.you.vy);
    if (yours !== null) {
      ev.push("hit");
      // Der Bot zielt nicht perfekt. Gerade Baelle haelt er meist; Kantentreffer (steil, schnell,
      // oft ueber die Bande) und hohes Tempo bringen ihn ins Schwimmen.
      const edge = Math.abs(yours);
      const fast = clamp((this.speed - this.lv.serve) / (this.lv.cap - this.lv.serve), 0, 1);
      const miss = this.lv.miss + (edge > 0.55 ? (this.lv.edge * (edge - 0.55)) / 0.45 : 0) + fast * 0.08;
      const reach = this.PH / 2 + R;
      this.bot.err = Math.random() < miss
        ? (Math.random() < 0.5 ? -1 : 1) * (reach + rand(15, 70))
        : rand(-1, 1) * this.PH * this.lv.err * (1 + edge * 0.5);
      if (edge > 0.7) this.edgeFlash = 1;
    }
    if (hitPaddle(this.botX, this.bot.y, -1, 0) !== null) ev.push("hit");
    if (b.x < c.x - R * 2 || b.x > c.x + c.w + R * 2) {
      const youScored = b.x < c.x;
      if (youScored) this.score.you++;
      else this.score.bot++;
      ev.push(youScored ? "score" : "lose");
      this.flash = 1;
      this.serve = 1.0;
      this.serveDir = youScored ? -1 : 1;
      this.bot.err = 0;
      if (this.score.you >= this.TARGET || this.score.bot >= this.TARGET) {
        const won = this.score.you > this.score.bot;
        if (won) this.best += 1;
        this.result = {
          won,
          title: won ? t("Gewonnen!", "You win!") : t("Knapp daneben", "So close"),
          detail: t(`${this.score.you} : ${this.score.bot} gegen den Bot. Siege insgesamt: ${this.best}`, `${this.score.you} : ${this.score.bot} against the bot. Total wins: ${this.best}`),
          best: this.best,
        };
        ev.push(won ? "win" : "over");
      }
    }
    return ev;
  }

  draw(g: CanvasRenderingContext2D, alpha: number) {
    const c = this.court;
    g.save();
    g.globalAlpha = alpha;
    glassPanel(g, c.x, c.y, c.w, c.h, 28);
    // Mittellinie und grosse, leise Spielstaende in den Haelften.
    g.save();
    g.setLineDash([10, 14]);
    g.strokeStyle = "rgba(255,255,255,0.16)";
    g.lineWidth = 3;
    g.beginPath();
    g.moveTo(c.x + c.w / 2, c.y + 20);
    g.lineTo(c.x + c.w / 2, c.y + c.h - 20);
    g.stroke();
    g.restore();
    g.font = "700 120px 'Inter Variable', 'Segoe UI', sans-serif";
    g.textAlign = "center";
    g.textBaseline = "middle";
    g.fillStyle = `rgba(255,255,255,${0.07 + this.flash * 0.15})`;
    g.fillText(String(this.score.bot), c.x + c.w * 0.25, c.y + c.h / 2);
    g.fillText(String(this.score.you), c.x + c.w * 0.75, c.y + c.h / 2);
    if (this.edgeFlash > 0) {
      g.font = "800 30px 'Inter Variable', 'Segoe UI', sans-serif";
      g.fillStyle = `rgba(255, 214, 10, ${this.edgeFlash})`;
      g.fillText(t("Kantentreffer!", "Edge hit!"), c.x + c.w / 2, c.y + 70 - (1 - this.edgeFlash) * 24);
    }
    paddle(g, this.botX, this.bot.y, this.PW, this.PH, "#ff6b6b");
    paddle(g, this.youX, this.you.y, this.PW, this.PH, this.arena.accent);
    g.restore();
  }

  pieces() {
    const c = this.court;
    return [
      { x: this.botX, y: this.bot.y, r: 70 },
      { x: this.youX, y: this.you.y, r: 70 },
      { x: c.x + c.w / 2, y: c.y + c.h / 2, r: 120 },
      { x: c.x + 60, y: c.y + 60, r: 80 },
      { x: c.x + c.w - 60, y: c.y + 60, r: 80 },
      { x: c.x + 60, y: c.y + c.h - 60, r: 80 },
      { x: c.x + c.w - 60, y: c.y + c.h - 60, r: 80 },
    ];
  }
}

// ---------------------------------------------------------------- Basketball

class Basket implements Game {
  kind = "basket" as const;
  ball: Ball;
  result: Result | null = null;
  private G = 1900;
  private hoop = { x: 0, y: 0, r: 76 };
  /** Schleuder: wo Milo gepackt wurde (dort ist das Gummiband befestigt). Ziehen spannt, Loslassen
   *  wirft in Gegenrichtung. Die Vorschau rechnet genau den Wurf, der beim Loslassen passiert –
   *  ohne unsichtbare Zielhilfe: was man sieht, bekommt man. */
  private anchor: { x: number; y: number } | null = null;
  /** Wie weit die Flugbahn vorausgezeigt wird (Sekunden) und ob sie gruen wird, wenn sie trifft. */
  private preview: { time: number; green: boolean };
  private range: { x: [number, number]; y: [number, number] };
  private moveTo: { x: number; y: number } | null = null;
  private time = 60;
  private points = 0;
  private streak = 0;
  private held = false;
  private thrown = false;
  private touched = false;
  private scored = false;
  private swish = 0;
  private popup: { text: string; t: number; x: number; y: number } | null = null;
  private started = false;
  /** Laengster Zug am Gummiband (px) und Wurftempo je Pixel Zug. */
  private readonly PULL = 240;
  private readonly POWER = 12.5;
  /** Kuerzerer Zug zaehlt nicht als Wurf (Milo wird nur abgesetzt). */
  private readonly DEAD = 24;
  /** Feste Schrittweite der Physik (unabhaengig von der Bildrate) und der noch offene Rest. */
  private readonly STEP = 1 / 120;
  private acc = 0;

  constructor(private arena: Arena, private best: number, level: Level) {
    // Leicht: grosser Ring, nah und tief; schwer: kleinerer Ring, weiter weg und hoeher.
    this.hoop.r = { easy: 92, normal: 76, hard: 60 }[level];
    this.preview = { easy: { time: 2, green: true }, normal: { time: 0.75, green: true }, hard: { time: 0.32, green: false } }[level];
    this.range = ({
      easy: { x: [0.48, 0.7], y: [0.45, 0.66] },
      normal: { x: [0.52, 0.8], y: [0.36, 0.64] },
      hard: { x: [0.55, 0.88], y: [0.26, 0.62] },
    } as Record<Level, { x: [number, number]; y: [number, number] }>)[level];
    this.ball = newBall(arena.width * 0.28, arena.ground - R);
    this.placeHoop();
    this.hoop.x = this.moveTo!.x;
    this.hoop.y = this.moveTo!.y;
    this.moveTo = null;
  }

  private placeHoop() {
    const a = this.arena;
    this.moveTo = { x: rand(...this.range.x) * a.width, y: rand(...this.range.y) * a.ground };
  }

  /** Brett rechts vom Ring, oben ueberstehend. */
  private get board() {
    return { x: this.hoop.x + this.hoop.r + 10, top: this.hoop.y - this.hoop.r * 2.3, bottom: this.hoop.y + this.hoop.r * 0.4 };
  }

  hud(): Hud {
    return {
      aLabel: t("Zeit", "Time"),
      a: String(Math.ceil(this.time)),
      bLabel: t("Punkte", "Points"),
      b: String(this.points),
      note: this.streak >= 2 ? t(`Serie ×${this.streak}`, `Streak ×${this.streak}`) : t(`Bestwert ${this.best}`, `Best ${this.best}`),
    };
  }

  pointerDown(p: Pointer) {
    if (this.result) return;
    if (Math.hypot(p.x - this.ball.x, p.y - this.ball.y) < R * 2.4) {
      this.held = true;
      this.started = true;
      // Liegt Milo (fast) am Boden, sitzt das Gummiband ein Stueck darueber: so kann man nach
      // unten-hinten ziehen, ohne dass der Boden im Weg ist. Sonst dort, wo er gepackt wurde.
      const floor = this.arena.ground - R;
      const y = this.ball.y > floor - 190 ? floor - 190 : this.ball.y;
      this.anchor = { x: this.ball.x, y: clamp(y, R, floor) };
    }
  }

  /** Milo haengt am Gummiband: hoechstens PULL vom Ankerpunkt entfernt. */
  private pulled(p: Pointer): { x: number; y: number } {
    const a = this.anchor!;
    const dx = p.x - a.x, dy = p.y - a.y;
    const d = Math.hypot(dx, dy);
    const k = d > this.PULL ? this.PULL / d : 1;
    return { x: clamp(a.x + dx * k, R, this.arena.width - R), y: clamp(a.y + dy * k, R, this.arena.ground - R) };
  }

  /** Wurf aus dem Zug: Gegenrichtung, Staerke proportional zur Zuglaenge. null = zu wenig gezogen. */
  private launch(): { vx: number; vy: number } | null {
    const a = this.anchor, b = this.ball;
    if (!a) return null;
    const dx = a.x - b.x, dy = a.y - b.y;
    if (Math.hypot(dx, dy) < this.DEAD) return null;
    return { vx: dx * this.POWER, vy: dy * this.POWER };
  }

  /** Ein fester Physikschritt: Flug, Ring, Brett, Waende, Decke. Spiel und Vorschau rechnen damit
   *  identisch (gleiche Schrittweite), sonst stimmte die gruene Linie bei Ringberuehrungen nicht. */
  private physics(b: Ball): { rim: boolean; board: boolean; through: boolean } {
    const h = this.hoop, a = this.arena, dt = this.STEP;
    const prevY = b.y;
    b.vy += this.G * dt;
    b.x += b.vx * dt;
    b.y += b.vy * dt;
    const e1 = bouncePoint(b, h.x - h.r, h.y, 0.55);
    const e2 = bouncePoint(b, h.x + h.r, h.y, 0.55);
    const bd = this.board;
    let board = false;
    if (b.y > bd.top - R * 0.5 && b.y < bd.bottom + R * 0.5 && Math.abs(b.x - bd.x) < R + 4) {
      const side = b.x < bd.x ? -1 : 1;
      b.x = bd.x + side * (R + 4);
      b.vx = side * Math.abs(b.vx) * 0.6;
      board = true;
    }
    if (b.x < R) {
      b.x = R;
      b.vx = Math.abs(b.vx) * 0.6;
    }
    if (b.x > a.width - R) {
      b.x = a.width - R;
      b.vx = -Math.abs(b.vx) * 0.6;
    }
    if (b.y < R) {
      b.y = R;
      b.vy = Math.abs(b.vy) * 0.5;
    }
    // Treffer: Mitte faellt von oben durch den Ring.
    const through = prevY < h.y && b.y >= h.y && b.vy > 0 && Math.abs(b.x - h.x) < h.r - R * 0.35;
    return { rim: e1 || e2, board, through };
  }

  /** Geht dieser Wurf in den Korb? Rechnet den Flug mit derselben Physik wie das Spiel, damit eine
   *  gruene Linie wirklich einen Treffer bedeutet (auch ueber Ring oder Brett). */
  private goesIn(x0: number, y0: number, v: { vx: number; vy: number }): boolean {
    const sim = newBall(x0, y0);
    sim.vx = v.vx;
    sim.vy = v.vy;
    for (let i = 0; i < 4 / this.STEP; i++) {
      if (this.physics(sim).through) return true;
      if (sim.y >= this.arena.ground - R) return false;
    }
    return false;
  }

  pointerUp(p: Pointer) {
    if (!this.held) return;
    this.held = false;
    const b = this.ball;
    if (this.anchor) {
      const at = this.pulled(p);
      b.x = at.x;
      b.y = at.y;
    }
    const v = this.launch();
    this.anchor = null;
    if (!v) {
      // Kaum gezogen: Milo wird nur abgesetzt und faellt.
      b.vx = b.vy = 0;
      return;
    }
    b.vx = v.vx;
    b.vy = v.vy;
    b.vrot = b.vx * 0.006;
    this.thrown = true;
    this.touched = false;
    this.scored = false;
  }

  update(dt: number, p: Pointer): GameEvent[] {
    const ev: GameEvent[] = [];
    const a = this.arena, b = this.ball, h = this.hoop;
    this.swish = Math.max(0, this.swish - dt * 1.8);
    if (this.popup) {
      this.popup.t += dt;
      if (this.popup.t > 1.2) this.popup = null;
    }
    if (this.moveTo) {
      // Der Korb gleitet an seinen neuen Platz.
      const k = 1 - Math.pow(0.02, dt);
      h.x += (this.moveTo.x - h.x) * k;
      h.y += (this.moveTo.y - h.y) * k;
      if (Math.hypot(this.moveTo.x - h.x, this.moveTo.y - h.y) < 1) this.moveTo = null;
    }
    if (this.started && !this.result) {
      this.time = Math.max(0, this.time - dt);
      if (this.time === 0) {
        const record = this.points > this.best;
        this.best = Math.max(this.best, this.points);
        this.result = {
          won: record,
          title: record ? t("Neuer Bestwert!", "New best!") : t("Zeit um", "Time's up"),
          detail: t(`${this.points} Punkte. Bestwert: ${this.best}`, `${this.points} points. Best: ${this.best}`),
          best: this.best,
        };
        ev.push(record ? "win" : "over");
      }
    }
    if (this.held && this.anchor) {
      const at = this.pulled(p);
      b.x = at.x;
      b.y = at.y;
      b.vx = 0;
      b.vy = 0;
      // Milo neigt sich leicht in Wurfrichtung; je staerker gespannt, desto mehr gestaucht.
      const pull = Math.hypot(this.anchor.x - at.x, this.anchor.y - at.y) / this.PULL;
      const tilt = (this.anchor.x - at.x) / this.PULL * 0.5;
      b.rot += (clamp(tilt, -0.6, 0.6) - b.rot) * Math.min(1, dt * 8);
      b.squash = Math.max(b.squash, pull * 0.35);
      this.acc = 0;
      return ev;
    }
    this.acc = Math.min(this.acc + dt, 0.1);
    while (this.acc >= this.STEP) {
      this.acc -= this.STEP;
      this.step(ev);
    }
    return ev;
  }

  /** Ein Physikschritt im Spiel samt Ereignissen (Ring, Brett, Treffer, Boden). */
  private step(ev: GameEvent[]) {
    const a = this.arena, b = this.ball, h = this.hoop, dt = this.STEP;
    b.rot += b.vrot * dt;
    const hit = this.physics(b);
    if (hit.rim) {
      if (!this.touched) ev.push("rim");
      this.touched = true;
      b.squash = 0.4;
    }
    if (hit.board) {
      this.touched = true;
      b.squash = 0.5;
      ev.push("bounce");
    }
    if (this.thrown && !this.scored && hit.through) {
      this.scored = true;
      this.streak++;
      const clean = !this.touched;
      const gain = 2 + (clean ? 1 : 0) + (this.streak >= 3 ? 1 : 0);
      this.points += gain;
      this.swish = 1;
      this.popup = { text: clean ? `+${gain} Swish!` : `+${gain}`, t: 0, x: h.x, y: h.y - 70 };
      b.vx *= 0.4;
      ev.push("score");
      window.setTimeout(() => this.placeHoop(), 900);
    }
    // Boden
    if (b.y > a.ground - R) {
      b.y = a.ground - R;
      if (b.vy > 140) {
        b.vy = -b.vy * 0.55;
        b.squash = Math.min(1, b.vy / -900);
        ev.push("bounce");
      } else b.vy = 0;
      b.vx *= Math.pow(0.25, dt);
      b.vrot = b.vx / R;
      // Liegt der Ball nach einem Wurf ohne Treffer, ist die Serie vorbei.
      if (this.thrown && !this.scored && Math.abs(b.vy) < 1 && Math.abs(b.vx) < 30) {
        this.thrown = false;
        if (this.streak > 0) ev.push("lose");
        this.streak = 0;
      }
    }
  }

  draw(g: CanvasRenderingContext2D, alpha: number) {
    const h = this.hoop, bd = this.board;
    g.save();
    g.globalAlpha = alpha;
    // Brett: Glasscheibe mit Zielquadrat
    glassPanel(g, bd.x - 4, bd.top, 10, bd.bottom - bd.top, 4);
    g.strokeStyle = "rgba(255,255,255,0.45)";
    g.lineWidth = 2;
    g.strokeRect(bd.x - 2, h.y - h.r * 0.9, 6, h.r * 0.7);
    // Halterung
    g.fillStyle = "rgba(255,255,255,0.35)";
    g.fillRect(h.x + h.r, h.y - 2, 12, 4);
    // Netz: schwingt nach einem Treffer
    const sway = Math.sin(this.swish * 18) * 6 * this.swish;
    g.strokeStyle = "rgba(255,255,255,0.75)";
    g.lineWidth = 1.4;
    const depth = h.r * 1.1 + this.swish * 14;
    const top = (i: number) => h.x - h.r + (2 * h.r * i) / 6;
    const bottom = (i: number) => h.x - h.r * 0.55 + (1.1 * h.r * i) / 6 + sway;
    for (let i = 0; i <= 6; i++) {
      g.beginPath();
      g.moveTo(top(i), h.y);
      g.lineTo(bottom(i), h.y + depth);
      g.stroke();
    }
    for (let row = 1; row <= 3; row++) {
      const u = row / 3;
      g.beginPath();
      for (let i = 0; i <= 6; i++) {
        const x = top(i) + (bottom(i) - top(i)) * u;
        const y = h.y + depth * u;
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      }
      g.stroke();
    }
    // Ring
    g.save();
    g.shadowColor = "#ff9f0a";
    g.shadowBlur = 14;
    g.strokeStyle = "#ff9f0a";
    g.lineWidth = 6;
    g.lineCap = "round";
    g.beginPath();
    g.ellipse(h.x, h.y, h.r, Math.max(7, h.r * 0.13), 0, 0, Math.PI * 2);
    g.stroke();
    g.restore();
    // Schleuder: Gummiband zum Ankerpunkt, Pfeil in Wurfrichtung, ruhige Flugbahn genau fuer diesen Wurf.
    if (this.held && this.anchor) {
      const b = this.ball, an = this.anchor;
      const v = this.launch();
      g.save();
      g.lineCap = "round";
      // Ankerpunkt
      g.fillStyle = "rgba(255,255,255,0.55)";
      g.beginPath();
      g.arc(an.x, an.y, 6, 0, Math.PI * 2);
      g.fill();
      // Gummiband: je staerker gespannt, desto duenner und roeter.
      const tension = Math.min(1, Math.hypot(an.x - b.x, an.y - b.y) / this.PULL);
      g.strokeStyle = `rgba(255, ${Math.round(214 - tension * 120)}, ${Math.round(120 - tension * 80)}, 0.85)`;
      g.lineWidth = 5 - tension * 2.5;
      g.beginPath();
      g.moveTo(an.x, an.y);
      g.lineTo(b.x, b.y);
      g.stroke();
      if (v) {
        const green = this.preview.green && this.goesIn(b.x, b.y, v);
        const color = green ? "48, 209, 88" : "255, 255, 255";
        // Pfeil vom Anker in Wurfrichtung, Laenge = Staerke
        const sp = Math.hypot(v.vx, v.vy);
        const ux = v.vx / sp, uy = v.vy / sp;
        const len = 30 + tension * 90;
        const tipX = an.x + ux * len, tipY = an.y + uy * len;
        g.strokeStyle = `rgba(${color}, 0.9)`;
        g.lineWidth = 4;
        g.beginPath();
        g.moveTo(an.x, an.y);
        g.lineTo(tipX, tipY);
        g.moveTo(tipX, tipY);
        g.lineTo(tipX - ux * 14 - uy * 9, tipY - uy * 14 + ux * 9);
        g.moveTo(tipX, tipY);
        g.lineTo(tipX - ux * 14 + uy * 9, tipY - uy * 14 - ux * 9);
        g.stroke();
        // Flugbahn: dieselbe Physik wie der echte Wurf (auch Abpraller an Ring und Brett), haengt
        // nur am Zug, nicht an der Mausgeschwindigkeit.
        const steps = Math.max(4, Math.round(this.preview.time / 0.045));
        const sim = newBall(b.x, b.y);
        sim.vx = v.vx;
        sim.vy = v.vy;
        const per = Math.round(0.045 / this.STEP);
        for (let i = 1; i <= steps; i++) {
          for (let k = 0; k < per; k++) this.physics(sim);
          const x = sim.x, y = sim.y;
          if (y > this.arena.ground - R) break;
          const fadeOut = 1 - i / (steps + 1);
          g.fillStyle = `rgba(${color}, ${0.85 * fadeOut})`;
          g.beginPath();
          g.arc(x, y, green ? 5 : 4.5 - (i / steps) * 2, 0, Math.PI * 2);
          g.fill();
        }
        if (green) {
          g.font = "700 15px 'Inter Variable', 'Segoe UI', sans-serif";
          g.textAlign = "center";
          g.fillStyle = "rgba(48, 209, 88, 0.95)";
          g.fillText(t("Passt!", "On target!"), this.hoop.x, this.hoop.y - this.hoop.r * 0.5 - 18);
        }
      }
      g.restore();
    }
    if (!this.started) {
      g.font = "600 15px 'Inter Variable', 'Segoe UI', sans-serif";
      g.textAlign = "center";
      g.fillStyle = "rgba(255,255,255,0.85)";
      g.fillText(t("Milo packen, nach hinten ziehen, loslassen", "Grab Milo, pull back, let go"), this.ball.x, this.ball.y - 52);
    }
    if (this.popup) {
      const u = this.popup.t / 1.2;
      g.font = "800 30px 'Inter Variable', 'Segoe UI', sans-serif";
      g.textAlign = "center";
      g.fillStyle = `rgba(255, 214, 10, ${1 - u})`;
      g.fillText(this.popup.text, this.popup.x, this.popup.y - u * 50);
    }
    g.restore();
  }

  pieces() {
    const h = this.hoop, bd = this.board;
    return [
      { x: h.x, y: h.y + 20, r: 80 },
      { x: bd.x, y: (bd.top + bd.bottom) / 2, r: 70 },
    ];
  }
}

// ---------------------------------------------------------------- Tennis

interface TennisLevel {
  /** Laufgeschwindigkeit des Bots (px/s) und Reaktionszeit (s). */
  speed: number;
  delay: number;
  /** Normale Ungenauigkeit beim Treffpunkt (px; erst ab ca. 55 px verfehlt er). */
  noise: number;
  /** Grundfehlerquote, Zuschlag fuer lange/kraeftige Baelle und je Ballwechsel. */
  miss: number;
  deep: number;
  tire: number;
  /** Wohin er zurueckspielt (Anteil deiner Seite). */
  place: [number, number];
  /** Anteil schneller, flacher Schlaege (schwerer zu erreichen) und deren Tempo. */
  drive: number;
  drivePace: [number, number];
}

const TENNIS_LEVELS: Record<Level, TennisLevel> = {
  easy: { speed: 430, delay: 0.38, noise: 30, miss: 0.1, deep: 0.12, tire: 0.006, place: [0.35, 0.65], drive: 0, drivePace: [0, 0] },
  normal: { speed: 560, delay: 0.25, noise: 22, miss: 0.05, deep: 0.08, tire: 0.004, place: [0.25, 0.85], drive: 0.15, drivePace: [1100, 1400] },
  hard: { speed: 760, delay: 0.14, noise: 14, miss: 0.05, deep: 0.06, tire: 0.004, place: [0.15, 0.95], drive: 0.2, drivePace: [1250, 1550] },
};

interface Racket {
  x: number;
  y: number;
  vx: number;
  vy: number;
  cool: number;
}

class Tennis implements Game {
  kind = "tennis" as const;
  ball: Ball;
  result: Result | null = null;
  private G = 1300;
  private lv: TennisLevel;
  /** Ballwechsel im aktuellen Punkt (lange Wechsel werden fuer den Bot schwerer). */
  private rally = 0;
  /** Seit wann der Ball auf dem Weg zum Bot ist (seine Reaktionszeit laeuft). */
  private incoming = 0;
  private left: number;
  private right: number;
  private netX: number;
  private readonly netH = 120;
  private readonly RH = 46;
  private readonly TARGET = 5;
  private you: Racket;
  private bot: Racket;
  private score = { you: 0, bot: 0 };
  private server: "you" | "bot" = "you";
  private lastHitter: "you" | "bot" | null = null;
  private crossed = false;
  private bounceSide: "you" | "bot" | null = null;
  private bounces = 0;
  private pause = 0.6;
  private message = "";
  private botTarget: { x: number; y: number } | null = null;
  private botErr = 0;
  /** Kurz eingeblendet nach einem Schmetterball. */
  private smash = 0;
  /** Ab dieser Schwunggeschwindigkeit Richtung Netz (Pixel/s) wird der Schlag ein Schmetterball. */
  private readonly SMASH_SWING = 1300;

  constructor(private arena: Arena, private best: number, level: Level) {
    // Abgestimmt mit einem menschlichen Spielermodell (Reaktionszeit, begrenztes Maustempo,
    // Schaetzfehler), Ziel-Siegquote etwa Leicht 75 %, Mittel 50 %, Schwer 20–25 %.
    this.lv = TENNIS_LEVELS[level];
    // Breiter Platz: fast die ganze Bildschirmbreite, damit fuer Anlauf und Schmetterball Raum ist.
    const half = Math.max(400, arena.width / 2 - 50);
    this.netX = arena.width / 2;
    this.left = this.netX - half;
    this.right = this.netX + half;
    this.you = { x: this.netX + half * 0.55, y: arena.ground - 160, vx: 0, vy: 0, cool: 0 };
    this.bot = { x: this.netX - half * 0.55, y: arena.ground - 160, vx: 0, vy: 0, cool: 0 };
    this.ball = newBall(0, 0);
    this.setupServe();
  }

  private side(x: number): "you" | "bot" {
    return x > this.netX ? "you" : "bot";
  }

  private setupServe() {
    const b = this.ball;
    const half = this.right - this.netX;
    b.x = this.server === "you" ? this.netX + half * 0.6 : this.netX - half * 0.6;
    b.y = this.arena.ground - 420;
    b.vx = b.vy = 0;
    b.vrot = 0;
    this.lastHitter = null;
    this.crossed = false;
    this.bounceSide = null;
    this.bounces = 0;
    this.botTarget = null;
    this.rally = 0;
  }

  hud(): Hud {
    return { aLabel: "Bot", a: String(this.score.bot), bLabel: t("Du", "You"), b: String(this.score.you), note: this.message || t(`Bis ${this.TARGET}`, `First to ${this.TARGET}`) };
  }

  private point(winner: "you" | "bot", why: string, ev: GameEvent[]) {
    this.score[winner]++;
    this.message = winner === "you" ? t(`Punkt für dich – ${why}`, `Your point – ${why}`) : t(`Punkt für den Bot – ${why}`, `Bot's point – ${why}`);
    ev.push(winner === "you" ? "score" : "lose");
    this.server = this.server === "you" ? "bot" : "you";
    this.pause = 1.3;
    if (this.score.you >= this.TARGET || this.score.bot >= this.TARGET) {
      const won = this.score.you > this.score.bot;
      if (won) this.best += 1;
      this.result = {
        won,
        title: won ? t("Spiel, Satz, Sieg!", "Game, set, match!") : t("Der Bot gewinnt", "The bot wins"),
        detail: t(`${this.score.you} : ${this.score.bot}. Siege insgesamt: ${this.best}`, `${this.score.you} : ${this.score.bot}. Total wins: ${this.best}`),
        best: this.best,
      };
      ev.push(won ? "win" : "over");
    }
  }

  /** Abschlag, der sicher uebers Netz geht und bei tx landet. */
  private aim(x0: number, y0: number, tx: number) {
    const ground = this.arena.ground - R;
    const netTop = this.arena.ground - this.netH;
    let best = { vx: (tx - x0) / 1.2, vy: -1300 };
    for (let vy = -800; vy >= -1600; vy -= 100) {
      const time = (-vy + Math.sqrt(vy * vy + 2 * this.G * Math.max(0, ground - y0))) / this.G;
      const vx = (tx - x0) / time;
      const tn = (this.netX - x0) / vx;
      const yn = y0 + vy * tn + 0.5 * this.G * tn * tn;
      best = { vx, vy };
      if (tn < 0 || yn < netTop - R - 18) break;
    }
    return best;
  }

  /** Schneller, flacher Schlag, der knapp uebers Netz geht und bei tx landet (wie ein Schmetterball). */
  private drive(x0: number, y0: number, tx: number, speed: number) {
    const ground = this.arena.ground - R;
    const netTop = this.arena.ground - this.netH;
    const dir = Math.sign(tx - x0) || 1;
    const vx = dir * speed * 0.92;
    const time = Math.max(0.05, (tx - x0) / vx);
    let vy = (ground - y0 - 0.5 * this.G * time * time) / time;
    const tn = (this.netX - x0) / vx;
    if (tn > 0) vy = Math.min(vy, (netTop - R - 14 - y0 - 0.5 * this.G * tn * tn) / tn);
    return { vx, vy };
  }

  /** Wo kann der Bot den Ball treffen? Kurze Vorausrechnung der Flugbahn. */
  private predict(): { x: number; y: number } | null {
    let x = this.ball.x, y = this.ball.y, vx = this.ball.vx, vy = this.ball.vy;
    const ground = this.arena.ground - R;
    for (let i = 0; i < 120; i++) {
      const dt = 1 / 60;
      vy += this.G * dt;
      x += vx * dt;
      y += vy * dt;
      if (y > ground) {
        y = ground;
        vy = -vy * 0.78;
      }
      if (x < this.left + R) {
        x = this.left + R;
        vx = Math.abs(vx) * 0.6;
      }
      if (x < this.netX - 40 && y > this.arena.ground - 320 && y < this.arena.ground - 40 && vy > -200) return { x, y };
    }
    return null;
  }

  private hitRacket(r: Racket, who: "you" | "bot", ev: GameEvent[]) {
    const b = this.ball;
    if (r.cool > 0) return;
    const dx = b.x - r.x, dy = b.y - r.y;
    const d = Math.hypot(dx, dy);
    if (d > this.RH + R || d === 0) return;
    const nx = dx / d, ny = dy / d;
    b.x = r.x + nx * (this.RH + R);
    b.y = r.y + ny * (this.RH + R);
    if (who === "bot") {
      // Der Bot spielt gezielt, mit etwas Streuung.
      const half = this.right - this.netX;
      const tx = this.netX + rand(...this.lv.place) * half;
      const shot = Math.random() < this.lv.drive && b.y < this.arena.ground - 200
        ? this.drive(b.x, b.y, clamp(tx, this.netX + 120, this.right - 30), rand(...this.lv.drivePace))
        : this.aim(b.x, b.y, clamp(tx, this.netX + 60, this.right - 30));
      b.vx = shot.vx;
      b.vy = shot.vy;
    } else {
      const rvx = b.vx - r.vx, rvy = b.vy - r.vy;
      const dot = rvx * nx + rvy * ny;
      // Der Schlaeger gibt nur einen Teil seiner Geschwindigkeit weiter: die Maus ist viel schneller
      // als ein echter Arm, sonst fliegt Milo schon bei leichtem Schwung quer ueber den Bildschirm.
      const svx = r.vx * 0.4, svy = r.vy * 0.4;
      const qvx = b.vx - svx, qvy = b.vy - svy;
      const qdot = qvx * nx + qvy * ny;
      if (dot < 0 || qdot < 0) {
        b.vx = svx + qvx - 1.6 * Math.min(0, qdot) * nx;
        b.vy = svy + qvy - 1.6 * Math.min(0, qdot) * ny;
      }
      const swing = -r.vx;
      const smash = swing > this.SMASH_SWING;
      /** 0..1: wie kraeftig/lang der Schlag ist (Schmetterball = 1). */
      let power = 1;
      if (smash) {
        // Schmetterball: schnell und flach in die Haelfte des Bots, aber immer uebers Netz.
        const sp = clamp(1500 + (swing - this.SMASH_SWING) * 0.35, 1500, 2300);
        const vx = -sp * 0.92;
        const ground = this.arena.ground - R;
        const netTop = this.arena.ground - this.netH;
        const tx = this.netX - (this.netX - this.left) * rand(0.35, 0.75);
        const time = Math.max(0.05, (b.x - tx) / -vx);
        let vy = (ground - b.y - 0.5 * this.G * time * time) / time;
        const tn = (b.x - this.netX) / -vx;
        if (tn > 0) vy = Math.min(vy, (netTop - R - 14 - b.y - 0.5 * this.G * tn * tn) / tn);
        b.vx = vx;
        b.vy = vy;
        this.smash = 1;
      } else {
        // Normaler Schlag: geht immer uebers Netz (frueher fielen schlappe Treffer oft ins Netz, das
        // war der Hauptgrund, warum man kaum Punkte machte). Der Schwung bestimmt, wie tief der Ball
        // in die Haelfte des Bots fliegt: kraeftig Richtung Netz = lang und schwer zu erreichen.
        power = clamp((-b.vx - 300) / 1500, 0, 1);
        const depth = clamp(0.3 + power * 0.6 + rand(-0.08, 0.08), 0.22, 0.92);
        const tx = this.netX - (this.netX - this.left) * depth;
        const shot = this.aim(b.x, b.y, clamp(tx, this.left + 30, this.netX - 60));
        b.vx = shot.vx;
        b.vy = shot.vy;
      }
      // Erreicht der Bot den naechsten Ball? Grundfehlerquote je Stufe, dazu: lange Baelle, lange
      // Ballwechsel und Schmetterbaelle machen es ihm schwerer; liegt er vorn, wird er nachlaessiger.
      const lead = Math.max(0, this.score.bot - this.score.you);
      const miss = this.lv.miss + power * this.lv.deep + Math.min(this.rally, 10) * this.lv.tire + lead * 0.04 + (smash ? 0.35 : 0);
      this.botErr = Math.random() < miss
        ? (Math.random() < 0.5 ? -1 : 1) * (this.RH + R + rand(10, 50))
        : rand(-1, 1) * this.lv.noise;
    }
    b.vrot = (b.vx > 0 ? 1 : -1) * 9;
    b.squash = 0.8;
    r.cool = 0.25;
    this.rally++;
    this.lastHitter = who;
    this.crossed = false;
    this.bounceSide = null;
    this.bounces = 0;
    this.botTarget = null;
    ev.push("hit");
  }

  update(dt: number, p: Pointer): GameEvent[] {
    const ev: GameEvent[] = [];
    const a = this.arena, b = this.ball;
    // Dein Schlaeger: folgt der Maus auf deiner Seite.
    const ox = this.you.x, oy = this.you.y;
    this.smash = Math.max(0, this.smash - dt * 1.4);
    this.you.x = clamp(p.x, this.netX + this.RH + 6, this.right + 40);
    this.you.y = clamp(p.y, a.ground - 620, a.ground - this.RH * 0.6);
    const ivx = (this.you.x - ox) / Math.max(dt, 1e-3), ivy = (this.you.y - oy) / Math.max(dt, 1e-3);
    this.you.vx = this.you.vx * 0.5 + ivx * 0.5;
    this.you.vy = this.you.vy * 0.5 + ivy * 0.5;
    this.you.cool = Math.max(0, this.you.cool - dt);
    this.bot.cool = Math.max(0, this.bot.cool - dt);
    // Bot: reagiert mit kurzer Verzoegerung, laeuft dann zum (etwas verschaetzten) Treffpunkt,
    // sonst zurueck in die Mitte seiner Seite.
    const coming = this.lastHitter !== "bot" && this.pause <= 0 && (b.vx < 0 || b.x < this.netX);
    this.incoming = coming ? this.incoming + dt : 0;
    if (!coming) this.botTarget = null;
    else if (this.incoming >= this.lv.delay && b.x < this.netX + 200) {
      // Treffpunkt nur sanft nachfuehren: die Vorausrechnung springt von Bild zu Bild ein Stueck,
      // frueher folgte der Schlaeger jedem Sprung und vibrierte. Faellt die Rechnung kurz aus,
      // bleibt das alte Ziel stehen (kein Wechsel zur Grundposition im Bildtakt).
      const hit = this.predict();
      if (hit) {
        const want = { x: hit.x + this.botErr, y: hit.y };
        if (!this.botTarget) this.botTarget = want;
        else {
          const k = Math.min(1, dt * 6);
          this.botTarget.x += (want.x - this.botTarget.x) * k;
          this.botTarget.y += (want.y - this.botTarget.y) * k;
        }
      }
    }
    const home = { x: this.netX - (this.netX - this.left) * 0.55, y: a.ground - 160 };
    const goal = this.botTarget ? { x: this.botTarget.x - 22, y: this.botTarget.y + 30 } : home;
    const botSpeed = clamp(this.lv.speed + (this.score.you - this.score.bot) * 30, this.lv.speed * 0.8, this.lv.speed * 1.25);
    const bx = this.bot.x, by = this.bot.y;
    const gx = goal.x - bx, gy = goal.y - by;
    const gd = Math.hypot(gx, gy);
    // Totzone: die letzten Pixel nicht hin und her korrigieren.
    if (gd > 4) {
      const stepLen = Math.min(gd - 3, botSpeed * dt);
      this.bot.x += (gx / gd) * stepLen;
      this.bot.y += (gy / gd) * stepLen;
    }
    this.bot.x = clamp(this.bot.x, this.left, this.netX - this.RH - 6);
    this.bot.y = clamp(this.bot.y, a.ground - 560, a.ground - this.RH * 0.6);
    // Geglaettet, weil die Neigung des Schlaegers daran haengt.
    this.bot.vx = this.bot.vx * 0.8 + ((this.bot.x - bx) / Math.max(dt, 1e-3)) * 0.2;
    this.bot.vy = this.bot.vy * 0.8 + ((this.bot.y - by) / Math.max(dt, 1e-3)) * 0.2;

    if (this.result) return ev;
    if (this.pause > 0) {
      this.pause -= dt;
      if (this.pause <= 0) {
        this.message = "";
        this.setupServe();
      } else return ev;
    }

    const wasSide = this.side(b.x);
    b.vy += this.G * dt;
    b.x += b.vx * dt;
    b.y += b.vy * dt;
    b.rot += b.vrot * dt;
    b.vrot *= Math.pow(0.6, dt);
    if (this.side(b.x) !== wasSide) this.crossed = true;

    this.hitRacket(this.you, "you", ev);
    this.hitRacket(this.bot, "bot", ev);

    // Netz: Pfosten und Kante
    const netTop = a.ground - this.netH;
    if (bouncePoint(b, this.netX, netTop, 0.3)) ev.push("bounce");
    if (b.y > netTop && Math.abs(b.x - this.netX) < R) {
      const s = b.x < this.netX ? -1 : 1;
      b.x = this.netX + s * R;
      b.vx = s * Math.abs(b.vx) * 0.25;
      ev.push("bounce");
    }
    // Seitliche Zaeune und Decke
    if (b.x < this.left - 60 + R) {
      b.x = this.left - 60 + R;
      b.vx = Math.abs(b.vx) * 0.5;
    }
    if (b.x > this.right + 60 - R) {
      b.x = this.right + 60 - R;
      b.vx = -Math.abs(b.vx) * 0.5;
    }
    if (b.y < R) {
      b.y = R;
      b.vy = Math.abs(b.vy) * 0.4;
    }
    // Boden: Aufkommen zaehlen
    if (b.y > a.ground - R) {
      b.y = a.ground - R;
      b.vy = -Math.abs(b.vy) * 0.78;
      b.vx *= 0.92;
      b.squash = 0.6;
      ev.push("bounce");
      const side = this.side(b.x);
      if (this.lastHitter && side === this.lastHitter && !this.crossed) {
        this.point(side === "you" ? "bot" : "you", t("nicht übers Netz", "didn't clear the net"), ev);
      } else {
        this.bounces = this.bounceSide === side ? this.bounces + 1 : 1;
        this.bounceSide = side;
        if (this.bounces >= 2) this.point(side === "you" ? "bot" : "you", t("zweimal aufgekommen", "bounced twice"), ev);
      }
    }
    return ev;
  }

  private drawRacket(g: CanvasRenderingContext2D, r: Racket, color: string, dir: number) {
    const tilt = clamp(r.vx * 0.0004, -0.5, 0.5) + dir * 0.35;
    g.save();
    g.translate(r.x, r.y);
    g.rotate(tilt);
    // Griff
    g.strokeStyle = "rgba(255,255,255,0.55)";
    g.lineWidth = 7;
    g.lineCap = "round";
    g.beginPath();
    g.moveTo(0, this.RH);
    g.lineTo(0, this.RH + 46);
    g.stroke();
    // Bespannung
    g.save();
    g.beginPath();
    g.ellipse(0, 0, this.RH * 0.82, this.RH, 0, 0, Math.PI * 2);
    g.fillStyle = "rgba(255,255,255,0.08)";
    g.fill();
    g.clip();
    g.strokeStyle = "rgba(255,255,255,0.3)";
    g.lineWidth = 1;
    for (let i = -this.RH; i <= this.RH; i += 9) {
      g.beginPath();
      g.moveTo(i, -this.RH);
      g.lineTo(i, this.RH);
      g.moveTo(-this.RH, i);
      g.lineTo(this.RH, i);
      g.stroke();
    }
    g.restore();
    // Rahmen
    g.shadowColor = color;
    g.shadowBlur = 16;
    g.strokeStyle = color;
    g.lineWidth = 5;
    g.beginPath();
    g.ellipse(0, 0, this.RH * 0.82, this.RH, 0, 0, Math.PI * 2);
    g.stroke();
    g.restore();
  }

  draw(g: CanvasRenderingContext2D, alpha: number) {
    const a = this.arena;
    g.save();
    g.globalAlpha = alpha;
    // Platz: Grundlinie auf der Taskleiste, Seitenlinien zart.
    g.strokeStyle = "rgba(255,255,255,0.7)";
    g.lineWidth = 3;
    g.beginPath();
    g.moveTo(this.left, a.ground - 1.5);
    g.lineTo(this.right, a.ground - 1.5);
    g.stroke();
    g.strokeStyle = "rgba(255,255,255,0.18)";
    g.lineWidth = 2;
    for (const x of [this.left, this.right]) {
      g.beginPath();
      g.moveTo(x, a.ground);
      g.lineTo(x, a.ground - 14);
      g.stroke();
    }
    // Netz: Maschen und weisses Band
    const top = a.ground - this.netH;
    g.strokeStyle = "rgba(255,255,255,0.28)";
    g.lineWidth = 1;
    for (let y = top + 10; y < a.ground; y += 10) {
      g.beginPath();
      g.moveTo(this.netX - 4, y);
      g.lineTo(this.netX + 4, y);
      g.stroke();
    }
    g.strokeStyle = "rgba(255,255,255,0.5)";
    g.lineWidth = 2;
    g.beginPath();
    g.moveTo(this.netX, top);
    g.lineTo(this.netX, a.ground);
    g.stroke();
    g.fillStyle = "#f5f5f7";
    roundRect(g, this.netX - 6, top - 4, 12, 8, 3);
    g.fill();
    this.drawRacket(g, this.bot, "#ff6b6b", 1);
    this.drawRacket(g, this.you, this.arena.accent, -1);
    if (this.smash > 0) {
      g.font = "800 34px 'Inter Variable', 'Segoe UI', sans-serif";
      g.textAlign = "center";
      g.fillStyle = `rgba(255, 214, 10, ${this.smash})`;
      g.fillText(t("Schmetterball!", "Smash!"), this.netX, a.ground - this.netH - 180 - (1 - this.smash) * 24);
    }
    // Landepunkt-Schatten des Balls hilft beim Einschaetzen.
    const lift = Math.max(0, a.ground - this.ball.y);
    g.fillStyle = `rgba(0,0,0,${0.35 * Math.max(0.15, 1 - lift / 600)})`;
    g.beginPath();
    g.ellipse(this.ball.x, a.ground - 2, R * (0.5 + 0.5 * Math.max(0, 1 - lift / 600)), 4, 0, 0, Math.PI * 2);
    g.fill();
    g.restore();
  }

  pieces() {
    const a = this.arena;
    return [
      { x: this.you.x, y: this.you.y, r: 75 },
      { x: this.bot.x, y: this.bot.y, r: 75 },
      { x: this.netX, y: a.ground - this.netH / 2, r: 80 },
      { x: this.left, y: a.ground - 20, r: 50 },
      { x: this.right, y: a.ground - 20, r: 50 },
    ];
  }
}
