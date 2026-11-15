// Milo-Charakter: ein kleiner, sanft leuchtender Wuerfel mit zwei kurzen runden Armen,
// komplett prozedural gezeichnet (keine Bilddateien).
//
// Aufbau: Jede Stimmung (Mood) liefert pro Bild Zielwerte fuer eine Pose (Lage, Stauchung, Augen,
// Arme, Requisiten ...). Die Zielwerte werden nicht direkt gezeichnet, sondern von Federn verfolgt.
// Dadurch sind alle Uebergaenge weich. Darauf liegen kleine Kopplungen, die die Figur lebendig
// machen: Arme schwingen der Koerperbewegung nach und biegen sich leicht, die Augen springen
// schnell und blinzeln beim Blickwechsel, beim Sprechen pulsiert der Koerper im Takt der Woerter.
//
// Der Wuerfel steht auf dem Boden: Stauchen und Strecken gehen vom Boden aus, gedreht wird um
// den Bodenkontakt. Wo er steht und wohin er laeuft, bestimmt die Buehne (stage.ts).

export type Mood =
  | "idle" | "listening" | "thinking" | "answering" | "working" | "searching"
  | "fileHover" | "fileOut" | "timer" | "ringing"
  | "sleeping" | "waking" | "music" | "collab"
  | "walk" | "dash" | "sit" | "doze" | "push";

/** Einmalige Gesten, die ueber der aktuellen Stimmung abgespielt werden. */
export type OneShot =
  | "success" | "error" | "gulp" | "wave" | "hop" | "stretch" | "lookAround" | "tilt"
  | "perk" | "nod" | "appear" | "vanish" | "handoff" | "kick" | "startle" | "giggle" | "shrug"
  | "jump" | "land";

/** Arm-Varianten: kurze Kapsel-Arme, schwebende Kugelhaende, duenne Federlinie mit Kugelhand. */
export type ArmStyle = "capsule" | "hands" | "spring";

const TAU = Math.PI * 2;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const ease = (u: number) => u * u * (3 - 2 * u);
/** Glockenfoermiger Verlauf 0 -> 1 -> 0 ueber u in [0,1]. */
const bell = (u: number) => Math.sin(Math.PI * clamp(u, 0, 1));
/** Anteil von u, der im Fenster [a, b] liegt, als 0..1. */
const span = (u: number, a: number, b: number) => clamp((u - a) / (b - a), 0, 1);
/** Weiches, wiederholbares Rauschen in [0,1]. */
const noise = (t: number) => (Math.sin(t) + Math.sin(t * 2.31 + 1.7) * 0.6 + Math.sin(t * 4.07 + 0.4) * 0.3) / 3.8 + 0.5;

const POSE = {
  // x und y in Koerpergroessen; y negativ = in der Luft.
  x: 0, y: 0, sx: 1, sy: 1, rot: 0, scale: 1,
  /** Lidoeffnung 0..1. */
  lid: 1, eyeX: 0, eyeY: 0, eyeScale: 1,
  /** Froehliche Bogenaugen. */
  smile: 0,
  /** Zusammengekniffene Augen > <. */
  squint: 0,
  /** Entspannt geschlossene Augen (Schlaf). */
  rest: 0,
  blush: 0,
  /** Mund: 0 = keiner, 1 = weit offen. Nur beim Sprechen, Staunen und Gaehnen zu sehen. */
  mouth: 0,
  // Armwinkel: 0 = haengt nach unten, PI/2 = waagerecht nach aussen, PI = nach oben, negativ = vor den Koerper.
  armL: 1.0, armR: 1.0, lenL: 1, lenR: 1, bendL: 0, bendR: 0,
  /** Ueber 0.5 wird der Arm vor dem Koerper gezeichnet. */
  frontL: 0, frontR: 0,
  glow: 1, alarm: 0, think: 0,
  // Requisiten, jeweils 0..1 eingeblendet.
  phones: 0, laptop: 0, lens: 0, bell: 0,
  /** Ausschlag der Glocke in Radiant. */
  swing: 0,
};
type Pose = typeof POSE;
type Key = keyof Pose;
const KEYS = Object.keys(POSE) as Key[];

// Federhaerte und Daempfung je Wert: Koerper leicht federnd, Augen schnell, Arme weich nachschwingend.
const SPRING: Record<Key, [number, number]> = {
  x: [190, 17], y: [260, 19], sx: [240, 15], sy: [240, 15], rot: [170, 14], scale: [210, 17],
  lid: [1100, 50], eyeX: [520, 34], eyeY: [520, 34], eyeScale: [300, 22],
  smile: [320, 28], squint: [400, 30], rest: [200, 24], blush: [60, 14], mouth: [420, 26],
  armL: [105, 9], armR: [105, 9], lenL: [170, 14], lenR: [170, 14], bendL: [140, 11], bendR: [140, 11],
  frontL: [600, 44], frontR: [600, 44],
  glow: [55, 12], alarm: [90, 14], think: [140, 18],
  phones: [150, 13], laptop: [170, 15], lens: [190, 15], bell: [190, 14], swing: [420, 16],
};

class Spring {
  v = 0;
  target: number;
  constructor(public x: number, private k: number, private c: number) {
    this.target = x;
  }
  step(dt: number) {
    this.v += (-this.k * (this.x - this.target) - this.c * this.v) * dt;
    this.x += this.v * dt;
  }
  snap(x: number) {
    this.x = this.target = x;
    this.v = 0;
  }
}

interface Particle {
  kind: "note" | "z" | "spark" | "orb" | "ding" | "dust";
  x: number; y: number; vx: number; vy: number;
  life: number; max: number; size: number; spin: number; variant: number;
}

interface Ctx {
  /** Sekunden seit Beginn der Stimmung bzw. Geste. */
  t: number;
  /** Lautstaerke 0..1 (Mikrofon oder Musik). */
  level: number;
  /** Taktphase in Schlaegen. */
  beat: number;
  /** Blickrichtung zum Partner: -1 links, 1 rechts. */
  facing: number;
  /** Sprechimpuls 0..1, folgt den ankommenden Woertern. */
  talk: number;
  /** Laufrichtung: -1 links, 1 rechts. */
  dir: number;
  /** Schrittphase in Radiant; die Buehne dreht sie mit der zurueckgelegten Strecke weiter. */
  step: number;
  /** 0..1: wie dringend etwas ist (z. B. Timer kurz vor Ablauf). */
  urgency: number;
}

