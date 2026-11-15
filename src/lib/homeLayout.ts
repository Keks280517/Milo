import { t } from "./i18n";
// Raster der Startseite (Home): 12 Spalten, 2 Reihen. Jede Karte hat eine Groesse; ohne eigene Wahl
// richtet sich das Layout nach der Anzahl der Karten. Mehr als passt, laesst sich scrollen.

export type CardSize = "s" | "m" | "w" | "t" | "l" | "f";

export const SIZES: { id: CardSize; label: string; cols: number; rows: number }[] = [
  { id: "s", label: t("Klein", "Small"), cols: 3, rows: 1 },
  { id: "m", label: t("Mittel", "Medium"), cols: 4, rows: 1 },
  { id: "w", label: t("Breit", "Wide"), cols: 6, rows: 1 },
  { id: "t", label: t("Hoch", "Tall"), cols: 4, rows: 2 },
  { id: "l", label: t("Groß", "Large"), cols: 6, rows: 2 },
  { id: "f", label: t("Ganze Breite", "Full width"), cols: 12, rows: 2 },
];

export const sizeInfo = (id: CardSize) => SIZES.find((s) => s.id === id) ?? SIZES[0];

/** Automatische Groessen je nach Anzahl: fuellt die zwei Reihen moeglichst ohne Luecken. */
export function autoSizes(count: number): CardSize[] {
  const fixed: Record<number, CardSize[]> = {
    1: ["f"],
    2: ["l", "l"],
    3: ["t", "t", "t"],
    4: ["w", "w", "w", "w"],
    5: ["l", "s", "s", "s", "s"],
    6: ["m", "m", "m", "m", "m", "m"],
    7: ["m", "m", "m", "s", "s", "s", "s"],
  };
  return fixed[count] ?? Array.from({ length: count }, () => "s" as CardSize);
}

/** Karten mit ihrer Groesse: eigene Wahl vor Automatik. */
export function layout(cards: string[], sizes: Record<string, string> | undefined): { id: string; size: CardSize }[] {
  const auto = autoSizes(cards.length);
  return cards.map((id, i) => {
    const own = sizes?.[id];
    return { id, size: SIZES.some((s) => s.id === own) ? (own as CardSize) : auto[i] };
  });
}

/** Belegte Flaeche in Rasterzellen (12 x 2 = 24 passen ohne Scrollen). */
export const usedCells = (items: { size: CardSize }[]) => items.reduce((sum, item) => sum + sizeInfo(item.size).cols * sizeInfo(item.size).rows, 0);
