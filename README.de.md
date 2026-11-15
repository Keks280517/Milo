# Milo

**Ein persönlicher KI-Desktop-Assistent für Windows, der als „Dynamic Island“ oben am Bildschirm wohnt.**

Milo ist ein kleiner leuchtender Würfel in einer schwarzen Pille am oberen Bildschirmrand. Ein Klick, und die Insel klappt zu einem vollen Assistenten auf: Chat, Sprache, Wetter, Dateien, Notizen, Systemwerte, Timer und eine Live-Ansicht dessen, was Claude Code gerade tut. Milo denkt mit Google Gemini über deinen eigenen kostenlosen API-Schlüssel oder mit einer lokalen KI, die komplett auf deinem PC läuft. Alltagsbefehle wie Timer, Lautstärke oder Programme öffnen laufen sofort auf deinem PC, ganz ohne KI.

> 🇬🇧 English version: [README.md](README.md)

---

## Highlights

- **Dynamic-Island-Oberfläche.** Eine Pille, die nicht im Weg ist, auf Klick aufspringt, sich bei Vollbild-Programmen versteckt und Klicks rundherum durchlässt. Liquid-Glass-Optik mit Lichtbrechung und gleitender Linse.
- **Ein Charakter mit eigenem Leben.** Milo hat Stimmungen, Gesten und kleine Abenteuer: eine Pflanze, die über die Tage wächst, Wetter von deinem Ort (Regen, Schnee, Wolken), Sterne und Glühwürmchen in der Nacht und kurze Spaziergänge über die Taskleiste.
- **Sofort-Befehle.** Timer, Wecker, Erinnerungen, Lautstärke, Medientasten, Programme und Webseiten öffnen, Bildschirmfotos und „Was läuft gerade?“ funktionieren offline, ganz ohne KI.
- **KI-Chat mit Werkzeugen.** Gemini (optional mit einem sehr schnellen Groq-Modell als Partner und Ersatz) kann Programme über mehrere Monitore öffnen und anordnen, Webseiten bedienen und lesen, mit Quellen im Netz recherchieren, **Word-Berichte** schreiben, deine Dateien lesen (PDF, Word, Excel, PowerPoint), Notizen verwalten und mehr.
- **Lokale KI, wenn du willst.** Kein Google? Milo arbeitet mit [Ollama](https://ollama.com) oder [LM Studio](https://lmstudio.ai) und erkennt einen laufenden Server von selbst. Mit deiner Erlaubnis prüft Milo Prozessor, Arbeitsspeicher und Grafikkarte, empfiehlt ein passendes Modell, lädt es mit einem Klick herunter (Prozent und Restzeit stehen direkt in der Pille) und verbindet es danach von selbst. Ist Ollama installiert, läuft aber nicht, startet Milo es. Allein nutzen (nichts verlässt deinen PC) oder mit Gemini mischen: einfache Fragen lokal, schwere in der Cloud.
- **Sprache, komplett lokal.** Push-to-Talk (`Strg + Alt + K`) oder das Aktivierungswort „Hey Milo“. Die Spracherkennung läuft auf deinem PC (Parakeet, optional Whisper). Ton wird nie gespeichert oder verschickt.
- **Text-Werkzeuge überall.** Text in einem beliebigen Programm markieren, `Strg + Alt + T` drücken und Korrigieren, Förmlicher, Lockerer, Kürzer, Übersetzen, Zusammenfassen oder Erklären wählen. Das Ergebnis ersetzt die Markierung.
- **Begleiter für Claude Code.** Live-Ansicht der Datei, die Claude gerade bearbeitet (mit Änderungen), Befehle, Aufgabenliste und letzte Nachricht. Freigaben erteilen oder ablehnen, Fragen und Pläne direkt in der Insel beantworten, 5-Stunden- und Wochenlimit sehen und mit `/claude` direkt aus Milo mit Claude chatten.
- **Die Pille als Live-Anzeige.** Timer- und Countdown-Karten, anstehende Termine, Claude-Status, eine Uhr, Lautstärke per Mausrad sowie Hinweise zu Downloads, Zwischenablage, Akku und wenn Mikrofon oder Kamera benutzt werden.
- **Eigene Widgets.** Kleine Widgets mit einem Baustein-Editor bauen oder beschreiben und von der KI entwerfen lassen. Sie reihen sich neben der Insel auf.
- **Dateien und Downloads.** Dateien auf die Insel ziehen und wieder herausziehen, laufende Downloads verfolgen, Dateien mit Windows-Vorschaubildern ansehen und verschieben oder kopieren, ohne je etwas zu überschreiben.
- **Verlauf mit Rückgängig.** Jede Aktion wird 30 Tage protokolliert; Lautstärke, geplantes Herunterfahren und verschobene Dateien lassen sich rückgängig machen.
- **Arbeitsplätze.** „Speichere Arbeitsplatz als Schule“ und später „Arbeitsplatz Schule“ öffnet deine Programme wieder und stellt jedes Fenster an seinen Platz.
- **Spielemodus.** Milo erkennt laufende Spiele (Steam, Epic, Riot, Xbox, EA, Ubisoft, GOG, Battle.net, Rockstar, Minecraft und mehr) und legt sich schlafen; dabei schließt er seine eigenen Fenster und braucht fast keine Leistung.
- **Tagesrhythmus.** Ein kurzes Morgen-Briefing und ein Abend-Rückblick, an den Milo sich erinnert.
- **Zuverlässiges Herunterfahren.** „Fahr in 10 Minuten herunter“ klappt ohne Rückfrage, zeigt einen abbrechbaren Countdown und sorgt mit einem Wächter dafür, dass der PC wirklich ausgeht.
- **Eine Startseite, die du selbst anordnest.** Musik, heutige Termine, Wetter, Timer, System, Claude, Notizen und mehr als Karten. Per Ziehen anordnen, Größe wählen oder Milo das Layout machen lassen. Die Wetterkarte ist der Live-Himmel: ziehende Wolken, Regen, Schnee, Blitze, Sterne und Sternschnuppen.
- **Geführter Start.** Ein Willkommens-Assistent (Deutsch oder Englisch), in dem du zwischen Google Gemini und „Ohne Google“ wählst und die lokale KI gleich dort einrichten kannst, dazu eine interaktive Tour. Und vielleicht findest du beim Herumspielen ein kleines Geheimnis.

## Wie Milo ein Modell wählt

Milo bewertet jede Anfrage auf deinem PC von 0 bis 100, sofort und ohne KI, und leitet sie weiter:

| Anfrage | Wer antwortet |
|---|---|
| Sofort-Befehl | Milo selbst, ohne KI, unbegrenzt |
| Leichte Frage, Browser-Aufgaben, Text-Werkzeuge | Gemini Flash-Lite (oder Groq, falls eingerichtet) oder im gemischten Modus deine lokale KI |
| Schwere Frage | Gemini Flash, mit automatischem Ausweichen auf ein anderes Modell |
| Alles, im Modus „Nur lokal“ | Dein lokales Modell über Ollama oder LM Studio |
| Bericht | Recherche über mehrere Suchen, zusammengefasst und als Word-Datei geschrieben |

Milo zählt seinen Verbrauch pro Minute, pro Tag und in Tokens. So weiß er vorher, wann ein Gratis-Kontingent aufgebraucht ist, und wechselt das Modell, statt in Fehler zu laufen. Der Bereich **Verbrauch** zeigt den aktuellen Stand.

## Datenschutz

- **Bleibt auf deinem PC:** Spracherkennung, Gedächtnis und Notizen, Kopien deiner Dateien, Sofort-Befehle und alles, was die lokale KI tut. Ton wird nie gespeichert oder verschickt.
- **Geht an Google (Gemini):** deine Frage, die letzten 8 Gesprächsschritte und was Milo zum Antworten liest (zum Beispiel eine Datei, die du ihm gibst, oder ein Bildschirmfoto, wenn du danach fragst). Milo fragt einmal um Erlaubnis, und du kannst „vor jedem Senden fragen“ einschalten. Ein Protokoll alles Gesendeten findest du in den Einstellungen.
- **API-Schlüssel** liegen nur in der Windows-Anmeldeinformationsverwaltung, nie in einer Datei.
- **Andere Dienste** bekommen nur, was sie brauchen: deinen Ort fürs Wetter (Open-Meteo), Suchbegriffe für Recherchen (DuckDuckGo, Wikipedia).
- **Claude Code:** Milo liest nur, was Claude Code über seine eigenen Hooks, die Statuszeile und das Sitzungsprotokoll meldet. Er gibt nie selbst etwas frei, nur wenn du klickst.

## Loslegen

### Voraussetzungen

- Windows 11 (WebView2 ist dabei)
- Ein kostenloser **Gemini-API-Schlüssel** von [Google AI Studio](https://aistudio.google.com/apikey) **oder** eine lokale KI mit [Ollama](https://ollama.com/download/windows) oder [LM Studio](https://lmstudio.ai) (ohne beides funktionieren nur Sofort-Befehle)
- Optional: ein kostenloser **Groq-API-Schlüssel** von [console.groq.com/keys](https://console.groq.com/keys)
- Optional: die Sprachmodelle (Parakeet, Whisper, Aktivierungswort) für die Spracheingabe

### Installieren

Starte den Installer `Milo_<version>_x64-setup.exe`. Er installiert nur für deinen Benutzer und braucht **keine Administratorrechte**. Beim ersten Start klappt die Insel von selbst auf, und der Willkommens-Assistent fragt nach Sprache, einem freiwilligen Namen und ob Milo mit Google Gemini oder mit einer lokalen KI denken soll; danach zeigt er dir alles. Jeder Schritt lässt sich überspringen und später in den Einstellungen nachholen.

### Selbst bauen

Du brauchst [Rust](https://www.rust-lang.org/tools/install), [Node.js](https://nodejs.org/) und die Visual Studio Build Tools.

```bash
npm install
npm run tauri dev     # Entwicklungsmodus
npm run tauri build   # Release-Build mit Installer
```

Der Installer landet in `src-tauri/target/release/bundle/nsis/`.

### Browser-Erweiterung (optional)

„Milo für den Browser“ lässt Milo in Chrome, Edge, Brave und anderen Chromium-Browsern zuverlässiger klicken, tippen, scrollen und Seiten lesen. Lade `src-tauri/browser-extension/` als entpackte Erweiterung. Ohne sie nutzt Milo die Windows-Bedienungshilfen (UI Automation).

## Technik

- [Tauri 2](https://tauri.app/) mit **Rust**-Backend
- **Svelte 5** + SvelteKit (statisch) + TypeScript + Vite für die Oberfläche
- Google Gemini und Groq (OpenAI-kompatibel) als Cloud-KI, Ollama und LM Studio als lokale KI
- Lokale Spracherkennung (Parakeet / Whisper) und Aktivierungswort-Erkennung

## Projektaufbau

| Pfad | Inhalt |
|---|---|
| `src-tauri/src` | Rust-Backend: `agent` (Gespräch und Werkzeugrunden), `gemini`, `groq`, `local` (lokale KI), `router` (Bewertung der Anfragen), `limits` (Kontingentzähler), `tools`, `quick` (Offline-Befehle), `browser` / `browser_bridge`, `screens`, `power`, `claude` / `claudechat`, `voice`, `tts`, `widgets`, `actionlog`, `workspaces`, `gamemode`, `pulse` und mehr |
| `src/lib` | Svelte-Oberfläche: `Island.svelte`, `Chat.svelte`, `character/` (Milo und die Kulisse), `views/` (Bereiche), `cards/`, `widgets/`, `theme.css` (Design-Tokens) |
| `src/routes` | Weitere Fenster: Einstellungen, Widgets, Desktop-Ebene |
| `src-tauri/browser-extension` | Die Erweiterung „Milo für den Browser“ (Manifest V3) |
| `scripts/` | Claude-Code-Hook und Statuszeile, Entwicklungs- und Testhelfer, ein Gemini-Testserver |

## Stand und bekannte Grenzen

Milo ist ein persönliches Projekt in aktiver Entwicklung und läuft **nur unter Windows**.

- Ohne Internet funktionieren nur Sofort-Befehle (und die lokale KI).
- Das Aktivierungswort „Hey Milo“ lässt sich von Fernsehton oder anderen Stimmen täuschen. Die Empfindlichkeit ist einstellbar.
- Die Insel und Milos Spaziergänge erscheinen nur auf dem Hauptmonitor. Fensterplatzierung über mehrere Monitore und andere Anzeigeskalierungen als 100 % sind nicht vollständig getestet.
- Claudes Nutzungslimits kommen aus der Statuszeile von Claude Code und brauchen einen Pro- oder Max-Plan.
- Alte `.doc`- / `.xls`-Dateien und eingescannte PDFs lassen sich nicht lesen.
- Twitch-Live-Prüfungen und Web-Recherche hängen an öffentlichen Webseiten und können brechen, wenn sich diese ändern.

## Sicherheit

Eine Sicherheitslücke gefunden? Bitte privat melden, siehe [SECURITY.md](SECURITY.md).

## Lizenz

[MIT](LICENSE)

Milo ist ein unabhängiges Projekt und steht in keiner Verbindung zu Anthropic, Google, Apple, Microsoft oder Twitch. Alle Produktnamen und Marken gehören ihren jeweiligen Inhabern.
