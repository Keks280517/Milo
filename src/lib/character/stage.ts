// Die Buehne: der Boden der Insel, auf dem Milo lebt. Sie bestimmt, wo er steht und wohin er
// laeuft, laesst ihn im Leerlauf von selbst etwas unternehmen (schlendern, Ball spielen, gegen
// die Wand druecken, seine Pflanze giessen, Sterne gucken, Besuch von einem Freund bekommen,
// hinsetzen, eindoesen) und richtet seinen Blick auf den Mauszeiger.
// Wird er dabei unterbrochen, endet nichts abrupt: der Freund verabschiedet sich, Ball und Sterne
// blenden aus, die Wand federt zurueck. Wie er sich bewegt, steckt in engine.ts.
//
// Dazu kommt eine feste Kulisse: eine Pflanze, die ueber die Tage waechst,
// Sterne und Gluehwuermchen in der Nacht, Schmetterlinge am Tag, Wolken, Regen oder Schnee je nach
// Wetter am gespeicherten Ort, und in der offenen Insel eine Laterne und ein Briefkasten.

import { Character, type Mood, type OneShot } from "./engine";
import { decorInfo, drawDecor, type DecorKind } from "./decor";

export interface StageLayout {
  /** Sichtbarer Bereich der Insel in Zeichenkoordinaten. */
  left: number;
  right: number;
  /** Bodenlinie. */
  floor: number;
  /** Bereich, in dem sich Milos Mitte aufhalten darf. */
  roamMin: number;
  roamMax: number;
  /** Koerpergroesse in Pixeln. */
  size: number;
  /** Offene Insel: so viel Platz rechts belegen die Knoepfe; Claudes Wuerfel bleibt links davon. */
  buttons?: number;
}

type Step =
  | { kind: "walk"; to: number; brisk?: boolean }
  | { kind: "wait"; time: number }
  | { kind: "shot"; shot: OneShot }
  | { kind: "mood"; mood: Mood; time: number }
  | { kind: "ball" }
  | { kind: "push"; side: -1 | 1; time: number }
  | { kind: "plant" }
  | { kind: "stars"; time: number }
  | { kind: "friend"; time: number }
  | { kind: "chat"; time: number };

interface Ball {
  x: number; y: number; vx: number; vy: number;
  age: number;
  kicks: number;
  maxKicks: number;
  /** Sekunden bis zum naechsten moeglichen Stoss. */
  cooldown: number;
  /** Ein Stoss ist ausgeholt und trifft in so vielen Sekunden. */
  pending: number;
  /** Stauchung beim Aufprall, klingt ab. */
  squash: number;
  /** 1 = voll sichtbar; laeuft beim Verschwinden gegen 0. */
  alpha: number;
  leaving: boolean;
}

/** Der Freund: der zweite, etwas kleinere Wuerfel (derselbe, der spaeter als Cloud-Helfer mitarbeitet). */
interface Friend extends Partial<Follow> {
  c: Character;
  x: number;
  goal: number | null;
  /** Sekunden, die er schon da ist. */
  age: number;
  /** Verabschiedet sich gerade; nach so vielen Sekunden ist er weg. */
  leaving: number | null;
  /** Er ist als Cloud-Helfer da und arbeitet mit, statt nur zu plaudern. */
  helper?: boolean;
  /** Uebergibt gerade sein Ergebnis; nach so vielen Sekunden verabschiedet er sich. */
  parting?: number;
  /** Die Uebergabe-Geste wurde schon gespielt. */
  handed?: boolean;
}

/** Was der Cloud-Helfer gerade tut: kommt an, arbeitet, traegt vor, uebergibt. */
export type HelperPhase = "arrive" | "working" | "talking" | "done";

/** Was Claude gerade tut; bestimmt, wie sich sein Wuerfel verhaelt. */
export type BuddyState = "idle" | "working" | "question" | "confirm" | "done";

/**
 * Eigene Bewegung eines Begleiters (Claudes Wuerfel, Freund): Hoehe der Fuesse, laufender Sprung,
 * ob er gerade laeuft (mit Nachlauf, damit kleine Verschiebungen kein Zappeln ausloesen) und eine
 * kurze Wartezeit, bevor er Milo auf eine andere Flaeche nachspringt.
 */
interface Follow {
  y: number;
  hop: { fromX: number; fromY: number; toX: number; toY: number; t: number; dur: number; height: number } | null;
  walking: boolean;
  wait: number;
}

/** Claudes oranges Wuerfelchen: steht rechts in der Insel, solange Claude offen ist. */
interface Buddy extends Follow {
  c: Character;
  x: number;
  state: BuddyState;
  /** Sekunden im aktuellen Zustand. */
  since: number;
  /** Verschwindet gerade; nach so vielen Sekunden ist es weg. */
  leaving: number | null;
  /** Bis zur naechsten kleinen Geste, mit der es auf sich aufmerksam macht. */
  nudge: number;
}

/** Was um Milo herum los ist: Tageszeit, Wetter, wartende Post, Alter der Pflanze. */
export type WeatherKind = "clear" | "cloudy" | "rain" | "snow" | "storm" | "fog";
export interface Scenery {
  /** Stunde 0 bis 23. */
  hour: number;
  weather: WeatherKind | null;
  /** Im Briefkasten wartet etwas (Fahne oben). */
  mail: boolean;
  /** Wachstum der Pflanze: 0 = Keimling bis 4 = bluehend. */
  growth: number;
}

/** Regentropfen oder Schneeflocke. */
interface Precip {
  x: number;
  y: number;
  phase: number;
}

/** Schmetterling oder Gluehwuermchen. */
interface Critter {
  kind: "butterfly" | "firefly";
  x: number;
  y: number;
  vx: number;
  base: number;
  age: number;
  life: number;
  phase: number;
  hue: string;
}

/** Wassertropfen beim Giessen. */
interface Drop {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

/** Feste Zufallszahlen, damit die Kulisse bei jedem Bild gleich aussieht. */
function seeded(seed: number) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 4294967296;
  };
}
const SKY_RANDOM = seeded(42);
const SKY_STARS = Array.from({ length: 9 }, () => ({ u: SKY_RANDOM(), v: SKY_RANDOM(), phase: SKY_RANDOM() * 6.28, size: 0.6 + SKY_RANDOM() * 0.9 }));

const isNight = (hour: number) => hour >= 20 || hour < 6;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const rand = (lo: number, hi: number) => lo + Math.random() * (hi - lo);
const pick = <T>(items: T[]) => items[Math.floor(Math.random() * items.length)];

function pickWeighted<T>(items: [T, number][]): T {
  let roll = Math.random() * items.reduce((sum, [, w]) => sum + w, 0);
  for (const [item, weight] of items) {
    roll -= weight;
    if (roll <= 0) return item;
  }
  return items[0][0];
}

export class Stage {
  readonly milo = new Character();
  private raw: StageLayout = { left: 0, right: 400, floor: 56, roamMin: 30, roamMax: 370, size: 34 };
  set layout(value: StageLayout) {
    this.raw = value;
  }
  /** Aufteilung der Buehne; steht Claudes Wuerfel rechts, endet Milos Bereich vor ihm. */
  get layout(): StageLayout {
    const buddy = this.buddy;
    if (!buddy) return this.raw;
    return { ...this.raw, roamMax: Math.max(this.raw.roamMin, Math.min(this.raw.roamMax, buddy.x - this.raw.size * 1.5)) };
  }
  /** Stimmung, die die App vorgibt (Zuhoeren, Denken, Timer ...). */
  mood: Mood = "idle";
  /** Darf Milo von selbst herumlaufen und spielen? */
  roaming = true;
  /** Der Mauszeiger steht ueber der Insel: Milo bleibt stehen und schaut hin. */
  hovered = false;
  /** Mauszeiger in Zeichenkoordinaten; null = ausser Reichweite. */
  cursor: { x: number; y: number } | null = null;
  /** 0 = ruhig, 1 = normal, 2 = verspielt: wie oft Milo von selbst etwas unternimmt. */
  liveliness = 1;
  /** Milos Position (Koerpermitte). */
  x = 60;

