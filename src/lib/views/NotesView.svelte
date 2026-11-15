<script lang="ts">
  import { t } from "../i18n";
  // Notizen wie ein kleines Schreibprogramm: links die Liste mit Suche, rechts der Editor mit
  // Werkzeugleiste (Ueberschriften, fett, kursiv, unterstrichen, durchgestrichen, Listen, Checkliste,
  // Textmarker, Schriftart und Groesse der Notiz). Gespeichert wird automatisch; der Kern bereinigt
  // das HTML und behaelt den reinen Text fuer Suche und Milo.
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { app, inTauri, niceDate, type Note } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoNotes } from "../demoData";
  import { rise } from "../motion";

  let notes = $state<Note[]>([]);
  let query = $state("");
  let current = $state<Note | null>(null);
  let editor = $state<HTMLDivElement>();
  let font = $state("f-sans");
  let size = $state("s-md");
  let status = $state("");
  let confirmDelete = $state(false);
  let saveTimer = 0;
  let clipsOpen = $state(false);
  let copied = $state("");
  async function useClip(id: number) {
    if (!inTauri) return;
    try {
      await invoke("clipboard_use", { id });
      copied = t("Kopiert!", "Copied!");
    } catch (err) {
      copied = String(err);
    }
    window.setTimeout(() => (copied = ""), 1600);
  }
  let searchTimer = 0;
  /** Zustand der Auswahl fuer die Werkzeugleiste. */
  let active = $state<Record<string, boolean>>({});

  async function load() {
    if (tour.demo === "notes") {
      notes = demoNotes();
      return;
    }
    notes = inTauri ? await app.listNotes(query) : notes;
  }

  onMount(() => {
    load().then(() => {
      if (notes.length > 0) open(notes[0]);
    });
    const onSelection = () => {
      if (!editor || !editor.contains(document.getSelection()?.anchorNode ?? null)) return;
      active = {
        bold: document.queryCommandState("bold"),
        italic: document.queryCommandState("italic"),
        underline: document.queryCommandState("underline"),
        strike: document.queryCommandState("strikeThrough"),
        ul: document.queryCommandState("insertUnorderedList"),
        ol: document.queryCommandState("insertOrderedList"),
      };
    };
    document.addEventListener("selectionchange", onSelection);
    return () => {
      document.removeEventListener("selectionchange", onSelection);
      clearTimeout(saveTimer);
    };
  });

  const escape = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

  /** Notiz in den Editor laden. Alte Textnotizen werden zu Absaetzen. */
  async function open(note: Note | null) {
    await flush();
    current = note;
    confirmDelete = false;
    await tick();
    if (!editor) return;
    let html = note?.rich ?? "";
    font = "f-sans";
    size = "s-md";
    if (html) {
      const wrap = new DOMParser().parseFromString(html, "text/html").body.firstElementChild;
      if (wrap && wrap.tagName === "DIV") {
        font = ["f-sans", "f-serif", "f-mono"].find((c) => wrap.classList.contains(c)) ?? "f-sans";
        size = ["s-sm", "s-md", "s-lg"].find((c) => wrap.classList.contains(c)) ?? "s-md";
        html = wrap.innerHTML;
      }
    } else if (note) {
      html = note.text.split(/\n/).map((line) => `<p>${escape(line) || "<br>"}</p>`).join("");
    }
    editor.innerHTML = html || "<p><br></p>";
    status = "";
  }

  async function create() {
    await flush();
    current = null;
    await tick();
    if (editor) {
      editor.innerHTML = "<h2><br></h2>";
      font = "f-sans";
      size = "s-md";
      editor.focus();
    }
  }

  function changed() {
    status = t("Bearbeitet", "Edited");
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(save, 700);
  }

  async function flush() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = 0;
      await save();
    }
  }

  async function save() {
    saveTimer = 0;
    if (!editor) return;
    const text = editor.innerText.trim();
    if (!text) return;
    const rich = `<div class="${font} ${size}">${editor.innerHTML}</div>`;
    if (!inTauri) {
      status = t("Gespeichert (Vorschau)", "Saved (preview)");
      return;
    }
    try {
      const id = await invoke<number>("save_rich_note", { id: current?.id ?? null, text, rich });
      status = t("Gespeichert", "Saved");
      const fresh = await app.listNotes(query);
      notes = fresh;
      current = fresh.find((n) => n.id === id) ?? current;
    } catch (err) {
      status = String(err);
    }
  }

  async function remove() {
    if (!current) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    await app.deleteNote(current.id);
    current = null;
    await load();
    open(notes[0] ?? null);
  }

  // ---------- Werkzeuge ----------

  function cmd(name: string, value?: string) {
    editor?.focus();
    document.execCommand(name, false, value);
    changed();
  }

  function block(tag: "h1" | "h2" | "p" | "blockquote") {
    cmd("formatBlock", tag);
  }

  /** Checkliste: eine Aufzaehlung mit der Klasse "check"; Klick auf das Kaestchen hakt ab. */
  function checklist() {
    editor?.focus();
    document.execCommand("insertUnorderedList");
    const node = document.getSelection()?.anchorNode;
    const list = (node instanceof Element ? node : node?.parentElement)?.closest("ul");
    list?.classList.add("check");
    changed();
  }

  function highlight() {
    const selection = document.getSelection();
    const text = selection?.toString() ?? "";
    if (!text) return;
    cmd("insertHTML", `<mark>${escape(text)}</mark>`);
  }

  function onEditorClick(e: MouseEvent) {
    const li = (e.target as HTMLElement).closest("ul.check > li");
    if (!li) return;
    // Nur das Kaestchen links schaltet um, Klicks in den Text setzen den Cursor.
    const box = li.getBoundingClientRect();
    if (e.clientX - box.left < 26) {
      li.classList.toggle("done");
      changed();
    }
  }

  function setFont(value: string) {
    font = value;
    changed();
  }
  function setSize(value: string) {
    size = value;
    changed();
  }

  const title = (n: Note) => n.text.split("\n").find((l) => l.trim())?.slice(0, 60) ?? t("Ohne Titel", "Untitled");
  const preview = (n: Note) => n.text.split("\n").filter((l) => l.trim()).slice(1, 3).join(" · ").slice(0, 80);