const MOODS: Record<Mood, (c: Ctx) => Partial<Pose>> = {
  idle: ({ t }) => ({
    // Ruhig stehen: atmen und sehr langsam das Gewicht verlagern.
    sy: 1 + Math.sin(t * 1.6) * 0.017,
    rot: Math.sin(t * 0.29 + 0.4) * 0.02,
    x: Math.sin(t * 0.29) * 0.014,
    armL: 1.0 + Math.sin(t * 1.6 + 2) * 0.06,
    armR: 1.0 + Math.sin(t * 1.6 + 2.6) * 0.06,
    glow: 0.95 + Math.sin(t * 0.9) * 0.04,
  }),
  listening: ({ t, level }) => {
    // Aufgerichtet, eine Hand am Ohr; der Koerper geht mit der Lautstaerke mit.
    const l = level > 0.01 ? level : (Math.sin(t * 4) + 1) * 0.08;
    return {
      sy: 1.04 + l * 0.05, rot: 0.075 + Math.sin(t * 0.9) * 0.015,
      eyeScale: 1.18 + l * 0.08, eyeY: -0.12,
      armR: 2.5 + l * 0.08, lenR: 0.84, bendR: 0.5,
      armL: 1.15 + l * 0.2,
      glow: 1.0 + l * 0.15,
    };
  },
  thinking: ({ t }) => {
    // Hand ans Kinn, Blick nach oben; alle paar Sekunden tippt der Finger zweimal.
    const tap = t % 3.2 < 0.5 ? Math.abs(Math.sin((t % 3.2) * TAU * 2)) : 0;
    return {
      rot: 0.06 + Math.sin(t * 1.1) * 0.025,
      eyeX: -0.45 + Math.sin(t * 0.7) * 0.3, eyeY: -0.7,
      armR: -0.95 - tap * 0.16, lenR: 0.92, frontR: 1, bendR: -0.1,
      armL: 0.55,
      glow: 0.92 + Math.sin(t * 2.2) * 0.06,
      think: 1,
    };
  },
  answering: ({ t, talk }) => {
    // Der Koerper pulsiert mit den ankommenden Woertern, die Arme gestikulieren ruhig mit.
    const beat = Math.max(talk, noise(t * 7) * 0.25);
    return {
      sy: 1 + beat * 0.045,
      rot: Math.sin(t * 1.3) * 0.03,
      armL: 1.1 + Math.sin(t * 2.3) * 0.3 + beat * 0.25,
      armR: 1.1 + Math.sin(t * 2.3 + 2.4) * 0.3 + beat * 0.25,
      eyeScale: 1.05, mouth: 0.22 + beat * 0.78,
      glow: 1.02 + beat * 0.07,
    };
  },
  working: ({ t }) => ({
    // Hinter einem kleinen Laptop: Blick nach unten auf den Bildschirm, die Haende tippen.
    sy: 0.97, rot: Math.sin(t * 2) * 0.01,
    eyeY: 0.62, eyeX: Math.sin(t * 2.7) * 0.3, lid: 0.8,
    armL: 0.2 + Math.max(0, Math.sin(t * 15)) * 0.22,
    armR: 0.2 + Math.max(0, Math.sin(t * 15 + Math.PI)) * 0.22,
    lenL: 0.92, lenR: 0.92,
    laptop: 1,
  }),
  searching: ({ t }) => {
    // Mit der Lupe absuchen: langsam von links nach rechts und zurueck.
    const scan = Math.sin(t * 1.5);
    return {
      rot: scan * 0.05, x: scan * 0.035, sy: 0.985,
      eyeX: scan * 0.45, eyeY: 0.12,
      armR: -0.5, frontR: 1, bendR: 0.08,
      armL: 0.75 + Math.sin(t * 3) * 0.05,
      lens: 1,
    };
  },
  fileHover: ({ t }) => ({
    // Arme zum Fangen hoch, grosse Augen nach oben, staunender Mund.
    sy: 1.09 + Math.sin(t * 5) * 0.012,
    eyeY: -0.75, eyeScale: 1.3, mouth: 0.75,
    armL: 2.45 + Math.sin(t * 5) * 0.08, armR: 2.45 + Math.sin(t * 5 + 1) * 0.08,
    lenL: 1.15, lenR: 1.15,
    glow: 1.08,
  }),
  fileOut: ({ t, facing }) => ({
    rot: 0.06 * facing, x: 0.04 * facing, eyeX: 0.6 * facing,
    armR: facing > 0 ? 1.57 + Math.sin(t * 3) * 0.05 : 0.7,
    armL: facing > 0 ? 0.7 : 1.57 + Math.sin(t * 3) * 0.05,
    lenR: facing > 0 ? 1.45 : 1, lenL: facing > 0 ? 1 : 1.45,
    glow: 1.04,
  }),
  timer: ({ t, urgency }) => {
    // Wiegt im Sekundentakt und schielt zur Anzeige; kurz vor Ablauf wird Milo hibbelig.
    const swing = Math.sin(t * Math.PI);
    const tick = Math.sign(swing) * Math.pow(Math.abs(swing), 0.6);
    const hop = Math.pow(Math.abs(swing), 3) * urgency;
    return {
      rot: tick * (0.04 + urgency * 0.025), eyeX: 0.3 + tick * 0.2,
      y: -hop * 0.09, sy: 1 + hop * 0.04,
      armL: 1.0 + hop * 0.7, armR: 1.0 + hop * 0.7,
      eyeScale: 1 + urgency * 0.12, glow: 0.96,
    };
  },
  ringing: ({ t }) => {
    // Drei Schlaege im Takt des Klingeltons (0, 0.14, 0.28 s), danach eine kurze Pause.
    const u = t % 1.2;
    const hit = (at: number) => bell(span(u, at - 0.03, at + 0.12));
    const ding = Math.max(hit(0), hit(0.14), hit(0.28));
    const fade = u < 0.36 ? 1 : Math.exp(-(u - 0.36) * 7);
    return {
      y: -ding * 0.075, sy: 1 + ding * 0.05,
      rot: -0.04 + Math.cos((Math.PI * u) / 0.14) * 0.02 * fade,
      armR: 2.1 + ding * 0.2, lenR: 1.15, bendR: -0.12,
      armL: 1.25 + ding * 0.45,
      eyeScale: 1.14 + ding * 0.08, mouth: 0.55 * fade,
      bell: 1, swing: Math.cos((Math.PI * u) / 0.14) * 0.55 * fade,
      glow: 1.0 + ding * 0.08,
    };
  },
  sleeping: ({ t }) => ({
    rot: -0.05,
    sy: 0.9 + Math.sin(t * 1.1) * 0.025,
    lid: 0, rest: 1,
    armL: 0.3, armR: 0.3, lenL: 0.75, lenR: 0.75,
    glow: 0.24 + Math.sin(t * 1.1) * 0.04,
  }),
  waking: ({ t }) => {
    // Gaehnen und Strecken, solange das Modell laedt; das Leuchten nimmt dabei langsam zu.
    const u = (t % 2.6) / 2.6;
    const stretch = bell(span(u, 0.05, 0.7));
    return {
      lid: 0.4 + (1 - stretch) * 0.25 + span(t, 4, 9) * 0.2,
      rest: stretch > 0.7 ? 1 : 0,
      sy: 1 + stretch * 0.13,
      rot: Math.sin(u * TAU) * 0.03,
      armL: 0.6 + stretch * 2.1, armR: 0.6 + stretch * 2.0,
      lenL: 1 + stretch * 0.2, lenR: 1 + stretch * 0.2,
      mouth: stretch > 0.25 ? 0.4 + stretch * 0.6 : 0,
      glow: 0.45 + (1 - Math.exp(-t / 3)) * 0.45,
    };
  },
  music: ({ t, beat, level }) => {
    // Wippt im Takt: auf den Schlag leicht in die Knie, dazwischen hoch.
    const down = Math.pow(Math.max(0, Math.cos(beat * TAU)), 2) * (0.7 + level * 0.6);
    const up = Math.pow(Math.max(0, -Math.cos(beat * TAU)), 2) * (0.7 + level * 0.6);
    return {
      y: -up * 0.05, sy: 1 - down * 0.065 + up * 0.02,
      rot: Math.sin(beat * Math.PI) * 0.07,
      x: Math.sin(beat * Math.PI) * 0.03,
      smile: t % 8 < 6.4 ? 1 : 0, blush: 0.5,
      armL: 0.95 + Math.sin(beat * TAU) * 0.3,
      armR: 0.95 + Math.sin(beat * TAU + Math.PI) * 0.3,
      phones: 1, glow: 1 + down * 0.08,
    };
  },
  collab: ({ t, facing }) => ({
    eyeX: 0.6 * facing, rot: 0.05 * facing,
    y: -Math.abs(Math.sin(t * 3)) * 0.03,
    armR: facing > 0 ? 1.4 + Math.sin(t * 4) * 0.3 : 0.7,
    armL: facing > 0 ? 0.7 : 1.4 + Math.sin(t * 4) * 0.3,
    glow: 1.06,
  }),
  walk: ({ step, dir }) => {
    // Watscheln: von einer Bodenkante auf die andere wiegen, die Arme pendeln gegengleich.
    const rock = Math.sin(step);
    const lift = Math.abs(rock);
    return {
      rot: dir * 0.05 + rock * 0.085,
      y: -lift * 0.045,
      sy: 1 + lift * 0.025 - (1 - lift) * 0.03,
      eyeX: dir * 0.6,
      armL: 0.9 + rock * 0.5, armR: 0.9 - rock * 0.5,
    };
  },
  dash: ({ t, dir }) => ({
    // Flitzen: weit nach vorn gelehnt und gestreckt, die Arme wehen hinterher.
    rot: dir * 0.3, sx: 1.1, sy: 0.93,
    y: -Math.abs(Math.sin(t * 26)) * 0.03,
    eyeX: dir * 0.85, lid: 0.72,
    armL: dir > 0 ? 1.75 : 0.25, armR: dir > 0 ? 0.25 : 1.75,
    lenL: dir > 0 ? 1.15 : 0.85, lenR: dir > 0 ? 0.85 : 1.15,
    bendL: dir > 0 ? -0.25 : 0, bendR: dir > 0 ? 0 : -0.25,
  }),
  sit: ({ t }) => ({
    // Hinhocken und ausruhen.
    sy: 0.9 + Math.sin(t * 1.2) * 0.012, lid: 0.72, eyeY: 0.15,
    rot: Math.sin(t * 0.4) * 0.015,
    armL: 0.55, armR: 0.55, lenL: 0.9, lenR: 0.9,
    glow: 0.86,
  }),
  doze: ({ t }) => {
    // Eingenickt: der Kopf sinkt langsam zur Seite und zuckt wieder hoch.
    const u = (t % 5.2) / 5.2;
    const droop = u < 0.88 ? ease(u / 0.88) : 1 - ease((u - 0.88) / 0.12);
    return {
      sy: 0.88 + Math.sin(t * 1.0) * 0.02, lid: 0, rest: 1,
      rot: droop * 0.11, x: droop * 0.02,
      armL: 0.4, armR: 0.4, lenL: 0.85, lenR: 0.85,
      glow: 0.58,
    };
  },
  push: ({ t, dir }) => {
    // Gegen die Wand stemmen.
    const strain = (Math.sin(t * 9) + 1) * 0.5;
    return {
      rot: dir * (0.17 + strain * 0.015), x: dir * 0.03, sy: 0.97,
      squint: 1, blush: 0.6,
      armL: dir > 0 ? 0.3 : 1.62, armR: dir > 0 ? 1.62 : 0.3,
      lenL: dir > 0 ? 0.9 : 1.2, lenR: dir > 0 ? 1.2 : 0.9,
    };
  },
};

