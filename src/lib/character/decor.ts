// Deko-Objekte: kleine Dinge, die in der Insel neben Milo stehen oder auf dem Desktop auf der
// Taskleiste liegen. Jedes Objekt zeichnet sich selbst, stehend auf einer Bodenlinie (x = Mitte,
// floor = Unterkante). `size` ist Milos Koerpergroesse, damit alles im selben Massstab bleibt.

import { aquarium, bonsai, candle, crystal, globe, hourglass, radio, teddy } from "./decor-extra";
import { t } from "../i18n";

export type DecorKind =
  | "plant" | "mailbox" | "lantern" | "cactus" | "duck" | "mug" | "books" | "lamp"
  | "candle" | "aquarium" | "hourglass" | "bonsai" | "radio" | "crystal" | "globe" | "teddy";

export interface DecorInfo {
  kind: DecorKind;
  label: string;
  hint: string;
  /** Breite und Hoehe als Vielfaches von `size` (fuer Treffer und Physik). */
  w: number;
  h: number;
}

export const DECOR: DecorInfo[] = [
  { kind: "plant", label: t("Blumentopf", "Flower pot"), hint: t("Wächst über die Tage und blüht irgendwann", "Grows over the days and blooms eventually"), w: 0.8, h: 1.0 },
  { kind: "mailbox", label: t("Briefkasten", "Mailbox"), hint: t("Fahne hoch, wenn etwas auf dich wartet", "Flag goes up when something is waiting for you"), w: 0.6, h: 1.0 },
  { kind: "lantern", label: t("Laterne", "Lantern"), hint: t("Leuchtet abends", "Glows in the evening"), w: 0.4, h: 1.25 },
  { kind: "cactus", label: t("Kaktus", "Cactus"), hint: t("Pflegeleicht, blüht nachts", "Low-maintenance, blooms at night"), w: 0.6, h: 0.95 },
  { kind: "duck", label: t("Badeente", "Rubber duck"), hint: t("Wackelt, wenn man sie anstupst", "Wobbles when you poke it"), w: 0.75, h: 0.65 },
  { kind: "mug", label: t("Kaffeetasse", "Coffee mug"), hint: t("Dampft morgens", "Steams in the morning"), w: 0.6, h: 0.55 },
  { kind: "books", label: t("Bücherstapel", "Book stack"), hint: t("Drei Bände, schief gestapelt", "Three volumes, stacked crookedly"), w: 0.8, h: 0.6 },
  { kind: "lamp", label: t("Lavalampe", "Lava lamp"), hint: t("Blasen steigen langsam auf", "Blobs rise slowly"), w: 0.45, h: 1.05 },
  { kind: "candle", label: t("Kerze", "Candle"), hint: t("Die Flamme flackert", "The flame flickers"), w: 0.4, h: 1.2 },
  { kind: "aquarium", label: t("Aquarium", "Aquarium"), hint: t("Ein Fisch zieht seine Runden", "A fish swims its laps"), w: 1.0, h: 0.75 },
  { kind: "hourglass", label: t("Sanduhr", "Hourglass"), hint: t("Rieselt und dreht sich um", "Trickles and flips over"), w: 0.5, h: 0.95 },
  { kind: "bonsai", label: t("Bonsai", "Bonsai"), hint: t("Kleiner Baum in flacher Schale", "A small tree in a shallow pot"), w: 0.9, h: 0.9 },
  { kind: "radio", label: t("Radio", "Radio"), hint: t("Noten steigen auf, wenn Musik läuft", "Notes float up when music plays"), w: 0.85, h: 0.6 },
  { kind: "crystal", label: t("Kristall", "Crystal"), hint: t("Leuchtet sanft", "Glows softly"), w: 0.5, h: 0.85 },
  { kind: "globe", label: t("Globus", "Globe"), hint: t("Dreht sich langsam", "Turns slowly"), w: 0.6, h: 0.95 },
  { kind: "teddy", label: t("Teddy", "Teddy"), hint: t("Winkt ab und zu", "Waves now and then"), w: 0.65, h: 0.8 },
];

export const decorInfo = (kind: string) => DECOR.find((d) => d.kind === kind);

export interface DecorContext {
  /** Laufende Zeit in Sekunden (fuer Wiegen, Dampf, Blasen). */
  clock: number;
  hour: number;
  /** Briefkasten: es wartet etwas. */
  mail: boolean;
  /** Pflanze: 0 = Keimling bis 4 = bluehend. */
  growth: number;
  /** Pflanze richtet sich nach dem Giessen auf (0 bis 1). */
  perk: number;
  /** Wackeln nach einem Stupser oder Aufprall (0 bis 1). */
  wobble?: number;
  /** Musik laeuft (das Radio zeigt dann Noten). */
  music?: boolean;
}

