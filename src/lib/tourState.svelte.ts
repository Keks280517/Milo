import { t } from "./i18n";
import type { TabId } from "./app.svelte";

// Gefuehrte Tour nach der Einrichtung: geht mit dem Nutzer durch alle wichtigen Bereiche der Insel.
// Jeder Schritt hebt einen Bereich hervor (der Rest wird abgedunkelt) und erklaert ihn in einem Kasten.
// Wo es hilft, laufen gespielte Beispiele ab ("Geisterdaten"): sie sind reine Animation, werden nie
// gespeichert und verschwinden mit dem Schritt. Der Schritt wird gemerkt, damit ein Neuladen
// (Sprachwechsel) oder Zuklappen die Tour nicht verliert.

/** Welche gespielte Vorfuehrung zu einem Schritt gehoert. */
export type Demo = "" | "chat" | "claude" | "agenda" | "timer" | "weather" | "files" | "notes" | "system" | "claudeLive" | "buddy" | "usage" | "history" | "milo";

export interface TourStep {
  /** Diese Ansicht wird fuer den Schritt geoeffnet. */
  tab?: TabId;
  /** CSS-Auswahl des hervorgehobenen Bereichs; leer = Kasten mittig ohne Hervorhebung. */
  target?: string;
  title: string;
  text: string;
  demo?: Demo;
  /** Kleiner Zusatzhinweis unter dem Text (z. B. eine Einschraenkung). */
  note?: string;
  /** Fuer diesen Schritt ein eigenes Fenster zeigen (danach schliesst es sich wieder). */
  window?: "settings";
}

const STORE = "milo.tourStep";