  /** Milo drueckt gegen eine Inselwand: Seite und Staerke 0..1. */
  onPush: (side: -1 | 1, amount: number) => void = () => {};

  private vx = 0;
  private goal: number | null = null;
  private brisk = false;
  private plan: Step[] = [];
  private current: Step | null = null;
  private elapsed = 0;
  /** Pause bis zur naechsten eigenen Unternehmung. */
  private rest = 4;
  /** Sekunden, seit zuletzt etwas los war; je laenger, desto eher setzt sich Milo hin. */
  private quiet = 0;
  private tempMood: Mood | null = null;
  private ball: Ball | null = null;
  private friend: Friend | null = null;
  private buddy: Buddy | null = null;
  private buddyWanted: BuddyState | null = null;
  /** Rechte Kante der Buehne beim letzten Bild (Claudes Wuerfel wandert mit ihr). */
  private buddyEdge: number | null = null;
  private helperWanted: HelperPhase | null = null;
  /** Milo ist gerade auf Ausflug auf dem Desktop: hier nicht zeichnen und nicht herumlaufen. */
  away = false;
  scenery: Scenery = { hour: 12, weather: null, mail: false, growth: 1 };
  private precip: Precip[] = [];
  private clouds: { u: number; y: number; w: number; speed: number }[] = [];
  private critters: Critter[] = [];
  /** Bis zum naechsten Besuch eines Schmetterlings oder der Gluehwuermchen. */
  private critterIn = 40;
  private drops: Drop[] = [];
  /** Die Pflanze richtet sich nach dem Giessen kurz auf (1 = frisch gegossen). */
  private plantPerk = 0;
  /** Wann zuletzt gegossen wurde (Uhr der Buehne). */
  private lastWatered = -999;
  /** Deko-Objekte in der Insel, in dieser Reihenfolge. */
  decor: string[] = ["plant", "mailbox", "lantern"];
  /** 0 = geschlossene Pille, 1 = offen; laeuft weich mit. */
  private openness = 0;
  /**
   * Offene Insel: die Flaeche, auf der Milo steht (Oberkante eines Knopfs, einer Karte oder der
   * Glasleiste), in Zeichenkoordinaten. null = er steht auf dem Boden der Kopfzeile.
   */
  perch: { x: number; y: number } | null = null;
  /** Hoehe, auf der Milos Fuesse gerade stehen (springt mit). */
  private feetY = NaN;
  /** Laufender Sprung zu einer neuen Flaeche. */
  private hop: { fromX: number; fromY: number; toX: number; toY: number; t: number; dur: number; height: number } | null = null;
  private stars: { x: number; y: number; phase: number; size: number }[] = [];
  private starTarget = 0;
  private starAmount = 0;
  private pushSide: -1 | 1 = 1;
  private pushTarget = 0;
  private pushNow = 0;
  private wasNear = false;
  private lastWave = -999;
  private clock = 0;
  private placed = false;

  /** True, solange Bewegung sichtbar ist, die volle Bildrate verdient. */
  get lively() {
    const buddy = this.buddy;
    const buddyBusy = buddy !== null && (buddy.c.lively || buddy.state === "question" || buddy.state === "confirm" || (buddy.state === "done" && buddy.since < 10));
    if (this.away) return this.critters.length > 0 || buddyBusy;
    return this.milo.lively || this.ball !== null || this.friend !== null || this.goal !== null || this.pushNow > 0.01 || buddyBusy
      || this.critters.length > 0 || this.drops.length > 0;
  }

  /** Laeuft gerade mehr als Milos eigene Bewegung (Ball, Besuch, Laufen, Gesten)? */
  get busyScene() {
    return this.ball !== null || this.friend !== null || this.goal !== null || this.milo.busy || this.pushNow > 0.01;
  }

  /** Es ist etwas los (Eingabe, Klick): Milo ist wieder wach und aufmerksam. */
  stir() {
    this.quiet = 0;
    if (this.tempMood === "sit" || this.tempMood === "doze") this.abandon();
  }

  /** Klick auf die Insel: trifft er Milo, kichert er. */
  hits(x: number, y: number): boolean {
    const { size } = this.layout;
    const floor = this.feet;
    return !this.away && Math.abs(x - this.x) < size * 0.75 && y > floor - size * 1.25 && y < floor + 6;
  }

  /** Wo Milos Fuesse gerade stehen. */
  get feet(): number {
    return Number.isNaN(this.feetY) ? this.layout.floor : this.feetY;
  }

  /**
   * Springt zur Flaeche (offen) bzw. zurueck auf den Boden der Kopfzeile (geschlossen). Kleine
   * Verschiebungen gleicht er ohne Sprung aus. Liefert, ob die Flaeche Milos Platz bestimmt.
   */
  private updatePerch(dt: number): boolean {
    const { floor, size } = this.layout;
    if (Number.isNaN(this.feetY)) this.feetY = floor;
    const target = this.perch ?? { x: this.x, y: floor };
    const hop = this.hop;
    if (hop) {
      hop.t = Math.min(1, hop.t + dt / hop.dur);
      hop.toX = target.x;
      hop.toY = target.y;
      // Waagrecht gleichmaessig, senkrecht als Wurfparabel.
      const t = hop.t;
      const e = t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
      this.x = hop.fromX + (hop.toX - hop.fromX) * e;
      this.feetY = hop.fromY + (hop.toY - hop.fromY) * t - hop.height * 4 * t * (1 - t);
      this.goal = null;
      if (t >= 1) {
        this.hop = null;
        this.feetY = hop.toY;
        this.milo.play("land");
      }
      return true;
    }
    const dx = target.x - this.x;
    const dy = target.y - this.feetY;
    if (Math.abs(dy) > 6 || (this.perch && Math.abs(dx) > size * 0.8)) {
      const dist = Math.hypot(dx, dy);
      this.hop = {
        fromX: this.x, fromY: this.feetY, toX: target.x, toY: target.y, t: 0,
        dur: clamp(0.42 + dist / 1400, 0.45, 0.8),
        // Nach oben springt er hoeher ab, nach unten genuegt ein kleiner Hopser.
        height: clamp(size * 0.9 + Math.max(0, -dy) * 0.18 + Math.abs(dx) * 0.05, size * 0.8, size * 3),
      };
      this.milo.play("jump");
      this.stir();
      return true;
    }
    this.feetY = target.y;
    if (this.perch) {
      this.x = target.x;
      this.goal = null;
      return true;
    }
    return false;
  }

  poke(x: number, y: number): boolean {
    const hit = this.hits(x, y);
    if (hit) {
      this.stir();
      this.milo.play("giggle");
    }
    return hit;
  }

  play(kind: OneShot) {
    return this.milo.play(kind);
  }

  /** Der Cloud-Helfer wird gebraucht (mit dem, was er gerade tut) oder nicht mehr (null). */
  setHelper(phase: HelperPhase | null) {
    this.helperWanted = phase;
  }

  /** Holt den zweiten Wuerfel als Helfer, laesst ihn uebergeben oder schickt ihn weg. */
  private syncHelper() {
    const wanted = this.helperWanted;
    const friend = this.friend;
    if (wanted && wanted !== "done") {
      if (!friend) {
        const c = new Character({ accent: ["#7fdcff", "#b98cf5"] });
        c.idleGestures = false;
        c.play("appear");
        const { left, right, size } = this.layout;
        const side = this.helperSide();
        this.friend = { c, x: clamp(this.x + side * size * 3.4, left + size * 0.8, right - size * 0.8), goal: null, age: 0, leaving: null, helper: true };
        this.milo.play("perk");
      } else if (!friend.helper && friend.leaving === null) {
        // Er war ohnehin gerade zu Besuch: dann hilft er gleich mit.
        friend.helper = true;
      }
    } else if (friend?.helper && friend.leaving === null && friend.parting === undefined) {
      if (wanted === "done") {
        // Ergebnis uebergeben, Milo bedankt sich, dann geht der Helfer.
        friend.parting = 1;
      } else {
        friend.c.setMood("idle");
        friend.leaving = friend.c.play("vanish");
      }
    }
  }