const SHOTS: Record<OneShot, { dur: number; pose: (u: number, c: Ctx) => Partial<Pose> }> = {
  success: {
    // In die Knie, Sprung mit gestrecktem Koerper, weiche Landung.
    dur: 0.95,
    pose: (u) => {
      const crouch = bell(span(u, 0, 0.16));
      const air = bell(span(u, 0.14, 0.62));
      const land = bell(span(u, 0.6, 0.82));
      return {
        y: -0.34 * air,
        sy: 1 + air * 0.11 - crouch * 0.16 - land * 0.13,
        smile: u > 0.1 ? 1 : 0, blush: 0.7,
        armL: 0.5 + span(u, 0.1, 0.3) * 1.8, armR: 0.5 + span(u, 0.1, 0.3) * 1.8,
        lenL: 1.15, lenR: 1.15, glow: 1.12,
      };
    },
  },
  error: {
    // Kurzes Kopfschuetteln, dann sinken die Arme.
    dur: 0.9,
    pose: (u) => ({
      x: Math.sin(u * TAU * 3.5) * 0.12 * (1 - u),
      rot: Math.sin(u * TAU * 3.5) * 0.05 * (1 - u),
      squint: u < 0.7 ? 1 : 0, alarm: 1, armL: 0.25, armR: 0.25, sy: 0.95, glow: 1.06,
    }),
  },
  gulp: {
    // Datei gefangen: zusammenstauchen, hochfedern, zufrieden.
    dur: 0.8,
    pose: (u) => {
      const down = bell(span(u, 0, 0.3)), up = bell(span(u, 0.28, 0.7));
      return {
        sy: 1 - down * 0.22 + up * 0.14, y: -up * 0.12,
        smile: u > 0.3 ? 1 : 0, lid: u < 0.3 ? 0.1 : 1, blush: 0.6,
        armL: 0.6 + up * 1.6, armR: 0.6 + up * 1.6, glow: 1.1,
      };
    },
  },
  wave: {
    dur: 1.5,
    pose: (u) => {
      const up = span(u, 0, 0.18) * (1 - span(u, 0.85, 1));
      return {
        armR: 1.0 + up * (1.6 + Math.sin(u * TAU * 3.2) * 0.38), lenR: 1 + up * 0.18,
        rot: 0.06 * up, x: -0.02 * up, smile: up > 0.5 ? 1 : 0, blush: 0.4 * up, eyeX: 0.25 * up,
      };
    },
  },
  hop: {
    dur: 0.6,
    pose: (u) => {
      const crouch = bell(span(u, 0, 0.2)), air = bell(span(u, 0.18, 0.75)), land = bell(span(u, 0.72, 1));
      return { y: -0.17 * air, sy: 1 + air * 0.07 - crouch * 0.09 - land * 0.07 };
    },
  },
  jump: {
    // Absprung zu einer anderen Flaeche: kurz in die Knie, dann lang gestreckt mit Armen nach oben.
    dur: 0.6,
    pose: (u) => {
      const crouch = bell(span(u, 0, 0.22)), air = ease(span(u, 0.15, 0.4));
      return { sy: 1 - crouch * 0.14 + air * 0.08, armL: 0.6 + air * 1.6, armR: 0.6 + air * 1.6, eyeScale: 1 + air * 0.15, smile: air > 0.5 ? 1 : 0 };
    },
  },
  land: {
    // Aufkommen: zusammenstauchen, Arme zum Ausgleich zur Seite, dann zufrieden stehen.
    dur: 0.5,
    pose: (u) => {
      const squash = bell(span(u, 0, 0.45)), settle = bell(span(u, 0.35, 0.8));
      return { sy: 1 - squash * 0.2 + settle * 0.05, armL: 1 + squash * 0.9, armR: 1 + squash * 0.9, lid: squash > 0.6 ? 0.4 : 1 };
    },
  },
  stretch: {
    // Gaehnen: Augen zu, Mund auf, lang machen, Arme hoch.
    dur: 1.9,
    pose: (u) => {
      const s = bell(u);
      return {
        sy: 1 + s * 0.13, lid: 1 - s * 0.95, rest: s > 0.5 ? 1 : 0, mouth: s > 0.3 ? s : 0,
        armL: 1.0 + s * 1.8, armR: 1.0 + s * 1.65, lenL: 1 + s * 0.25, lenR: 1 + s * 0.25,
        rot: Math.sin(u * TAU) * 0.035,
      };
    },
  },
  lookAround: {
    // Neugierig erst nach links, dann nach rechts schauen.
    dur: 2.4,
    pose: (u) => {
      const dir = u < 0.45 ? -1 : u < 0.9 ? 1 : 0;
      return { eyeX: dir * 0.9, rot: dir * 0.045, x: dir * 0.025, eyeScale: 1.1 };
    },
  },
  tilt: {
    // Kopf schief legen wie bei einer Frage.
    dur: 1.6,
    pose: (u) => {
      const s = ease(span(u, 0, 0.2)) * (1 - ease(span(u, 0.78, 1)));
      return { rot: 0.16 * s, x: 0.02 * s, eyeY: -0.25 * s, eyeScale: 1 + 0.15 * s, armL: 1.0 + 0.4 * s };
    },
  },
  perk: {
    // Aufhorchen: kurzer Ruck nach oben.
    dur: 0.38,
    pose: (u) => ({ y: -0.08 * bell(u), sy: 1 + 0.09 * bell(u), eyeScale: 1 + 0.3 * bell(u), armL: 1.55, armR: 1.55 }),
  },
  nod: {
    dur: 0.45,
    pose: (u) => ({ sy: 1 - 0.07 * bell(u), eyeY: 0.4 * bell(u) }),
  },
  appear: {
    dur: 0.75,
    pose: (u) => ({ rot: -1.2 * (1 - ease(u)), glow: 1.2 - u * 0.2, armL: 1.6, armR: 1.6, eyeScale: 1.2 }),
  },
  vanish: {
    dur: 0.7,
    pose: (u, { facing }) => ({
      scale: u < 0.45 ? 1 + u * 0.15 : Math.max(0, 1 - ease(span(u, 0.45, 1))),
      armR: facing > 0 ? 2.5 + Math.sin(u * 30) * 0.3 : 0.7,
      armL: facing > 0 ? 0.7 : 2.5 + Math.sin(u * 30) * 0.3,
      smile: 1, glow: 1 + u * 0.25, rot: u > 0.45 ? (u - 0.45) * 2 : 0,
    }),
  },
  handoff: {
    dur: 0.9,
    pose: (u, { facing }) => {
      const reach = bell(u);
      return {
        x: 0.06 * facing * reach, eyeX: 0.7 * facing,
        armR: facing > 0 ? 1.57 : 0.7, lenR: facing > 0 ? 1 + reach * 0.5 : 1,
        armL: facing > 0 ? 0.7 : 1.57, lenL: facing > 0 ? 1 : 1 + reach * 0.5,
      };
    },
  },
  kick: {
    // Ausholen und den Ball wegstupsen.
    dur: 0.45,
    pose: (u, { dir }) => {
      const back = bell(span(u, 0, 0.4)), forth = bell(span(u, 0.28, 1));
      return {
        rot: -dir * 0.13 * back + dir * 0.2 * forth, x: dir * 0.12 * forth,
        y: -0.06 * forth, sy: 1 - 0.06 * back + 0.05 * forth,
        eyeX: dir * 0.7, armL: 1.45, armR: 1.45,
      };
    },
  },
  startle: {
    // Aufschrecken: Satz nach oben mit grossen Augen.
    dur: 0.6,
    pose: (u) => {
      const s = bell(span(u, 0, 0.6));
      return {
        y: -0.2 * s, sy: 1 + 0.12 * s, lid: 1, eyeScale: 1 + 0.45 * (1 - u), mouth: 0.7 * (1 - u),
        armL: 1.0 + 1.4 * s, armR: 1.0 + 1.4 * s,
      };
    },
  },
  giggle: {
    // Angestupst: kichernd hin und her wackeln.
    dur: 0.85,
    pose: (u) => ({
      rot: Math.sin(u * TAU * 4) * 0.07 * (1 - u),
      y: -Math.abs(Math.sin(u * TAU * 4)) * 0.045 * (1 - u),
      smile: 1, blush: 0.9, armL: 1.35, armR: 1.35,
    }),
  },
  shrug: {
    // Schulterzucken: "weiss ich nicht" oder "dann eben nicht".
    dur: 1.1,
    pose: (u) => {
      const s = ease(span(u, 0, 0.25)) * (1 - ease(span(u, 0.7, 1)));
      return {
        armL: 1.0 + s * 0.8, armR: 1.0 + s * 0.8, bendL: s * 0.4, bendR: s * 0.4,
        sy: 1 - s * 0.035, rot: s * 0.04, eyeY: -0.15 * s, lid: 1 - s * 0.25,
      };
    },
  },
};

