// Weitere Deko-Objekte mit eigener kleiner Animation: Kerze, Aquarium, Sanduhr, Bonsai, Radio,
// Kristall, Globus, Teddy. Gleiche Regeln wie in decor.ts: gezeichnet ab der Bodenlinie (0, 0),
// Groesse relativ zu Milos Koerper.
import type { DecorContext } from "./decor";

type G = CanvasRenderingContext2D;
const night = (hour: number) => hour >= 20 || hour < 6;

export function candle(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  g.fillStyle = "#8e8e93";
  g.beginPath();
  g.ellipse(0, -k * 0.05, k * 0.38, k * 0.08, 0, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#f5efe0";
  g.beginPath();
  g.roundRect(-k * 0.16, -k * 0.95, k * 0.32, k * 0.9, k * 0.05);
  g.fill();
  g.fillStyle = "rgba(0,0,0,0.08)";
  g.fillRect(k * 0.06, -k * 0.95, k * 0.1, k * 0.9);
  g.strokeStyle = "#3a3a3e";
  g.lineWidth = 1;
  g.beginPath();
  g.moveTo(0, -k * 0.95);
  g.lineTo(0, -k * 1.05);
  g.stroke();
  // Flamme flackert aus zwei ueberlagerten Schwingungen.
  const flick = Math.sin(c.clock * 13) * 0.08 + Math.sin(c.clock * 7.3) * 0.06;
  const glow = g.createRadialGradient(0, -k * 1.25, 0, 0, -k * 1.25, k * 0.9);
  glow.addColorStop(0, "rgba(255,190,90,0.35)");
  glow.addColorStop(1, "rgba(255,190,90,0)");
  g.fillStyle = glow;
  g.fillRect(-k, -k * 2.2, k * 2, k * 2);
  g.save();
  g.translate(0, -k * 1.05);
  g.rotate(flick);
  g.fillStyle = "#ffb340";
  g.beginPath();
  g.ellipse(0, -k * 0.2, k * 0.11, k * (0.22 + flick * 0.3), 0, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#fff4c2";
  g.beginPath();
  g.ellipse(0, -k * 0.14, k * 0.05, k * 0.11, 0, 0, Math.PI * 2);
  g.fill();
  g.restore();
}

export function aquarium(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  const w = k * 1.9, h = k * 1.35;
  const radii = [k * 0.5, k * 0.5, k * 0.15, k * 0.15];
  g.fillStyle = "rgba(100,210,255,0.28)";
  g.beginPath();
  g.roundRect(-w / 2, -h, w, h, radii);
  g.fill();
  g.fillStyle = "#e4c7a1";
  g.fillRect(-w / 2 + 2, -k * 0.18, w - 4, k * 0.16);
  g.strokeStyle = "#34c77b";
  g.lineWidth = 1.4;
  g.beginPath();
  g.moveTo(-w * 0.3, -k * 0.2);
  g.quadraticCurveTo(-w * 0.3 + Math.sin(c.clock * 1.5) * 3, -h * 0.55, -w * 0.28, -h * 0.75);
  g.stroke();
  // Fisch schwimmt hin und her und dreht am Rand um.
  const t = c.clock * 0.6;
  const fx = Math.sin(t) * w * 0.28;
  const dir = Math.cos(t) >= 0 ? 1 : -1;
  const fy = -h * 0.5 + Math.sin(c.clock * 1.7) * 2;
  g.fillStyle = "#ff9f0a";
  g.beginPath();
  g.ellipse(fx, fy, k * 0.2, k * 0.12, 0, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.moveTo(fx - dir * k * 0.18, fy);
  g.lineTo(fx - dir * k * 0.34, fy - k * 0.1);
  g.lineTo(fx - dir * k * 0.34, fy + k * 0.1);
  g.closePath();
  g.fill();
  g.fillStyle = "#1c1c1e";
  g.beginPath();
  g.arc(fx + dir * k * 0.1, fy - k * 0.03, 0.9, 0, Math.PI * 2);
  g.fill();
  const by = -k * 0.3 - ((c.clock * 12) % (h * 0.7));
  g.strokeStyle = "rgba(255,255,255,0.6)";
  g.lineWidth = 0.8;
  g.beginPath();
  g.arc(fx + dir * k * 0.25, by, 1.3, 0, Math.PI * 2);
  g.stroke();
  g.strokeStyle = "rgba(255,255,255,0.45)";
  g.lineWidth = 1;
  g.beginPath();
  g.roundRect(-w / 2, -h, w, h, radii);
  g.stroke();
}

export function hourglass(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  const period = 20;
  const phase = (c.clock % period) / period;
  // Am Ende dreht sie sich einmal um.
  const flip = phase > 0.92 ? ((phase - 0.92) / 0.08) * Math.PI : 0;
  g.save();
  g.translate(0, -k * 0.85);
  g.rotate(flip);
  g.fillStyle = "#8b5a3c";
  g.fillRect(-k * 0.42, -k * 0.85, k * 0.84, k * 0.1);
  g.fillRect(-k * 0.42, k * 0.75, k * 0.84, k * 0.1);
  g.strokeStyle = "rgba(255,255,255,0.55)";
  g.lineWidth = 1;
  g.beginPath();
  g.moveTo(-k * 0.32, -k * 0.75);
  g.lineTo(k * 0.32, -k * 0.75);
  g.lineTo(k * 0.04, 0);
  g.lineTo(k * 0.32, k * 0.75);
  g.lineTo(-k * 0.32, k * 0.75);
  g.lineTo(-k * 0.04, 0);
  g.closePath();
  g.stroke();
  const left = flip ? 0 : 1 - phase / 0.92;
  g.fillStyle = "#ffd60a";
  const top = -k * 0.08 - left * k * 0.6;
  g.beginPath();
  g.moveTo(-k * 0.04 - left * k * 0.24, top);
  g.lineTo(k * 0.04 + left * k * 0.24, top);
  g.lineTo(k * 0.03, -k * 0.04);
  g.lineTo(-k * 0.03, -k * 0.04);
  g.closePath();
  g.fill();
  const pile = (1 - left) * k * 0.55;
  g.beginPath();
  g.moveTo(-k * 0.3, k * 0.74);
  g.lineTo(k * 0.3, k * 0.74);
  g.lineTo(0, k * 0.74 - pile);
  g.closePath();
  g.fill();
  if (!flip && left > 0) g.fillRect(-0.5, 0, 1, k * 0.74 - pile);
  g.restore();
}

export function bonsai(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  g.fillStyle = "#5e5ce6";
  g.beginPath();
  g.roundRect(-k * 0.7, -k * 0.28, k * 1.4, k * 0.28, [2, 2, k * 0.12, k * 0.12]);
  g.fill();
  g.strokeStyle = "#7a5230";
  g.lineWidth = k * 0.12;
  g.lineCap = "round";
  g.beginPath();
  g.moveTo(0, -k * 0.28);
  g.quadraticCurveTo(-k * 0.3, -k * 0.7, k * 0.1, -k * 1.0);
  g.moveTo(-k * 0.12, -k * 0.62);
  g.lineTo(-k * 0.5, -k * 0.9);
  g.stroke();
  const sway = Math.sin(c.clock * 1.1) * 1.2;
  g.fillStyle = night(c.hour) ? "#2e8a44" : "#34c77b";
  for (const [x, y, r] of [[0.15, -1.15, 0.38], [-0.5, -1.0, 0.3], [-0.2, -1.3, 0.28], [0.45, -1.0, 0.25]] as const) {
    g.beginPath();
    g.ellipse(x * k + sway, y * k, r * k * 1.2, r * k * 0.8, 0, 0, Math.PI * 2);
    g.fill();
  }
}

export function radio(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  g.fillStyle = "#ff6961";
  g.beginPath();
  g.roundRect(-k * 0.8, -k * 1.0, k * 1.6, k * 1.0, k * 0.18);
  g.fill();
  g.fillStyle = "#f5efe0";
  g.beginPath();
  g.arc(-k * 0.35, -k * 0.5, k * 0.28, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#3a3a3e";
  for (let i = 0; i < 3; i++) g.fillRect(k * 0.08, -k * 0.78 + i * k * 0.18, k * 0.55, k * 0.07);
  g.strokeStyle = "#3a3a3e";
  g.lineWidth = 1.2;
  g.beginPath();
  g.moveTo(k * 0.5, -k * 1.0);
  g.lineTo(k * 0.8, -k * 1.5);
  g.stroke();
  // Noten steigen auf, wenn Musik laeuft.
  if (c.music) {
    g.fillStyle = "#ffffff";
    for (let i = 0; i < 2; i++) {
      const t = (c.clock * 0.6 + i * 0.5) % 1;
      g.globalAlpha = 1 - t;
      const x = -k * 0.2 + i * k * 0.5 + Math.sin(t * 6) * 3;
      const y = -k * 1.1 - t * k * 1.4;
      g.beginPath();
      g.ellipse(x, y, 2.2, 1.6, -0.3, 0, Math.PI * 2);
      g.fill();
      g.fillRect(x + 1.6, y - 6, 1, 6);
    }
    g.globalAlpha = 1;
  }
}

export function crystal(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  const pulse = 0.5 + Math.sin(c.clock * 1.6) * 0.5;
  const glow = g.createRadialGradient(0, -k * 0.7, 0, 0, -k * 0.7, k * 1.1);
  glow.addColorStop(0, `rgba(191,90,242,${0.25 + pulse * 0.25})`);
  glow.addColorStop(1, "rgba(191,90,242,0)");
  g.fillStyle = glow;
  g.fillRect(-k * 1.2, -k * 1.9, k * 2.4, k * 2.2);
  const facets: [string, number[]][] = [
    ["#d9b4ff", [0, -1.6, 0.32, -0.6, 0, -0.1]],
    ["#bf5af2", [0, -1.6, -0.32, -0.6, 0, -0.1]],
    ["#9b3fd1", [-0.32, -0.6, -0.5, -0.3, 0, -0.1]],
    ["#e8d1ff", [0.32, -0.6, 0.5, -0.3, 0, -0.1]],
  ];
  for (const [color, p] of facets) {
    g.fillStyle = color;
    g.beginPath();
    g.moveTo(p[0] * k, p[1] * k);
    g.lineTo(p[2] * k, p[3] * k);
    g.lineTo(p[4] * k, p[5] * k);
    g.closePath();
    g.fill();
  }
  g.fillStyle = "#3a3a3e";
  g.fillRect(-k * 0.45, -k * 0.1, k * 0.9, k * 0.1);
}

export function globe(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  const r = k * 0.55;
  const cy = -k * 1.15;
  g.fillStyle = "#3a3a3e";
  g.fillRect(-k * 0.3, -k * 0.08, k * 0.6, k * 0.08);
  g.fillRect(-1, cy + r, 2, -cy - r);
  g.fillStyle = "#3a8ee6";
  g.beginPath();
  g.arc(0, cy, r, 0, Math.PI * 2);
  g.fill();
  // Kontinente wandern mit der Drehung durch.
  g.save();
  g.beginPath();
  g.arc(0, cy, r, 0, Math.PI * 2);
  g.clip();
  g.fillStyle = "#34c77b";
  const shift = (c.clock * 6) % (r * 4);
  for (const [x, y, w, h] of [[0, -0.3, 0.5, 0.35], [0.9, 0.2, 0.6, 0.4], [1.8, -0.1, 0.4, 0.5], [2.8, 0.3, 0.5, 0.3]] as const) {
    for (const off of [0, r * 4]) {
      g.beginPath();
      g.ellipse(x * r - shift + off - r * 2, cy + y * r, w * r, h * r, 0.3, 0, Math.PI * 2);
      g.fill();
    }
  }
  g.restore();
  g.strokeStyle = "#8e8e93";
  g.lineWidth = 1.2;
  g.beginPath();
  g.arc(0, cy, r + 2, Math.PI * 0.6, Math.PI * 1.9);
  g.stroke();
}

export function teddy(g: G, size: number, c: DecorContext) {
  const k = size * 0.5;
  const wave = Math.max(0, Math.sin(c.clock * 0.7)) ** 8;
  g.fillStyle = "#b07a4f";
  g.beginPath();
  g.ellipse(0, -k * 0.4, k * 0.42, k * 0.4, 0, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.arc(0, -k * 1.05, k * 0.36, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.arc(-k * 0.3, -k * 1.35, k * 0.13, 0, Math.PI * 2);
  g.arc(k * 0.3, -k * 1.35, k * 0.13, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.ellipse(-k * 0.45, -k * 0.45, k * 0.13, k * 0.24, 0.5, 0, Math.PI * 2);
  g.fill();
  // Ein Arm winkt ab und zu.
  g.save();
  g.translate(k * 0.38, -k * 0.6);
  g.rotate(-0.5 - wave * 1.4);
  g.beginPath();
  g.ellipse(k * 0.05, k * 0.15, k * 0.13, k * 0.24, 0, 0, Math.PI * 2);
  g.fill();
  g.restore();
  g.fillStyle = "#e9c9a3";
  g.beginPath();
  g.ellipse(0, -k * 0.97, k * 0.16, k * 0.12, 0, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.ellipse(0, -k * 0.38, k * 0.22, k * 0.24, 0, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#1c1c1e";
  for (const [x, y, r] of [[-0.13, -1.12, 1.4], [0.13, -1.12, 1.4], [0, -1.0, 1.6]] as const) {
    g.beginPath();
    g.arc(x * k, y * k, r, 0, Math.PI * 2);
    g.fill();
  }
}