  /** Auf welcher Seite von Milo der Helfer steht: dort, wo Platz ist. */
  private helperSide(): -1 | 1 {
    const { left, roamMax, size } = this.layout;
    if (this.friend && this.friend.x !== this.x) return this.friend.x > this.x ? 1 : -1;
    const fitsRight = this.x + size * 1.9 <= roamMax + size * 0.4;
    const fitsLeft = this.x - size * 1.9 >= left + size * 0.8;
    return fitsRight || !fitsLeft ? 1 : -1;
  }

  /** Claude ist offen (mit dem, was es gerade tut) oder nicht da (null). */
  setBuddy(state: BuddyState | null) {
    this.buddyWanted = state;
  }

  private updateBuddy(dt: number) {
    const { right, size } = this.raw;
    const wanted = this.buddyWanted;
    let buddy = this.buddy;
    if (!buddy) {
      if (wanted === null) return;
      const c = new Character({ accent: ["#f5a57a", "#d97757"], detail: "#ffd9c2" });
      c.idleGestures = false;
      c.play("appear");
      buddy = this.buddy = { c, x: right - size * 1.1, state: "idle", since: 0, leaving: null, nudge: 1, y: NaN, hop: null, walking: false, wait: 0 };
      this.milo.play("perk");
      this.buddyEdge = null;
    }
    const c = buddy.c;
    if (buddy.leaving !== null) {
      // Claude wurde geschlossen: verabschieden. Kommt es vorher zurueck, bleibt der Wuerfel einfach.
      buddy.leaving -= dt;
      if (buddy.leaving <= 0) {
        this.buddy = null;
        return;
      }
    } else if (wanted === null) {
      c.setMood("idle");
      buddy.leaving = c.play("vanish");
    } else {
      if (wanted !== buddy.state) {
        buddy.state = wanted;
        buddy.since = 0;
        buddy.nudge = 0.4;
        if (wanted === "done") c.play("success");
        if (wanted === "question" || wanted === "confirm") {
          // Milo bemerkt, dass Claude etwas will.
          this.stir();
          this.milo.play("perk");
        }
      }
      buddy.since += dt;
      // Sein Platz: geschlossen rechts in der Pille, links neben der Anzeige bzw. den Knoepfen; offen
      // neben Milo auf derselben Flaeche. Er springt selbst hinterher, statt an Milo zu kleben.
      const spot = this.buddySpot();
      // Waechst oder schrumpft die Insel, wandert die rechte Kante: mitnehmen statt hinterherlaufen.
      if (!this.perch && !buddy.hop && this.buddyEdge !== null) buddy.x += this.raw.right - this.buddyEdge;
      this.buddyEdge = this.raw.right;
      const motion = this.follow(buddy, c, spot.x, spot.y, dt);
      if (motion === "still") {
        const talking = this.current?.kind === "chat";
        if (!talking) c.setMood(buddy.state === "working" ? "working" : "idle");
        // Bei Frage oder Bestaetigung macht es immer wieder kurz auf sich aufmerksam.
        if (buddy.state === "question" || buddy.state === "confirm") {
          buddy.nudge -= dt;
          if (buddy.nudge <= 0 && !c.busy) {
            c.play(buddy.state === "confirm" ? "hop" : pick<OneShot>(["tilt", "wave"]));
            buddy.nudge = buddy.state === "confirm" ? 1.6 : 2.6;
          }
        }
      }
    }
    // Es schaut zu Milo, ausser es wartet auf den Nutzer: dann schaut es nach vorn.
    const waiting = buddy.state === "question" || buddy.state === "confirm";
    c.gaze = waiting ? null : { x: Math.sign(this.x - buddy.x) * 0.6, y: 0 };
    c.update(dt);
  }

  /** Wohin Claudes Wuerfel gehoert (Fuesse). */
  private buddySpot(): { x: number; y: number } {
    const { left, right, size, floor } = this.raw;
    if (this.perch) {
      // Offen: neben Milos Ziel-Flaeche, rechts wenn Platz ist, sonst links.
      const px = this.hop ? this.hop.toX : this.x;
      const py = this.perch.y;
      const rightSide = px + size * 1.35;
      return { x: rightSide <= right - size * 0.6 ? rightSide : Math.max(left + size * 0.6, px - size * 1.35), y: py };
    }
    const x = this.roaming
      ? Math.min(right - size * 1.1, this.raw.roamMax)
      : right - (this.raw.buttons ? this.raw.buttons + size * 0.9 : size * 4.3);
    return { x, y: floor };
  }

  /** Wohin der Freund gehoert, solange Milo offen auf einer Flaeche steht. */
  private friendSpot(): { x: number; y: number } | null {
    if (!this.perch) return null;
    const { left, right, size } = this.raw;
    const px = this.hop ? this.hop.toX : this.x;
    const leftSide = px - size * 1.35;
    return { x: leftSide >= left + size * 0.6 ? leftSide : Math.min(right - size * 0.6, px + size * 1.35), y: this.perch.y };
  }

  /**
   * Bewegt einen Begleiter zu (tx, ty): auf derselben Hoehe gehend (erst ab einem kleinen Abstand,
   * dann bis ganz hin, damit Lauf- und Standpose nicht flackern), auf eine andere Hoehe mit einem
   * eigenen Sprung, kurz nachdem Milo gesprungen ist. Verschiebt sich nur die Insel selbst, gleitet
   * er ohne Schritte mit.
   */
  private follow(f: Follow & { x: number }, c: Character, tx: number, ty: number, dt: number): "still" | "walk" | "hop" | "wait" {
    const { size } = this.raw;
    if (Number.isNaN(f.y)) f.y = ty;
    const hop = f.hop;
    if (hop) {
      hop.t = Math.min(1, hop.t + dt / hop.dur);
      hop.toX = tx;
      hop.toY = ty;
      const t = hop.t;
      const e = t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
      f.x = hop.fromX + (hop.toX - hop.fromX) * e;
      f.y = hop.fromY + (hop.toY - hop.fromY) * t - hop.height * 4 * t * (1 - t);
      if (t >= 1) {
        f.hop = null;
        f.y = ty;
        f.x = tx;
        c.play("land");
      }
      c.setMood("idle");
      return "hop";
    }
    const dx = tx - f.x;
    const dy = ty - f.y;
    if (Math.abs(dy) > 4) {
      // Erst Milo springen lassen, dann hinterher.
      f.wait += dt;
      f.walking = false;
      c.setMood("idle");
      if (f.wait < 0.22 || (this.hop && this.hop.t < 0.5)) return "wait";
      f.wait = 0;
      const dist = Math.hypot(dx, dy);
      f.hop = {
        fromX: f.x, fromY: f.y, toX: tx, toY: ty, t: 0,
        dur: clamp(0.42 + dist / 1400, 0.45, 0.8),
        height: clamp(size * 0.8 + Math.max(0, -dy) * 0.18 + Math.abs(dx) * 0.05, size * 0.7, size * 2.6),
      };
      c.play("jump");
      return "hop";
    }
    f.wait = 0;
    f.y = ty;
    if (!f.walking && Math.abs(dx) > size * 0.45) f.walking = true;
    if (!f.walking) {
      // Kleine Verschiebung (Insel waechst, Knoepfe kommen dazu): weich mitgleiten, ohne Schritte.
      f.x += dx * Math.min(1, dt * 10);
      return "still";
    }
    const speed = clamp(Math.abs(dx) * 3.5, 50, 300) * (size / 34);
    const moved = Math.sign(dx) * Math.min(Math.abs(dx), speed * dt);
    f.x += moved;
    c.dir = Math.sign(dx) || c.dir;
    c.step += (Math.PI * Math.abs(moved)) / (size * 0.31);
    c.setMood(Math.abs(dx) > size * 3 ? "dash" : "walk");
    if (Math.abs(tx - f.x) < 1) {
      f.x = tx;
      f.walking = false;
      return "still";
    }
    return "walk";
  }