/** Stimmungen, in denen von selbst geblinzelt wird. */
const BLINKS: ReadonlySet<Mood> = new Set([
  "idle", "answering", "timer", "fileOut", "collab", "working", "searching", "listening", "walk", "sit", "thinking", "ringing",
]);
/** Stimmungen, in denen der Blick einem Ziel (Mauszeiger) folgen darf. */
const GAZES: ReadonlySet<Mood> = new Set(["idle", "answering", "timer", "listening", "sit", "walk", "ringing"]);
/** Stimmungen, die volle Bildrate brauchen; alle anderen kommen mit der halben aus. */
const LIVELY: ReadonlySet<Mood> = new Set([
  "listening", "answering", "working", "searching", "fileHover", "ringing", "music", "waking", "collab", "walk", "dash", "push",
]);
/** Kleine Leerlauf-Gesten mit Gewichtung: unauffaellige oefter, grosse selten. */
const IDLE_GESTURES: [OneShot, number][] = [["lookAround", 3], ["tilt", 3], ["hop", 2], ["stretch", 1.5], ["wave", 1]];

let squirclePath: Path2D | null = null;
/** Einheits-Squircle (Superellipse) mit Kantenlaenge 1: weicher als ein Rechteck mit runden Ecken. */
function squircle(): Path2D {
  if (squirclePath) return squirclePath;
  const path = new Path2D();
  const n = 4.4, steps = 72;
  for (let i = 0; i <= steps; i++) {
    const a = (i / steps) * TAU;
    const c = Math.cos(a), s = Math.sin(a);
    const x = 0.5 * Math.sign(c) * Math.pow(Math.abs(c), 2 / n);
    const y = 0.5 * Math.sign(s) * Math.pow(Math.abs(s), 2 / n);
    if (i === 0) path.moveTo(x, y);
    else path.lineTo(x, y);
  }
  path.closePath();
  return (squirclePath = path);
}

export interface CharacterOptions {
  armStyle?: ArmStyle;
  /** Farbverlauf fuer den zweiten Wuerfel (Cloud-Helfer); ohne Angabe weiss. */
  accent?: [string, string] | null;
  /** Farbe von Kopfhoerern, Bildschirmlicht und Noten. */
  detail?: string;
}

export class Character {
  mood: Mood = "idle";
  level = 0;
  bpm = 112;
  facing = 1;
  /** Laufrichtung und Schrittphase; die Buehne setzt beide beim Gehen. */
  dir = 1;
  step = 0;
  urgency = 0;
  armStyle: ArmStyle;
  /** Blickziel von aussen (z. B. Mauszeiger), Werte -1..1; null = automatisch. */
  gaze: { x: number; y: number } | null = null;
  /** Kleine Leerlauf-Gesten (Umschauen, Hopser, Strecken, Winken) von selbst einstreuen. */
  idleGestures = true;

  private s = {} as Record<Key, Spring>;
  private time = 0;
  private moodStart = 0;
  private shot: { kind: OneShot; start: number } | null = null;
  private particles: Particle[] = [];
  private nextBlink = 2;
  private blinkUntil = 0;
  private glance = { x: 0, y: 0, until: 0, next: 4 };
  private nextGesture = 18;
  private nextEmit = 0;
  private talkLevel = 0;
  private accent: [string, string] | null;
  private detail: string;
  private glowSprite: HTMLCanvasElement | null = null;
  private glowFor = "";

  constructor(options: CharacterOptions = {}) {
    this.armStyle = options.armStyle ?? "capsule";
    this.accent = options.accent ?? null;
    this.detail = options.detail ?? "#64d2ff";
    for (const key of KEYS) this.s[key] = new Spring(POSE[key], ...SPRING[key]);
  }

  /** Farbverlauf des Koerpers; null = weiss. */
  setAccent(accent: [string, string] | null) {
    this.accent = accent;
  }

  setMood(mood: Mood) {
    if (mood === this.mood) return;
    const from = this.mood;
    this.mood = mood;
    this.moodStart = this.time;
    this.nextEmit = this.time;
    const moving = (m: Mood) => m === "walk" || m === "dash";
    // Kleine Uebergangsgesten machen den Wechsel lesbar.
    if (mood === "listening" || (from === "thinking" && mood === "answering")) this.play("perk");
    else if (mood === "idle" && (from === "answering" || from === "working" || from === "searching")) this.play("nod");
    else if (from === "doze" && !moving(mood) && mood !== "sit" && mood !== "sleeping") this.play("startle");
    if (mood === "idle") this.nextGesture = this.time + 14 + Math.random() * 14;
  }

  /** Spielt eine einmalige Geste; liefert ihre Dauer in Sekunden. */
  play(kind: OneShot): number {
    this.shot = { kind, start: this.time };
    if (kind === "appear") this.s.scale.snap(0);
    if (kind === "success") this.burst(7);
    if (kind === "appear" || kind === "vanish") this.burst(6);
    if (kind === "handoff") {
      this.particles.push({ kind: "orb", x: 0.55 * this.facing, y: 0.05, vx: 1.6 * this.facing, vy: -0.25, life: 0, max: 0.8, size: 0.11, spin: 0, variant: 0 });
    }
    return SHOTS[kind].dur;
  }

  /** Laeuft gerade eine einmalige Geste? */
  get busy() {
    return this.shot !== null;
  }

  /** Ein Wort wurde ausgegeben: laesst den Koerper beim Antworten im Sprechtakt pulsieren. */
  talk() {
    this.talkLevel = Math.min(1, this.talkLevel + 0.55);
  }

  /** True, solange Bewegung sichtbar ist, die volle Bildrate verdient. */
  get lively() {
    return this.shot !== null || LIVELY.has(this.mood) || this.particles.length > 0;
  }

  /** Hoehe ueber dem Boden in Koerpergroessen (0 = steht). */
  get airborne() {
    return Math.max(0, -this.s.y.x);
  }