export function tourSteps(name: string): TourStep[] {
  return [
    {
      title: t(`Schön, dass du da bist!`, `Great to have you here!`),
      text: t(`Ich zeige dir in gut zwei Minuten alles Wichtige, mit ein paar Beispielen, wie es aussehen kann.`, `In about two minutes I'll show you everything important, with a few examples of how it can look.`),
    },
    {
      target: ".shelf",
      title: t("Die Leiste", "The bar"),
      text: t("Hier unten wechselst du zwischen allen Bereichen. Jedes Symbol ist eine Ansicht; die Glaslinse zeigt, wo du gerade bist, und Punkte melden Neues. Welche Symbole hier stehen und in welcher Reihenfolge, stellst du in den Einstellungen ein.", "Down here you switch between all areas. Each symbol is a view; the glass lens shows where you are, and dots report news. Which symbols appear and in what order is up to you in the settings."),
    },
    {
      tab: "chat",
      target: ".body > .pane",
      demo: "chat",
      title: t(`Mit ${name} chatten`, `Chat with ${name}`),
      text: t(`Schreib oder sprich einfach, was du brauchst: Timer, Wetter, Programme öffnen, Musik, Fragen, Recherche. ${name} erledigt es und zeigt das Ergebnis direkt im Chat.`, `Just type or say what you need: timers, weather, opening apps, music, questions, research. ${name} takes care of it and shows the result right in the chat.`),
    },
    {
      tab: "chat",
      target: ".body > .pane",
      demo: "claude",
      title: t("Claude direkt aus dem Chat", "Claude right from the chat"),
      text: t("Tippe „/claude “ und das Feld wird orange: alles Weitere geht an Claude Code, in die zuletzt benutzte Sitzung. Oben wählst du eine andere Sitzung, einen neuen Chat oder ein anderes Projekt. Mit „/“ siehst du alle Befehle, Skills und Plugins.", "Type \"/claude \" and the field turns orange: everything after that goes to Claude Code, into the most recently used session. At the top you can pick another session, a new chat or another project. Type \"/\" to see all commands, skills and plugins."),
      note: t("Gut zu wissen: Die Claude-App zeigt Nachrichten, die du hier schreibst, erst, wenn du die Sitzung dort neu öffnest. Es ist trotzdem dieselbe Unterhaltung.", "Good to know: the Claude app only shows messages you write here once you reopen the session there. It's still the same conversation."),
    },
    {
      tab: "home",
      target: ".body > .pane",
      title: t("Die Übersicht", "Home"),
      text: t("Deine wichtigsten Karten auf einen Blick: Musik, Termine, Wetter, System und mehr. Welche Karten es gibt und wie groß sie sind, legst du in den Einstellungen fest.", "Your most important cards at a glance: music, events, weather, system and more. Which cards appear and how big they are is up to you in the settings."),
    },
    {
      tab: "agenda",
      target: ".body > .pane",
      demo: "agenda",
      title: t("Agenda", "Agenda"),
      text: t("Deine Termine aus dem Kalender. Kurz vor einem Termin meldet sich die Pille oben mit einem Countdown.", "Your events from your calendar. Shortly before an event, the pill at the top shows a countdown."),
    },
    {
      tab: "timer",
      target: ".body > .pane",
      demo: "timer",
      title: t("Timer, Fokus und Wecker", "Timers, focus and alarms"),
      text: t("„Timer 10 Minuten“, „Weck mich um 7“ oder eine Fokus-Phase: alles läuft hier zusammen. Ist ein Timer fertig, klingelt es und die Pille zeigt es.", "\"Timer 10 minutes\", \"Wake me at 7\" or a focus session: it all comes together here. When a timer is done, it rings and the pill shows it."),
    },
    {
      tab: "weather",
      target: ".body > .pane",
      demo: "weather",
      title: t("Wetter", "Weather"),
      text: t("Jetzt, die nächsten Stunden und die nächsten Tage. Zieht Regen auf, sagt dir die Pille vorher Bescheid.", "Right now, the next few hours and the next days. If rain is on its way, the pill lets you know beforehand."),
    },
    {
      tab: "files",
      target: ".body > .pane",
      demo: "files",
      title: t("Dateien", "Files"),
      text: t("Downloads und Lieblingsordner mit Vorschau. Verschieben und Kopieren lässt sich im Verlauf rückgängig machen.", "Downloads and favorite folders with previews. Moving and copying can be undone in the history."),
    },
    {
      tab: "notes",
      target: ".body > .pane",
      demo: "notes",
      title: t("Notizen", "Notes"),
      text: t(`„Merk dir …“ im Chat oder die Schnellnotiz per Tastenkürzel landen hier. ${name} findet sie später wieder, wenn du danach fragst.`, `"Remember …" in the chat or the quick note shortcut end up here. ${name} finds them again later when you ask.`),
    },
    {
      tab: "system",
      target: ".body > .pane",
      demo: "system",
      title: t("Was dein PC gerade macht", "What your PC is doing"),
      text: t("Prozessor, Arbeitsspeicher, Laufwerke und Netzwerk live, dazu die Programme, die gerade am meisten brauchen. Gemessen wird nur, solange diese Ansicht offen ist.", "Processor, memory, drives and network live, plus the programs using the most right now. It only measures while this view is open."),
    },
    {
      tab: "claude",
      target: ".body > .pane",
      demo: "claudeLive",
      title: t("Claude live mitverfolgen", "Follow Claude live"),
      text: t("Ist Claude Code verbunden, siehst du hier, woran Claude gerade schreibt, die Aufgabenliste und Claudes letzte Nachricht. Fragen und Freigaben beantwortest du direkt hier, ohne die Claude-App.", "With Claude Code connected you see here what Claude is writing right now, its task list and its last message. You answer questions and approvals right here, without the Claude app."),
    },
    {
      tab: "claude",
      target: ".stage",
      demo: "buddy",
      title: t("Dein Claude-Buddy", "Your Claude buddy"),
      text: t(`Der kleine orange Würfel neben ${name} ist Claude. Er taucht auf, sobald Claude Code auf deinem PC arbeitet, hüpft beim Arbeiten mit, hebt die Hand, wenn Claude eine Frage oder Freigabe hat, und freut sich, wenn Claude fertig ist. So siehst du auf einen Blick, was Claude gerade macht, ohne die Claude-App zu öffnen.`, `The little orange cube next to ${name} is Claude. It shows up as soon as Claude Code is working on your PC, bounces along while it works, raises its hand when Claude has a question or needs approval, and cheers when Claude is done. That way you see at a glance what Claude is doing, without opening the Claude app.`),
    },
    {
      tab: "usage",
      target: ".body > .pane",
      demo: "usage",
      title: t("Verbrauch", "Usage"),
      text: t(`Wie viel von den kostenlosen Kontingenten bei Gemini und Groq heute schon genutzt ist und wann sie sich zurücksetzen. Wird eins knapp, weicht ${name} auf das andere Modell aus.`, `How much of the free Gemini and Groq quotas has been used today and when they reset. If one runs low, ${name} switches to the other model.`),
    },
    {
      tab: "history",
      target: ".body > .pane",
      demo: "history",
      title: t("Verlauf mit Rückgängig", "History with undo"),
      text: t(`Alles, was ${name} auf deinem PC getan hat, steht hier, und vieles lässt sich mit einem Klick rückgängig machen.`, `Everything ${name} did on your PC is listed here, and much of it can be undone with one click.`),
    },
    {
      tab: "home",
      target: ".menu-btn[aria-expanded]",
      title: t("Das Menü", "The menu"),
      text: t("Hier oben rechts geht es zum Widget-Baukasten, zu den Einstellungen und zur Deko-Bibliothek.", "Up here on the right you'll find the widget builder, the settings and the decor library."),
    },
    {
      tab: "widgets",
      target: ".body > .pane",
      title: t("Eigene Widgets", "Your own widgets"),
      text: t("Stell dir Widgets aus über 40 Bausteinen zusammen, oder beschreib sie der KI und such dir einen von drei Entwürfen aus. Sie hängen dann oben am Bildschirmrand neben der Insel.", "Build widgets from over 40 blocks, or describe them to the AI and pick one of three designs. They then hang at the top edge of your screen next to the island."),
    },
    {
      tab: "home",
      target: ".shelf .util:last-child",
      title: t("Deko-Bibliothek", "Decor library"),
      text: t(`Pflanze, Laterne, Ente und mehr: Stell sie in die Insel oder zieh sie auf den Desktop. ${name} besucht sie dort manchmal.`, `Plant, lantern, duck and more: put them in the island or drag them onto your desktop. ${name} sometimes pays them a visit.`),
    },
    {
      tab: "home",
      target: ".menu-btn[aria-expanded]",
      window: "settings",
      title: t("Einstellungen", "Settings"),
      text: t("Die Einstellungen haben ein eigenes Fenster – es ist gerade links aufgegangen, und du erreichst es jederzeit über das Menü hier oben. Dort passt du alles an: Aussehen, die Leiste unten, Sprache, Stimme, Schlüssel, den Spielemodus und die Browser-Erweiterung, mit der Netflix und Co. viel zuverlässiger klappen.", "The settings have their own window – it just opened on the left, and you can always reach it from the menu up here. That's where you adjust everything: appearance, the bar at the bottom, language, voice, keys, game mode and the browser extension that makes Netflix and co. much more reliable."),
    },
    {
      tab: "home",
      target: ".stage",
      title: t(`${name} selbst`, `${name} himself`),
      demo: "milo",
      text: t(`Zum Schluss noch ein Tipp: Du kannst ${name} einfach aus der Insel ziehen und auf deinem Desktop oder der Taskleiste absetzen. Dort läuft er herum, und du kannst ihn greifen, werfen und wieder zurückholen. Mit den Deko-Objekten geht das genauso.`, `One last tip: you can simply drag ${name} out of the island and drop him on your desktop or taskbar. He wanders around there, and you can grab him, toss him and bring him back. The decor works the same way.`),
      note: t(`Und wer weiß … vielleicht entdeckst du dabei ja noch etwas, das er dir bisher verschwiegen hat.`, `And who knows … maybe you'll discover something he hasn't told you yet.`),
    },
  ];
}

