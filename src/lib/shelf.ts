import { t } from "./i18n";
// Leiste unten in der Insel: welche Wuerfel es gibt, ihre Symbole und die vom Nutzer gewaehlte
// Reihenfolge bzw. Auswahl (Einstellungen > Leiste unten).

/** Alle Wuerfel in Standard-Reihenfolge (wie `SHELF_TABS` in settings.rs). */
export const SHELF_TABS: { id: string; label: string }[] = [
  { id: "home", label: t("Übersicht", "Home") },
  { id: "chat", label: "Chat" },
  { id: "media", label: t("Musik", "Music") },
  { id: "agenda", label: "Agenda" },
  { id: "timer", label: "Timer" },
  { id: "weather", label: t("Wetter", "Weather") },
  { id: "files", label: t("Dateien", "Files") },
  { id: "notes", label: t("Notizen", "Notes") },
  { id: "system", label: "System" },
  { id: "claude", label: "Claude" },
  { id: "usage", label: t("Verbrauch", "Usage") },
  { id: "history", label: t("Verlauf", "History") },
];

/** Symbole der Reiter (Pfade in einem 16er-Raster). */
export const SHELF_ICONS: Record<string, string> = {
  home: "M2.6 7.4 8 2.8l5.4 4.6V13a.9.9 0 0 1-.9.9H9.6V10H6.4v3.9H3.5a.9.9 0 0 1-.9-.9Z",
  chat: "M3 3.5h10a1.5 1.5 0 0 1 1.5 1.5v5a1.5 1.5 0 0 1-1.5 1.5H7l-3 2.5V11.5H3A1.5 1.5 0 0 1 1.5 10V5A1.5 1.5 0 0 1 3 3.5Z",
  media: "M6 3.5v7.2M6 3.5l6-1.5v7.2M6 10.7a1.8 1.8 0 1 1-3.6 0 1.8 1.8 0 0 1 3.6 0ZM12 9.2a1.8 1.8 0 1 1-3.6 0 1.8 1.8 0 0 1 3.6 0Z",
  timer: "M8 14.2a5.6 5.6 0 1 0 0-11.2 5.6 5.6 0 0 0 0 11.2ZM8 5.6v3l2 1.4M6.4 1.6h3.2",
  weather: "M4.8 12.6h6.6a2.7 2.7 0 0 0 .2-5.4 3.9 3.9 0 0 0-7.4 1A2.2 2.2 0 0 0 4.8 12.6Z",
  files: "M2.5 4.2A1.2 1.2 0 0 1 3.7 3h3l1.4 1.6h4.2a1.2 1.2 0 0 1 1.2 1.2v6.3a1.2 1.2 0 0 1-1.2 1.2H3.7a1.2 1.2 0 0 1-1.2-1.2Z",
  notes: "M4.6 2.6h6.8v10.8L8 11.1l-3.4 2.3Z",
  system: "M2 8h2.6l1.6-4 3 8 1.8-4H14",
  claude: "M8 2.2v11.6M2.2 8h11.6M3.9 3.9l8.2 8.2M12.1 3.9l-8.2 8.2",
  usage: "M2.6 13.4V8.6M6.2 13.4V5M9.8 13.4V7M13.4 13.4V2.6",
  history: "M2.4 8a5.6 5.6 0 1 0 1.7-4M2.4 2.6v2.6H5M8 5.2V8l2 1.4",
  agenda: "M3.6 3.4h8.8a1.2 1.2 0 0 1 1.2 1.2v7.8a1.2 1.2 0 0 1-1.2 1.2H3.6a1.2 1.2 0 0 1-1.2-1.2V4.6a1.2 1.2 0 0 1 1.2-1.2ZM2.4 6.6h11.2M5.4 1.9v2.6M10.6 1.9v2.6",
};

/** Alle Wuerfel-Ids in der Reihenfolge des Nutzers (unbekannte raus, neue hinten an). */
export function shelfOrder(order: string[]): string[] {
  const all = SHELF_TABS.map((s) => s.id);
  const known = order.filter((id, i) => all.includes(id) && order.indexOf(id) === i);
  return [...known, ...all.filter((id) => !known.includes(id))];
}