  update(dt: number) {
    dt = clamp(dt, 0, 0.1);
    this.time += dt;
    this.talkLevel *= Math.exp(-dt / 0.13);
    const ctx: Ctx = {
      t: this.time - this.moodStart, level: this.level, beat: (this.time * this.bpm) / 60,
      facing: this.facing, talk: this.talkLevel, dir: this.dir, step: this.step, urgency: this.urgency,
    };

    const moodPose = MOODS[this.mood](ctx);
    const target: Pose = { ...POSE, ...moodPose };
    let custom: Partial<Pose> = moodPose;

    this.autoEyes(target);
    if (this.mood === "idle" && this.idleGestures && !this.shot && this.time > this.nextGesture) {
      this.play(pickWeighted(IDLE_GESTURES));
      this.nextGesture = this.time + 16 + Math.random() * 22;
    }

    if (this.shot) {
      const def = SHOTS[this.shot.kind];
      const u = (this.time - this.shot.start) / def.dur;
      if (u >= 1) {
        this.shot = null;
      } else {
        const shotPose = def.pose(u, { ...ctx, t: u * def.dur });
        Object.assign(target, shotPose);
        custom = { ...moodPose, ...shotPose };
      }
    }

    // Volumen erhalten: wer gestaucht wird, wird breiter (sofern die Pose nichts anderes vorgibt).
    if (custom.sx === undefined) target.sx = 1 + (1 - target.sy) * 0.62;
    // Der Koerper kann nicht in den Boden sinken.
    target.y = Math.min(0, target.y);

    // Nachschwingen: Arme folgen der Koerperbewegung mit Verzoegerung und biegen sich dabei.
    const lift = clamp(this.s.y.v * 0.5, -0.55, 0.55);
    const twist = clamp(this.s.rot.v * 0.22, -0.35, 0.35);
    // Begrenzen, damit die Arme nie ueber den Kopf hinweg hinter dem Koerper verschwinden.
    target.armL = clamp(target.armL + lift - twist, -1.5, 2.6);
    target.armR = clamp(target.armR + lift + twist, -1.5, 2.6);
    target.bendL += clamp(-this.s.armL.v * 0.04, -0.3, 0.3);
    target.bendR += clamp(-this.s.armR.v * 0.04, -0.3, 0.3);

    for (const key of KEYS) this.s[key].target = target[key];
    // Kleine Teilschritte halten die Federn auch bei Rucklern stabil.
    for (let left = dt; left > 1e-6; left -= 1 / 120) {
      const step = Math.min(left, 1 / 120);
      for (const key of KEYS) this.s[key].step(step);
    }
    // Die Feder darf beim Landen nicht durch den Boden schwingen.
    if (this.s.y.x > 0) {
      this.s.y.x = 0;
      this.s.y.v = Math.min(0, this.s.y.v);
    }

    this.emitParticles(ctx);
    for (const p of this.particles) {
      p.life += dt;
      p.x += p.vx * dt;
      p.y += p.vy * dt;
      if (p.kind === "spark") {
        p.vx *= Math.exp(-dt * 3);
        p.vy = p.vy * Math.exp(-dt * 3) + 0.5 * dt;
      } else if (p.kind === "note") {
        p.x += Math.sin(p.life * 4 + p.spin * 9) * 0.12 * dt;
      } else if (p.kind === "dust") {
        p.vx *= Math.exp(-dt * 4);
      }
    }
    this.particles = this.particles.filter((p) => p.life < p.max);
  }

  private autoEyes(target: Pose) {
    if (BLINKS.has(this.mood)) {
      if (this.time > this.nextBlink) {
        this.blinkUntil = this.time + 0.09;
        // Gelegentlich doppelt blinzeln, das wirkt lebendiger als ein starrer Takt.
        this.nextBlink = this.time + (Math.random() < 0.16 ? 0.26 : 2.6 + Math.random() * 4);
      }
      if (this.time < this.blinkUntil) target.lid = 0;
    }
    if (!GAZES.has(this.mood)) return;

    if (this.gaze) {
      // Beim Gehen schaut Milo vor allem nach vorn.
      const weight = this.mood === "walk" ? 0.35 : 0.85;
      target.eyeX += clamp(this.gaze.x, -1, 1) * weight;
      target.eyeY += clamp(this.gaze.y, -1, 1) * weight;
    } else if ((this.mood === "idle" || this.mood === "sit") && !this.shot) {
      if (this.time > this.glance.next) {
        this.glance = {
          x: (Math.random() * 2 - 1) * 0.75, y: (Math.random() * 2 - 1) * 0.4,
          until: this.time + 0.7 + Math.random() * 1.3,
          next: this.time + 3 + Math.random() * 5,
        };
        // Beim Blickwechsel blinzeln Lebewesen oft.
        if (Math.random() < 0.4) this.blinkUntil = this.time + 0.09;
      }
      if (this.time < this.glance.until) {
        target.eyeX += this.glance.x;
        target.eyeY += this.glance.y;
      }
    }
  }

  private burst(count: number) {
    for (let i = 0; i < count; i++) {
      const a = (i / count) * TAU + Math.random() * 0.5;
      const speed = 1.5 + Math.random() * 1.1;
      this.particles.push({
        kind: "spark", x: Math.cos(a) * 0.45, y: Math.sin(a) * 0.45, vx: Math.cos(a) * speed, vy: Math.sin(a) * speed,
        life: 0, max: 0.55 + Math.random() * 0.3, size: 0.045 + Math.random() * 0.04, spin: Math.random() * TAU, variant: 0,
      });
    }
  }

  // Partikel liegen in Koerpergroessen relativ zur Koerpermitte der Ruhelage.
  private emitParticles(ctx: Ctx) {
    if (this.time < this.nextEmit) return;
    const add = (p: Partial<Particle> & Pick<Particle, "kind" | "x" | "y" | "max" | "size">) =>
      this.particles.push({ vx: 0, vy: 0, life: 0, spin: 0, variant: 0, ...p });
    switch (this.mood) {
      case "music": {
        const side = Math.random() < 0.5 ? -1 : 1;
        add({ kind: "note", x: side * 0.62, y: -0.42, vx: side * 0.16, vy: -0.4, max: 1.5, size: 0.2, spin: (Math.random() - 0.5) * 0.5, variant: Math.random() < 0.4 ? 1 : 0 });
        this.nextEmit = this.time + (60 / this.bpm) * 2;
        break;
      }
      case "sleeping":
      case "doze":
        add({ kind: "z", x: 0.42, y: -0.48, vx: 0.11, vy: -0.2, max: 2.6, size: this.mood === "doze" ? 0.13 : 0.16 });
        this.nextEmit = this.time + 2.4;
        break;
      case "collab":
        add({ kind: "spark", x: 0.78 * this.facing, y: -0.25 + Math.random() * 0.35, vy: -0.3, max: 0.7, size: 0.05, spin: Math.random() * TAU });
        this.nextEmit = this.time + 0.35;
        break;

      case "ringing": {
        // Zu jedem Glockenschlag ein Klangbogen links und rechts der Glocke.
        const u = ctx.t % 1.2;
        if (u < 0.34) {
          for (const side of [-1, 1]) add({ kind: "ding", x: 0.74 + side * 0.2, y: -0.02, vx: side * 0.5, max: 0.34, size: 0.13, variant: side });
          this.nextEmit = this.time + 0.14;
        } else {
          this.nextEmit = this.time + (1.2 - u);
        }
        break;
      }
      case "dash":
        add({ kind: "dust", x: -this.dir * 0.45, y: 0.46, vx: -this.dir * 0.9, vy: -0.25, max: 0.38, size: 0.07 + Math.random() * 0.04 });
        this.nextEmit = this.time + 0.06;
        break;
      default:
        this.nextEmit = this.time + 0.3;
    }
  }

  // ---------- Zeichnen ----------

  private glow(size: number, color: string): HTMLCanvasElement {
    const id = `${size}|${color}`;
    if (this.glowSprite && this.glowFor === id) return this.glowSprite;
    // Bewusst zurueckhaltend: ein weicher Schein knapp um den Koerper, keine grosse Lichtwolke.
    const r = Math.ceil(size * 1.08);
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = r * 2;
    const g = canvas.getContext("2d")!;
    const grad = g.createRadialGradient(r, r, size * 0.42, r, r, r);
    grad.addColorStop(0, color + "4d");
    grad.addColorStop(0.4, color + "1a");
    grad.addColorStop(1, color + "00");
    g.fillStyle = grad;
    g.fillRect(0, 0, r * 2, r * 2);
    this.glowSprite = canvas;
    this.glowFor = id;
    return canvas;
  }