class Tour {
  active = $state(false);
  index = $state(0);
  /** Ende erreicht: "Geschafft!"-Bildschirm. */
  done = $state(false);
  /** Rueckfrage beim Ueberspringen. */
  confirmSkip = $state(false);
  /** Gespielte Vorfuehrung des aktuellen Schritts; die Tour setzt sie, Ansichten wie der Chat spielen sie. */
  demo = $state<Demo>("");
  /** Mitte des hervorgehobenen Bereichs, waagrecht, relativ zur Inselmitte (px); der Kasten rueckt leicht dorthin. */
  focusX = $state(0);

  constructor() {
    try {
      const saved = localStorage.getItem(STORE);
      if (saved !== null) {
        this.active = true;
        this.index = Math.max(0, Number(saved) || 0);
      }
    } catch {
      /* ohne Speicher keine Fortsetzung */
    }
    // Im Einstellungsfenster gestartet: die Insel (anderes Fenster, gleicher Speicher) uebernimmt.
    if (typeof window !== "undefined") {
      window.addEventListener("storage", (e) => {
        if (e.key !== STORE) return;
        if (e.newValue === null) {
          this.active = false;
          this.done = false;
        } else if (!this.active) {
          this.active = true;
          this.done = false;
          this.index = Math.max(0, Number(e.newValue) || 0);
        }
      });
    }
  }

  private persist() {
    try {
      if (this.active) localStorage.setItem(STORE, String(this.index));
      else localStorage.removeItem(STORE);
    } catch {
      /* egal */
    }
  }

  start() {
    this.active = true;
    this.done = false;
    this.confirmSkip = false;
    this.index = 0;
    this.persist();
  }

  go(index: number, count: number) {
    if (index >= count) {
      this.done = true;
      return;
    }
    this.index = Math.max(0, index);
    this.persist();
  }

  /** Tour beenden (nach "Geschafft!" oder bestaetigtem Ueberspringen). */
  end() {
    this.active = false;
    this.done = false;
    this.confirmSkip = false;
    this.demo = "";
    this.index = 0;
    this.persist();
  }
}

export const tour = new Tour();
