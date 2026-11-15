<script lang="ts">
  import { t, lang } from "./i18n";
  // Willkommens-Assistent beim ersten Start: Sprache, Name (freiwillig), Gemini-Schluessel mit
  // Anleitung, Groq-Schluessel (ausdruecklich freiwillig) und eine kurze Tour durch die Insel.
  // Jeder Schritt laesst sich ueberspringen. Der Schritt wird gemerkt, weil ein Sprachwechsel die
  // Fenster neu laedt. Die Minispiele verraten wir nicht: nur ein Hinweis auf ein verstecktes Geheimnis.
  import { fly } from "svelte/transition";
  import { app, inTauri } from "./app.svelte";
  import { tour } from "./tourState.svelte";
  import LocalAiSetup from "./LocalAiSetup.svelte";
  import { localAi } from "./localAi.svelte";

  type Step = "hello" | "name" | "gemini" | "groq" | "tour";
  const STEPS: Step[] = ["hello", "name", "gemini", "groq", "tour"];
  const STORE = "milo.welcomeStep";

  let step = $state<Step>(saved());
  function saved(): Step {
    try {
      const v = localStorage.getItem(STORE) as Step | null;
      return v && STEPS.includes(v) ? v : "hello";
    } catch {
      return "hello";
    }
  }
  function go(next: Step) {
    step = next;
    try {
      localStorage.setItem(STORE, next);
    } catch {
      /* ohne Speicher: nach einem Neuladen beginnt es eben vorn */
    }
  }
  const index = $derived(STEPS.indexOf(step));
  const next = () => go(STEPS[Math.min(STEPS.length - 1, index + 1)]);
  const back = () => go(STEPS[Math.max(0, index - 1)]);

  async function open(url: string) {
    if (!inTauri) return window.open(url, "_blank");
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    openUrl(url);
  }

  // ---------- Sprache ----------
  function pickLanguage(next: "de" | "en") {
    // Nur umstellen, nicht weiterspringen: die Fenster laden in der neuen Sprache neu und der
    // Assistent bleibt bei der Sprachwahl. Weiter geht es erst mit dem Knopf.
    if (next !== lang || app.settings.language !== next) app.saveSettings({ language: next });
  }

  // ---------- Name ----------
  let name = $state(app.settings.userName);
  function saveName() {
    app.saveSettings({ userName: name.trim() });
    next();
  }

  // ---------- KI: Google Gemini oder eine KI nur auf diesem PC ----------
  // Wer „Ohne Google“ waehlt, richtet die lokale KI gleich hier ein; Groq (auch Cloud) entfaellt dann.
  let path = $state<"google" | "local">(app.settings.aiMode === "local" ? "local" : "google");
  function choosePath(next: "google" | "local") {
    path = next;
    if (next === "local" && app.settings.aiMode !== "local") app.saveSettings({ aiMode: "local" });
    // Zurueck zu Google, ohne dass schon eine lokale KI eingerichtet ist: wieder ueber die Cloud.
    if (next === "google" && app.settings.aiMode === "local") app.saveSettings({ aiMode: app.settings.localModel ? "mixed" : "cloud" });
  }
  const localReady = $derived(!!app.settings.localModel && app.settings.aiMode !== "cloud" && !localAi.running);
  /** Weiter nach dem KI-Schritt: ohne Google gleich zur Tour (Groq ist auch eine Cloud). */
  function afterAi() {
    if (path === "local") go("tour");
    else next();
  }
  function backFromTour() {
    if (path === "local") go("gemini");
    else back();
  }

  let geminiKey = $state("");
  let geminiBusy = $state(false);
  let geminiError = $state("");
  async function saveGemini() {
    // Vorschau im Browser: nichts speichern, nur weiterblaettern.
    if (!inTauri) return next();
    if (!geminiKey.trim() || geminiBusy) return;
    geminiBusy = true;
    geminiError = "";
    try {
      await app.setCloudKey(geminiKey.trim());
      geminiKey = "";
      // Mit Schluessel und schon eingerichteter lokaler KI: beides nutzen.
      if (app.settings.aiMode === "local") await app.saveSettings({ aiMode: "mixed" });
      next();
    } catch (err) {
      geminiError = String(err);
    } finally {
      geminiBusy = false;
    }
  }

  // ---------- Groq ----------
  let groqKey = $state("");
  let groqBusy = $state(false);
  let groqError = $state("");
  let groqSaved = $state(false);
  async function saveGroq() {
    // Vorschau im Browser: nichts speichern, nur weiterblaettern.
    if (!inTauri) return next();
    if (!groqKey.trim() || groqBusy) return;
    groqBusy = true;
    groqError = "";
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_groq_key", { key: groqKey.trim() });
      groqKey = "";
      groqSaved = true;
      next();
    } catch (err) {
      groqError = String(err);
    } finally {
      groqBusy = false;
    }
  }

  // ---------- Fertig ----------
  function finish() {
    try {
      localStorage.removeItem(STORE);
    } catch {
      /* egal */
    }
    app.saveSettings({ onboarded: true });
    app.tab = "home";
    // Danach fuehrt die Tour durch alle Bereiche.
    tour.start();
  }

  const TOUR = $derived([
    {
      icon: "M3 6.5h10M5 3.5h6a2.5 2.5 0 0 1 2.5 2.5v4A2.5 2.5 0 0 1 11 12.5H5A2.5 2.5 0 0 1 2.5 10V6A2.5 2.5 0 0 1 5 3.5Z",
      title: t("Die Insel", "The island"),
      text: t(`Sie sitzt oben am Bildschirm. Ein Klick klappt sie auf und zu, ${app.settings.hotkey} öffnet sie von überall.`, `It sits at the top of your screen. One click opens and closes it, ${app.settings.hotkey} opens it from anywhere.`),
    },
    {
      icon: "M5 4.5h6a3.5 3.5 0 0 1 0 7H5a3.5 3.5 0 0 1 0-7Z",
      title: t("Die Pille meldet sich", "The pill keeps you posted"),
      text: t("Timer, Termine, Downloads, Bildschirmfotos oder Regen: Die Pille wächst kurz und zeigt es. Mausrad darüber ändert die Lautstärke.", "Timers, events, downloads, screenshots or rain: the pill grows briefly to show it. Scroll over it to change the volume."),
    },
    {
      icon: "M3 11.5h10M4.5 11.5V6.5M8 11.5V4M11.5 11.5V8",
      title: t("Die Würfelleiste", "The cube shelf"),
      text: t("Unten wechselst du zwischen Übersicht, Chat, Kalender, Wetter, Dateien, Notizen, Claude und mehr.", "At the bottom you switch between home, chat, calendar, weather, files, notes, Claude and more."),
    },
    {
      icon: "M8 2.5v6M5.5 6.5a2.5 2.5 0 0 0 5 0V4.5a2.5 2.5 0 0 0-5 0ZM4 7.5a4 4 0 0 0 8 0M8 11.5v2",
      title: t("Einfach fragen", "Just ask"),
      text: t(`Schreib oder sprich: „Timer 10 Minuten“, „Wie wird das Wetter?“, „Öffne Spotify“. Mit dem Mikrofon geht es auch per Stimme.`, `Type or talk: "Timer 10 minutes", "What's the weather like?", "Open Spotify". The microphone lets you use your voice too.`),
    },
    {
      icon: "M3 5h10M3 8h10M3 11h6",
      title: t("Menü und Einstellungen", "Menu and settings"),
      text: t("Oben rechts in der offenen Insel. Dort findest du auch Widgets, Deko und alle Schlüssel wieder.", "Top right in the open island. That's also where you'll find widgets, decor and all your keys again."),
    },
    {
      icon: "M5 13.5h6M6 13.5l-.6-4h5.2l-.6 4M8 9.5V7M8 7c-.2-1.8-1.4-2.8-3-2.9.1 1.7 1.2 2.8 3 2.9ZM8 7c.2-2.2 1.6-3.4 3.4-3.5-.1 2-1.4 3.4-3.4 3.5Z",
      title: t("Deko", "Decor"),
      text: t("Pflanze, Laterne und Co. lassen sich verschieben. Rechtsklick auf ein Objekt öffnet sein Menü.", "Plant, lantern and friends can be moved around. Right-click an object to open its menu."),
    },
  ]);