  /**
   * Zeichnet den Charakter: `x` ist die Mitte, `floor` die Bodenlinie, `size` die Koerpergroesse
   * in Pixeln. In Ruhe steht die Unterkante des Koerpers genau auf dem Boden.
   */
  draw(g: CanvasRenderingContext2D, x: number, floor: number, size: number) {
    const p = {} as Pose;
    for (const key of KEYS) p[key] = this.s[key].x;
    const S = size * Math.max(0, p.scale);
    // Partikel haengen an der Ruhelage, nicht am huepfenden Koerper.
    const restX = x, restY = floor - size / 2;
    if (S < 0.5) {
      this.drawParticles(g, restX, restY, size);
      return;
    }
    const alarm = clamp(p.alarm, 0, 1);
    const glowColor = alarm > 0.5 ? "#ff9d96" : this.accent ? this.accent[0] : "#ffffff";
    const air = Math.max(0, -p.y);
    // Beim Wiegen kippt der Wuerfel ueber seine Bodenkante und hebt sich dabei minimal.
    const tip = Math.abs(Math.sin(p.rot)) * 0.3;

    // Bodenlicht: ein Hauch Widerschein unter dem Koerper verankert ihn; im Sprung verblasst es.
    const contact = clamp(p.glow, 0, 1.3) * Math.max(0, 1 - air * 3.2);
    if (contact > 0.02) {
      g.save();
      g.translate(x + p.x * S, floor + 0.5);
      g.scale(1, 0.14);
      const light = g.createRadialGradient(0, 0, 0, 0, 0, S * 0.66);
      light.addColorStop(0, `rgba(255, 255, 255, ${0.2 * contact})`);
      light.addColorStop(1, "rgba(255, 255, 255, 0)");
      g.fillStyle = light;
      g.beginPath();
      g.arc(0, 0, S * 0.66, 0, TAU);
      g.fill();
      g.restore();
    }

    g.save();
    g.translate(x + p.x * S, floor + (p.y - tip) * S);
    g.rotate(p.rot);
    g.scale(p.sx, p.sy);
    g.translate(0, -S / 2);
    // Ab hier ist die Koerpermitte der Ursprung.

    // Leuchten: vorgerendertes Bild statt Weichzeichner, das kostet pro Bild fast nichts.
    const sprite = this.glow(Math.round(size), glowColor);
    g.globalAlpha = clamp(p.glow, 0, 1.6) / 1.6;
    const gs = (sprite.width * S) / size;
    g.drawImage(sprite, -gs / 2, -gs / 2, gs, gs);
    g.globalAlpha = 1;

    const frontL = p.frontL > 0.5, frontR = p.frontR > 0.5;
    const left = this.arm(S, -1, p.armL, p.lenL, p.bendL);
    const right = this.arm(S, 1, p.armR, p.lenR, p.bendR);
    if (!frontL) this.drawArm(g, S, left, alarm, false);
    if (!frontR) this.drawArm(g, S, right, alarm, false);

    this.drawBody(g, S, alarm, p.laptop);
    this.drawFace(g, S, p);
    if (p.phones > 0.02) this.drawHeadphones(g, S, p.phones);
    if (p.laptop > 0.02) this.drawLaptop(g, S, p.laptop);

    if (frontL) this.drawArm(g, S, left, alarm, true);
    if (frontR) this.drawArm(g, S, right, alarm, true);
    if (p.lens > 0.02) this.drawLens(g, S, p, right);
    if (p.bell > 0.02) this.drawBell(g, S, p.bell, p.swing, right);
    g.restore();

    // Mitte des Koerpers in Bildkoordinaten, fuer alles, was ueber dem Kopf schwebt.
    const half = (S / 2) * p.sy;
    const bodyX = x + p.x * S + Math.sin(p.rot) * half;
    const bodyY = floor + (p.y - tip) * S - Math.cos(p.rot) * half;
    if (p.think > 0.02) this.drawThinking(g, bodyX, bodyY, S, p.think);
    this.drawParticles(g, restX, restY, size);
  }

  /** Koerperfarbe: weiss mit kaum sichtbarem Verlauf nach unten, das gibt dem Wuerfel etwas Volumen. */
  private bodyColors(alarm: number): [string, string] {
    if (this.accent) return this.accent;
    const warm = Math.round(255 - alarm * 34);
    return [`rgb(255, ${warm}, ${warm})`, `rgb(${Math.round(232 + alarm * 14)}, ${Math.round(233 - alarm * 34)}, ${Math.round(240 - alarm * 38)})`];
  }

  private drawBody(g: CanvasRenderingContext2D, S: number, alarm: number, laptop: number) {
    const [top, bottom] = this.bodyColors(alarm);
    g.save();
    g.scale(S, S);
    const grad = this.accent ? g.createLinearGradient(-0.5, -0.5, 0.5, 0.5) : g.createLinearGradient(0, -0.5, 0, 0.5);
    grad.addColorStop(this.accent ? 0 : 0.35, top);
    grad.addColorStop(1, bottom);
    g.fillStyle = grad;
    g.fill(squircle());
    if (laptop > 0.02) {
      // Der Bildschirm scheint von unten sanft ins Gesicht.
      g.clip(squircle());
      const shine = g.createRadialGradient(0, 0.42, 0, 0, 0.42, 0.62);
      shine.addColorStop(0, this.detail + "55");
      shine.addColorStop(1, this.detail + "00");
      // Das Licht flackert leicht, als wuerde sich auf dem Bildschirm etwas tun.
      g.globalAlpha = clamp(laptop, 0, 1) * (0.72 + 0.28 * noise(this.time * 9));
      g.fillStyle = shine;
      g.fillRect(-0.5, -0.5, 1, 1);
    }
    g.restore();
  }

  /** Lage eines Arms: Schulter, Kruemmungspunkt und Hand. */
  private arm(S: number, side: number, angle: number, len: number, bend: number) {
    const shoulderX = side * S * 0.43, shoulderY = S * 0.05;
    const reach = S * (this.armStyle === "spring" ? 0.36 : 0.31) * Math.max(0.25, len);
    const dx = side * Math.sin(angle), dy = Math.cos(angle);
    const handX = shoulderX + dx * reach, handY = shoulderY + dy * reach;
    // Leichte Biegung: waagerechte Arme haengen minimal durch, dazu kommt das Nachschwingen.
    const curve = (bend + 0.1 * Math.sin(angle)) * reach;
    return {
      side, dx, dy, shoulderX, shoulderY, handX, handY,
      ctrlX: (shoulderX + handX) / 2 - dy * side * curve,
      ctrlY: (shoulderY + handY) / 2 + dx * side * curve,
    };
  }

  private drawArm(g: CanvasRenderingContext2D, S: number, a: ReturnType<Character["arm"]>, alarm: number, front: boolean) {
    const style = this.armStyle;
    // Arme hinter dem Koerper bekommen den Farbton, den der Koerper auf Schulterhoehe hat.
    const warm = Math.round(255 - alarm * 34);
    const color = this.accent
      ? a.side < 0 ? this.accent[0] : this.accent[1]
      : front ? this.bodyColors(alarm)[0] : `rgb(${Math.round(246 + alarm * 6)}, ${warm - 9}, ${warm - 5})`;
    const stroke = (width: number, paint: string) => {
      g.strokeStyle = paint;
      g.lineWidth = width;
      g.lineCap = "round";
      g.beginPath();
      g.moveTo(a.shoulderX - a.side * S * 0.05, a.shoulderY);
      g.quadraticCurveTo(a.ctrlX, a.ctrlY, a.handX, a.handY);
      g.stroke();
    };
    const hand = (radius: number, paint: string, ox = 0, oy = 0) => {
      g.fillStyle = paint;
      g.beginPath();
      g.arc(a.handX + ox, a.handY + oy, radius, 0, TAU);
      g.fill();
    };
    // Liegt die Hand vor dem Koerper, trennt ein hauchduenner Schatten Weiss von Weiss.
    const shade = "rgba(20, 20, 35, 0.16)";

    if (style === "capsule") {
      if (front) {
        stroke(S * 0.2, shade);
        hand(S * 0.115, shade);
      }
      stroke(S * 0.16, color);
      hand(S * 0.094, color);
    } else if (style === "spring") {
      if (front) hand(S * 0.105, shade);
      stroke(S * 0.05, color);
      hand(S * 0.085, color);
    } else {
      const gapX = a.dx * S * 0.1, gapY = a.dy * S * 0.1;
      if (front) hand(S * 0.12, shade, gapX, gapY);
      hand(S * 0.1, color, gapX, gapY);
    }
  }

