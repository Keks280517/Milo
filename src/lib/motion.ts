// Gemeinsame Bewegungen: eine gedaempfte Feder als Zeitkurve und die Uebergaenge, mit denen
// Glasflaechen auftauchen (aufploppen aus ihrem Ursprung) und Ansichten wechseln.
import type { TransitionConfig } from "svelte/transition";

/** Gedaempfte Feder (leichtes Ueberschwingen, ruhiges Ausklingen); t von 0 bis 1. */
export function spring(t: number): number {
  if (t >= 1) return 1;
  const w = 10;
  const d = 6.2;
  return 1 - Math.exp(-d * t) * (Math.cos(w * t) + (d / w) * Math.sin(w * t));
}

const reduced = () =>
  typeof document !== "undefined" &&
  (document.documentElement.classList.contains("reduce-motion") || matchMedia("(prefers-reduced-motion: reduce)").matches);

/** Menues und Glasflaechen: wachsen federnd aus ihrem Ursprung und werden dabei scharf. */
export function pop(_node: Element, { duration = 420 }: { duration?: number } = {}): TransitionConfig {
  if (reduced()) return { duration: 120, css: (t) => `opacity:${t}` };
  return {
    duration,
    easing: spring,
    css: (t, u) => `opacity:${Math.min(1, t * 1.6)};transform:scale(${0.9 + 0.1 * t});filter:blur(${Math.max(0, u) * 6}px)`,
  };
}

/** Wechsel zwischen Ansichten: kurz unscharf, ein paar Pixel tiefer, dann scharf an Ort und Stelle. */
export function paneIn(_node: Element, { duration = 380 }: { duration?: number } = {}): TransitionConfig {
  if (reduced()) return { duration: 0 };
  return {
    duration,
    easing: spring,
    css: (t, u) => `opacity:${Math.min(1, t * 1.4)};transform:translateY(${u * 8}px) scale(${0.985 + 0.015 * t});filter:blur(${Math.max(0, u) * 4}px)`,
  };
}

/** Elemente nacheinander einblenden (Listen, Bausteine): Index mal Abstand als Verzoegerung. */
export function rise(_node: Element, { delay = 0, duration = 460, y = 14 }: { delay?: number; duration?: number; y?: number } = {}): TransitionConfig {
  if (reduced()) return { duration: 0 };
  return {
    delay,
    duration,
    easing: spring,
    css: (t, u) => `opacity:${Math.min(1, t * 1.5)};transform:translateY(${u * y}px) scale(${0.96 + 0.04 * t})`,
  };
}
