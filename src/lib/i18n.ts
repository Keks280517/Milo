// Zwei Sprachen: Deutsch und Englisch. Jeder Text steht direkt neben seiner Uebersetzung im Code,
// t("Deutsch", "English"), damit nichts auseinanderlaeuft und beide Fassungen als ganze Saetze
// geschrieben werden (keine Wort-fuer-Wort-Uebersetzung).
//
// Die Sprache steht vor dem ersten Zeichnen fest: aus localStorage (alle Fenster teilen sich den
// Ursprung) oder aus der Systemsprache. Aendert sie sich in den Einstellungen, laden die Fenster neu;
// so stimmen auch Texte in Konstanten, die nur einmal ausgewertet werden.

export type Lang = "de" | "en";

const KEY = "milo.lang";

export function systemLang(): Lang {
  if (typeof navigator === "undefined") return "de";
  return (navigator.language || "en").toLowerCase().startsWith("de") ? "de" : "en";
}

function initial(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === "de" || saved === "en") return saved;
  } catch {
    /* ohne Speicher: Englisch */
  }
  // Erster Start: Englisch. Die Sprache waehlt man im Willkommens-Assistenten.
  return "en";
}

export const lang: Lang = typeof window === "undefined" ? "de" : initial();

/** Text in der aktuellen Sprache. */
export const t = (de: string, en: string): string => (lang === "en" ? en : de);

/** Fuer toLocaleString und Co. */
export const locale = lang === "en" ? "en-US" : "de-DE";

/** Sprache aus den Einstellungen ("auto" = Systemsprache) uebernehmen; weicht sie ab, neu laden. */
export function applyLanguage(setting: string | undefined) {
  if (typeof window === "undefined") return;
  const want: Lang = setting === "de" || setting === "en" ? setting : systemLang();
  try {
    localStorage.setItem(KEY, want);
  } catch {
    /* dann eben ohne Merken */
  }
  if (want !== lang) location.reload();
}