const night = (hour: number) => hour >= 20 || hour < 6;

let glowSprite: HTMLCanvasElement | null = null;
function glow(): HTMLCanvasElement {
  if (glowSprite) return glowSprite;
  glowSprite = document.createElement("canvas");
  glowSprite.width = glowSprite.height = 64;
  const gg = glowSprite.getContext("2d")!;
  const paint = gg.createRadialGradient(32, 32, 0, 32, 32, 32);
  paint.addColorStop(0, "rgba(255,214,120,0.55)");
  paint.addColorStop(1, "rgba(255,214,120,0)");
  gg.fillStyle = paint;
  gg.fillRect(0, 0, 64, 64);
  return glowSprite;
}

export function drawDecor(g: CanvasRenderingContext2D, kind: DecorKind, x: number, floor: number, size: number, c: DecorContext) {
  g.save();
  g.translate(x, floor);
  const wob = c.wobble ?? 0;
  if (wob > 0) g.rotate(Math.sin(c.clock * 22) * wob * 0.12);
  switch (kind) {
    case "plant":
      plant(g, size, c);
      break;
    case "mailbox":
      mailbox(g, size, c);
      break;
    case "lantern":
      lantern(g, size, c);
      break;
    case "cactus":
      cactus(g, size, c);
      break;
    case "duck":
      duck(g, size);
      break;
    case "mug":
      mug(g, size, c);
      break;
    case "books":
      books(g, size);
      break;
    case "lamp":
      lamp(g, size, c);
      break;
    case "candle":
      candle(g, size, c);
      break;
    case "aquarium":
      aquarium(g, size, c);
      break;
    case "hourglass":
      hourglass(g, size, c);
      break;
    case "bonsai":
      bonsai(g, size, c);
      break;
    case "radio":
      radio(g, size, c);
      break;
    case "crystal":
      crystal(g, size, c);
      break;
    case "globe":
      globe(g, size, c);
      break;
    case "teddy":
      teddy(g, size, c);
      break;
  }
  g.restore();
}

function plant(g: CanvasRenderingContext2D, size: number, c: DecorContext) {
  const k = size * 0.5;
  const growth = Math.max(0, Math.min(4, c.growth));
  const scale = 0.55 + 0.45 * Math.min(1, growth / 3);
  const sway = Math.sin(c.clock * 1.3) * 0.07;
  const lift = c.perk * 0.18;
  const leaves: [number, number][] = [[-0.75, 0.62], [0.7, 0.66], [-0.05, 0.86]];
  if (growth >= 2) leaves.unshift([-1.15, 0.5], [1.1, 0.52]);
  if (growth < 1) leaves.splice(0, leaves.length, [-0.5, 0.5], [0.45, 0.52]);
  g.save();
  g.translate(0, -k * 0.42);
  g.scale(scale, scale);
  g.fillStyle = night(c.hour) ? "#2e8a44" : "#4cd964";
  for (const [angle, len] of leaves) {
    g.save();
    g.rotate(angle * (1 - lift) + sway);
    g.beginPath();
    g.ellipse(0, -k * len * 0.5, k * 0.17, k * len * 0.5, 0, 0, Math.PI * 2);
    g.fill();
    g.restore();
  }
  if (growth >= 4) {
    for (const [angle, len, color] of [[-0.05, 0.86, "#ff9fce"], [0.7, 0.66, "#ffd60a"]] as const) {
      g.save();
      g.rotate(angle * (1 - lift) + sway);
      g.fillStyle = color;
      g.beginPath();
      g.arc(0, -k * len, k * 0.13, 0, Math.PI * 2);
      g.fill();
      g.restore();
    }
  }
  g.restore();
  g.fillStyle = "#d9825b";
  g.beginPath();
  g.roundRect(-k * 0.34, -k * 0.46, k * 0.68, k * 0.14, k * 0.05);
  g.fill();
  g.beginPath();
  g.moveTo(-k * 0.28, -k * 0.34);
  g.lineTo(k * 0.28, -k * 0.34);
  g.lineTo(k * 0.2, 0);
  g.lineTo(-k * 0.2, 0);
  g.closePath();
  g.fill();
}

