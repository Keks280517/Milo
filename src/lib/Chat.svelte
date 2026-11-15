<script lang="ts">
  import { t } from "./i18n";
  // Gespraechsverlauf und Texteingabe in der ausgeklappten Insel.
  import { tick, untrack } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app, inTauri } from "./app.svelte";
  import { formatAnswer } from "./format";
  import CardView from "./cards/CardView.svelte";
  import RatingView from "./cards/RatingView.svelte";
  import { claude, type ClaudeProject } from "./claudeChat.svelte";
  import { tour } from "./tourState.svelte";

  let draft = $state("");
  let list: HTMLDivElement;
  let input: HTMLTextAreaElement;

  /** Vorschlaege fuer den leeren Verlauf; `fill` setzt nur den Anfang ein, statt gleich zu senden. */
  const suggestions: { label: string; text: string; fill?: boolean }[] = [
    { label: t("Timer 10 Minuten", "10-minute timer"), text: t("Stell einen Timer auf 10 Minuten", "Set a timer for 10 minutes") },
    { label: t("Wetter heute", "Weather today"), text: t("Wie wird das Wetter heute?", "What's the weather like today?") },
    { label: t("Merk dir …", "Remember …"), text: t("Merk dir, dass ", "Remember that "), fill: true },
    { label: t("Öffne …", "Open …"), text: t("Öffne ", "Open "), fill: true },
    { label: t("Such im Netz …", "Search the web …"), text: t("Recherchiere: ", "Research: "), fill: true },
  ];

  export function focus() {
    // preventScroll: sonst scrollt der Browser die Insel (overflow) nach oben und schneidet den Kopf ab.
    input?.focus({ preventScroll: true });
  }

  // ---------- Claude-Modus: "/claude " schaltet um, alles Weitere geht an Claude Code ----------

  /** Kurze Umschalt-Animation (oranger Lichtstreif ueber die Eingabe). */
  let switching = $state(false);

  // ---------- Vorfuehrung waehrend der Tour ("Geisterdaten"): nur Animation, nie gespeichert ----------

  type Ghost = { id: number; role: "user" | "milo"; text: string; tools: string[]; pending: boolean };
  let ghostLines = $state<Ghost[]>([]);
  let ghostDraft = $state("");
  let ghostClaude = $state(false);
  const demoing = $derived(tour.demo === "chat" || tour.demo === "claude");

  $effect(() => {
    const demo = tour.demo;
    if (demo !== "chat" && demo !== "claude") return;
    let stopped = false;
    const wait = (ms: number) => new Promise<void>((resolve, reject) => window.setTimeout(() => (stopped ? reject(new Error("stop")) : resolve()), ms));
    let next = 1;
    const type = async (text: string) => {
      for (let i = 1; i <= text.length; i++) {
        ghostDraft = text.slice(0, i);
        await wait(45);
      }
    };
    const exchange = async (question: string, tools: string[], answer: string) => {
      await type(question);
      await wait(350);
      ghostDraft = "";
      ghostLines.push({ id: next++, role: "user", text: question, tools: [], pending: false });
      const reply: Ghost = { id: next++, role: "milo", text: "", tools: [], pending: true };
      ghostLines.push(reply);
      for (const tool of tools) {
        await wait(700);
        ghostLines[ghostLines.length - 1].tools.push(tool);
      }
      await wait(800);
      const last = ghostLines[ghostLines.length - 1];
      last.pending = false;
      for (let i = 1; i <= answer.length; i += 3) {
        last.text = answer.slice(0, i);
        await wait(22);
      }
      last.text = answer;
    };
    const run = async () => {
      while (!stopped) {
        ghostLines = [];
        ghostDraft = "";
        ghostClaude = false;
        await wait(600);
        if (demo === "chat") {
          await exchange(t("Stell einen Timer auf 10 Minuten", "Set a timer for 10 minutes"), [t("Timer gestellt: 10 Minuten", "Timer set: 10 minutes")], t("Erledigt! Dein Timer läuft 10 Minuten. Ich sag Bescheid, wenn er klingelt.", "Done! Your timer is running for 10 minutes. I'll let you know when it rings."));
          await wait(1600);
          await exchange(t("Wie wird das Wetter morgen?", "What's the weather like tomorrow?"), [t("Wetter für morgen geholt", "Fetched tomorrow's weather")], t("Morgen wird es sonnig mit bis zu 14 °C, am Abend ziehen ein paar Wolken auf. Regen ist nicht in Sicht.", "Tomorrow will be sunny with up to 14 °C, a few clouds roll in during the evening. No rain in sight."));
        } else {
          await type("/claude ");
          await wait(250);
          ghostDraft = "";
          ghostClaude = true;
          switching = true;
          window.setTimeout(() => (switching = false), 700);
          await wait(900);
          await exchange(t("Bau einen Dark Mode für die Kopfzeile", "Build a dark mode for the header"), [t("Read · Header.tsx", "Read · Header.tsx"), t("Edit · Header.tsx", "Edit · Header.tsx"), t("Bash · npm test", "Bash · npm test")], t("Fertig: Die Kopfzeile wechselt jetzt mit dem Dark Mode ihre Farben, und alle Tests laufen durch.", "Done: the header now switches colors with dark mode, and all tests pass."));
          await wait(1200);
          await type("/code");
        }
        await wait(3500);
      }
    };
    run().catch(() => {});
    return () => {
      stopped = true;
      ghostLines = [];
      ghostDraft = "";
      ghostClaude = false;
    };
  });
  /** Waehrend der Claude-Vorfuehrung: Befehlsliste mit Beispielen zeigen, wenn "/code" getippt ist. */
  const ghostSlash = $derived(ghostClaude && ghostDraft.startsWith("/") ? [
    { name: "code-review", description: t("Prüft deine Änderungen auf Fehler", "Reviews your changes for bugs"), source: "claude" },
    { name: "plan", description: t("Planmodus: Claude plant erst und ändert noch nichts", "Plan mode: Claude plans first and changes nothing yet"), source: "milo" },
  ] : []);
  let picker = $state(false);
  /** Aufgeklapptes Projekt in der Auswahl (nur eins; Standard: das aktuelle). */
  let openProject = $state("");
  let pickQuery = $state("");
  $effect(() => {
    if (!picker) return;
    untrack(() => {
      openProject = claude.target?.cwd ?? "";
      pickQuery = "";
    });
  });
  /** Projekte (aktuelles zuerst), bei Suche nur Treffer samt passender Sitzungen. */
  const pickProjects = $derived.by(() => {
    const q = pickQuery.trim().toLowerCase();
    const list = [...claude.projects].sort((a, b) => Number(b.cwd === claude.target?.cwd) - Number(a.cwd === claude.target?.cwd));
    return list
      .map((project) => {
        if (!q) return { project, sessions: project.sessions };
        const own = project.name.toLowerCase().includes(q) || project.cwd.toLowerCase().includes(q);
        return { project, sessions: own ? project.sessions : project.sessions.filter((s) => (s.title || "").toLowerCase().includes(q)) };
      })
      .filter((p) => !q || p.sessions.length > 0 || p.project.name.toLowerCase().includes(q));
  });
  /** Die Auswahl waechst nach oben, aber nie ueber die Kopfzeile der Insel hinaus: Hoehe an den
   *  freien Platz binden (oberer Rand der Ansicht), innen scrollen. */
  function fitPicker(node: HTMLElement) {
    const fit = () => {
      const pane = node.closest(".pane") as HTMLElement | null;
      const top = (pane?.getBoundingClientRect().top ?? 0) + 6;
      const bottom = node.getBoundingClientRect().bottom;
      node.style.maxHeight = `${Math.max(120, Math.min(340, bottom - top))}px`;
    };
    fit();
    const ro = new ResizeObserver(fit);
    const pane = node.closest(".pane");
    if (pane) ro.observe(pane);
    window.addEventListener("resize", fit);
    return { destroy() { ro.disconnect(); window.removeEventListener("resize", fit); } };
  }
  let slashIndex = $state(0);

  function enterClaude(rest: string) {
    draft = rest;
    switching = true;
    window.setTimeout(() => (switching = false), 700);
    claude.enter();
    tick().then(() => {
      resize();
      input?.focus({ preventScroll: true });
    });
  }

  function leaveClaude() {
    claude.leave();
    picker = false;
    switching = true;
    window.setTimeout(() => (switching = false), 700);
    tick().then(() => input?.focus({ preventScroll: true }));
  }

  /** Schnellauswahl der Slash-Befehle, solange nur "/befehl" (ohne Leerzeichen) im Feld steht. */
  const slash = $derived.by(() => {
    if (!claude.active || !/^\/[^\s]*$/.test(draft)) return [];
    const q = draft.slice(1).toLowerCase();
    const hits = claude.commands.filter((c) => c.name.toLowerCase().includes(q) || (q.length > 2 && c.description.toLowerCase().includes(q)));
    // Treffer am Anfang des Namens zuerst
    return hits.sort((a, b) => Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q))).slice(0, 8);
  });
  $effect(() => {
    void slash.length;
    slashIndex = 0;
  });

  function pickCommand(name: string) {
    draft = `/${name} `;
    tick().then(() => {
      input?.focus({ preventScroll: true });
      input?.setSelectionRange(draft.length, draft.length);
    });
  }

  function chooseTarget(project: ClaudeProject, session: ClaudeProject["sessions"][number] | null) {
    claude.choose(project, session);
    picker = false;
    tick().then(() => input?.focus({ preventScroll: true }));
  }

  function ago(ms: number): string {
    const min = Math.round((Date.now() - ms) / 60000);
    if (min < 1) return t("gerade", "now");
    if (min < 60) return t(`vor ${min} Min`, `${min} min ago`);
    const h = Math.round(min / 60);
    if (h < 24) return t(`vor ${h} Std`, `${h} h ago`);
    return t(`vor ${Math.round(h / 24)} T`, `${Math.round(h / 24)} d ago`);
  }

  function onInput() {
    resize();
    if (!claude.active && /^\/claude\s/i.test(draft)) enterClaude(draft.replace(/^\/claude\s+/i, ""));
  }

  function submit() {
    if (claude.active) {
      if (claude.running || !draft.trim()) return;
      const text = draft;
      draft = "";
      claude.send(text);
      tick().then(resize);
      return;
    }
    if (/^\/claude\s*$/i.test(draft.trim())) return enterClaude("");
    if (app.busy) return;
    const text = draft;
    draft = "";
    app.send(text);
    tick().then(resize);
  }

  function suggest(item: (typeof suggestions)[number]) {
    if (item.fill) {
      draft = item.text;
      tick().then(() => {
        input?.focus({ preventScroll: true });
        input?.setSelectionRange(draft.length, draft.length);
      });
    } else {
      app.send(item.text);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (slash.length > 0) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        slashIndex = (slashIndex + (e.key === "ArrowDown" ? 1 : slash.length - 1)) % slash.length;
        return;
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey && draft.slice(1) !== slash[slashIndex].name)) {
        e.preventDefault();
        pickCommand(slash[slashIndex].name);
        return;
      }
    }
    // Leeres Feld und Rueckschritt: zurueck zu Milo.
    if (claude.active && e.key === "Backspace" && draft === "") {
      e.preventDefault();
      leaveClaude();
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  /** Eingabefeld waechst mit dem Text, bis zu vier Zeilen. */
  function resize() {
    if (!input) return;
    input.style.height = "auto";
    input.style.height = Math.min(input.scrollHeight, 92) + "px";
  }

  function host(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  // Links aus Antworten im Standardbrowser oeffnen statt in der Insel.
  function onClick(e: MouseEvent) {
    const link = (e.target as HTMLElement).closest("a[data-external]") as HTMLAnchorElement | null;
    if (!link) return;
    e.preventDefault();
    if (inTauri) openUrl(link.href);
  }

  // Bei neuem Text ans Ende rollen.
  $effect(() => {
    const last = app.messages.at(-1);
    void last?.text.length;
    void last?.tools.length;
    void last?.tools.at(-1)?.status;
    void app.messages.length;
    const own = claude.lines.at(-1);
    void own?.text.length;
    void own?.tools.length;
    void claude.lines.length;
    void claude.active;
    tick().then(() => {
      if (!list) return;
      const newest = list.lastElementChild as HTMLElement | null;
      // Hohe Antworten (z. B. mit Karte) beginnen oben, statt dass ihr Anfang aus dem Bild rutscht.
      const tall = newest !== null && newest.offsetHeight > list.clientHeight - 16;
      list.scrollTo({ top: tall ? newest.offsetTop - 4 : list.scrollHeight, behavior: "smooth" });
    });
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="list" class:claude={claude.active || ghostClaude} bind:this={list} onclick={onClick}>
  {#if demoing}
    {#each ghostLines as line (line.id)}
      {#if line.role === "user"}
        <div class="user" class:claude-user={ghostClaude}>{line.text}</div>
      {:else}
        <div class="milo" class:claude-answer={ghostClaude}>
          {#each line.tools as tool}
            <div class="tool" class:claude-tool={ghostClaude}><span class="dot"></span>{tool}</div>
          {/each}
          {#if line.text}
            <div class="text">{line.text}</div>
          {:else if line.pending}
            <div class="typing" class:claude-typing={ghostClaude}><span></span><span></span><span></span></div>
          {/if}
        </div>
      {/if}
    {/each}
  {:else if claude.active}
    {#if claude.lines.length === 0}
      <div class="empty claude-empty">
        <span class="claude-mark" aria-hidden="true"></span>
        <p>{t(`Du schreibst jetzt an Claude${claude.target ? ` in „${claude.target.name}“` : ""}.`, `You're now writing to Claude${claude.target ? ` in "${claude.target.name}"` : ""}.`)}</p>
        <small>{t("Tippe / für alle Befehle, Skills und Plugins. Mit ⌫ im leeren Feld geht es zurück zu", "Type / for all commands, skills and plugins. Press ⌫ in the empty field to go back to")} {app.settings.name}.</small>
      </div>
    {/if}
    {#if claude.lines.some((l) => l.past)}
      <div class="past-note">{t("Bisheriger Verlauf dieser Sitzung", "Earlier in this session")}</div>
    {/if}
    {#each claude.lines as line, i (line.id)}
      {#if i > 0 && claude.lines[i - 1].past && !line.past}
        <div class="past-note now">{t("Ab hier aus Milo", "From Milo")}</div>
      {/if}
      {#if line.role === "user"}
        <div class="user claude-user">{line.text}</div>
      {:else}
        <div class="milo claude-answer" class:error={line.error}>
          {#each line.tools as tool}
            <div class="tool claude-tool"><span class="dot"></span><b>{tool.name}</b>{#if tool.detail}<span class="tdetail">{tool.detail}</span>{/if}</div>
          {/each}
          {#if line.text}
            <div class="text">{@html formatAnswer(line.text)}</div>
          {:else if line.pending}
            <div class="typing claude-typing"><span></span><span></span><span></span></div>
          {/if}
          {#if line.login}
            <button class="login" onclick={() => claude.login()}>{t("Bei Claude anmelden", "Sign in to Claude")}</button>
            <small class="login-hint">{t("Es öffnet sich ein Fenster mit Claude Code. Melde dich dort an, schließ es und schick deine Nachricht noch einmal.", "A Claude Code window opens. Sign in there, close it and send your message again.")}</small>
          {/if}
        </div>
      {/if}
    {/each}
  {:else}
  {#if app.messages.length === 0}
    <div class="empty">
      <p>{t("Frag mich etwas oder gib mir einen Auftrag.", "Ask me anything or give me a task.")}</p>
      <div class="suggestions">
        {#each suggestions as item, i}
          <!-- Auf dem ersten Vorschlag steht Milo, solange das Gespraech leer ist. -->
          <button onclick={() => suggest(item)} data-perch={i === 0 ? "center" : undefined}>{item.label}</button>
        {/each}
      </div>
    </div>
  {/if}
  {#each app.messages as message (message.id)}
    {#if message.role === "user"}
      <div class="user">{message.text}</div>
    {:else}
      <div class="milo" class:error={message.error}>
        {#if message.rating && (app.settings.showRating || message.rating.kind === "hard" || message.rating.kind === "report")}
          <RatingView rating={message.rating} />
        {/if}
        {#each message.tools as tool}
          {#if tool.card && tool.status === "ok"}
            <div class="card"><CardView card={tool.card} /></div>
          {:else}
            <div class="tool" class:cloud={tool.name === "cloud"} class:failed={tool.status === "error"} class:declined={tool.status === "declined"} class:running={tool.status === "start"}>
              <span class="dot"></span>{tool.summary}
            </div>
            {#if tool.links.length > 0}
              <div class="sources">
                {#each tool.links as link}
                  <a href={link.url} data-external title={link.url}>{link.title || host(link.url)}</a>
                {/each}
              </div>
            {/if}
          {/if}
        {/each}
        {#if message.text}
          <!-- formatAnswer entschaerft zuerst alles HTML, daher ist das Einsetzen sicher. -->
          <div class="text">{@html formatAnswer(message.text)}</div>
        {:else if message.pending && message.tools.length === 0 && !(message.rating && (app.settings.showRating || message.rating.kind === "hard" || message.rating.kind === "report"))}
          <div class="typing"><span></span><span></span><span></span></div>
        {/if}
      </div>
    {/if}
  {/each}
  {/if}
</div>

<div class="composer-wrap">
{#if demoing && ghostClaude}
  <div class="claude-bar">
    <span class="target"><span class="claude-mark small" aria-hidden="true"></span><span class="tname">Claude</span><span class="tproj">Shop-App</span><span class="tsess">Dark Mode</span><span class="chev">›</span></span>
    <span class="leave">× {app.settings.name}</span>
  </div>
  {#if ghostSlash.length > 0}
    <div class="slash">
      {#each ghostSlash as command, i (command.name)}
        <button class:on={i === 0} tabindex="-1"><span class="sname">/{command.name}</span><span class="sdesc">{command.description}</span><span class="ssrc">{command.source}</span></button>
      {/each}
    </div>
  {/if}
{:else if claude.active}
  <div class="claude-bar" class:open={picker}>
    <button class="target" onclick={() => { picker = !picker; if (picker) claude.refresh(); }} aria-expanded={picker} title={claude.target?.cwd}>
      <span class="claude-mark small" aria-hidden="true"></span>
      <span class="tname">Claude</span>
      {#if claude.target}<span class="tproj">{claude.target.name}</span><span class="tsess">{claude.target.title}</span>{:else}<span class="tsess">{t("Projekt wählen", "Choose a project")}</span>{/if}
      <span class="chev" class:up={picker}>›</span>
    </button>
    <button class="leave" onclick={leaveClaude} title={t(`Zurück zu ${app.settings.name}`, `Back to ${app.settings.name}`)}>
      <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      {app.settings.name}
    </button>
  </div>
  {#if picker}
    <div class="picker glass" use:fitPicker>
      {#if claude.projects.length > 4}
        <input class="pfind" placeholder={t("Projekt oder Sitzung suchen …", "Search project or session …")} bind:value={pickQuery} onkeydown={(e) => { if (e.key === "Escape") { e.stopPropagation(); picker = false; } }} />
      {/if}
      {#if claude.projects.length === 0}
        <p class="pick-empty">{claude.loading ? t("Lade Sitzungen …", "Loading sessions …") : t("Noch keine Claude-Sitzungen gefunden.", "No Claude sessions found yet.")}</p>
      {:else if pickProjects.length === 0}
        <p class="pick-empty">{t("Nichts gefunden.", "Nothing found.")}</p>
      {/if}
      {#each pickProjects as { project, sessions }, i (project.cwd)}
        {@const expanded = pickQuery.trim() !== "" || openProject === project.cwd || (openProject === "" && i === 0)}
        <div class="pgroup" class:collapsed={!expanded}>
          <button class="phead" onclick={() => (openProject = expanded && !pickQuery.trim() ? "-" : project.cwd)} aria-expanded={expanded} title={project.cwd}>
            <span class="pchev" class:up={expanded} aria-hidden="true">›</span>
            <strong>{project.name}</strong><span class="pcwd">{project.cwd}</span>
            {#if !expanded && project.sessions[0]}<span class="pago">{ago(project.sessions[0].at)}</span>{/if}
          </button>
          {#if expanded}
            {#each sessions.slice(0, pickQuery.trim() ? 8 : 5) as session (session.id)}
              <button class="psess" class:on={claude.target?.session === session.id} onclick={() => chooseTarget(project, session)}>
                <span class="ptitle">{session.title || t("Ohne Titel", "Untitled")}</span><span class="pago">{ago(session.at)}</span>
              </button>
            {/each}
            <button class="psess pnew" class:on={claude.target?.cwd === project.cwd && !claude.target?.session} onclick={() => chooseTarget(project, null)}>
              + {t("Neuer Chat in diesem Projekt", "New chat in this project")}
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
  {#if slash.length > 0}
    <div class="slash glass" role="listbox" aria-label={t("Befehle", "Commands")}>
      {#each slash as command, i (command.name)}
        <button role="option" aria-selected={i === slashIndex} class:on={i === slashIndex} onmouseenter={() => (slashIndex = i)} onclick={() => pickCommand(command.name)}>
          <span class="sname">/{command.name}</span>
          <span class="sdesc">{command.description || (command.source === "claude" ? t("Eingebauter Claude-Befehl", "Built-in Claude command") : "")}</span>
          <span class="ssrc">{command.source}</span>
        </button>
      {/each}
    </div>
  {/if}
{/if}
<form class="composer" class:claude={claude.active || (demoing && ghostClaude)} class:switching data-perch={app.messages.length > 0 ? "right" : undefined} onsubmit={(e) => { e.preventDefault(); submit(); }}>
  <textarea
    bind:this={input}
    value={demoing ? ghostDraft : draft}
    oninput={(e) => { if (!demoing) draft = e.currentTarget.value; onInput(); }}
    readonly={demoing}
    rows="1"
    placeholder={claude.active ? t("Nachricht an Claude · / für Befehle", "Message Claude · / for commands") : app.voice === "listening" ? t("Ich höre zu …", "Listening …") : app.voice === "transcribing" ? t("Moment …", "One moment …") : t(`Nachricht an ${app.settings.name} · /claude für Claude`, `Message ${app.settings.name} · /claude for Claude`)}
    onkeydown={onKeydown}
    onfocus={() => app.warmUp()}
    spellcheck="false"
  ></textarea>
  {#if claude.active}
    {#if claude.running}
      <button type="button" class="send stop claude-send" onclick={() => claude.stop()} aria-label={t("Claude abbrechen", "Stop Claude")}>
        <span class="square"></span>
      </button>
    {:else}
      <button type="submit" class="send claude-send" disabled={!draft.trim() || !claude.target} aria-label={t("An Claude senden", "Send to Claude")}>
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
          <path d="M8 13V3M8 3L3.5 7.5M8 3l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
    {/if}
  {:else if !app.busy && !draft.trim()}
    <!-- Leeres Feld: statt Senden das Mikrofon. Waehrend der Aufnahme beendet derselbe Knopf sie. -->
    <button
      type="button" class="send mic" class:live={app.voice === "listening"} disabled={app.voice === "transcribing"}
      style:--level={app.voiceLevel}
      onclick={() => app.toggleVoice()}
      title={app.voice === "listening" ? t("Aufnahme beenden", "Stop recording") : t(`Sprechen (${app.settings.hotkey || "kein Hotkey"})`, `Speak (${app.settings.hotkey || "no shortcut"})`)}
      aria-label={app.voice === "listening" ? t("Aufnahme beenden", "Stop recording") : t("Sprechen", "Speak")}
    >
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <rect x="5.6" y="1.8" width="4.8" height="8" rx="2.4" fill="currentColor" />
        <path d="M3.4 7.6a4.6 4.6 0 0 0 9.2 0M8 12.2v2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </button>
  {:else if app.busy}
    <button type="button" class="send stop" onclick={() => app.cancel()} aria-label={t("Antwort abbrechen", "Stop answer")}>
      <span class="square"></span>
    </button>
  {:else}
    <button type="submit" class="send" disabled={!draft.trim()} aria-label={t("Senden", "Send")}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path d="M8 13V3M8 3L3.5 7.5M8 3l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
  {/if}
</form>
</div>

<style>
  .list {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 22px 44px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    user-select: text;
  }
  /* Schmaler Rollbalken ohne Pfeile, der erst beim Ueberfahren deutlicher wird. */
  .list::-webkit-scrollbar {
    width: 10px;
  }
  .list::-webkit-scrollbar-thumb {
    border: 3px solid transparent;
    border-radius: var(--r-xs);
    background: #2c2c2e content-box;
  }
  .list::-webkit-scrollbar-thumb:hover {
    background-color: #48484a;
  }
  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    /* Platz fuer Milo, der auf dem ersten Vorschlag steht. */
    gap: 52px;
    user-select: none;
  }
  .empty p {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-callout);
  }
  .suggestions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
    max-width: 440px;
  }
  .suggestions button {
    height: 30px;
    padding: 0 12px;
    border-radius: var(--r-lg);
    border: 1px solid var(--line);
    background: var(--card);
    box-shadow: var(--card-edge);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
    transition: background var(--t-fast) ease, color var(--t-fast) ease, border-color var(--t-fast) ease, transform var(--t-fast) ease;
  }
  .suggestions button:hover {
    background: var(--chip);
    border-color: #3a3a3e;
    color: var(--text);
  }
  .suggestions button:active {
    transform: scale(0.96);
  }
  .user {
    align-self: flex-end;
    max-width: 80%;
    padding: 8px 12px;
    border-radius: 18px 18px 6px 18px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 34%, #2a2a30), color-mix(in srgb, var(--accent) 22%, #1c1c20));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.16), 0 4px 14px rgba(0, 0, 0, 0.25);
    font-size: var(--fs-callout);
    line-height: 1.45;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    animation: in var(--t-slow) cubic-bezier(0.3, 1.2, 0.4, 1) both;
  }
  .milo {
    align-self: stretch;
    font-size: var(--fs-callout);
    line-height: 1.52;
    overflow-wrap: anywhere;
  }
  .milo.error .text {
    color: #ff8a80;
  }
  .text {
    max-width: 94%;
  }
  .card {
    margin: 2px 0 8px;
    display: flex;
  }
  .text :global(code) {
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: var(--fs-footnote);
    padding: 1px 6px;
    border-radius: var(--r-xs);
    background: var(--chip);
  }
  /* Formeln: etwas groesser als der Text, breite Formeln duerfen seitlich rollen. */
  .text :global(.katex) {
    font-size: 1.06em;
  }
  .text :global(.katex-display) {
    margin: 2px 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
  }
  .text :global(pre) {
    margin: 6px 0;
    padding: 8px 12px;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
    overflow-x: auto;
    line-height: 1.45;
    scrollbar-width: thin;
    scrollbar-color: #3a3a3c transparent;
  }
  .text :global(pre code) {
    padding: 0;
    background: none;
    font-size: var(--fs-footnote);
    white-space: pre;
  }
  .text :global(a) {
    color: var(--accent);
    text-decoration: none;
  }
  .text :global(.li) {
    display: block;
    padding-left: 14px;
    position: relative;
  }
  .text :global(.li)::before {
    content: "";
    position: absolute;
    left: 3px;
    top: 0.72em;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .tool {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-3);
    font-size: var(--fs-footnote);
    margin-bottom: 4px;
  }
  .tool .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ok);
    flex: none;
  }
  .tool.running .dot {
    background: var(--accent);
    animation: pulse 1s ease-in-out infinite;
  }
  .tool.failed {
    color: #ff8a80;
  }
  .tool.failed .dot {
    background: var(--bad);
  }
  /* Der Cloud-Helfer traegt die Farben des zweiten Wuerfels. */
  .tool.cloud:not(.failed):not(.declined) .dot {
    background: linear-gradient(135deg, #7fdcff, #b98cf5);
  }
  .tool.declined .dot {
    background: var(--text-3);
  }
  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 2px 0 6px 12px;
  }
  .sources a {
    max-width: 190px;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
    border: 1px solid var(--line);
    color: #9fdcf7;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    text-decoration: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: background var(--t-fast) ease, border-color var(--t-fast) ease;
  }
  .sources a:hover {
    background: var(--chip);
    border-color: #3a3a3e;
  }
  .typing {
    display: flex;
    gap: 4px;
    padding: 8px 0;
  }
  .typing span {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-3);
    animation: pulse 1.1s ease-in-out infinite;
  }
  .typing span:nth-child(2) { animation-delay: 0.15s; }
  .typing span:nth-child(3) { animation-delay: 0.3s; }
  @keyframes pulse {
    0%, 100% { opacity: 0.3; transform: translateY(0); }
    50% { opacity: 1; transform: translateY(-2px); }
  }
  @keyframes in {
    from { opacity: 0; transform: translateY(6px) scale(0.97); }
    to { opacity: 1; transform: none; }
  }

  /* ---------- Claude-Modus ---------- */
  .composer-wrap {
    position: relative;
    --claude: #e8875d;
  }
  .claude-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 22px 6px;
    animation: in var(--t-slow) cubic-bezier(0.3, 1.2, 0.4, 1) both;
  }
  .target,
  .leave {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-md);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .target {
    flex: 1;
    min-width: 0;
    background: color-mix(in srgb, var(--claude) 16%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--claude) 35%, transparent);
    color: var(--text);
  }
  .target:hover {
    background: color-mix(in srgb, var(--claude) 24%, transparent);
  }
  .tname {
    font-weight: var(--fw-bold);
    color: var(--claude);
  }
  .tproj {
    font-weight: var(--fw-semibold);
  }
  .tsess {
    min-width: 0;
    flex: 1;
    text-align: left;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chev {
    display: inline-block;
    color: var(--text-3);
    transform: rotate(-90deg);
    transition: transform var(--t-base) ease;
  }
  .chev.up {
    transform: rotate(90deg);
  }
  .leave {
    flex: none;
    background: var(--chip);
    color: var(--text-2);
  }
  .leave:hover {
    background: var(--chip-strong);
    color: #fff;
  }
  .target:focus-visible,
  .leave:focus-visible,
  .psess:focus-visible,
  .slash button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  /* Kleines Claude-Zeichen: Stern aus vier Strichen in Orange. */
  .claude-mark {
    width: 26px;
    height: 26px;
    flex: none;
    background: var(--claude, #e8875d);
    -webkit-mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Cpath d='M8 1.5v13M1.5 8h13M3.4 3.4l9.2 9.2M12.6 3.4l-9.2 9.2' stroke='black' stroke-width='2' stroke-linecap='round'/%3E%3C/svg%3E") center / contain no-repeat;
    mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Cpath d='M8 1.5v13M1.5 8h13M3.4 3.4l9.2 9.2M12.6 3.4l-9.2 9.2' stroke='black' stroke-width='2' stroke-linecap='round'/%3E%3C/svg%3E") center / contain no-repeat;
  }
  .claude-mark.small {
    width: 12px;
    height: 12px;
  }
  .claude-empty {
    gap: 10px;
    text-align: center;
    max-width: 380px;
  }
  .claude-empty .claude-mark {
    --claude: #e8875d;
    animation: spin-in 700ms cubic-bezier(0.3, 1.2, 0.4, 1) both;
  }
  .claude-empty small {
    color: var(--text-4);
    font-size: var(--fs-footnote);
    line-height: 1.5;
  }
  @keyframes spin-in {
    from { transform: rotate(-90deg) scale(0.4); opacity: 0; }
    to { transform: none; opacity: 1; }
  }
  .claude-user {
    background: linear-gradient(180deg, color-mix(in srgb, #e8875d 38%, #2a2a30), color-mix(in srgb, #e8875d 22%, #1c1c20));
  }
  .past-note {
    align-self: center;
    font-size: var(--fs-caption);
    color: var(--text-4);
  }
  .past-note.now {
    color: #f5b08a;
  }
  .claude-tool {
    min-width: 0;
  }
  .claude-tool .dot {
    background: #e8875d;
  }
  .claude-tool b {
    font-weight: var(--fw-semibold);
    color: #f5b08a;
  }
  .tdetail {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .claude-typing span {
    background: #e8875d;
  }
  .login {
    margin-top: 6px;
    height: 30px;
    padding: 0 14px;
    border: 0;
    border-radius: var(--r-lg);
    background: #e8875d;
    color: #000;
    font: inherit;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .login-hint {
    display: block;
    margin-top: 4px;
    color: var(--text-3);
    font-size: var(--fs-footnote);
  }
  .picker,
  .slash {
    position: absolute;
    left: 22px;
    right: 22px;
    bottom: calc(100% + 4px);
    z-index: 5;
    max-height: 290px;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 6px;
    border-radius: var(--r-lg);
    /* Deckend, damit der Verlauf dahinter nicht durchscheint. */
    background: rgba(24, 24, 28, 0.97);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08), 0 12px 32px rgba(0, 0, 0, 0.5);
    scrollbar-width: none;
    animation: in var(--t-base) cubic-bezier(0.3, 1.2, 0.4, 1) both;
  }
  .picker {
    box-sizing: border-box;
    scrollbar-width: thin;
    scrollbar-color: #3a3a3e transparent;
  }
  .pick-empty {
    margin: 8px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .pgroup + .pgroup {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .phead {
    width: 100%;
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 8px;
    min-width: 0;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--text-2);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .phead:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .phead strong {
    flex: none;
    font-size: var(--fs-footnote);
  }
  .pchev {
    flex: none;
    width: 8px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
    transition: transform var(--t-fast) ease;
  }
  .pchev.up {
    transform: rotate(90deg);
  }
  .pcwd {
    flex: 1;
  }
  .pgroup.collapsed + .pgroup.collapsed {
    margin-top: 0;
    padding-top: 0;
    border-top: 0;
  }
  .pfind {
    position: sticky;
    top: -6px;
    z-index: 1;
    width: 100%;
    box-sizing: border-box;
    margin-bottom: 6px;
    padding: 6px 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: #2c2c30;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    outline: none;
  }
  .pfind:focus-visible {
    box-shadow: 0 0 0 1.5px #e8875d;
  }
  .phead span {
    min-width: 0;
    font-size: var(--fs-caption);
    color: var(--text-4);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .psess {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    text-align: left;
    cursor: pointer;
  }
  .psess:hover,
  .psess.on {
    background: color-mix(in srgb, #e8875d 18%, transparent);
  }
  .ptitle {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pago {
    flex: none;
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .pnew {
    color: #f5b08a;
  }
  .slash button {
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, auto) minmax(0, 1fr) auto;
    align-items: baseline;
    gap: 10px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--text-2);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .slash button.on {
    background: color-mix(in srgb, #e8875d 20%, transparent);
  }
  .sname {
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    color: #f5b08a;
    white-space: nowrap;
  }
  .sdesc {
    min-width: 0;
    font-size: var(--fs-footnote);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ssrc {
    font-size: var(--fs-caption);
    color: var(--text-4);
  }
  .composer.claude {
    box-shadow: var(--glass-edge), 0 0 0 1.5px color-mix(in srgb, #e8875d 70%, transparent), 0 8px 28px rgba(232, 135, 93, 0.22);
  }
  .composer.claude:focus-within {
    box-shadow: var(--glass-edge), 0 0 0 2px #e8875d, 0 8px 28px rgba(232, 135, 93, 0.3);
  }
  .send.claude-send {
    background: #e8875d;
  }
  /* Umschalten: ein Lichtstreif laeuft einmal ueber die Eingabe. */
  .composer.switching {
    position: relative;
    overflow: hidden;
  }
  .composer.switching::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(100deg, transparent 20%, rgba(232, 135, 93, 0.55) 50%, transparent 80%);
    transform: translateX(-100%);
    animation: sweep 650ms ease-out forwards;
    pointer-events: none;
  }
  @keyframes sweep {
    to { transform: translateX(100%); }
  }
  @media (prefers-reduced-motion: reduce) {
    .composer.switching::after,
    .claude-empty .claude-mark,
    .claude-bar {
      animation: none;
    }
  }

  .composer {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    margin: 0 22px 14px;
    padding: 8px 8px 8px 16px;
    border-radius: var(--r-lg);
    /* Glaskapsel: Bedienelement, das ueber dem Verlauf liegt. */
    background: var(--glass-fill);
    -webkit-backdrop-filter: var(--glass-blur);
    backdrop-filter: var(--glass-blur);
    box-shadow: var(--glass-edge), 0 8px 24px rgba(0, 0, 0, 0.35);
    transition: box-shadow var(--t-slow) ease;
  }
  /* Den Fokus zeigt die ganze Kapsel mit ihrem Leuchtrand, nicht ein eckiger Kasten im Feld. */
  .composer:focus-within {
    box-shadow: var(--glass-edge), 0 0 0 2px color-mix(in srgb, var(--accent) 60%, transparent), 0 8px 24px rgba(0, 0, 0, 0.35);
  }
  .composer textarea:focus-visible {
    outline: none !important;
  }
  textarea {
    flex: 1;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: var(--fs-callout);
    line-height: 1.45;
    padding: 6px 0;
    max-height: 92px;
    scrollbar-width: none;
  }
  textarea::placeholder {
    color: var(--text-4);
  }
  .send.mic {
    background: var(--chip);
    color: var(--text-2);
  }
  .send.mic:hover:not(:disabled) {
    background: var(--chip-strong);
    color: var(--text);
  }
  /* Aufnahme laeuft: der Knopf leuchtet und atmet mit der Lautstaerke. */
  .send.mic.live {
    background: var(--bad);
    color: #fff;
    box-shadow: 0 0 0 calc(2px + var(--level, 0) * 7px) rgba(255, 69, 58, 0.28);
    transition: box-shadow var(--t-fast) linear;
  }
  .send {
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    border: 0;
    background: var(--accent);
    color: #000;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: opacity var(--t-fast) ease, transform var(--t-fast) ease;
  }
  .send:disabled {
    opacity: 0.25;
    cursor: default;
  }
  .send:not(:disabled):active {
    transform: scale(0.92);
  }
  .square {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    background: #000;
  }
</style>