</script>

<div class="welcome" role="dialog" aria-modal="true" aria-label={t("Willkommen", "Welcome")}>
  <div class="dots" aria-hidden="true">
    {#each STEPS as s, i}<span class:on={i === index} class:done={i < index}></span>{/each}
  </div>

  {#key step}
    <div class="card" in:fly={{ y: 10, duration: 260 }}>
      {#if step === "hello"}
        <!-- Nur die aktuelle Sprache; ein Klick auf die andere stellt sofort um (die Seite laedt neu). -->
        <h1>{t(`Willkommen bei ${app.settings.name}!`, `Welcome to ${app.settings.name}!`)}</h1>
        <p class="lead">{t("Dein kleiner Helfer oben am Bildschirm. Wähle zuerst deine Sprache.", "Your little helper at the top of your screen. First, pick your language.")}</p>
        <div class="langs">
          <button class="lang" class:on={lang === "de"} aria-pressed={lang === "de"} onclick={() => pickLanguage("de")}>
            <b>Deutsch</b><small>{lang === "de" ? "✓ Ausgewählt" : "Auf Deutsch umstellen"}</small>
          </button>
          <button class="lang" class:on={lang === "en"} aria-pressed={lang === "en"} onclick={() => pickLanguage("en")}>
            <b>English</b><small>{lang === "en" ? "✓ Selected" : "Switch to English"}</small>
          </button>
        </div>
      {:else if step === "name"}
        <h1>{t("Wie soll ich dich nennen?", "What should I call you?")}</h1>
        <p class="lead">{t("Dein Vorname, ein Spitzname oder ein Alias. Ganz freiwillig: ohne Namen spreche ich dich einfach mit „du“ an.", "Your first name, a nickname or an alias. Completely optional: without a name I'll simply address you as \"you\".")}</p>
        <form class="row" onsubmit={(e) => { e.preventDefault(); saveName(); }}>
          <!-- svelte-ignore a11y_autofocus -->
          <input bind:value={name} maxlength="40" placeholder={t("z. B. Alex oder Sternenfuchs", "e.g. Alex or Starfox")} spellcheck="false" autofocus />
          <button class="primary" type="submit">{name.trim() ? t("Weiter", "Next") : t("Ohne Namen weiter", "Continue without a name")}</button>
        </form>
      {:else if step === "gemini"}
        <h1>{t("Womit soll ich denken?", "What should I think with?")}</h1>
        <div class="paths" role="radiogroup" aria-label={t("KI auswählen", "Choose the AI")}>
          <button class="path" class:on={path === "google"} role="radio" aria-checked={path === "google"} onclick={() => choosePath("google")}>
            <span class="path-ico google" aria-hidden="true"><svg viewBox="0 0 20 20" width="18" height="18"><path d="M10 2.5 11.6 8.4 17.5 10l-5.9 1.6L10 17.5l-1.6-5.9L2.5 10l5.9-1.6Z" fill="currentColor" /></svg></span>
            <span class="path-text"><b>{t("Mit Google Gemini", "With Google Gemini")}</b><small>{t("Kostenlos, sehr schlau, läuft in der Cloud", "Free, very smart, runs in the cloud")}</small></span>
          </button>
          <button class="path" class:on={path === "local"} role="radio" aria-checked={path === "local"} onclick={() => choosePath("local")}>
            <span class="path-ico local" aria-hidden="true"><svg viewBox="0 0 20 20" width="18" height="18"><rect x="2.5" y="3.5" width="15" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.6" /><path d="M7 16.5h6M10 13.5v3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg></span>
            <span class="path-text"><b>{t("Ohne Google", "Without Google")}</b><small>{t("Eigene KI, nur auf deinem PC", "Your own AI, only on your PC")}</small></span>
          </button>
        </div>
        {#if path === "local"}
          <p class="lead">{t(`${app.settings.name} nutzt dann eine KI, die komplett auf deinem Rechner läuft (Ollama oder LM Studio). Nichts geht an Google. Ich prüfe gern, welches Modell zu deinem PC passt, und lade es mit einem Klick.`, `${app.settings.name} then uses an AI that runs entirely on your computer (Ollama or LM Studio). Nothing goes to Google. I'm happy to check which model fits your PC and download it with one click.`)}</p>
          <LocalAiSetup compact />
          <p class="note">{t("Du kannst jederzeit in den Einstellungen unter KI zu Google wechseln oder beides mischen.", "You can switch to Google or mix both at any time under Settings → AI.")}</p>
        {:else}
        <p class="lead">{t(`Für richtige Antworten denkt ${app.settings.name} mit Google Gemini. Dafür brauchst du einen eigenen, kostenlosen Schlüssel. Timer, Lautstärke, Programme öffnen und Co. klappen auch ohne.`, `For real answers, ${app.settings.name} thinks with Google Gemini. For that you need your own free key. Timers, volume, opening apps and the like work without it.`)}</p>
        <ol class="how">
          <li>{t("Öffne", "Open")} <button class="link" onclick={() => open("https://aistudio.google.com/apikey")}>Google AI Studio</button> {t("und melde dich mit deinem Google-Konto an.", "and sign in with your Google account.")}</li>
          <li>{t("Klicke auf „API-Schlüssel erstellen“ (Create API key) und wähle ein Projekt, oder lass eins anlegen.", "Click \"Create API key\" and choose a project, or let it create one.")}</li>
          <li>{t("Kopiere den Schlüssel (beginnt meist mit „AIza…“) und füge ihn hier ein.", "Copy the key (it usually starts with \"AIza…\") and paste it here.")}</li>
        </ol>
        <form class="row" onsubmit={(e) => { e.preventDefault(); saveGemini(); }}>
          <input type="password" bind:value={geminiKey} placeholder={app.cloud.hasKey ? t("Schon hinterlegt – neuen einfügen?", "Already saved – paste a new one?") : t("Gemini-API-Schlüssel", "Gemini API key")} autocomplete="off" spellcheck="false" />
          <button class="primary" type="submit" disabled={!geminiKey.trim() || geminiBusy}>{geminiBusy ? t("Prüfe …", "Checking …") : t("Speichern", "Save")}</button>
        </form>
        {#if geminiError}<p class="error">{geminiError}</p>{/if}
        <p class="note">{t("Der Schlüssel liegt nur in der Windows-Anmeldeinformationsverwaltung. Du kannst ihn jederzeit in den Einstellungen unter KI ändern und dort auch später auf eine KI auf deinem PC umsteigen.", "The key is stored only in Windows Credential Manager. You can change it any time under Settings → AI, and switch to an AI on your PC there later too.")}</p>
        {/if}
      {:else if step === "groq"}
        <h1>{t("Groq (freiwillig)", "Groq (optional)")}</h1>
        <p class="lead"><span class="pill">{t("Nicht nötig", "Not required")}</span> {t(`Ein Groq-Schlüssel ist keine Pflicht, kann aber helfen: ${app.settings.name} hat dann ein zweites, sehr schnelles Modell, das mit Gemini zusammenarbeitet und einspringt, wenn Gemini an seine Gratis-Grenze kommt.`, `A Groq key isn't required, but it can help: ${app.settings.name} then has a second, very fast model that works together with Gemini and steps in when Gemini hits its free limit.`)}</p>
        <ol class="how">
          <li>{t("Öffne", "Open")} <button class="link" onclick={() => open("https://console.groq.com/keys")}>console.groq.com/keys</button> {t("und melde dich an (z. B. mit Google).", "and sign in (for example with Google).")}</li>
          <li>{t("Klicke auf „Create API Key“, gib ihm einen Namen und kopiere ihn (beginnt mit „gsk_…“).", "Click \"Create API Key\", give it a name and copy it (it starts with \"gsk_…\").")}</li>
          <li>{t("Füge ihn hier ein. Oder überspring den Schritt einfach.", "Paste it here. Or simply skip this step.")}</li>
        </ol>
        <form class="row" onsubmit={(e) => { e.preventDefault(); saveGroq(); }}>
          <input type="password" bind:value={groqKey} placeholder={groqSaved ? t("Gespeichert", "Saved") : t("Groq-API-Schlüssel (optional)", "Groq API key (optional)")} autocomplete="off" spellcheck="false" />
          <button class="primary" type="submit" disabled={!groqKey.trim() || groqBusy}>{groqBusy ? t("Prüfe …", "Checking …") : t("Speichern", "Save")}</button>
        </form>
        {#if groqError}<p class="error">{groqError}</p>{/if}
      {:else}
        <h1>{t("Alles eingerichtet!", "All set up!")}</h1>
        <p class="lead">{t(`Gleich zeige ich dir in einer kurzen Tour jeden Bereich von ${app.settings.name}, mit Beispielen, wie es aussehen kann. Überspringen kannst du jederzeit.`, `Next, a short tour shows you every area of ${app.settings.name}, with examples of how it can look. You can skip it at any time.`)}</p>
        <ul class="tour">
          {#each TOUR.slice(0, 3) as item, i}
            <li in:fly={{ x: -8, duration: 260, delay: 80 + i * 70 }}>
              <span class="ic"><svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d={item.icon} fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg></span>
              <span><b>{item.title}</b><small>{item.text}</small></span>
            </li>
          {/each}
        </ul>
      {/if}

      <div class="foot">
        {#if index > 0}<button class="ghost" onclick={step === "tour" ? backFromTour : back}>{t("Zurück", "Back")}</button>{/if}
        <span class="grow"></span>
        {#if step === "gemini"}
          {#if path === "local" ? localReady : app.cloud.hasKey}
            <button class="primary" onclick={afterAi}>{t("Weiter", "Next")}</button>
          {:else if path === "local" && localAi.running}
            <button class="ghost" onclick={afterAi} title={t("Der Download läuft im Hintergrund weiter und ist in der Pille zu sehen.", "The download keeps running in the background and shows in the pill.")}>{t("Weiter, lädt im Hintergrund", "Continue, keeps downloading")}</button>
          {:else}
            <button class="ghost" onclick={afterAi}>{t("Später einrichten", "Set up later")}</button>
          {/if}
        {:else if step === "groq"}
          <button class="ghost" onclick={next}>{t("Überspringen", "Skip")}</button>
        {:else if step === "tour"}
          <button class="primary" onclick={finish}>{t("Tour starten", "Start the tour")}</button>
        {:else if step === "hello"}
          <button class="primary" onclick={() => { pickLanguage(lang); go("name"); }}>{t("Weiter", "Next")}</button>
        {/if}
      </div>
    </div>
  {/key}
</div>

<style>
  .welcome {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 6px 18px 16px;
    overflow-y: auto;
    scrollbar-width: none;
    /* Ganz dunkel: von der Insel dahinter soll nichts ablenken. */
    background: var(--ink);
    color: var(--text);
  }
  .dots {
    display: flex;
    justify-content: center;
    gap: 6px;
    padding-top: 4px;
  }
  .dots span {
    width: 6px;
    height: 6px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.18);
    transition: width var(--t-base) ease, background var(--t-base) ease;
  }
  .dots span.done {
    background: rgba(255, 255, 255, 0.45);
  }
  .dots span.on {
    width: 18px;
    background: var(--accent);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px 18px 14px;
    border-radius: var(--r-lg, 18px);
    background: var(--card, rgba(255, 255, 255, 0.06));
    box-shadow: var(--card-edge, inset 0 0 0 1px rgba(255, 255, 255, 0.08));
  }
  h1 {
    margin: 0;
    font-size: var(--fs-title);
    font-weight: var(--fw-bold);
    letter-spacing: -0.01em;
  }
  .lead {
    margin: 0;
    font-size: var(--fs-body);
    line-height: 1.5;
    color: var(--text-2);
  }
  .langs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .lang {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 12px 14px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.06);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
    color: inherit;
    font: inherit;
    cursor: pointer;
    text-align: left;
    transition: background var(--t-fast) ease, transform var(--t-fast) ease;
  }
  .lang:hover {
    background: rgba(255, 255, 255, 0.12);
    transform: translateY(-1px);
  }
  .lang.on {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .lang b {
    font-size: var(--fs-callout);
  }
  .lang small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .paths {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .path {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 10px 12px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.06);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
    color: inherit;
    text-align: left;
    transition: background var(--t-fast) ease, box-shadow var(--t-fast) ease;
  }
  .path:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .path.on {
    background: rgba(255, 255, 255, 0.1);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .path-ico {
    flex: none;
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border-radius: 10px;
  }
  .path-ico.google {
    background: linear-gradient(135deg, rgba(127, 220, 255, 0.25), rgba(185, 140, 245, 0.25));
    color: #b9d9ff;
  }
  .path-ico.local {
    background: rgba(47, 191, 113, 0.2);
    color: #8ff0b6;
  }
  .path-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .path-text b {
    font-size: var(--fs-body);
  }
  .path-text small {
    font-size: var(--fs-footnote);
    color: var(--text-3);
    line-height: 1.3;
  }
  input {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.07);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    outline: none;
  }
  input:focus {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  button {
    font: inherit;
    cursor: pointer;
  }
  .primary,
  .ghost {
    height: 34px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--r-md);
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
  }
  .primary {
    background: var(--text);
    color: #000;
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .ghost {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text);
  }
  .ghost:hover {
    background: rgba(255, 255, 255, 0.14);
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: var(--fw-semibold);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .how {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--fs-body);
    line-height: 1.45;
    color: var(--text-2);
  }
  .note {
    margin: 0;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .error {
    margin: 0;
    font-size: var(--fs-footnote);
    color: #ff6961;
  }
  .pill {
    display: inline-block;
    padding: 1px 8px;
    margin-right: 4px;
    border-radius: var(--r-full);
    background: rgba(48, 209, 88, 0.16);
    color: #7ee29a;
    font-size: var(--fs-caption);
    font-weight: var(--fw-bold);
  }
  .tour {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .tour li {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .ic {
    flex: none;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: var(--r-sm);
    background: rgba(255, 255, 255, 0.08);
    color: var(--accent);
  }
  .tour b {
    display: block;
    font-size: var(--fs-body);
  }
  .tour small {
    display: block;
    font-size: var(--fs-footnote);
    line-height: 1.4;
    color: #aeaeb2;
  }
  .foot {
    position: sticky;
    bottom: -16px;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 2px -18px -14px;
    padding: 8px 18px 12px;
    border-radius: 0 0 var(--r-lg, 18px) var(--r-lg, 18px);
    background: rgba(18, 18, 22, 0.97);
  }
  .grow {
    flex: 1;
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
</style>