function mailbox(g: CanvasRenderingContext2D, size: number, c: DecorContext) {
  const box = size * 0.34;
  g.fillStyle = "#3a3a3e";
  g.fillRect(-0.8, -box * 1.3, 1.6, box * 1.3);
  g.fillStyle = "#64d2ff";
  g.beginPath();
  g.roundRect(-box * 0.7, -box * 2.1, box * 1.4, box * 0.85, [box * 0.4, box * 0.4, 1.5, 1.5]);
  g.fill();
  g.strokeStyle = "#ff453a";
  g.fillStyle = "#ff453a";
  g.lineWidth = 1.2;
  g.beginPath();
  g.moveTo(box * 0.7, -box * 1.55);
  g.lineTo(box * 0.7, -box * (c.mail ? 2.6 : 1.7));
  g.stroke();
  if (c.mail) g.fillRect(box * 0.7, -box * 2.6, box * 0.5, box * 0.32);
}

function lantern(g: CanvasRenderingContext2D, size: number, c: DecorContext) {
  const lit = night(c.hour) || c.hour >= 18;
  const top = -size * 1.15;
  if (lit) {
    const r = size * 0.9 * (1 + Math.sin(c.clock * 2.3) * 0.03);
    g.drawImage(glow(), -r, top + 3 - r, r * 2, r * 2);
  }
  g.fillStyle = "#3a3a3e";
  g.fillRect(-1, top + 6, 2, -top - 6);
  g.fillRect(-4, -2, 8, 2);
  g.fillStyle = lit ? "#ffd27a" : "#5a5a60";
  g.beginPath();
  g.roundRect(-3.5, top, 7, 7, 2);
  g.fill();
  g.fillStyle = "#3a3a3e";
  g.fillRect(-4.5, top - 1.5, 9, 2);
}

function cactus(g: CanvasRenderingContext2D, size: number, c: DecorContext) {
  const k = size * 0.5;
  // Topf
  g.fillStyle = "#e4c7a1";
  g.beginPath();
  g.moveTo(-k * 0.34, -k * 0.42);
  g.lineTo(k * 0.34, -k * 0.42);
  g.lineTo(k * 0.26, 0);
  g.lineTo(-k * 0.26, 0);
  g.closePath();
  g.fill();
  // Stamm mit zwei Armen
  g.fillStyle = "#34c77b";
  g.beginPath();
  g.roundRect(-k * 0.16, -k * 1.7, k * 0.32, k * 1.32, k * 0.16);
  g.fill();
  g.beginPath();
  g.roundRect(-k * 0.5, -k * 1.25, k * 0.2, k * 0.5, k * 0.1);
  g.roundRect(-k * 0.5, -k * 0.85, k * 0.4, k * 0.18, k * 0.09);
  g.roundRect(k * 0.3, -k * 1.45, k * 0.2, k * 0.55, k * 0.1);
  g.roundRect(k * 0.1, -k * 1.0, k * 0.4, k * 0.18, k * 0.09);
  g.fill();
  // Stacheln als helle Punkte
  g.fillStyle = "rgba(255,255,255,0.55)";
  for (const [px, py] of [[-0.05, -1.5], [0.06, -1.2], [-0.06, -0.9], [0.05, -0.62], [-0.42, -1.1], [0.4, -1.3]]) {
    g.fillRect(px * k - 0.5, py * k - 0.5, 1, 1);
  }
  // Nachts eine kleine Bluete
  if (night(c.hour)) {
    g.fillStyle = "#ff6fae";
    g.beginPath();
    g.arc(0, -k * 1.74, k * 0.12, 0, Math.PI * 2);
    g.fill();
  }
}

