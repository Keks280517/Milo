<script lang="ts">
  import { locale, t } from "../i18n";
  // Claude-Ansicht: was Claude Code gerade tut (Datei mit Aenderung, Aufgabenliste, letzte Schritte,
  // letzte Nachricht), die offiziellen Nutzungsgrenzen und alles, worauf Claude wartet: Freigaben,
  // Plaene und Rueckfragen, die sich hier direkt beantworten lassen. Milo liest dafuer nur, was
  // Claude Code ueber Hooks, Statuszeile und den Sitzungsverlauf meldet; Claude wird nicht langsamer.
  import { app, type ClaudeQuestion } from "../app.svelte";
  import { diffLines } from "../diff";
  import { formatAnswer } from "../format";
  import { onDestroy, untrack } from "svelte";
  import ClaudeGuide from "./ClaudeGuide.svelte";
  import { tour } from "../tourState.svelte";
  import { demoClaude } from "../demoData";

  // Tour: Claudes Live-Ansicht mit Beispiel-Sitzung (Datei, Aenderung, Aufgaben, Nachricht).
  const c = $derived(tour.demo === "claudeLive" || tour.demo === "buddy" ? demoClaude() : app.claude);
  const request = $derived(c.permission);
  let connecting = $state(false);
  let error = $state("");

  const ACTIVITY = { idle: t("Ruht", "Resting"), working: t("Arbeitet", "Working"), question: t("Wartet auf dich", "Waiting for you"), confirm: t("Braucht eine Freigabe", "Needs approval"), done: t("Fertig", "Done") };

  function fileName(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function folder(path: string): string {
    const parts = path.split(/[\\/]/);
    return parts.slice(-3, -1).join("/");
  }

  function resetIn(seconds: number): string {
    const ms = seconds * 1000 - Date.now();
    if (ms <= 0) return "jetzt";
    const h = Math.floor(ms / 3600000), m = Math.floor((ms % 3600000) / 60000);
    if (h >= 24) {
      const date = new Date(seconds * 1000);
      return date.toLocaleDateString(locale, { weekday: "short", hour: "2-digit", minute: "2-digit" });
    }
    return h > 0 ? t(`in ${h} Std ${m} Min`, `in ${h} h ${m} min`) : t(`in ${m} Min`, `in ${m} min`);
  }

  function age(ms: number): string {
    const min = Math.round((Date.now() - ms) / 60000);
    if (min < 1) return t("gerade eben", "just now");
    if (min < 60) return t(`vor ${min} Min`, `${min} min ago`);
    return t(`vor ${Math.round(min / 60)} Std`, `${Math.round(min / 60)} h ago`);
  }

  function tone(percent: number): string {
    return percent >= 90 ? "#ff453a" : percent >= 70 ? "#ffd60a" : "#f5b08a";
  }

  async function connect(enable: boolean) {
    connecting = true;
    error = "";
    try {
      await app.connectClaude(enable);
    } catch (err) {
      error = String(err);
    } finally {
      connecting = false;
    }
  }

  // ---------- Rueckfragen ----------

  const questions = $derived<ClaudeQuestion[]>(request?.kind === "ask" && Array.isArray(request.questions) ? request.questions : []);
  /** Gewaehlte Optionen und eigener Text je Frage (Index). */
  let picked = $state<Record<number, string[]>>({});
  let own = $state<Record<number, string>>({});
  let feedback = $state("");
  let shownRequest = "";
  $effect(() => {
    // Neue Anfrage: Auswahl leeren.
    const id = request?.id ?? "";
    if (id !== shownRequest) {
      shownRequest = id;
      picked = {};
      own = {};
      feedback = "";
    }
  });

  function toggle(index: number, label: string, multi: boolean) {
    const current = picked[index] ?? [];
    picked[index] = multi ? (current.includes(label) ? current.filter((l) => l !== label) : [...current, label]) : current[0] === label ? [] : [label];
    if (!multi) own[index] = "";
  }

  function answerOf(index: number): string {
    const text = (own[index] ?? "").trim();
    return text || (picked[index] ?? []).join(", ");
  }

  const complete = $derived(questions.length > 0 && questions.every((_, i) => answerOf(i) !== ""));

  function sendAnswers() {
    if (!complete) return;
    const answers: Record<string, string> = {};
    questions.forEach((q, i) => (answers[q.question] = answerOf(i)));
    app.answerClaudeQuestion(answers);
  }

  /** Restzeit, bis Claude wieder selbst fragt. */
  const remaining = $derived(request && request.until > 0 ? Math.max(0, Math.round((request.until - app.now) / 1000)) : 0);
  const remainingText = $derived(remaining >= 60 ? t(`noch ${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, "0")} Min`, `${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, "0")} min left`) : t(`noch ${remaining} s`, `${remaining} s left`));

  // ---------- Arbeitsansicht ----------

  /** Befehle sind keine Aenderung: ohne Plus und Minus zeigen. */
  const command = $derived(["Bash", "PowerShell"].includes(c.tool));
  const diff = $derived(
    command ? c.snippet.split("\n").map((text) => ({ kind: "same" as const, text })) : c.old || c.snippet ? diffLines(c.old, c.snippet).slice(-120) : [],
  );
  const changes = $derived({ add: diff.filter((l) => l.kind === "add").length, del: diff.filter((l) => l.kind === "del").length });
  const steps = $derived([...c.steps].reverse());
  const doneCount = $derived(c.todos.filter((t) => t.status === "completed").length);
  let showSaid = $state(false);
  /** Letzte Schritte sind zugeklappt, bis man sie aufklappt. */
  let showSteps = $state(false);

  // ---------- Live mitschreiben ----------
  // Claude meldet eine Aenderung als Ganzes. Damit man sieht, wo Claude arbeitet, laeuft sie hier wie
  // getippt durch: Zeile fuer Zeile, die aktuelle Zeile mit orangem Strich und Cursor, und die Ansicht
  // folgt ihr. Das passiert nur in Milo; Claude selbst arbeitet ungebremst weiter.

  /** Neue Zeilen (und Befehle) werden getippt; unveraenderte und geloeschte stehen sofort da. */
  const typedLine = (kind: string) => kind === "add" || command;
  /** Wo jede Zeile im getippten Text beginnt. */
  const starts = $derived.by(() => {
    let at = 0;
    return diff.map((line) => {
      const start = at;
      if (typedLine(line.kind)) at += line.text.length + 1;
      return start;
    });
  });
  const total = $derived(diff.reduce((sum, line) => sum + (typedLine(line.kind) ? line.text.length + 1 : 0), 0));
  let typedChars = $state(Infinity);
  let typingFrame = 0;
  let codeEl = $state<HTMLElement>();
  let typingFor = "";
  $effect(() => {
    const key = c.snippet + "\u0000" + c.file;
    const length = total;
    if (key === typingFor) return;
    // Beim Oeffnen der Ansicht steht das Bisherige schon da; getippt wird nur, was neu kommt.
    const first = typingFor === "";
    typingFor = key;
    cancelAnimationFrame(typingFrame);
    if (first || app.settings.reduceMotion || length === 0) {
      typedChars = Infinity;
      return;
    }
    // Hoechstens gut zwei Sekunden, damit man nie hinterherhinkt.
    const speed = Math.max(380, length / 2.2);
    const begin = performance.now();
    typedChars = 0;
    const tick = (now: number) => {
      typedChars = ((now - begin) / 1000) * speed;
      if (typedChars < length) typingFrame = requestAnimationFrame(tick);
      else typedChars = Infinity;
      follow();
    };
    typingFrame = requestAnimationFrame(tick);
  });
  /** Zeile, an der gerade geschrieben wird (waehrend des Tippens), sonst die letzte neue Zeile. */
  const cursorLine = $derived.by(() => {
    if (diff.length === 0) return -1;
    if (typedChars === Infinity) {
      for (let i = diff.length - 1; i >= 0; i--) if (typedLine(diff[i].kind)) return i;
      return diff.length - 1;
    }
    let line = -1;
    for (let i = 0; i < diff.length; i++) if (typedLine(diff[i].kind) && starts[i] <= typedChars) line = i;
    return line;
  });
  function visibleText(i: number): string | null {
    const start = starts[i];
    if (typedChars < start) return null;
    const line = diff[i];
    if (!typedLine(line.kind)) return line.text;
    return line.text.slice(0, Math.max(0, Math.floor(typedChars - start)));
  }
  /** Haelt die Zeile, an der geschrieben wird, im Blick. */
  function follow() {
    const box = codeEl;
    const line = box?.querySelector<HTMLElement>(".writing");
    if (!box || !line) return;
    const bottom = line.offsetTop + line.offsetHeight + 8;
    if (bottom > box.scrollTop + box.clientHeight) box.scrollTop = bottom - box.clientHeight;
    else if (line.offsetTop < box.scrollTop) box.scrollTop = line.offsetTop - 8;
  }

  /** Claudes Nachricht laeuft ebenfalls mit; kommt Text dazu, geht es dort weiter. */
  let saidChars = $state(Infinity);
  let saidFrame = 0;
  let saidBefore = "";
  let saidSeen = false;
  $effect(() => {
    const said = c.said;
    if (said === saidBefore) return;
    const first = !saidSeen;
    saidSeen = true;
    const before = untrack(() => saidChars);
    const from = !saidBefore || !said.startsWith(saidBefore) ? 0 : before === Infinity ? saidBefore.length : Math.min(before, said.length);
    saidBefore = said;
    cancelAnimationFrame(saidFrame);
    if (first || app.settings.reduceMotion || !said) {
      saidChars = Infinity;
      return;
    }
    const speed = Math.max(260, (said.length - from) / 2.5);
    const begin = performance.now();
    saidChars = from;
    const tick = (now: number) => {
      saidChars = from + ((now - begin) / 1000) * speed;
      if (saidChars < said.length) saidFrame = requestAnimationFrame(tick);
      else saidChars = Infinity;
    };
    saidFrame = requestAnimationFrame(tick);
  });
  onDestroy(() => {
    cancelAnimationFrame(typingFrame);
    cancelAnimationFrame(saidFrame);
  });
  const saidTyping = $derived(saidChars !== Infinity);
  const working = $derived(c.present && c.activity === "working");

  const TOOL_LABEL: Record<string, string> = { Edit: t("Bearbeitet", "Edits"), MultiEdit: t("Bearbeitet", "Edits"), Write: t("Schreibt", "Writes"), Read: t("Liest", "Reads"), Bash: t("Befehl", "Command"), PowerShell: t("Befehl", "Command"), Grep: t("Sucht", "Searches"), Glob: t("Sucht", "Searches"), WebFetch: t("Liest Web", "Reads web"), WebSearch: t("Sucht im Web", "Searches web"), Agent: t("Helfer", "Helper"), Task: t("Helfer", "Helper"), NotebookEdit: "Notebook" };
  const label = (tool: string) => TOOL_LABEL[tool] ?? tool;
</script>

<div class="view">
  {#if !c.connected}
    <section class="tile intro">
      <p>{t(`Verbinde ${app.settings.name} mit Claude Code, dann siehst du hier live, woran Claude arbeitet, wie viel von deinem Limit verbraucht ist, und kannst Freigaben und Rückfragen direkt hier beantworten.`, `Connect ${app.settings.name} to Claude Code to see live what Claude is working on and how much of your limit is used, and to answer approvals and questions right here.`)}</p>
      <ClaudeGuide />
      <button class="primary" disabled={connecting} onclick={() => connect(true)}>{connecting ? t("Verbinde …", "Connecting …") : t("Mit Claude Code verbinden", "Connect to Claude Code")}</button>
      {#if error}<p class="error">{error}</p>{/if}
    </section>
  {:else}
    {#if request?.kind === "ask" && questions.length > 0}
      <section class="tile request ask">
        <div class="kicker row-between"><span>{t("Claude fragt", "Claude asks")}</span>{#if remaining > 0}<span class="countdown">{remainingText}</span>{/if}</div>
        {#each questions as q, i}
          <div class="question">
            {#if q.header}<span class="chip">{q.header}</span>{/if}
            <p class="qtext">{q.question}</p>
            <div class="options">
              {#each q.options ?? [] as option}
                {@const on = (picked[i] ?? []).includes(option.label)}
                <button class="option" class:on onclick={() => toggle(i, option.label, !!q.multiSelect)}>
                  <span class="mark" class:multi={q.multiSelect}>{#if on}✓{/if}</span>
                  <span class="otext">
                    <span class="olabel">{option.label}</span>
                    {#if option.description}<span class="odesc">{option.description}</span>{/if}
                  </span>
                </button>
              {/each}
            </div>
            <input class="own" placeholder={t("Eigene Antwort …", "Your own answer …")} bind:value={own[i]} oninput={() => { if (!q.multiSelect && own[i]) picked[i] = []; }} onkeydown={(e) => e.key === "Enter" && sendAnswers()} />
          </div>
        {/each}
        <div class="row">
          <button class="link small-link" onclick={() => app.passClaudeQuestion()}>{t("In Claude beantworten", "Answer in Claude")}</button>
          <button class="allow" disabled={!complete} onclick={sendAnswers}>{t("Antworten", "Answer")}</button>
        </div>
      </section>
    {:else if request?.kind === "plan"}
      <section class="tile request">
        <div class="kicker row-between"><span>{t("Claude hat einen Plan", "Claude has a plan")}</span>{#if remaining > 0}<span class="countdown">{remainingText}</span>{/if}</div>
        {#if request.plan}
          <div class="plan">{@html formatAnswer(request.plan)}</div>
        {:else}
          <p class="small">{t("Den Plantext hat Claude nicht mitgeschickt; er steht in der Claude-App.", "Claude didn't send the plan text; you'll find it in the Claude app.")}</p>
        {/if}
        <input class="own" placeholder={t("Rückmeldung (optional, beim Ablehnen) …", "Feedback (optional, when rejecting) …")} bind:value={feedback} />
        <div class="row">
          <button class="link small-link" onclick={() => app.passClaudeQuestion()}>{t("In Claude entscheiden", "Decide in Claude")}</button>
          <button class="deny" onclick={() => app.answerClaude(false, feedback.trim())}>{t("Ablehnen", "Reject")}</button>
          <button class="allow" onclick={() => app.answerClaude(true)}>{t("Plan annehmen", "Accept plan")}</button>
        </div>
      </section>
    {:else if request}
      <section class="tile request">
        <div class="kicker row-between"><span>{t(`Claude möchte ${request.tool || "etwas"} benutzen`, `Claude wants to use ${request.tool || "something"}`)}</span>{#if remaining > 0}<span class="countdown">{remainingText}</span>{/if}</div>
        <pre>{request.detail}</pre>
        <div class="row">
          <button class="link small-link" onclick={() => app.passClaudeQuestion()}>{t("In Claude entscheiden", "Decide in Claude")}</button>
          <button class="deny" onclick={() => app.answerClaude(false)}>{t("Ablehnen", "Deny")}</button>
          <button class="allow" onclick={() => app.answerClaude(true)}>{t("Erlauben", "Allow")}</button>
        </div>
      </section>
    {/if}

    <section class="tile">
      <div class="head">
        <span class="state {c.activity}"></span>
        <span class="what">{c.present ? ACTIVITY[c.activity] : t("Keine aktive Sitzung", "No active session")}</span>
        {#if c.project}<span class="project">{c.project}</span>{/if}
      </div>
      {#if c.text && !request}<p class="said-short">{c.text}</p>{/if}
      {#if c.limits && (c.limits.model || c.limits.context !== null)}
        <div class="meta">
          {#if c.limits.model}<span class="chip model">{c.limits.model}</span>{/if}
          {#if c.limits.context !== null}
            <span class="ctx-label">{t("Kontext", "Context")}</span>
            <div class="bar ctx"><span style:width="{Math.min(100, c.limits.context)}%" style:background={tone(c.limits.context)}></span></div>
            <span class="pct">{Math.round(c.limits.context)} %</span>
          {/if}
        </div>
      {/if}
    </section>

    <section class="tile live" class:active={working}>
      <div class="kicker row-between">
        <span class="now">{#if working}<span class="pen" aria-hidden="true"></span>{t("Claude schreibt gerade", "Claude is writing")}{:else}{t("Jetzt", "Now")}{/if}</span>
        {#if diff.length > 0 && c.old}<span class="counts"><span class="plus">+{changes.add}</span> <span class="minus">−{changes.del}</span></span>{/if}
      </div>
      {#if c.file || diff.length > 0}
        <div class="file">
          <span class="tool">{label(c.tool)}</span>
          {#if c.file}<span class="fname" title={c.file}>{fileName(c.file)}</span><span class="dir">{folder(c.file)}</span>{/if}
        </div>
        {#if diff.length > 0}
          <pre class="code" bind:this={codeEl}>{#each diff as line, i}{@const text = visibleText(i)}{#if text !== null}<span class="ln {line.kind}" class:writing={i === cursorLine}><span class="sign">{line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}</span>{text}{#if i === cursorLine && (typedChars !== Infinity || working)}<span class="caret" class:blink={typedChars === Infinity}></span>{:else if !text}{" "}{/if}</span>{/if}{/each}</pre>
        {/if}
      {:else}
        <p class="small">{t("Sobald Claude eine Datei schreibt oder einen Befehl ausführt, siehst du es hier.", "As soon as Claude writes a file or runs a command, you'll see it here.")}</p>
      {/if}
    </section>

    {#if c.todos.length > 0}
      <section class="tile">
        <div class="kicker row-between"><span>{t("Aufgaben", "Tasks")}</span><span class="counts">{doneCount}/{c.todos.length}</span></div>
        <ul class="todos">
          {#each c.todos as todo}
            <li class={todo.status}>
              <span class="check">{#if todo.status === "completed"}✓{/if}</span>
              <span>{todo.text}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if c.said}
      <section class="tile said" class:active={saidTyping}>
        <button class="kicker toggle" onclick={() => (showSaid = !showSaid)}>{saidTyping ? t("Claude schreibt …", "Claude is writing …") : t("Claudes letzte Nachricht", "Claude's last message")} <span class="chev" class:open={showSaid || saidTyping}>›</span></button>
        {#if saidTyping}
          <div class="message typing"><div>{c.said.slice(0, Math.floor(saidChars))}<span class="caret"></span></div></div>
        {:else}
          <div class="message" class:clamped={!showSaid}>{@html formatAnswer(c.said)}</div>
        {/if}
      </section>
    {/if}

    <!-- Letzte Schritte: zugeklappt nur eine schmale Zeile mit dem neuesten Schritt. -->
    {#if steps.length > 0}
      <section class="tile steps-tile">
        <button class="kicker toggle row-between" onclick={() => (showSteps = !showSteps)} aria-expanded={showSteps}>
          <span>{t("Letzte Schritte", "Recent steps")} <span class="counts">{steps.length}</span></span>
          <span class="chev" class:open={showSteps}>›</span>
        </button>
        {#if showSteps}
          <ul class="steps">
            {#each steps as step, i}
              <li class:latest={i === 0}>
                <span class="time">{step.at.slice(0, 5)}</span>
                <span class="stool">{label(step.tool)}</span>
                <span class="swhat" title={step.what}>{step.what}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="steps-peek" title={steps[0].what}><span class="stool">{label(steps[0].tool)}</span><span class="swhat">{steps[0].what}</span></div>
        {/if}
      </section>
    {/if}

    <section class="tile">
      <div class="kicker">{t("Nutzungsgrenzen", "Usage limits")}</div>
      {#if c.limits && (c.limits.fiveHour || c.limits.sevenDay)}
        {#each [[t("5 Stunden", "5 hours"), c.limits.fiveHour], [t("Woche", "Week"), c.limits.sevenDay]] as const as [name, window]}
          {#if window}
            <div class="limit">
              <span class="lname">{name}</span>
              <div class="bar"><span style:width="{Math.min(100, window.usedPercent)}%" style:background={tone(window.usedPercent)}></span></div>
              <span class="pct">{Math.round(window.usedPercent)} %</span>
              <span class="reset">{t("neu", "resets")} {resetIn(window.resetsAt)}</span>
            </div>
          {/if}
        {/each}
        <div class="small">{t("Stand", "As of")} {age(c.limits.at)}</div>
      {:else}
        <p class="small">{t("Noch keine Werte. Claude Code meldet sie nach der ersten Antwort einer Sitzung über die Statuszeile (nur mit Pro- oder Max-Abo). In der Claude-Desktop-App kommen sie eventuell gar nicht an, im Terminal schon.", "No values yet. Claude Code reports them through the status line after the first answer of a session (Pro or Max plan only). They may not arrive from the Claude desktop app, but they do from the terminal.")}</p>
      {/if}
    </section>

    <button class="link" disabled={connecting} onclick={() => connect(false)}>{t("Verbindung zu Claude Code trennen", "Disconnect from Claude Code")}</button>
  {/if}
</div>

<style>
  .view {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px 12px 12px;
    scrollbar-width: none;
    font-variant-numeric: tabular-nums;
  }
  .tile {
    border-radius: var(--r-md);
    background: var(--card);
    box-shadow: var(--card-edge);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .intro p {
    margin: 0;
    font-size: var(--fs-body);
    line-height: 1.5;
    color: var(--text-2);
  }
  .kicker {
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  .row-between {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .countdown,
  .counts {
    text-transform: none;
    letter-spacing: 0;
    font-weight: var(--fw-medium);
    color: var(--text-4);
  }
  .plus { color: #30d158; }
  .minus { color: #ff6961; }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-callout);
  }
  .state {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #48484a;
  }
  .state.working { background: #f5b08a; animation: pulse 1.4s ease-in-out infinite; }
  .state.question, .state.confirm { background: #ffd60a; animation: pulse 1s ease-in-out infinite; }
  .state.done { background: #30d158; }
  .what {
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .project {
    margin-left: auto;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .said-short {
    margin: 0;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .meta {
    display: grid;
    grid-template-columns: auto auto 1fr 42px;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .chip {
    justify-self: start;
    align-self: flex-start;
    padding: 1px 8px;
    border-radius: var(--r-xs);
    background: var(--chip);
    color: var(--text-2);
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
  }
  .chip.model {
    background: #2a1c14;
    color: #f5b08a;
  }
  .limit {
    display: grid;
    grid-template-columns: 70px 1fr 42px 110px;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .pct {
    text-align: right;
    font-weight: var(--fw-semibold);
    color: var(--text-2);
  }
  .reset {
    color: var(--text-3);
    font-size: var(--fs-footnote);
  }
  .bar {
    position: relative;
    height: 6px;
    border-radius: 3px;
    background: var(--chip-strong);
    overflow: hidden;
  }
  .bar span {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 3px;
    transition: width var(--t-spring) ease;
  }
  .small {
    margin: 0;
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-3);
  }
  .file {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: var(--fs-footnote);
    min-width: 0;
  }
  .tool {
    padding: 1px 8px;
    border-radius: var(--r-xs);
    background: #2a1c14;
    color: #f5b08a;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
  }
  .fname {
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .dir {
    color: var(--text-4);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  pre {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: var(--fs-caption);
    line-height: 1.5;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 230px;
    overflow: auto;
  }
  .code {
    padding: 6px 0;
  }
  .ln {
    display: block;
    padding: 0 10px 0 4px;
  }
  .ln.add {
    background: rgba(48, 209, 88, 0.12);
    color: #d6f5de;
  }
  .ln.del {
    background: rgba(255, 69, 58, 0.12);
    color: #f5c9c6;
    text-decoration: line-through;
    text-decoration-color: rgba(255, 105, 97, 0.45);
  }
  .sign {
    display: inline-block;
    width: 14px;
    color: var(--text-4);
    text-decoration: none;
  }
  .ln.add .sign { color: #30d158; }
  .ln.del .sign { color: #ff6961; }
  .todos,
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--fs-footnote);
  }
  .todos li {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    color: var(--text-2);
  }
  .check {
    flex: none;
    width: 14px;
    height: 14px;
    margin-top: 2px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px #48484a;
    font-size: 9px;
    line-height: 14px;
    text-align: center;
    color: #000;
  }
  .todos li.completed { color: #6e6e73; }
  .todos li.completed .check {
    background: var(--ok);
    box-shadow: none;
  }
  .todos li.in_progress { color: #f5f5f7; font-weight: 600; }
  .todos li.in_progress .check {
    box-shadow: inset 0 0 0 1.5px #f5b08a;
    animation: pulse 1.4s ease-in-out infinite;
  }
  .steps li {
    display: grid;
    grid-template-columns: 40px auto 1fr;
    gap: 8px;
    align-items: baseline;
    color: var(--text-3);
  }
  .steps li.latest { color: #f5f5f7; }
  /* Schritte bewusst kleiner und ruhiger als Aufgaben und Nachricht. */
  .steps-tile {
    padding: 8px 12px;
    gap: 4px;
  }
  .steps-tile .toggle {
    display: flex;
  }
  .steps-tile .steps {
    font-size: var(--fs-caption);
    gap: 2px;
    max-height: 160px;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .steps-peek {
    display: flex;
    gap: 8px;
    align-items: baseline;
    min-width: 0;
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .time {
    font-size: var(--fs-caption);
    color: var(--text-4);
  }
  .stool {
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    color: #f5b08a;
  }
  .swhat {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toggle {
    all: unset;
    cursor: pointer;
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  .chev {
    display: inline-block;
    transition: transform var(--t-fast) ease;
  }
  .chev.open { transform: rotate(90deg); }
  /* Live: oranger Strich links an der Kachel, solange Claude schreibt. */
  .live,
  .said {
    position: relative;
    overflow: hidden;
    flex: none;
  }
  .live::before,
  .said::before {
    content: "";
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: #d97757;
    opacity: 0;
    transition: opacity var(--t-base) ease;
  }
  .live.active::before,
  .said.active::before {
    opacity: 1;
    box-shadow: 0 0 10px rgba(217, 119, 87, 0.8);
    animation: pulse 1.6s ease-in-out infinite;
  }
  .now {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .live.active .now {
    color: #f5b08a;
  }
  .pen {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #d97757;
    box-shadow: 0 0 8px #d97757;
    animation: pulse 1s ease-in-out infinite;
  }
  /* Zeile, an der Claude gerade schreibt: oranger Strich und Cursor. */
  .ln.writing {
    background: rgba(217, 119, 87, 0.14);
    box-shadow: inset 3px 0 0 #d97757;
    color: #fbe3d6;
  }
  .caret {
    display: inline-block;
    width: 2px;
    height: 1.15em;
    margin-left: 1px;
    vertical-align: text-bottom;
    border-radius: 1px;
    background: #d97757;
    box-shadow: 0 0 6px rgba(217, 119, 87, 0.9);
  }
  .caret.blink,
  .typing .caret {
    animation: blink 1s steps(1) infinite;
  }
  .message.typing {
    white-space: pre-wrap;
    max-height: 200px;
    overflow: hidden;
    display: flex;
    flex-direction: column-reverse;
  }
  @keyframes blink {
    0%, 50% { opacity: 1; }
    50.01%, 100% { opacity: 0; }
  }
  .message,
  .plan {
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-2);
  }
  .message.clamped {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .plan {
    max-height: 240px;
    overflow: auto;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
  }
  .request {
    background: #1d1708;
    box-shadow: inset 0 0 0 1px #4a3a10;
  }
  .request .kicker { color: #ffd60a; }
  .question {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .question + .question {
    padding-top: 8px;
    border-top: 1px solid #3a2e0e;
  }
  .qtext {
    margin: 0;
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .option {
    all: unset;
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    background: #15120a;
    cursor: pointer;
    transition: background var(--t-fast) ease;
  }
  .option:hover { background: #241d0b; }
  .option.on {
    background: #2e250c;
    box-shadow: inset 0 0 0 1px var(--warn);
  }
  .mark {
    flex: none;
    width: 14px;
    height: 14px;
    margin-top: 1px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px #8e7a3a;
    font-size: 9px;
    line-height: 14px;
    text-align: center;
    color: #000;
  }
  .mark.multi { border-radius: 4px; }
  .option.on .mark {
    background: var(--warn);
    box-shadow: none;
  }
  .otext {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .olabel {
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .odesc {
    font-size: var(--fs-footnote);
    color: #a8a29a;
  }
  .own {
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    outline: none;
  }
  .own:focus { box-shadow: inset 0 0 0 1px #8e7a3a; }
  .row {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
  }
  button {
    height: 30px;
    padding: 0 14px;
    border: 0;
    border-radius: var(--r-lg);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .primary,
  .allow {
    background: var(--text);
    color: #000;
  }
  .deny {
    background: var(--chip-strong);
    color: var(--text);
  }
  .primary {
    align-self: flex-start;
  }
  .link {
    align-self: center;
    background: none;
    color: var(--text-4);
    font-weight: var(--fw-medium);
  }
  .small-link {
    margin-right: auto;
    padding: 0 4px;
  }
  .link:hover {
    color: var(--text-2);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .error {
    color: #ff6961;
    font-size: var(--fs-footnote);
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
  @keyframes appear {
    from { opacity: 0; transform: translateX(-4px); }
    to { opacity: 1; transform: none; }
  }
</style>