  /** Abzeichen ueber Claudes Wuerfel: Fragezeichen, Ausrufezeichen oder Haken. */
  private drawBadge(g: CanvasRenderingContext2D, buddy: Buddy, floor: number, size: number) {
    const sign = buddy.state === "question" ? "?" : buddy.state === "confirm" ? "!" : buddy.state === "done" && buddy.since < 10 ? "✓" : "";
    if (!sign || buddy.leaving !== null) return;
    const S = size * 0.86;
    const pop = Math.min(1, buddy.since * 5);
    const bob = Math.sin(this.clock * 4) * 1.2;
    const r = S * 0.24 * (0.6 + 0.4 * pop);
    const x = buddy.x + S * 0.42, y = floor - S * 1.12 + bob;
    g.save();
    g.globalAlpha = pop;
    g.fillStyle = buddy.state === "done" ? "#30d158" : buddy.state === "confirm" ? "#ffd60a" : "#ffffff";
    g.beginPath();
    g.arc(x, y, r, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = "#111114";
    g.font = `700 ${Math.round(r * 1.5)}px "Inter Variable", "Segoe UI", sans-serif`;
    g.textAlign = "center";
    g.textBaseline = "middle";
    g.fillText(sign, x, y + r * 0.08);
    g.restore();
  }

  /** Laesst Milo sofort etwas Bestimmtes unternehmen (Labor und Pruefskripte). */
  perform(what: "ball" | "friend" | "plant" | "stars" | "push" | "sit" | "critter") {
    if (what === "critter") {
      this.critterIn = 0;
      return;
    }
    this.abandon();
    const { roamMin, roamMax } = this.layout;
    const plans: Record<typeof what, Step[]> = {
      ball: [{ kind: "ball" }, { kind: "shot", shot: "hop" }],
      friend: [{ kind: "friend", time: 9 }, { kind: "shot", shot: "wave" }],
      plant: [{ kind: "walk", to: this.plantX() + this.layout.size * 1.05 }, { kind: "plant" }, { kind: "shot", shot: "nod" }],
      stars: [{ kind: "stars", time: 8 }, { kind: "shot", shot: "stretch" }],
      push: [{ kind: "walk", to: this.x < (roamMin + roamMax) / 2 ? roamMin : roamMax }, { kind: "push", side: this.x < (roamMin + roamMax) / 2 ? -1 : 1, time: 2.2 }, { kind: "shot", shot: "shrug" }],
      sit: [{ kind: "mood", mood: "sit", time: 5 }, { kind: "mood", mood: "doze", time: 8 }, { kind: "shot", shot: "startle" }],
    };
    this.plan = plans[what];
    this.rest = 0;
  }

  /** Beendet weich, was Milo gerade von selbst tut. */
  private abandon() {
    this.plan = [];
    this.current = null;
    this.tempMood = null;
    this.goal = null;
    this.pushTarget = 0;
    this.starTarget = 0;
    if (this.ball) this.ball.leaving = true;
    this.dismissFriend();
  }

  private dismissFriend() {
    const friend = this.friend;
    // Der Helfer bleibt, bis seine Arbeit getan ist.
    if (!friend || friend.leaving !== null || friend.helper) return;
    friend.goal = null;
    friend.c.setMood("idle");
    friend.leaving = friend.c.play("vanish");
  }

  update(dt: number) {
    dt = clamp(dt, 0, 0.1);
    this.clock += dt;
    this.updateBuddy(dt);
    const { roamMin, roamMax, size } = this.layout;
    if (!this.placed) {
      // Beim ersten Bild in den erlaubten Bereich stellen, ohne Anlauf.
      this.x = clamp(this.x, roamMin, roamMax);
      this.placed = true;
    }

    // Ein laufender Timer haelt Milo nicht vom Herumlaufen ab, erst kurz vor Ablauf bleibt er stehen.
    const calm = this.mood === "idle" || (this.mood === "timer" && this.milo.urgency === 0);
    const free = this.roaming && calm && !this.away;
    if (free) {
      this.quiet += dt;
      if (this.hovered) {
        // Unter dem Mauszeiger: stehen bleiben, hinschauen, ab und zu winken.
        if (this.current) this.abandon();
        this.rest = Math.max(this.rest, 1.5);
        if (this.clock - this.lastWave > 45 && !this.milo.busy) {
          this.lastWave = this.clock;
          this.milo.play("wave");
        }
      } else {
        this.runPlan(dt);
      }
    } else {
      if (this.current || this.plan.length) this.abandon();
      this.quiet = 0;
      this.rest = rand(3, 6);
    }

    // Ausserhalb des erlaubten Bereichs (Insel hat sich veraendert): zuegig hineinlaufen.
    const inside = clamp(this.x, roamMin, roamMax);
    if (Math.abs(inside - this.x) > 1 && (this.goal === null || this.goal < roamMin || this.goal > roamMax)) {
      this.goal = inside;
      this.brisk = true;
    }

    if (!this.updatePerch(dt)) this.move(dt);
    this.updateBall(dt);
    this.lookAtCursor();
    this.syncHelper();
    this.updateFriend(dt);
    this.updateDecor(dt);
    this.updateScenery(dt);

    const moving = this.goal !== null;
    const far = moving && Math.abs(this.goal! - this.x) > size * 2.2;
    const mood: Mood = moving ? (this.brisk && far ? "dash" : "walk") : calm ? (this.tempMood ?? this.mood) : this.mood;
    this.milo.setMood(mood);
    this.milo.idleGestures = false;
    this.milo.update(dt);
  }

  private move(dt: number) {
    if (this.goal === null) {
      this.vx = 0;
      return;
    }
    const { left, right, size } = this.layout;
    const dx = this.goal - this.x;
    const dir = Math.sign(dx) || 1;
    if (Math.abs(dx) < 0.8) {
      this.x = this.goal;
      this.goal = null;
      this.vx = 0;
      return;
    }
    const scale = size / 34;
    let speed: number;
    if (this.brisk) {
      // Flitzen: schnell los, weich abbremsen.
      speed = clamp(Math.abs(dx) * 6, 46 * scale, 430 * scale);
    } else {
      // Gemuetlich gehen, vor dem Ziel langsamer werden.
      speed = clamp(Math.abs(dx) * 3, 12 * scale, 32 * scale);
    }
    // Beschleunigen statt springen.
    this.vx += clamp(dir * speed - this.vx, -900 * dt, 900 * dt);
    const moved = clamp(this.vx * dt, -Math.abs(dx), Math.abs(dx));
    this.x = clamp(this.x + moved, left + size * 0.8, right - size * 0.8);
    this.milo.dir = dir;
    // Die Schrittphase haengt an der Strecke, so rutschen die Schritte nicht.
    this.milo.step += (Math.PI * Math.abs(moved)) / (size * 0.36);
  }

  private lookAtCursor() {
    const { floor, size } = this.layout;
    if (!this.cursor) {
      this.milo.gaze = null;
      this.wasNear = false;
      return;
    }
    const dx = this.cursor.x - this.x;
    const dy = this.cursor.y - (floor - size / 2);
    const dist = Math.hypot(dx, dy);
    // Voller Blick bis 260 px, darueber laesst das Interesse nach.
    const strength = clamp(1 - (dist - 260) / 260, 0, 1);
    if (strength < 0.03 || dist < 3) {
      this.milo.gaze = null;
    } else {
      const reach = Math.min(1, dist / 50) * strength;
      this.milo.gaze = { x: (dx / dist) * reach, y: (dy / dist) * reach };
    }
    // Kommt der Zeiger nach laengerer Ruhe nah heran, horcht Milo auf.
    const near = dist < 130;
    if (near && !this.wasNear && this.quiet > 6 && this.mood === "idle" && !this.milo.busy) {
      if (this.tempMood === "doze" || this.tempMood === "sit") this.stir();
      else this.milo.play("perk");
    }
    this.wasNear = near;
  }

  // ---------- Eigene Unternehmungen ----------

  private runPlan(dt: number) {
    if (!this.current) {
      if (this.plan.length === 0) {
        this.rest -= dt;
        if (this.rest > 0) return;
        this.plan = this.think();
        // Verspielt = kuerzere Pausen, ruhig = laengere.
        this.rest = rand(2.5, 7) * [2.4, 1, 0.55][clamp(Math.round(this.liveliness), 0, 2)];
      }
      this.current = this.plan.shift() ?? null;
      this.elapsed = 0;
      if (this.current) this.begin(this.current);
      return;
    }

    this.elapsed += dt;
    const step = this.current;
    let done = false;
    switch (step.kind) {
      case "walk":
        done = this.goal === null;
        break;
      case "wait":
        done = this.elapsed >= step.time;
        break;
      case "shot":
        done = !this.milo.busy && this.elapsed > 0.1;
        break;
      case "mood":
        done = this.elapsed >= step.time;
        if (done) this.tempMood = null;
        break;
      case "ball":
        done = this.ball === null;
        break;
      case "push":
        // Druck baut sich auf, haelt und laesst wieder nach.
        this.pushTarget = this.elapsed < step.time * 0.75 ? 1 : 0;
        done = this.elapsed >= step.time;
        if (done) this.tempMood = null;
        break;
      case "plant":
        done = this.elapsed > 1.4;
        break;
      case "stars":
        done = this.elapsed >= step.time;
        if (done) {
          this.tempMood = null;
          this.starTarget = 0;
        }
        break;
      case "friend":
        if (this.elapsed >= step.time) this.dismissFriend();
        done = this.friend === null;
        if (done) this.tempMood = null;
        break;
      case "chat": {
        // Plausch mit Claudes Wuerfel: abwechselnd spricht einer, der andere hoert zu.
        const buddy = this.buddy;
        done = this.elapsed >= step.time || !buddy || buddy.leaving !== null || buddy.state !== "idle";
        if (buddy && !done) {
          const turn = Math.floor(this.elapsed / 2.4) % 2 === 0;
          if (Math.random() < dt * 7) (turn ? this.milo : buddy.c).talk();
          this.tempMood = turn ? "answering" : null;
          buddy.c.setMood(turn ? "idle" : "answering");
          if (!this.milo.gaze) this.milo.gaze = { x: 0.8, y: 0 };
          if (Math.random() < dt * 0.1 && !buddy.c.busy) buddy.c.play(pick<OneShot>(["giggle", "nod", "tilt"]));
        }
        if (done) this.tempMood = null;
        break;
      }
    }
    if (done) this.current = null;
  }

  private begin(step: Step) {
    const { left, right, floor, size, roamMin, roamMax } = this.layout;
    switch (step.kind) {
      case "walk":
        this.goal = clamp(step.to, roamMin, roamMax);
        this.brisk = step.brisk ?? false;
        break;
      case "shot":
        this.milo.play(step.shot);
        break;
      case "mood":
        this.tempMood = step.mood;
        break;
      case "ball":
        this.spawnBall();
        break;
      case "push":
        this.tempMood = "push";
        this.milo.dir = step.side;
        this.pushSide = step.side;
        break;
      case "plant": {
        // Giessen: zur Pflanze drehen, Wasser rieselt, die Pflanze richtet sich auf.
        const px = this.plantX();
        this.milo.dir = px < this.x ? -1 : 1;
        this.milo.play("nod");
        this.lastWatered = this.clock;
        for (let i = 0; i < 9; i++) {
          this.drops.push({ x: this.x + this.milo.dir * size * 0.42, y: floor - size * 0.8, vx: (px - this.x) * rand(0.9, 1.5), vy: -rand(10, 60) });
        }
        this.plantPerk = 1;
        break;
      }
      case "stars":
        this.tempMood = "sit";
        this.starTarget = 1;
        this.stars = Array.from({ length: 9 }, () => ({
          x: rand(left + 18, right - 18), y: rand(7, floor - size * 1.15), phase: rand(0, 6.28), size: rand(2, 3.6),
        }));
        break;
      case "friend": {
        // Der Freund taucht auf der Seite mit mehr Platz auf und kommt herueber.
        const side = this.x < (left + right) / 2 ? 1 : -1;
        const c = new Character({ accent: ["#7fdcff", "#b98cf5"] });
        c.idleGestures = false;
        const x = clamp(this.x + side * size * 4.2, left + size * 0.8, right - size * 0.8);
        c.play("appear");
        this.friend = { c, x, goal: this.x + side * size * 1.9, age: 0, leaving: null };
        this.milo.play("perk");
        break;
      }
    }
  }

  /** Ueberlegt sich die naechste kleine Unternehmung. */
  private think(): Step[] {
    const { roamMin, roamMax, left, right, size } = this.layout;
    const room = roamMax - roamMin;
    const gesture = (): Step => ({ kind: "shot", shot: pickWeighted<OneShot>([["lookAround", 3], ["tilt", 3], ["hop", 2], ["stretch", 1.5], ["wave", 1], ["giggle", 0.6]]) });
    if (room < size * 1.5) return [gesture()];

    const stroll = (): Step[] => {
      // Ein Ziel, das sich lohnt: mindestens ein gutes Stueck entfernt.
      let to = rand(roamMin, roamMax);
      if (Math.abs(to - this.x) < room * 0.25) to = this.x < (roamMin + roamMax) / 2 ? rand(this.x + room * 0.3, roamMax) : rand(roamMin, this.x - room * 0.3);
      const after: Step[] = Math.random() < 0.45 ? [{ kind: "wait", time: rand(0.4, 1.2) }, gesture()] : [{ kind: "wait", time: rand(1, 3) }];
      return [{ kind: "walk", to }, ...after];
    };

    const options: [() => Step[], number][] = [
      [stroll, 5],
      [() => [gesture()], 2],
      [() => [{ kind: "stars", time: rand(9, 15) }, { kind: "shot", shot: "stretch" }], 0.9],
    ];
    if (this.buddy && this.buddy.state === "idle" && this.buddy.leaving === null) {
      // Claude hat gerade nichts zu tun: hinuebergehen und ein wenig plaudern.
      options.push([() => [{ kind: "walk", to: roamMax }, { kind: "chat", time: rand(7, 12) }, { kind: "shot", shot: "nod" }], 1.6]);
    }
    if (this.quiet > 50) {
      // Nach einer Weile ohne Beschaeftigung hinhocken, spaeter auch einnicken.
      options.push([() => {
        const steps: Step[] = [{ kind: "mood", mood: "sit", time: rand(8, 14) }];
        if (this.quiet > 200) steps.push({ kind: "mood", mood: "doze", time: rand(14, 26) }, { kind: "shot", shot: "startle" }, { kind: "shot", shot: "lookAround" });
        else steps.push({ kind: "shot", shot: "stretch" });
        return steps;
      }, 1.6]);
    }
    if (room > size * 4.5) {
      if (this.quiet > 12) options.push([() => [{ kind: "ball" }, { kind: "shot", shot: "hop" }], 1.1]);
      if (this.quiet > 20) {
        // Besuch: in die Mitte gehen, plaudern, zum Abschied winken.
        options.push([() => [
          { kind: "walk", to: clamp(this.x, roamMin + size * 2, roamMax - size * 2) },
          { kind: "friend", time: rand(10, 16) },
          { kind: "shot", shot: "wave" },
        ], 1.2]);
      }
      // Ab und zu die Pflanze giessen (sie steht links am Rand).
      if (this.clock - this.lastWatered > 150 && this.decor.includes("plant")) options.push([() => [{ kind: "walk", to: this.plantX() + size * 1.05 }, { kind: "plant" }, { kind: "shot", shot: "nod" }], 0.7]);
    }
    // Gegen die Wand druecken geht nur dort, wo Milos Bereich bis nahe an den Inselrand reicht.
    const walls = ([[-1, roamMin, left], [1, roamMax, right]] as const).filter(([, edge, wall]) => Math.abs(edge - wall) < size * 1.6);
    if (walls.length > 0 && room > size * 3) {
      options.push([() => {
        const [side, edge] = pick([...walls]);
        return [{ kind: "walk", to: edge }, { kind: "push", side, time: 2.2 }, { kind: "shot", shot: "shrug" }];
      }, 0.7]);
    }
    return pickWeighted(options)();
  }

  // ---------- Freund ----------

  private updateFriend(dt: number) {
    const friend = this.friend;
    if (!friend) return;
    const { left, right, size } = this.layout;
    friend.age += dt;
    const side = Math.sign(friend.x - this.x) || 1;
    const c = friend.c;

    if (friend.leaving !== null) {
      friend.leaving -= dt;
      if (friend.leaving <= 0) {
        this.friend = null;
        return;
      }
    } else if (this.friendHops(friend, dt)) {
      // Springt oder laeuft gerade Milo hinterher.
    } else if (friend.helper) {
      this.updateHelper(friend, dt);
    } else if (friend.goal !== null && !this.perch) {
      // Herueberwatscheln.
      const dx = friend.goal - friend.x;
      if (Math.abs(dx) < 1) {
        friend.goal = null;
        c.setMood("idle");
        c.play("wave");
      } else {
        const moved = Math.sign(dx) * Math.min(Math.abs(dx), 34 * (size / 34) * dt);
        friend.x = clamp(friend.x + moved, left + size * 0.7, right - size * 0.7);
        c.dir = Math.sign(dx);
        c.step += (Math.PI * Math.abs(moved)) / (size * 0.31);
        c.setMood("walk");
      }
      this.tempMood = null;
    } else {
      // Plaudern: abwechselnd spricht einer, der andere hoert zu; zwischendurch wird gelacht.
      const turn = Math.floor(friend.age / 2.3) % 2 === 0;
      const speaker = turn ? c : this.milo;
      if (Math.random() < dt * 7) speaker.talk();
      c.setMood(turn ? "answering" : "idle");
      this.tempMood = turn ? null : "answering";
      if (Math.random() < dt * 0.12 && !c.busy && !this.milo.busy) (Math.random() < 0.5 ? c : this.milo).play(pick<OneShot>(["giggle", "hop", "nod", "tilt"]));
    }
    // Die beiden schauen sich an, solange der Mauszeiger nicht ablenkt.
    c.gaze = { x: -side * 0.8, y: 0 };
    c.facing = -side;
    if (!this.milo.gaze) this.milo.gaze = { x: side * 0.8, y: 0 };
    this.milo.facing = side;
    c.update(dt);
  }

  /**
   * Offen (Milo steht auf einer Flaeche) oder auf dem Weg zurueck auf den Boden: der Freund folgt mit
   * eigenen Spruengen. Liefert true, solange er dabei unterwegs ist.
   */
  private friendHops(friend: Friend, dt: number): boolean {
    const floor = this.raw.floor;
    const y = friend.y ?? NaN;
    if (!this.perch && !friend.hop && (Number.isNaN(y) || Math.abs(y - floor) <= 4)) {
      friend.y = floor;
      return false;
    }
    const f = friend as Friend & Follow;
    f.y ??= NaN;
    f.hop ??= null;
    f.walking ??= false;
    f.wait ??= 0;
    const spot = this.friendSpot() ?? { x: friend.x, y: floor };
    return this.follow(f, friend.c, spot.x, spot.y, dt) !== "still";
  }

  /** Der Freund als Cloud-Helfer: bleibt an Milos Seite, tippt, traegt vor und uebergibt. */
  private updateHelper(friend: Friend, dt: number) {
    const { left, right, size } = this.layout;
    const c = friend.c;
    // Neben Milo bleiben, auch wenn die Insel gerade auf- oder zuklappt (offen sorgt friendHops dafuer).
    const target = this.perch ? friend.x : clamp(this.x + this.helperSide() * size * 1.9, left + size * 0.7, right - size * 0.7);
    const dx = target - friend.x;
    if (Math.abs(dx) > 1.5) {
      const speed = clamp(Math.abs(dx) * 6, 34, 430) * (size / 34);
      const moved = Math.sign(dx) * Math.min(Math.abs(dx), speed * dt);
      friend.x += moved;
      c.dir = Math.sign(dx);
      c.step += (Math.PI * Math.abs(moved)) / (size * 0.31);
      c.setMood(Math.abs(dx) > size * 2 ? "dash" : "walk");
      return;
    }
    if (friend.parting !== undefined) {
      // Uebergabe erst, wenn er wirklich neben Milo steht.
      if (!friend.handed) {
        friend.handed = true;
        c.setMood("idle");
        friend.parting = c.play("handoff") + 0.45;
        this.milo.play("nod");
      }
      friend.parting -= dt;
      if (friend.parting <= 0) {
        friend.parting = undefined;
        friend.helper = false;
        friend.leaving = c.play("vanish");
        this.milo.play("wave");
      }
      return;
    }
    friend.x = target;
    switch (this.helperWanted) {
      case "working":
        c.setMood("working");
        break;
      case "talking":
        c.setMood("answering");
        if (Math.random() < dt * 9) c.talk();
        break;
      default:
        c.setMood("idle");
    }
  }

  // ---------- Sterne, Wand ----------

  private updateDecor(dt: number) {
    this.starAmount += clamp(this.starTarget - this.starAmount, -dt * 1.2, dt * 0.8);
    // Beim Sternegucken geht der Blick nach oben.
    if (this.starTarget > 0 && !this.milo.gaze) this.milo.gaze = { x: Math.sin(this.clock * 0.4) * 0.5, y: -0.9 };
    const before = this.pushNow;
    this.pushNow += (this.pushTarget - this.pushNow) * Math.min(1, dt * 7);
    if (this.pushNow < 0.004 && this.pushTarget === 0) this.pushNow = 0;
    if (before !== this.pushNow) this.onPush(this.pushSide, this.pushNow);
  }

  // ---------- Kulisse ----------

  /** Platz der Pflanze: geschlossen links am Inselrand, wo Milo nicht hinlaeuft. */
  private plantX() {
    const { left, size } = this.layout;
    return this.decorX("plant") ?? left + size * 0.62;
  }

  /** Platz eines Deko-Objekts in der geschlossenen Pille. */
  private decorX(kind: string): number | null {
    const i = this.decor.indexOf(kind);
    if (i < 0) return null;
    const { left, right, size } = this.layout;
    if (this.decorDrag?.kind === kind) return this.decorDrag.x;
    // Eigener Platz (vom Nutzer verschoben) als Anteil der Pillenbreite.
    const own = this.decorPositions[kind];
    if (own !== undefined) return left + size * 0.5 + own * Math.max(0, right - left - size);
    // Sonst nebeneinander am linken Rand der Pille; beim Aufklappen sinken sie weg.
    return left + size * 0.62 + i * size * 0.95;
  }

  /** Eigene Plaetze der Deko (Anteil der Breite) und gesperrte Objekte. */
  decorPositions: Record<string, number> = {};
  decorLocked: string[] = [];
  /** Wird gerade gezogen: Objekt und waagrechte Position in Zeichenkoordinaten. */
  decorDrag: { kind: string; x: number } | null = null;

  /** Welches Deko-Objekt liegt an dieser Stelle (nur in der geschlossenen Pille)? */
  decorAt(x: number, y: number): string | null {
    if (this.openness > 0.4) return null;
    const { floor, size } = this.layout;
    for (const kind of [...this.decor].reverse()) {
      const cx = this.decorX(kind);
      const info = decorInfo(kind);
      if (cx === null || !info) continue;
      const w = Math.max(14, size * info.w) / 2 + 3;
      if (Math.abs(x - cx) <= w && y <= floor + 4 && y >= floor - size * info.h - 4) return kind;
    }
    return null;
  }

  /** Anteil der Pillenbreite fuer eine Zeichenposition (zum Speichern nach dem Ziehen). */
  decorRatio(x: number): number {
    const { left, right, size } = this.layout;
    return Math.min(1, Math.max(0, (x - left - size * 0.5) / Math.max(1, right - left - size)));
  }

  private drawDecorItems(g: CanvasRenderingContext2D, floor: number, size: number) {
    const ctx = { clock: this.clock, hour: this.scenery.hour, mail: this.scenery.mail, growth: this.scenery.growth, perk: this.plantPerk, music: this.mood === "music" };
    for (const kind of this.decor) {
      const x = this.decorX(kind);
      if (x === null || !decorInfo(kind)) continue;
      const alpha = 1 - this.openness;
      if (alpha < 0.02) continue;
      g.save();
      g.globalAlpha = alpha;
      // Beim Ziehen haengt es leicht angehoben am Zeiger.
      const lift = this.decorDrag?.kind === kind ? -4 : 0;
      drawDecor(g, kind as DecorKind, x, floor + lift + this.openness * size * 0.8, size, { ...ctx, wobble: lift ? 0.4 : 0 });
      g.restore();
    }
  }

  private updateScenery(dt: number) {
    const { left, right, floor, size } = this.layout;
    const width = Math.max(1, right - left);
    const kind = this.scenery.weather;

    // Regen und Schnee: eine feste Zahl Teilchen, die oben neu beginnen.
    // Regen und Schnee nur in der geschlossenen Pille, nicht ueber dem Inhalt der offenen Insel.
    const wanted = this.openness > 0.3 ? 0 : kind === "rain" ? 12 : kind === "storm" ? 20 : kind === "snow" ? 9 : 0;
    while (this.precip.length < wanted) this.precip.push({ x: left + Math.random() * width, y: Math.random() * floor, phase: Math.random() * 6.28 });
    if (this.precip.length > wanted) this.precip.length = wanted;
    for (const p of this.precip) {
      if (kind === "snow") {
        p.y += 14 * dt;
        p.x += Math.sin(this.clock * 1.2 + p.phase) * 7 * dt;
      } else {
        p.y += (kind === "storm" ? 300 : 220) * dt;
        p.x -= (kind === "storm" ? 60 : 24) * dt;
      }
      if (p.y > floor) {
        p.y = -Math.random() * 20;
        p.x = left + Math.random() * width;
      }
      if (p.x < left) p.x += width;
      if (p.x > right) p.x -= width;
    }

    // Wolken ziehen langsam vorbei, wenn es nicht klar ist.
    const cloudy = kind !== null && kind !== "clear";
    const cloudCount = cloudy ? (kind === "cloudy" || kind === "fog" ? 3 : 2) : 0;
    while (this.clouds.length < cloudCount) this.clouds.push({ u: Math.random(), y: 6 + Math.random() * 10, w: 22 + Math.random() * 26, speed: 0.004 + Math.random() * 0.006 });
    if (this.clouds.length > cloudCount) this.clouds.length = cloudCount;
    for (const c of this.clouds) c.u = (c.u + c.speed * dt) % 1.1;

    // Besuch von Schmetterlingen (tags, ohne Regen) oder Gluehwuermchen (nachts).
    const night = isNight(this.scenery.hour);
    const wet = kind === "rain" || kind === "storm" || kind === "snow";
    this.critterIn -= dt * [0, 1, 1.8][clamp(Math.round(this.liveliness), 0, 2)];
    if (this.critters.length === 0 && this.critterIn <= 0 && !wet && this.liveliness > 0) {
      this.critterIn = rand(80, 180);
      if (night) {
        for (let i = 0; i < 4; i++) {
          this.critters.push({ kind: "firefly", x: left + rand(0.1, 0.9) * width, y: rand(10, floor - 8), vx: rand(-8, 8), base: 0, age: rand(-2, 0), life: rand(14, 20), phase: rand(0, 6.28), hue: "#e6ff7a" });
        }
      } else {
        const fromLeft = Math.random() < 0.5;
        this.critters.push({
          kind: "butterfly", x: fromLeft ? left - 6 : right + 6, y: rand(14, floor - size * 0.9), vx: (fromLeft ? 1 : -1) * rand(18, 28),
          base: rand(14, floor - size * 0.9), age: 0, life: 40, phase: rand(0, 6.28), hue: pick(["#ffd60a", "#ff9fce", "#9fd4ff", "#ffffff"]),
        });
      }
    }
    for (const c of this.critters) {
      c.age += dt;
      if (c.kind === "butterfly") {
        c.x += c.vx * dt;
        c.y = c.base + Math.sin(c.age * 2.2 + c.phase) * 7;
        // Ab und zu ein kleiner Schlenker nach oben.
        c.base = clamp(c.base + Math.sin(c.age * 0.7) * 4 * dt, 10, floor - size * 0.6);
      } else {
        c.vx += rand(-20, 20) * dt;
        c.vx = clamp(c.vx, -12, 12);
        c.x = clamp(c.x + c.vx * dt, left + 6, right - 6);
        c.y = clamp(c.y + Math.sin(c.age * 1.3 + c.phase) * 6 * dt, 8, floor - 6);
      }
    }
    this.critters = this.critters.filter((c) => c.age < c.life && c.x > left - 20 && c.x < right + 20);

    // Giesswasser faellt und verschwindet in der Erde.
    for (const d of this.drops) {
      d.vy += 380 * dt;
      d.x += d.vx * dt;
      d.y += d.vy * dt;
    }
    this.drops = this.drops.filter((d) => d.y < floor - 2);
    this.plantPerk = Math.max(0, this.plantPerk - dt * 0.25);
    this.openness += ((this.roaming ? 0 : 1) - this.openness) * Math.min(1, dt * 6);
  }

  // ---------- Ballspiel ----------

  private spawnBall() {
    const { left, right, size } = this.layout;
    const side = this.x < (left + right) / 2 ? 1 : -1;
    this.ball = {
      x: clamp(this.x + side * rand(size * 1.6, size * 3), left + 12, right - 12), y: -8,
      vx: rand(-30, 30), vy: 0, age: 0, kicks: 0, maxKicks: 3 + Math.floor(Math.random() * 3),
      cooldown: 0.6, pending: -1, squash: 0, alpha: 1, leaving: false,
    };
  }

  private updateBall(dt: number) {
    const ball = this.ball;
    if (!ball) return;
    const { left, right, floor, size } = this.layout;
    const r = size * 0.13;
    ball.age += dt;
    ball.cooldown -= dt;
    ball.squash *= Math.exp(-dt * 14);

    // Flug mit Schwerkraft, Abprallen am Boden und an den Inselwaenden.
    ball.vy += 1100 * dt;
    ball.x += ball.vx * dt;
    ball.y += ball.vy * dt;
    if (ball.y > floor - r) {
      ball.y = floor - r;
      if (ball.vy > 70) {
        ball.squash = Math.min(0.45, ball.vy / 900);
        ball.vy *= -0.62;
      } else {
        ball.vy = 0;
      }
      ball.vx *= Math.exp(-dt * 1.6);
    }
    if (ball.x < left + r) {
      ball.x = left + r;
      ball.vx = Math.abs(ball.vx) * 0.75;
    } else if (ball.x > right - r) {
      ball.x = right - r;
      ball.vx = -Math.abs(ball.vx) * 0.75;
    }

    const resting = ball.y >= floor - r - 0.5 && Math.abs(ball.vy) < 1;
    if (ball.kicks >= ball.maxKicks && resting && Math.abs(ball.vx) < 14) ball.leaving = true;
    if (ball.age > 22) ball.leaving = true;
    if (ball.leaving) {
      ball.alpha -= dt * 2.4;
      if (ball.alpha <= 0) this.ball = null;
      return;
    }

    // Milo laeuft dem Ball nach und stupst ihn an, sobald er in Reichweite am Boden liegt.
    const dx = ball.x - this.x;
    const dir = Math.sign(dx) || 1;
    if (ball.pending >= 0) {
      ball.pending -= dt;
      if (ball.pending < 0) {
        if (Math.abs(dx) < size * 1.1 && ball.y > floor - size) {
          ball.vx = dir * rand(110, 230);
          ball.vy = -rand(250, 400);
          ball.kicks++;
        }
        ball.cooldown = 0.8;
      }
      return;
    }
    const low = ball.y > floor - size * 0.55;
    if (Math.abs(dx) < size * 0.82 && low && ball.cooldown <= 0 && ball.kicks < ball.maxKicks) {
      this.goal = null;
      this.milo.dir = dir;
      this.milo.play("kick");
      ball.pending = 0.16;
    } else if (!this.milo.busy && ball.kicks < ball.maxKicks) {
      this.goal = clamp(ball.x - dir * size * 0.6, this.layout.roamMin, this.layout.roamMax);
      this.brisk = Math.abs(dx) > size * 4;
    }
  }

  // ---------- Zeichnen ----------

  draw(g: CanvasRenderingContext2D) {
    const { floor, size } = this.layout;
    // Kein Himmel mehr (Abendlicht, Wolken): die Insel bleibt ruhig und dunkel.

    if (this.starAmount > 0.01) {
      g.save();
      g.fillStyle = "#ffffff";
      for (const star of this.stars) {
        const twinkle = 0.45 + 0.55 * Math.sin(this.clock * 2.2 + star.phase) ** 2;
        g.globalAlpha = this.starAmount * twinkle * 0.9;
        g.beginPath();
        // Kleiner vierzackiger Stern
        for (let i = 0; i < 8; i++) {
          const rad = (i % 2 === 0 ? star.size : star.size * 0.35) * (0.8 + twinkle * 0.4);
          g.lineTo(star.x + Math.cos((i / 8) * Math.PI * 2) * rad, star.y + Math.sin((i / 8) * Math.PI * 2) * rad);
        }
        g.closePath();
        g.fill();
      }
      g.restore();
    }

    this.drawDecorItems(g, floor, size);
    // Offen stehen Freund und Claudes Wuerfel mit Milo auf derselben Flaeche.
    const feet = this.feet;
    const friend = this.friend;
    if (friend) friend.c.draw(g, friend.x, friend.y === undefined || Number.isNaN(friend.y) ? floor : friend.y, size * 0.86);
    if (this.buddy) {
      const by = Number.isNaN(this.buddy.y) ? floor : this.buddy.y;
      this.buddy.c.draw(g, this.buddy.x, by, size * 0.86);
      this.drawBadge(g, this.buddy, by, size);
    }
    if (!this.away) this.milo.draw(g, this.x, feet, size);
    this.drawFront(g, floor);

    const ball = this.ball;
    if (ball) {
      const r = size * 0.13;
      g.save();
      g.globalAlpha = clamp(ball.alpha, 0, 1);
      g.translate(ball.x, ball.y + r * ball.squash);
      g.scale(1 + ball.squash * 0.5, 1 - ball.squash);
      const paint = g.createRadialGradient(-r * 0.35, -r * 0.4, r * 0.1, 0, 0, r);
      paint.addColorStop(0, "#b6ecff");
      paint.addColorStop(1, "#64d2ff");
      g.fillStyle = paint;
      g.beginPath();
      g.arc(0, 0, r, 0, Math.PI * 2);
      g.fill();
      g.restore();
    }
  }

  /** Himmel hinter allem: Licht am Horizont, Nachtsterne, Wolken. */
  private drawBackdrop(g: CanvasRenderingContext2D) {
    const { left, right, floor } = this.layout;
    const width = right - left;
    const hour = this.scenery.hour;
    g.save();
    // Ein Hauch Licht ueber dem Boden: morgens warm, abends rosa, nachts blaeulich.
    const tint = hour >= 6 && hour < 9 ? "255,159,10" : hour >= 18 && hour < 21 ? "255,105,140" : isNight(hour) ? "100,140,255" : "";
    if (tint) {
      const glow = g.createLinearGradient(0, floor - 16, 0, floor);
      glow.addColorStop(0, `rgba(${tint},0)`);
      glow.addColorStop(1, `rgba(${tint},${isNight(hour) ? 0.07 : 0.11})`);
      g.fillStyle = glow;
      g.fillRect(left, floor - 16, width, 16);
    }
    if (isNight(hour) && this.starAmount < 0.5) {
      g.fillStyle = "#ffffff";
      for (const star of SKY_STARS) {
        g.globalAlpha = 0.18 + 0.22 * Math.sin(this.clock * 0.9 + star.phase) ** 2;
        g.beginPath();
        g.arc(left + 10 + star.u * (width - 20), 4 + star.v * (floor * 0.32), star.size, 0, Math.PI * 2);
        g.fill();
      }
      g.globalAlpha = 1;
    }
    const kind = this.scenery.weather;
    g.fillStyle = kind === "storm" ? "#5a5f6b" : "#8e93a0";
    for (const c of this.clouds) {
      const x = left - c.w + c.u * (width + c.w * 2);
      g.globalAlpha = kind === "fog" ? 0.16 : 0.13;
      g.beginPath();
      g.ellipse(x, c.y, c.w * 0.5, 3.4, 0, 0, Math.PI * 2);
      g.ellipse(x - c.w * 0.18, c.y - 2.2, c.w * 0.24, 3, 0, 0, Math.PI * 2);
      g.ellipse(x + c.w * 0.16, c.y - 1.8, c.w * 0.2, 2.6, 0, 0, Math.PI * 2);
      g.fill();
    }
    g.globalAlpha = 1;
    g.restore();
  }

  /** Vor allem anderen: Regen, Schnee, Schmetterlinge, Gluehwuermchen, Giesswasser. */
  private drawFront(g: CanvasRenderingContext2D, floor: number) {
    g.save();
    const kind = this.scenery.weather;
    if (this.precip.length > 0) {
      if (kind === "snow") {
        g.fillStyle = "#ffffff";
        g.globalAlpha = 0.75;
        for (const p of this.precip) {
          g.beginPath();
          g.arc(p.x, p.y, 1.3, 0, Math.PI * 2);
          g.fill();
        }
      } else {
        g.strokeStyle = "#8fb8ff";
        g.globalAlpha = 0.45;
        g.lineWidth = 1;
        for (const p of this.precip) {
          g.beginPath();
          g.moveTo(p.x, p.y);
          g.lineTo(p.x + 1.4, p.y - 5);
          g.stroke();
        }
      }
      g.globalAlpha = 1;
    }
    for (const c of this.critters) {
      const fade = clamp(Math.min(c.age, c.life - c.age), 0, 1);
      if (fade <= 0) continue;
      if (c.kind === "firefly") {
        const blink = 0.35 + 0.65 * Math.max(0, Math.sin(c.age * 2.6 + c.phase));
        g.globalAlpha = fade * blink;
        g.fillStyle = c.hue;
        g.shadowColor = c.hue;
        g.shadowBlur = 6;
        g.beginPath();
        g.arc(c.x, c.y, 1.5, 0, Math.PI * 2);
        g.fill();
        g.shadowBlur = 0;
      } else {
        // Zwei Fluegelpaare, die auf- und zuklappen.
        const flap = Math.abs(Math.sin(c.age * 14 + c.phase));
        g.globalAlpha = fade;
        g.fillStyle = c.hue;
        const dir = Math.sign(c.vx) || 1;
        for (const side of [-1, 1]) {
          g.beginPath();
          g.ellipse(c.x - dir * 0.6, c.y + side * 1.4 * flap, 2.6, 1.6 * flap + 0.3, side * 0.5, 0, Math.PI * 2);
          g.fill();
        }
        g.fillStyle = "#2a2a2e";
        g.fillRect(c.x - 1.2, c.y - 0.4, 2.4, 0.8);
      }
    }
    g.globalAlpha = 1;
    if (this.drops.length > 0) {
      g.fillStyle = "#64d2ff";
      for (const d of this.drops) {
        g.beginPath();
        g.arc(d.x, d.y, 1.1, 0, Math.PI * 2);
        g.fill();
      }
    }
    void floor;
    g.restore();
  }


}