function duck(g: CanvasRenderingContext2D, size: number) {
  const k = size * 0.5;
  g.fillStyle = "#ffd60a";
  g.beginPath();
  g.ellipse(0, -k * 0.32, k * 0.68, k * 0.32, 0, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.arc(k * 0.3, -k * 0.8, k * 0.26, 0, Math.PI * 2);
  g.fill();
  // Schwanz
  g.beginPath();
  g.moveTo(-k * 0.6, -k * 0.42);
  g.lineTo(-k * 0.82, -k * 0.7);
  g.lineTo(-k * 0.5, -k * 0.55);
  g.fill();
  g.fillStyle = "#ff9f0a";
  g.beginPath();
  g.ellipse(k * 0.6, -k * 0.76, k * 0.14, k * 0.07, 0.1, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#1c1c1e";
  g.beginPath();
  g.arc(k * 0.38, -k * 0.86, k * 0.045, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "rgba(255,255,255,0.45)";
  g.beginPath();
  g.ellipse(-k * 0.15, -k * 0.45, k * 0.25, k * 0.08, -0.2, 0, Math.PI * 2);
  g.fill();
}

function mug(g: CanvasRenderingContext2D, size: number, c: DecorContext) {
  const k = size * 0.5;
  g.fillStyle = "#f5f5f7";
  g.beginPath();
  g.roundRect(-k * 0.34, -k * 0.62, k * 0.6, k * 0.62, [k * 0.04, k * 0.04, k * 0.14, k * 0.14]);
  g.fill();
  g.strokeStyle = "#f5f5f7";
  g.lineWidth = Math.max(1.2, k * 0.08);
  g.beginPath();
  g.arc(k * 0.3, -k * 0.34, k * 0.14, -Math.PI / 2, Math.PI / 2);
  g.stroke();
  g.fillStyle = "#ff6961";
  g.fillRect(-k * 0.34, -k * 0.42, k * 0.6, k * 0.08);
  // Dampf am Vormittag
  if (c.hour >= 6 && c.hour < 12) {
    g.strokeStyle = "rgba(255,255,255,0.35)";
    g.lineWidth = 1;
    for (const off of [-0.12, 0.08]) {
      const t = c.clock * 1.4 + off * 9;
      g.beginPath();
      for (let i = 0; i <= 8; i++) {
        const y = -k * 0.7 - i * k * 0.07;
        const xx = off * k + Math.sin(t + i * 0.7) * k * 0.06;
        if (i === 0) g.moveTo(xx, y);
        else g.lineTo(xx, y);
      }
      g.stroke();
    }
  }
}

function books(g: CanvasRenderingContext2D, size: number) {
  const k = size * 0.5;
  const stack: [number, number, number, string][] = [
    [0, -k * 0.2, 0, "#bf5af2"],
    [k * 0.06, -k * 0.4, 0.05, "#64d2ff"],
    [-k * 0.05, -k * 0.6, -0.07, "#ff9f0a"],
  ];
  for (const [dx, y, rot, color] of stack) {
    g.save();
    g.translate(dx, y + k * 0.1);
    g.rotate(rot);
    g.fillStyle = color;
    g.beginPath();
    g.roundRect(-k * 0.55, -k * 0.1, k * 1.1, k * 0.2, k * 0.04);
    g.fill();
    g.fillStyle = "rgba(255,255,255,0.75)";
    g.fillRect(k * 0.38, -k * 0.07, k * 0.12, k * 0.14);
    g.restore();
  }
}

function lamp(g: CanvasRenderingContext2D, size: number, c: DecorContext) {
  const k = size * 0.5;
  // Sockel und Kappe
  g.fillStyle = "#8e8e93";
  g.beginPath();
  g.moveTo(-k * 0.3, 0);
  g.lineTo(k * 0.3, 0);
  g.lineTo(k * 0.16, -k * 0.4);
  g.lineTo(-k * 0.16, -k * 0.4);
  g.closePath();
  g.fill();
  g.beginPath();
  g.moveTo(-k * 0.1, -k * 1.95);
  g.lineTo(k * 0.1, -k * 1.95);
  g.lineTo(k * 0.13, -k * 1.8);
  g.lineTo(-k * 0.13, -k * 1.8);
  g.closePath();
  g.fill();
  // Glas
  const paint = g.createLinearGradient(0, -k * 1.8, 0, -k * 0.4);
  paint.addColorStop(0, "rgba(191,90,242,0.55)");
  paint.addColorStop(1, "rgba(255,105,140,0.75)");
  g.fillStyle = paint;
  g.beginPath();
  g.moveTo(-k * 0.13, -k * 1.8);
  g.lineTo(k * 0.13, -k * 1.8);
  g.lineTo(k * 0.22, -k * 0.4);
  g.lineTo(-k * 0.22, -k * 0.4);
  g.closePath();
  g.fill();
  // Wachsblasen steigen langsam
  g.fillStyle = "#ffb3c7";
  for (let i = 0; i < 3; i++) {
    const t = (c.clock * 0.12 + i / 3) % 1;
    const y = -k * 0.5 - t * k * 1.2;
    const r = k * (0.07 + 0.04 * Math.sin(i * 2 + c.clock));
    g.beginPath();
    g.arc(Math.sin(c.clock * 0.7 + i * 2) * k * 0.05, y, r, 0, Math.PI * 2);
    g.fill();
  }
}
