<script lang="ts">
  import { t } from "$lib/i18n";
  // Eigenes Einstellungsfenster: links die Bereiche, rechts deren Einstellungen. Alles wird sofort
  // gespeichert; die Insel uebernimmt Aenderungen ueber das Ereignis milo://settings.
  import "@fontsource-variable/inter/wght.css";
  import "@fontsource-variable/jetbrains-mono/wght.css";
  import "$lib/theme.css";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import SettingsView from "$lib/views/SettingsView.svelte";
  import { app, inTauri } from "$lib/app.svelte";
  import { paneIn } from "$lib/motion";
  import GlassDefs from "$lib/GlassDefs.svelte";
  import { lens } from "$lib/lens";

  const SECTIONS = [
    { id: "general", label: t("Allgemein", "General"), hint: t("Sprache, Name, Tagesablauf, Text-Werkzeuge", "Language, name, daily routine, text tools"), parts: ["general", "daily", "textwork"], icon: "M8 2.6v1.6M8 11.8v1.6M2.6 8h1.6M11.8 8h1.6M4.2 4.2l1.1 1.1M10.7 10.7l1.1 1.1M4.2 11.8l1.1-1.1M10.7 5.3l1.1-1.1M8 10.4a2.4 2.4 0 1 0 0-4.8 2.4 2.4 0 0 0 0 4.8Z" },
    { id: "look", label: t("Aussehen", "Appearance"), hint: t("Farbe, Größe, Glas, Übersicht, Leiste, Deko", "Color, size, glass, home, bar, decor"), parts: ["look", "home", "shelf", "decor"], icon: "M8 13.6a5.6 5.6 0 1 1 5.6-5.6c0 1.5-1.2 2.4-2.6 2.4H9.6a1.2 1.2 0 0 0-.8 2.1c.5.5.2 1.1-.8 1.1ZM5 7.4h.01M7 5h.01M10 5.3h.01" },
    { id: "pill", label: t("Pille & Hinweise", "Pill & alerts"), hint: t("Downloads, Bildschirmfotos, Zwischenablage, Akku, Regen, Schnellnotiz", "Downloads, screenshots, clipboard, battery, rain, quick note"), parts: ["pill"], icon: "M5 4.5h6a3.5 3.5 0 0 1 0 7H5a3.5 3.5 0 0 1 0-7ZM11 8h.01" },
    { id: "ai", label: t("KI", "AI"), hint: t("Gemini, Groq, lokale KI, Bewertung", "Gemini, Groq, local AI, rating"), parts: ["gemini", "groq", "local", "rating", "cloudlog"], icon: "M8 2.2v11.6M2.2 8h11.6M3.9 3.9l8.2 8.2M12.1 3.9l-8.2 8.2" },
    { id: "voice", label: t("Sprachsteuerung", "Voice"), hint: t("Mikrofon, „Hey Milo“, Vorlesen", "Microphone, \"Hey Milo\", read aloud"), parts: ["voice"], icon: "M8 10.2a2.2 2.2 0 0 0 2.2-2.2V4.2a2.2 2.2 0 0 0-4.4 0V8A2.2 2.2 0 0 0 8 10.2ZM4.2 7.6a3.8 3.8 0 0 0 7.6 0M8 11.4v2.2" },
    { id: "connect", label: t("Verbindungen", "Connections"), hint: t("Claude Code, Browser-Erweiterung, Kalender", "Claude Code, browser extension, calendar"), parts: ["claude", "browserext", "calendar"], icon: "M6.6 9.4l2.8-2.8M5.4 7.6l-1.2 1.2a2.3 2.3 0 0 0 3.2 3.2l1.2-1.2M10.6 8.4l1.2-1.2a2.3 2.3 0 0 0-3.2-3.2L7.4 5.2" },
    { id: "system", label: t("System", "System"), hint: t("Spielemodus, Über Milo", "Game mode, about Milo"), parts: ["gamemode", "about"], icon: "M2.6 4.2a1.2 1.2 0 0 1 1.2-1.2h8.4a1.2 1.2 0 0 1 1.2 1.2v5.6a1.2 1.2 0 0 1-1.2 1.2H3.8a1.2 1.2 0 0 1-1.2-1.2ZM6 13.2h4M8 11v2.2" },
  ];

  const initial = typeof location !== "undefined" ? new URLSearchParams(location.search).get("section") : null;
  let current = $state(SECTIONS.find((s) => s.id === initial)?.id ?? "general");
  const section = $derived(SECTIONS.find((s) => s.id === current) ?? SECTIONS[0]);

  $effect(() => {
    const root = document.documentElement;
    root.style.setProperty("--accent", app.settings.accent || "#64d2ff");
    root.style.setProperty("--focus", app.settings.accent || "#64d2ff");
    root.dataset.glass = String(app.settings.glass ?? 1);
    root.classList.toggle("reduce-motion", !!app.settings.reduceMotion);
  });

  onMount(() => {
    app.initLite();
    let stop = () => {};
    if (inTauri) listen<string>("milo://settings-section", ({ payload }) => {
      if (SECTIONS.some((s) => s.id === payload)) current = payload;
    }).then((fn) => (stop = fn));
    return () => stop();
  });
</script>

<svelte:head><title>{t("Milo – Einstellungen", "Milo – Settings")}</title></svelte:head>
<GlassDefs />