</script>

<div class="notes">
  <aside class="list">
    <div class="list-head">
      <input class="search" bind:value={query} oninput={() => { clearTimeout(searchTimer); searchTimer = window.setTimeout(load, 200); }} placeholder={t("Suchen", "Search")} aria-label={t("Notizen durchsuchen", "Search notes")} />
      <button class="new" onclick={create} title={t("Neue Notiz", "New note")} aria-label={t("Neue Notiz", "New note")}>
        <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
      </button>
    </div>
    <div class="items">
      {#each notes as note, i (note.id)}
        <button class="item" class:on={current?.id === note.id} onclick={() => open(note)} in:rise={{ delay: Math.min(i, 8) * 30, y: 6 }}>
          <b>{title(note)}</b>
          <small>{niceDate(note.updatedAt)}{preview(note) ? ` · ${preview(note)}` : ""}</small>
        </button>
      {:else}
        <p class="none">{query ? t("Nichts gefunden.", "Nothing found.") : t("Noch keine Notizen.", "No notes yet.")}</p>
      {/each}
    </div>
    {#if app.settings.clipboardHistory && app.clips.length > 0}
      <!-- Verlauf der Zwischenablage: nur im Arbeitsspeicher, Passwoerter landen hier nie. -->
      <div class="clips">
        <button class="clips-head" onclick={() => (clipsOpen = !clipsOpen)} aria-expanded={clipsOpen}>
          <span>{t("Zuletzt kopiert", "Recently copied")} <small>{app.clips.length}</small></span>
          <span class="chev" class:open={clipsOpen}>›</span>
        </button>
        {#if clipsOpen}
          <div class="clip-list">
            {#each app.clips as clip (clip.id)}
              <div class="clip">
                <button class="clip-text" onclick={() => useClip(clip.id)} title={t("Wieder kopieren", "Copy again")}>{clip.text.replace(/\s+/g, " ").slice(0, 140)}</button>
                <button class="clip-x" onclick={() => inTauri && invoke("clipboard_clear", { id: clip.id })} aria-label={t("Entfernen", "Remove")}>×</button>
              </div>
            {/each}
            <div class="clip-foot">
              <span>{copied || t("Klick kopiert wieder", "Click to copy again")}</span>
              <button class="clip-clear" onclick={() => inTauri && invoke("clipboard_clear", { id: null })}>{t("Leeren", "Clear")}</button>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </aside>

  <section class="page">
    <div class="toolbar glass" role="toolbar" aria-label={t("Formatierung", "Formatting")}>
      <div class="group">
        <button onclick={() => block("h1")} title={t("Große Überschrift", "Large heading")} aria-label={t("Große Überschrift", "Large heading")}><b class="t">H1</b></button>
        <button onclick={() => block("h2")} title={t("Überschrift", "Heading")} aria-label={t("Überschrift", "Heading")}><b class="t">H2</b></button>
        <button onclick={() => block("p")} title={t("Normaler Text", "Normal text")} aria-label={t("Normaler Text", "Normal text")}><span class="t">{t("Text", "Text")}</span></button>
      </div>
      <div class="group">
        <button class:on={active.bold} onclick={() => cmd("bold")} title={t("Fett (Strg+B)", "Bold (Ctrl+B)")} aria-label={t("Fett", "Bold")}><b class="t">{t("F", "B")}</b></button>
        <button class:on={active.italic} onclick={() => cmd("italic")} title={t("Kursiv (Strg+I)", "Italic (Ctrl+I)")} aria-label={t("Kursiv", "Italic")}><i class="t serif">{t("K", "I")}</i></button>
        <button class:on={active.underline} onclick={() => cmd("underline")} title={t("Unterstrichen (Strg+U)", "Underline (Ctrl+U)")} aria-label={t("Unterstrichen", "Underline")}><u class="t">U</u></button>
        <button class:on={active.strike} onclick={() => cmd("strikeThrough")} title={t("Durchgestrichen", "Strikethrough")} aria-label={t("Durchgestrichen", "Strikethrough")}><s class="t">S</s></button>
        <button onclick={highlight} title={t("Textmarker", "Highlighter")} aria-label={t("Textmarker", "Highlighter")}>
          <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M9.5 2.8l3.7 3.7-6 6H3.5V8.8Z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" /><path d="M2.5 14h11" stroke="#ffd60a" stroke-width="2" stroke-linecap="round" /></svg>
        </button>
      </div>
      <div class="group">
        <button class:on={active.ul} onclick={() => cmd("insertUnorderedList")} title={t("Aufzählung", "Bulleted list")} aria-label={t("Aufzählung", "Bulleted list")}>
          <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><circle cx="3.2" cy="4.5" r="1" fill="currentColor" /><circle cx="3.2" cy="8" r="1" fill="currentColor" /><circle cx="3.2" cy="11.5" r="1" fill="currentColor" /><path d="M6 4.5h7.5M6 8h7.5M6 11.5h7.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
        </button>
        <button class:on={active.ol} onclick={() => cmd("insertOrderedList")} title={t("Nummerierung", "Numbered list")} aria-label={t("Nummerierung", "Numbered list")}>
          <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><text x="1.5" y="6" font-size="4.6" fill="currentColor" font-weight="700">1</text><text x="1.5" y="12.6" font-size="4.6" fill="currentColor" font-weight="700">2</text><path d="M6 4.5h7.5M6 11h7.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
        </button>
        <button onclick={checklist} title={t("Checkliste", "Checklist")} aria-label={t("Checkliste", "Checklist")}>
          <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><rect x="1.8" y="2.6" width="4" height="4" rx="1" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M2.6 10.6l1 1 1.8-2" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" /><path d="M8 4.6h6M8 10.6h6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
        </button>
        <button onclick={() => block("blockquote")} title={t("Zitat", "Quote")} aria-label={t("Zitat", "Quote")}>
          <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M3 3.5v9M6 5.5h7M6 8h7M6 10.5h5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
        </button>
      </div>
      <div class="group">
        <select value={font} onchange={(e) => setFont(e.currentTarget.value)} aria-label={t("Schriftart", "Font")}>
          <option value="f-sans">{t("Klar", "Clean")}</option>
          <option value="f-serif">{t("Buch", "Book")}</option>
          <option value="f-mono">{t("Schreibmaschine", "Typewriter")}</option>
        </select>
        <select value={size} onchange={(e) => setSize(e.currentTarget.value)} aria-label={t("Schriftgröße", "Font size")}>
          <option value="s-sm">{t("Klein", "Small")}</option>
          <option value="s-md">{t("Normal", "Normal")}</option>
          <option value="s-lg">{t("Groß", "Large")}</option>
        </select>
      </div>
      <span class="status">{status}</span>
      {#if current}
        <button class="delete" class:sure={confirmDelete} onclick={remove} onmouseleave={() => (confirmDelete = false)} title={t("Notiz löschen", "Delete note")}>{confirmDelete ? t("Wirklich löschen?", "Really delete?") : t("Löschen", "Delete")}</button>
      {/if}
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div
      class="editor {font} {size}" bind:this={editor} contenteditable="true" spellcheck="true" role="textbox" tabindex="0" aria-multiline="true" aria-label={t("Notiz", "Note")}
      data-placeholder={t("Schreib los. Überschrift, Listen und Checklisten findest du oben.", "Start writing. Headings, lists and checklists are up top.")}
      oninput={changed} onclick={onEditorClick} onblur={() => flush()}
    ></div>
  </section>
</div>

<style>
  .notes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 230px 1fr;
    gap: 14px;
    padding: 0 22px 4px;
  }
  .list {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .clips {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 6px;
    border-top: 1px solid var(--line);
  }
  .clips-head {
    all: unset;
    display: flex;
    justify-content: space-between;
    align-items: center;
    cursor: pointer;
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  .clips-head small {
    margin-left: 4px;
    color: var(--text-4);
  }
  .chev {
    display: inline-block;
    transition: transform var(--t-fast) ease;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .clip-list {
    max-height: 170px;
    overflow-y: auto;
    scrollbar-width: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .clip {
    display: flex;
    gap: 4px;
  }
  .clip-text {
    all: unset;
    flex: 1;
    min-width: 0;
    padding: 6px 8px;
    border-radius: var(--r-sm);
    background: var(--well);
    font-size: var(--fs-footnote);
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .clip-text:hover {
    background: var(--chip-strong, #2c2c2e);
    color: var(--text);
  }
  .clip-x,
  .clip-clear {
    all: unset;
    cursor: pointer;
    color: var(--text-4);
    font-size: var(--fs-footnote);
    padding: 0 4px;
  }
  .clip-x:hover,
  .clip-clear:hover {
    color: var(--text);
  }
  .clip-foot {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-caption);
    color: var(--text-4);
    padding: 2px 2px 0;
  }
  .list-head {
    display: flex;
    gap: 6px;
  }
  .search {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--well);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
  }
  .new {
    flex: none;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: var(--r-full);
    background: var(--warn);
    color: #000;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: transform var(--t-slow) var(--ease-spring);
  }
  .new:active {
    transform: scale(0.9);
  }
  .items {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    scrollbar-width: thin;
    scrollbar-color: #2c2c2e transparent;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--r-md);
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background var(--t-fast) ease;
  }
  .item:hover {
    background: var(--well);
  }
  .item.on {
    background: color-mix(in srgb, var(--warn) 16%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--warn) 40%, transparent);
  }
  .item b {
    font-size: var(--fs-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item small {
    font-size: var(--fs-caption);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .none {
    margin: 10px 4px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .page {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border-radius: var(--r-lg);
    flex-wrap: wrap;
  }
  .group {
    position: relative;
    display: flex;
    gap: 1px;
    padding-right: 6px;
    margin-right: 1px;
    border-right: 1px solid rgba(255, 255, 255, 0.1);
  }
  .group:nth-last-child(3) {
    border-right: 0;
  }
  .toolbar button,
  .toolbar select {
    position: relative;
    height: 28px;
    min-width: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: background var(--t-fast) ease, color var(--t-fast) ease;
  }
  .toolbar select {
    display: block;
    padding: 0 var(--s-2);
    border-radius: var(--r-full);
    background-color: var(--chip);
  }
  .toolbar button:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }
  .toolbar button.on {
    background: rgba(255, 214, 10, 0.22);
    color: var(--warn);
  }
  .t {
    font-size: var(--fs-body);
  }
  .serif {
    font-family: Georgia, serif;
  }
  .status {
    position: relative;
    margin-left: auto;
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .toolbar .delete {
    color: #ff8a80;
    padding: 0 10px;
  }
  .toolbar .delete.sure {
    background: var(--bad);
    color: #fff;
  }

  /* Das Blatt */
  .editor {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 22px 40px;
    border-radius: var(--r-lg);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.06), rgba(255, 255, 255, 0.025));
    box-shadow: var(--card-edge);
    color: #f2f2f4;
    outline: none;
    line-height: 1.55;
    user-select: text;
    scrollbar-width: thin;
    scrollbar-color: #2c2c2e transparent;
  }
  .editor:focus-visible {
    box-shadow: var(--card-edge), 0 0 0 2px color-mix(in srgb, var(--warn) 50%, transparent);
  }
  .f-sans { font-family: var(--font-sans); }
  .f-serif { font-family: Georgia, "Iowan Old Style", "Times New Roman", serif; }
  .f-mono { font-family: var(--font-mono); }
  .s-sm { font-size: 13px; }
  .s-md { font-size: 15px; }
  .s-lg { font-size: 18px; }
  .editor :global(h1) {
    margin: 0 0 6px;
    font-size: 1.7em;
    line-height: 1.2;
    letter-spacing: -0.015em;
  }
  .editor :global(h2) {
    margin: 10px 0 4px;
    font-size: 1.3em;
    line-height: 1.25;
  }
  .editor :global(p) {
    margin: 0 0 6px;
  }
  .editor :global(ul),
  .editor :global(ol) {
    margin: 2px 0 8px;
    padding-left: 22px;
  }
  .editor :global(mark) {
    padding: 0 2px;
    border-radius: 3px;
    background: rgba(255, 214, 10, 0.45);
    color: inherit;
  }
  .editor :global(blockquote) {
    margin: 6px 0;
    padding: 2px 0 2px 12px;
    border-left: 3px solid var(--warn);
    color: var(--text-2);
  }
  .editor :global(ul.check) {
    list-style: none;
    padding-left: 4px;
  }
  .editor :global(ul.check > li) {
    position: relative;
    padding-left: 26px;
    margin: 4px 0;
  }
  .editor :global(ul.check > li)::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0.25em;
    width: 15px;
    height: 15px;
    border-radius: var(--r-xs);
    border: 1.5px solid rgba(255, 255, 255, 0.45);
    cursor: pointer;
    transition: background var(--t-fast) ease, border-color var(--t-fast) ease;
  }
  .editor :global(ul.check > li.done) {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .editor :global(ul.check > li.done)::before {
    background: #ffd60a center / 11px no-repeat url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'%3E%3Cpath d='M2.5 6.2l2.2 2.2 4.8-5' fill='none' stroke='black' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
    border-color: var(--warn);
  }
</style>