  private drawFace(g: CanvasRenderingContext2D, S: number, p: Pose) {
    const lid = clamp(p.lid, 0, 1);
    const smile = clamp(p.smile, 0, 1);
    const squint = clamp(p.squint, 0, 1);
    const blush = clamp(p.blush, 0, 1);
    const mouth = clamp(p.mouth, 0, 1);
    const ink = "#0c0c10";
    const gx = clamp(p.eyeX, -1.2, 1.2) * S * 0.07;
    const gy = clamp(p.eyeY, -1.2, 1.2) * S * 0.06;

    if (blush > 0.02) {
      g.fillStyle = `rgba(255, 122, 142, ${0.3 * blush})`;
      for (const side of [-1, 1]) {
        g.beginPath();
        g.ellipse(side * S * 0.29 + gx * 0.4, S * 0.1 + gy * 0.4, S * 0.075, S * 0.045, 0, 0, TAU);
        g.fill();
      }
    }

    g.fillStyle = g.strokeStyle = ink;
    g.lineCap = "round";
    g.lineJoin = "round";
    for (const side of [-1, 1]) {
      // Hinter der Lupe wirkt das Auge groesser.
      const magnify = side > 0 ? 1 + clamp(p.lens, 0, 1) * 0.5 : 1;
      const rx = S * 0.052 * p.eyeScale * magnify, ry = S * 0.068 * p.eyeScale * magnify;
      const ex = side * S * 0.17 + gx;
      const ey = -S * 0.05 + gy;
      if (squint > 0.5) {
        // Zusammengekniffen: > <
        g.lineWidth = S * 0.045;
        g.beginPath();
        g.moveTo(ex + side * rx * 1.15, ey - ry * 0.85);
        g.lineTo(ex - side * rx * 0.75, ey);
        g.lineTo(ex + side * rx * 1.15, ey + ry * 0.85);
        g.stroke();
      } else if (smile > 0.5) {
        // Froehlich: nach oben gewoelbte Boegen
        g.lineWidth = S * 0.046;
        g.beginPath();
        g.arc(ex, ey + ry * 0.75, rx * 1.35, Math.PI * 1.14, Math.PI * 1.86);
        g.stroke();
      } else if (lid < 0.16) {
        g.lineWidth = S * 0.042;
        g.beginPath();
        if (p.rest > 0.5) {
          // Schlafend: sanft nach unten gewoelbte Boegen
          g.arc(ex, ey - ry * 0.55, rx * 1.3, Math.PI * 0.16, Math.PI * 0.84);
        } else {
          g.moveTo(ex - rx * 1.1, ey + ry * 0.2);
          g.lineTo(ex + rx * 1.1, ey + ry * 0.2);
        }
        g.stroke();
      } else {
        // Offenes Auge: hochkantes Oval, das Lid schneidet es von oben gerade an.
        const top = ey - ry + (1 - lid) * ry * 1.7;
        g.save();
        g.beginPath();
        g.rect(ex - rx * 2, top, rx * 4, ry * 3);
        g.clip();
        g.beginPath();
        g.ellipse(ex, ey, rx, ry, 0, 0, TAU);
        g.fill();
        // Kleiner Lichtpunkt, der das Auge lebendig macht (bei sehr kleiner Darstellung weggelassen).
        if (lid > 0.6 && S >= 26) {
          g.fillStyle = "rgba(255, 255, 255, 0.92)";
          g.beginPath();
          g.arc(ex + rx * 0.3, ey - ry * 0.36, rx * 0.33, 0, TAU);
          g.fill();
          g.fillStyle = ink;
        }
        g.restore();
      }
    }

    if (mouth > 0.04) {
      // Kleiner runder Mund, der sich mit dem Sprechen oeffnet.
      g.fillStyle = ink;
      g.beginPath();
      g.ellipse(gx * 0.7, S * 0.115 + gy * 0.5, S * (0.034 + mouth * 0.022), S * (0.012 + mouth * 0.05), 0, 0, TAU);
      g.fill();
    }
  }

  private drawHeadphones(g: CanvasRenderingContext2D, S: number, amount: number) {
    const a = clamp(amount, 0, 1.12);
    g.save();
    // Kopfhoerer gleiten von oben auf den Kopf.
    g.translate(0, -(1 - a) * S * 0.5);
    g.globalAlpha = clamp(amount * 1.6, 0, 1);
    g.strokeStyle = this.detail;
    g.lineCap = "round";
    g.lineWidth = S * 0.07;
    g.beginPath();
    g.arc(0, S * 0.02, S * 0.585, Math.PI * 1.1, Math.PI * 1.9);
    g.stroke();
    for (const side of [-1, 1]) {
      g.fillStyle = this.detail;
      g.beginPath();
      g.roundRect(side * S * 0.555 - S * 0.085, -S * 0.2, S * 0.17, S * 0.37, S * 0.075);
      g.fill();
      // Hellerer Streifen als Polster-Andeutung
      g.fillStyle = "rgba(255, 255, 255, 0.28)";
      g.beginPath();
      g.roundRect(side * S * 0.555 - S * 0.085 + (side < 0 ? S * 0.105 : S * 0.02), -S * 0.14, S * 0.045, S * 0.25, S * 0.02);
      g.fill();
    }
    g.restore();
  }

  /** Aufgeklappter Laptop von hinten: Milo schaut ueber den Deckel. */
  private drawLaptop(g: CanvasRenderingContext2D, S: number, amount: number) {
    const a = clamp(amount, 0, 1.1);
    const w = S * 0.82, h = S * 0.4;
    g.save();
    // Nur oberhalb des Bodens sichtbar: der Laptop klappt von unten ins Bild.
    g.beginPath();
    g.rect(-S, -S * 2, S * 2, S * 2.5);
    g.clip();
    g.translate(0, S * 0.5 - h * a);
    g.fillStyle = "#2a2a2e";
    g.beginPath();
    g.roundRect(-w / 2, 0, w, h + S * 0.1, S * 0.07);
    g.fill();
    // Lichtkante oben und ein kleines leuchtendes Zeichen auf dem Deckel
    g.fillStyle = "rgba(255, 255, 255, 0.16)";
    g.beginPath();
    g.roundRect(-w / 2 + S * 0.03, S * 0.012, w - S * 0.06, S * 0.022, S * 0.011);
    g.fill();
    g.globalAlpha = 0.65 + 0.35 * Math.sin(this.time * 3);
    g.fillStyle = this.detail;
    g.beginPath();
    g.arc(0, h * 0.52, S * 0.045, 0, TAU);
    g.fill();
    g.restore();
  }

  /** Lupe vor dem rechten Auge; der Griff fuehrt zur Hand. */
  private drawLens(g: CanvasRenderingContext2D, S: number, p: Pose, hand: ReturnType<Character["arm"]>) {
    const a = clamp(p.lens, 0, 1.1);
    const cx = S * 0.17 + clamp(p.eyeX, -1.2, 1.2) * S * 0.07;
    const cy = -S * 0.05 + clamp(p.eyeY, -1.2, 1.2) * S * 0.06;
    const r = S * 0.17 * a;
    const dx = hand.handX - cx, dy = hand.handY - cy;
    const dist = Math.hypot(dx, dy) || 1;
    g.save();
    g.globalAlpha = clamp(p.lens * 1.5, 0, 1);
    g.lineCap = "round";
    // Griff
    g.strokeStyle = "#8e8e93";
    g.lineWidth = S * 0.065;
    g.beginPath();
    g.moveTo(cx + (dx / dist) * r, cy + (dy / dist) * r);
    g.lineTo(hand.handX, hand.handY);
    g.stroke();
    // Glas und Fassung
    g.fillStyle = this.detail + "26";
    g.beginPath();
    g.arc(cx, cy, r, 0, TAU);
    g.fill();
    g.strokeStyle = "#c7c7cc";
    g.lineWidth = S * 0.045;
    g.stroke();
    // Lichtreflex auf dem Glas
    g.strokeStyle = "rgba(255, 255, 255, 0.85)";
    g.lineWidth = S * 0.022;
    g.beginPath();
    g.arc(cx, cy, r * 0.62, Math.PI * 1.08, Math.PI * 1.42);
    g.stroke();
    g.restore();
    // Die Hand haelt den Griff und liegt deshalb darueber.
    g.fillStyle = this.accent ? this.accent[1] : "#ffffff";
    g.beginPath();
    g.arc(hand.handX, hand.handY, S * 0.094, 0, TAU);
    g.fill();
  }