<div class="window">
  <span class="ambient" aria-hidden="true"></span>
  <aside class="glass">
    <div class="brand">
      <span class="logo" aria-hidden="true"><i></i><i></i></span>
      <span>
        <strong>{app.settings.name}</strong>
        <small>{t("Einstellungen", "Settings")}</small>
      </span>
    </div>
    <nav aria-label={t("Bereiche", "Sections")} use:lens={{ selector: "button.on" }}>
      <span class="lens-thumb" aria-hidden="true"></span>
      {#each SECTIONS as item (item.id)}
        <button class:on={current === item.id} onclick={() => (current = item.id)} aria-current={current === item.id ? "page" : undefined}>
          <svg viewBox="0 0 16 16" width="17" height="17" aria-hidden="true"><path d={item.icon} fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
          <span>
            <span class="nlabel">{item.label}</span>
            <span class="nhint">{item.hint}</span>
          </span>
        </button>
      {/each}
    </nav>
    <p class="saved">{t("Änderungen gelten sofort.", "Changes apply right away.")}</p>
  </aside>
  <main>
    {#key current}
      <div class="page" in:paneIn>
        <header>
          <h1>{section.label}</h1>
          <p>{section.hint}</p>
        </header>
        <div class="card">
          <SettingsView only={section.parts} />
        </div>
      </div>
    {/key}
  </main>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    /* Durchscheinend fuer Mica; ohne Mica bleibt es ein dunkles, gut lesbares Grau. */
    background: rgba(18, 18, 20, 0.78);
    color: var(--text);
    font-family: var(--font-sans);
    font-feature-settings: "cv11", "ss03";
    -webkit-font-smoothing: antialiased;
    overflow: hidden;
    color-scheme: dark;
  }
  .window {
    position: relative;
    isolation: isolate;
    height: 100vh;
    display: grid;
    grid-template-columns: 268px 1fr;
  }
  /* Lichtebene wie in der Insel: gibt der Glasleiste links etwas zu brechen. */
  .ambient {
    position: absolute;
    inset: 0;
    z-index: -1;
    pointer-events: none;
    background:
      radial-gradient(46% 60% at 0% 100%, color-mix(in oklab, var(--accent) 22%, transparent), transparent 70%),
      radial-gradient(40% 50% at 8% 0%, color-mix(in oklab, color-mix(in oklch, var(--accent), #8a5cff 70%) 18%, transparent), transparent 70%);
  }
  /* Seitenleiste: eine schwebende Glasflaeche mit Abstand zum Fensterrand. */
  aside {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    margin: var(--s-2);
    padding: var(--s-4) var(--s-2) var(--s-3);
    border-radius: var(--r-xl);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px 8px;
  }
  .brand strong {
    display: block;
    font-size: var(--fs-callout);
  }
  .brand small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .logo {
    width: 34px;
    height: 34px;
    border-radius: var(--r-sm);
    background: linear-gradient(180deg, #fff, #dcdce2);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.4), inset 0 -2px 0 rgba(0, 0, 0, 0.08);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .logo i {
    width: 4px;
    height: 7px;
    border-radius: 2px;
    background: #1c1c1e;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  /* Auswahl: ein Stueck helleres Glas, das zum gewaehlten Bereich gleitet. */
  nav > :global(.lens-thumb) {
    --lens-r: calc(var(--r-xl) - var(--s-2));
    z-index: 0;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.16), rgba(255, 255, 255, 0.07));
    box-shadow: inset 0 1px 0.5px rgba(255, 255, 255, 0.3), inset 0 0 0 0.5px rgba(255, 255, 255, 0.1), var(--lift-1);
  }
  nav button {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-2) var(--s-3);
    border: 0;
    border-radius: calc(var(--r-xl) - var(--s-2));
    background: none;
    color: var(--text-2);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: color var(--t-fast) ease;
  }
  nav button > span {
    min-width: 0;
  }
  nav button:hover,
  nav button.on {
    color: var(--text);
  }
  nav button svg {
    flex: none;
    color: var(--text-3);
    transition: color var(--t-base) ease;
  }
  nav button.on svg {
    color: var(--accent);
  }
  nav button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .nlabel {
    display: block;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .nhint {
    display: block;
    font-size: var(--fs-caption);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .saved {
    margin: auto 10px 0;
    font-size: var(--fs-footnote);
    color: var(--text-4);
  }
  main {
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #3a3a3e transparent;
  }
  .page {
    max-width: 720px;
    padding: var(--s-8) var(--s-8) 40px var(--s-6);
  }
  header h1 {
    margin: 0;
    font-size: var(--fs-display);
    font-weight: var(--fw-bold);
    letter-spacing: var(--tr-display);
    line-height: 1.1;
  }
  header p {
    margin: var(--s-1) 0 var(--s-6);
    font-size: var(--fs-callout);
    color: var(--text-3);
  }
  .card {
    display: flex;
    flex-direction: column;
  }
  /* Die Einstellungen der Insel scrollen hier nicht selbst; das Fenster scrollt. Jeder Abschnitt
     wird zu einer eigenen Gruppe: Titel darueber, die Zeilen als Liste mit Haarlinien darin. */
  .card :global(.settings) {
    overflow: visible;
    padding: 0;
    gap: var(--s-6);
  }
  .card :global(.settings > section) {
    gap: 0;
    padding: var(--s-1) var(--s-4);
    border-radius: var(--r-lg);
    background: var(--card);
    box-shadow: var(--card-edge);
  }
  .card :global(.settings > section > *) {
    padding-block: var(--s-3);
  }
  .card :global(.settings > section > * + *) {
    border-top: 0.5px solid var(--line);
  }
  .card :global(.settings > section > h2) {
    padding-block: var(--s-3) var(--s-2);
  }
  .card :global(.settings > section > h2 + *) {
    border-top: 0;
    padding-top: var(--s-1);
  }
</style>