  /** Kleine Handglocke, die an der erhobenen Hand schwingt. */
  private drawBell(g: CanvasRenderingContext2D, S: number, amount: number, swing: number, hand: ReturnType<Character["arm"]>) {
    const a = clamp(amount, 0, 1.15);
    const k = S * 0.36 * a;
    g.save();
    g.globalAlpha = clamp(amount * 1.6, 0, 1);
    g.translate(hand.handX, hand.handY);
    g.rotate(swing);
    // Kloeppel schwingt etwas weiter aus als die Glocke.
    g.fillStyle = "#b8860b";
    g.beginPath();
    g.arc(Math.sin(swing) * k * 0.28, k * 1.02, k * 0.13, 0, TAU);
    g.fill();
    // Glockenkoerper: Kuppel, die unten ausschwingt
    const body = g.createLinearGradient(-k * 0.5, 0, k * 0.5, 0);
    body.addColorStop(0, "#ffd60a");
    body.addColorStop(1, "#f0b400");
    g.fillStyle = body;
    g.beginPath();
    g.moveTo(-k * 0.56, k * 0.92);
    g.quadraticCurveTo(-k * 0.34, k * 0.78, -k * 0.34, k * 0.42);
    g.bezierCurveTo(-k * 0.34, -k * 0.04, k * 0.34, -k * 0.04, k * 0.34, k * 0.42);
    g.quadraticCurveTo(k * 0.34, k * 0.78, k * 0.56, k * 0.92);
    g.closePath();
    g.fill();
    // Glanzlicht
    g.strokeStyle = "rgba(255, 255, 255, 0.55)";
    g.lineWidth = k * 0.09;
    g.lineCap = "round";
    g.beginPath();
    g.moveTo(-k * 0.17, k * 0.3);
    g.quadraticCurveTo(-k * 0.2, k * 0.5, -k * 0.24, k * 0.66);
    g.stroke();
    g.restore();
    // Die Hand umfasst den Griff.
    g.fillStyle = this.accent ? this.accent[1] : "#ffffff";
    g.beginPath();
    g.arc(hand.handX, hand.handY, S * 0.094, 0, TAU);
    g.fill();
  }

  private drawThinking(g: CanvasRenderingContext2D, x: number, y: number, S: number, amount: number) {
    g.save();
    g.fillStyle = "#ffffff";
    for (let i = 0; i < 3; i++) {
      const wave = Math.max(0, Math.sin(this.time * 4.6 - i * 0.85));
      g.globalAlpha = clamp(amount, 0, 1) * (0.3 + wave * 0.7);
      g.beginPath();
      // Die aeusseren Punkte sitzen etwas tiefer, so entsteht ein kleiner Bogen ueber dem Kopf.
      g.arc(x + (i - 1) * S * 0.2, y - S * 0.78 - wave * S * 0.06 + Math.abs(i - 1) * S * 0.03, S * (0.045 + wave * 0.015), 0, TAU);
      g.fill();
    }
    g.restore();
  }

  private drawParticles(g: CanvasRenderingContext2D, cx: number, cy: number, S: number) {
    for (const p of this.particles) {
      const u = p.life / p.max;
      const x = cx + p.x * S, y = cy + p.y * S, r = p.size * S;
      g.save();
      g.translate(x, y);
      if (p.kind === "note") {
        g.globalAlpha = Math.min(1, u * 5) * (1 - span(u, 0.6, 1));
        g.rotate(p.spin + Math.sin(p.life * 3) * 0.2);
        g.fillStyle = g.strokeStyle = this.detail;
        g.lineWidth = Math.max(1, r * 0.2);
        g.lineCap = g.lineJoin = "round";
        const head = (hx: number, hy: number) => {
          g.beginPath();
          g.ellipse(hx, hy, r * 0.32, r * 0.24, -0.4, 0, TAU);
          g.fill();
        };
        if (p.variant === 0) {
          // Achtelnote
          head(-r * 0.2, r * 0.4);
          g.beginPath();
          g.moveTo(r * 0.08, r * 0.38);
          g.lineTo(r * 0.08, -r * 0.5);
          g.quadraticCurveTo(r * 0.5, -r * 0.4, r * 0.48, -r * 0.05);
          g.stroke();
        } else {
          // Zwei verbundene Noten
          head(-r * 0.5, r * 0.45);
          head(r * 0.35, r * 0.3);
          g.beginPath();
          g.moveTo(-r * 0.22, r * 0.43);
          g.lineTo(-r * 0.22, -r * 0.4);
          g.lineTo(r * 0.63, -r * 0.55);
          g.lineTo(r * 0.63, r * 0.28);
          g.stroke();
        }
      } else if (p.kind === "z") {
        g.globalAlpha = Math.min(1, u * 4) * (1 - span(u, 0.55, 1)) * 0.8;
        const s = r * (0.55 + u * 0.75);
        g.rotate(-0.12);
        g.strokeStyle = "#ffffff";
        g.lineWidth = Math.max(1, s * 0.2);
        g.lineCap = g.lineJoin = "round";
        g.beginPath();
        g.moveTo(-s / 2, -s / 2);
        g.lineTo(s / 2, -s / 2);
        g.lineTo(-s / 2, s / 2);
        g.lineTo(s / 2, s / 2);
        g.stroke();
      } else if (p.kind === "orb") {
        g.globalAlpha = bell(u);
        const color = this.accent ? this.accent[0] : this.detail;
        const grad = g.createRadialGradient(0, 0, 0, 0, 0, r * 2.2);
        grad.addColorStop(0, "#ffffff");
        grad.addColorStop(0.35, color);
        grad.addColorStop(1, color + "00");
        g.fillStyle = grad;
        g.beginPath();
        g.arc(0, 0, r * 2.2, 0, TAU);
        g.fill();
      } else if (p.kind === "ding") {
        // Klangbogen, der sich von der Glocke entfernt
        g.globalAlpha = (1 - u) * 0.8;
        g.strokeStyle = "#ffd60a";
        g.lineWidth = Math.max(1, S * 0.03);
        g.lineCap = "round";
        g.beginPath();
        const mid = p.variant > 0 ? 0 : Math.PI;
        g.arc(-p.variant * r * 0.6, 0, r * (0.8 + u * 0.5), mid - 0.7, mid + 0.7);
        g.stroke();
      } else if (p.kind === "dust") {
        g.globalAlpha = (1 - u) * 0.3;
        g.fillStyle = "#ffffff";
        g.beginPath();
        g.arc(0, 0, r * (0.6 + u * 0.8), 0, TAU);
        g.fill();
      } else {
        // Vierzackiger Funke, der beim Ausblenden kleiner wird
        g.globalAlpha = 1 - span(u, 0.4, 1);
        g.rotate(p.spin + p.life * 2);
        const k = r * (1 - u * 0.5);
        g.fillStyle = this.accent ? this.accent[0] : "#ffffff";
        g.beginPath();
        for (let i = 0; i < 8; i++) {
          const rad = i % 2 === 0 ? k : k * 0.32;
          g.lineTo(Math.cos((i / 8) * TAU) * rad, Math.sin((i / 8) * TAU) * rad);
        }
        g.closePath();
        g.fill();
      }
      g.restore();
    }
  }
}

function pickWeighted<T>(items: [T, number][]): T {
  let roll = Math.random() * items.reduce((sum, [, w]) => sum + w, 0);
  for (const [item, weight] of items) {
    roll -= weight;
    if (roll <= 0) return item;
  }
  return items[0][0];
}
