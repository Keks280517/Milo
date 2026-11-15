<script lang="ts">
  import { lens } from "$lib/lens";
  import { t, lang } from "../i18n";
  // Eigene Widgets anlegen und bearbeiten: im Baukasten aus festen Bausteinen zusammenstellen
  // oder per KI aus einer Beschreibung entwerfen lassen (das Ergebnis landet zur Kontrolle im
  // Baukasten). Gespeicherte Widgets haengen am oberen Bildschirmrand neben der Insel.
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { app, inTauri } from "../app.svelte";
  import WidgetFace from "../widgets/WidgetFace.svelte";
  import Orb from "../widgets/Orb.svelte";
  import { WidgetFeed } from "../widgets/feed.svelte";
  import { ACCENTS, BLOCKS, PARAM, SHAPES, ZONES, emptyWidget, roomOf, type BlockKind, type OrbState, type Widget } from "../widgets/types";
  import { viewOf } from "../widgets/blocks";
  import { paneIn, rise } from "../motion";

  let widgets = $state<Widget[]>([]);
  let draft = $state<Widget>(emptyWidget());
  let mode = $state<"build" | "ai">("build");
  let description = $state("");
  let busy = $state(false);
  let message = $state("");
  let confirmDelete = $state("");
  const feed = new WidgetFeed();

  /** Was gefragt wurde; steht waehrend der Suche mittig unter der Kugel. */
  let asked = $state("");
  /** Entwuerfe zeigen (erst nach dem Lichtring der fertigen Kugel). */
  let showResults = $state(false);
  let variantsBox = $state<HTMLElement>();

  /** Entwuerfe der KI zur Auswahl. */
  let variants = $state<Widget[]>([]);
  /** Die Kugel hoert gerade fuer die Widget-KI zu. */
  let hearing = $state(false);
  let justDone = $state(false);

  // Im Baukasten laufen alle Quellen, damit jede Kachel ihren echten Wert zeigt.
  $effect(() => feed.use(mode === "build" ? BLOCKS.map((b) => b.kind) : [...draft.blocks, ...variants.flatMap((v) => v.blocks)].map((b) => b.kind)));

  // ---------- Bibliothek, Flug in das Widget, Aufhaengen ----------
  let query = $state("");
  const ALL = t("Alle", "All");
  let group = $state(ALL);
  const GROUPS = [ALL, ...new Set(BLOCKS.map((b) => b.group))];
  const catalog = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return BLOCKS.filter((b) => (group === ALL || b.group === group) && (!q || `${b.label} ${b.hint} ${b.group}`.toLowerCase().includes(q)));
  });
  /** Vorschlaege fuer die KI. */
  const IDEAS = lang === "en"
    ? ["End of workday with weather", "Gaming: CPU graph and RAM", "Analog clock with moon phase", "My day: event, sun, rain"]
    : ["Feierabend mit Wetter", "Gaming: CPU-Kurve und RAM", "Analoguhr mit Mondphase", "Mein Tag: Termin, Sonne, Regen"];
  let stageFace = $state<HTMLElement>();
  let bumping = $state(false);
  let launching = $state(false);

  function addFrom(e: MouseEvent, kind: BlockKind) {
    if (draft.blocks.length >= room) {
      message = t(`Diese Form hat ${room} Plätze. Für mehr Bausteine nimm die Karte.`, `This shape has ${room} slots. For more blocks, use the card.`);
      return;
    }
    const from = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const to = stageFace?.getBoundingClientRect();
    addBlock(kind);
    message = "";
    if (to) fly(label(kind), from, to);
  }

  /** Ein Abbild der Kachel fliegt im Bogen in das Widget und loest dort einen kleinen Puls aus. */
  function fly(text: string, from: DOMRect, to: DOMRect) {
    const reduced = document.documentElement.classList.contains("reduce-motion") || matchMedia("(prefers-reduced-motion: reduce)").matches;
    const pulse = () => {
      bumping = false;
      requestAnimationFrame(() => (bumping = true));
      window.setTimeout(() => (bumping = false), 560);
    };
    if (reduced) return pulse();
    const chip = document.createElement("div");
    chip.className = "fly-chip";
    chip.textContent = text;
    Object.assign(chip.style, { left: `${from.left}px`, top: `${from.top}px`, width: `${from.width}px`, height: `${from.height}px` });
    document.body.appendChild(chip);
    const dx = to.left + to.width / 2 - (from.left + from.width / 2);
    const dy = to.top + to.height / 2 - (from.top + from.height / 2);
    const animation = chip.animate(
      [
        { transform: "translate(0, 0) scale(1)", opacity: 1 },
        { transform: `translate(${dx * 0.5}px, ${dy * 0.5 - 60}px) scale(0.75)`, opacity: 1, offset: 0.5 },
        { transform: `translate(${dx}px, ${dy}px) scale(0.18)`, opacity: 0 },
      ],
      { duration: 620, easing: "cubic-bezier(0.33, 0.8, 0.25, 1)" },
    );
    animation.onfinish = () => {
      chip.remove();
      pulse();
    };
  }

  // Bricht das Zuhoeren ohne Ergebnis ab, gehoert der naechste Satz wieder dem Chat.
  $effect(() => {
    if (hearing && (app.voice === "off" || app.voice === "wake")) {
      const wait = window.setTimeout(() => {
        hearing = false;
        app.voiceTarget = null;
      }, 400);
      return () => clearTimeout(wait);
    }
  });

  const orb = $derived<OrbState>(busy ? "thinking" : hearing && app.voice === "listening" ? "listening" : hearing && app.voice === "transcribing" ? "thinking" : justDone ? "done" : "idle");
  const orbHint = $derived(
    busy ? t("Ich entwerfe drei Vorschläge …", "Designing three options …")
      : hearing && app.voice === "listening" ? t("Ich höre zu … sag, was dein Widget zeigen soll", "Listening … tell me what your widget should show")
      : hearing && app.voice === "transcribing" ? t("Verstehe …", "Understanding …")
      : showResults ? t("Fertig. Such dir einen Entwurf aus, oder sag es nochmal anders.", "Done. Pick a design, or describe it differently.")
      : t("Tippe auf die Kugel und sprich, oder schreib unten.", "Tap the orb and speak, or type below."),
  );

  function talk() {
    if (busy) return;
    if (hearing) {
      // Zweiter Tipp beendet die Aufnahme; das Ergebnis kommt dann von selbst.
      app.toggleVoice();
      return;
    }
    hearing = true;
    app.voiceTarget = (text) => {
      hearing = false;
      description = text;
      design();
    };
    app.toggleVoice();
  }

  function pick(widget: Widget) {
    draft = { ...structuredClone($state.snapshot(widget)), id: draft.id, x: draft.x, locked: draft.locked };
    variants = [];
    showResults = false;
    asked = "";
    mode = "build";
    message = t("Übernommen. Pass ihn hier an und speichere ihn.", "Taken. Adjust it here and save it.");
  }

  async function load() {
    if (inTauri) widgets = await invoke<Widget[]>("widgets_list");
  }

  onMount(() => {
    load();
    let stop: (() => void) | undefined;
    if (inTauri) listen<Widget[]>("milo://widgets", ({ payload }) => (widgets = payload)).then((fn) => (stop = fn));
    return () => {
      stop?.();
      feed.stop();
    };
  });

  // Rechtsklick auf ein Widget: dieses zum Bearbeiten laden.
  $effect(() => {
    const id = app.editWidget;
    if (!id) return;
    const found = widgets.find((w) => w.id === id);
    if (found) {
      edit(found);
      app.editWidget = "";
    }
  });

  function edit(widget: Widget) {
    draft = structuredClone($state.snapshot(widget));
    mode = "build";
    message = "";
  }

  function fresh() {
    draft = emptyWidget();
    message = "";
  }

  const room = $derived(roomOf(draft.shape));

  function setShape(shape: Widget["shape"]) {
    draft.shape = shape;
    draft.blocks = draft.blocks.slice(0, roomOf(shape));
  }

  function addBlock(kind: BlockKind) {
    if (draft.blocks.length >= room) return;
    draft.blocks = [...draft.blocks, { kind, text: PARAM[kind]?.initial ?? "" }];
  }

  function moveBlock(index: number, by: number) {
    const list = [...draft.blocks];
    const [block] = list.splice(index, 1);
    list.splice(Math.max(0, Math.min(list.length, index + by)), 0, block);
    draft.blocks = list;
  }

  function removeBlock(index: number) {
    draft.blocks = draft.blocks.filter((_, i) => i !== index);
  }

  async function save() {
    if (draft.blocks.length === 0) {
      message = t("Füge mindestens einen Baustein hinzu.", "Add at least one block.");
      return;
    }
    busy = true;
    message = "";
    try {
      const saved = await invoke<Widget>("widget_save", { widget: $state.snapshot(draft) });
      draft = saved;
      launching = true;
      window.setTimeout(() => (launching = false), 1150);
      message = t("Gespeichert. Das Widget hängt jetzt oben am Bildschirmrand; zum Verschieben einfach entlang der Kante ziehen.", "Saved. The widget now hangs from the top edge of the screen; drag it along the edge to move it.");
      load();
    } catch (err) {
      message = String(err);
    } finally {
      busy = false;
    }
  }

  async function remove(widget: Widget) {
    if (confirmDelete !== widget.id) {
      confirmDelete = widget.id;
      return;
    }
    confirmDelete = "";
    await invoke("widget_delete", { id: widget.id });
    if (draft.id === widget.id) fresh();
    load();
  }

  async function design() {
    const text = description.trim();
    if (!text || busy) return;
    asked = text;
    // Die Frage wandert aus dem Feld unter die Kugel.
    description = "";
    busy = true;
    message = "";
    variants = [];
    showResults = false;
    try {
      const found = await invoke<Widget[]>("widget_design", { description: text });
      busy = false;
      justDone = true;
      // Erst leuchtet die fertige Kugel auf, dann gleiten die Entwuerfe nach oben ins Bild.
      await new Promise((r) => setTimeout(r, 750));
      variants = found;
      showResults = true;
      await tick();
      variantsBox?.scrollIntoView({ behavior: "smooth", block: "end" });
      window.setTimeout(() => (justDone = false), 900);
    } catch (err) {
      busy = false;
      message = String(err);
      description = text;
    }
  }

  const label = (kind: BlockKind) => BLOCKS.find((b) => b.kind === kind)?.label ?? kind;
</script>

<div class="view">
  <div class="topbar">
    <div class="segments glass" use:lens={{ selector: "button.on" }} role="tablist" aria-label={t("Art", "Mode")}>
      <span class="lens-thumb pick" aria-hidden="true"></span>
      <button role="tab" aria-selected={mode === "build"} class:on={mode === "build"} onclick={() => (mode = "build")}>{t("Baukasten", "Builder")}</button>
      <button role="tab" aria-selected={mode === "ai"} class:on={mode === "ai"} onclick={() => (mode = "ai")}>{t("Mit KI entwerfen", "Design with AI")}</button>
    </div>
    <div class="mine" aria-label={t("Deine Widgets", "Your widgets")}>
      {#each widgets as widget (widget.id)}
        <span class="mine-item" class:current={draft.id === widget.id} in:rise={{ y: 6 }}>
          <button class="mine-chip" onclick={() => edit(widget)} title={t("Bearbeiten", "Edit")}>
            <span class="swatch" style:background={widget.accent}></span>{widget.name}{#if widget.locked}<small>{t("gesperrt", "locked")}</small>{/if}
          </button>
          <button class="mine-x" class:danger={confirmDelete === widget.id} onclick={() => remove(widget)} onmouseleave={() => (confirmDelete = "")} aria-label={t(`${widget.name} entfernen`, `Remove ${widget.name}`)} title={t("Entfernen", "Remove")}>
            {#if confirmDelete === widget.id}Wirklich?{:else}<svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>{/if}
          </button>
        </span>
      {/each}
      {#if draft.id}
        <button class="mine-new" onclick={fresh}>
          <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M6 2v8M2 6h8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
          {t("Neues Widget", "New widget")}
        </button>
      {/if}
    </div>
  </div>

  {#if mode === "ai"}
    <div class="ai" in:paneIn>
      <div class="ai-left" class:working={busy || (hearing && app.voice !== "wake")}>
        <button class="orb" class:busy onclick={talk} aria-label={hearing ? t("Aufnahme beenden", "Stop recording") : t("Sprechen", "Speak")} title={hearing ? t("Nochmal tippen: Aufnahme beenden", "Tap again to stop recording") : t("Tippen und sagen, was dein Widget zeigen soll", "Tap and say what your widget should show")}>
          <Orb phase={orb} level={app.voiceLevel} size={190} />
        </button>
        <p class="orb-hint" class:live={orb !== "idle"}>{orbHint}</p>
        {#if asked && (busy || showResults)}<p class="asked" in:rise={{ y: 6 }}>„{asked}“</p>{/if}
        {#if !busy && !(hearing && app.voice !== "wake")}
          <div class="write">
            <textarea rows="3" bind:value={description} placeholder={t("… oder schreib es, z. B. „Feierabend-Countdown mit Wetter für morgen“", "… or type it, e.g. \"end-of-day countdown with tomorrow's weather\"")} onkeydown={(e) => e.key === "Enter" && !e.shiftKey && (e.preventDefault(), design())}></textarea>
            <button class="primary" disabled={!description.trim()} onclick={design}>{t("Entwerfen", "Design")}</button>
          </div>
          <div class="ideas">
            {#each IDEAS as idea}
              <button onclick={() => { description = idea; design(); }}>{idea}</button>
            {/each}
          </div>
        {/if}
      </div>
      <div class="ai-right" bind:this={variantsBox}>
        {#if busy}
          {#each [0, 1, 2] as i}
            <div class="skeleton" style:animation-delay="{i * 140}ms">
              <span class="sk-edge"></span><span class="sk-face" style:width="{[60, 44, 72][i]}%"></span>
              <span class="sk-line"></span>
            </div>
          {/each}
        {:else if showResults}
          {#each variants as variant, i (i)}
            <button class="variant" onclick={() => pick(variant)} in:rise={{ delay: 80 + i * 120, y: 28 }}>
              <span class="mini-screen">
                <span class="vpreview" class:floating={variant.style === "floating"}><WidgetFace widget={variant} {feed} /></span>
              </span>
              <span class="vname">{variant.name}</span>
              <span class="vmeta">{SHAPES.find((x) => x.id === variant.shape)?.label} · {variant.style === "hanging" ? t("hängt an der Kante", "hangs from the edge") : t("schwebt frei", "floats freely")} · {variant.blocks.length} {t("Bausteine", "blocks")}</span>
              <span class="vtake">{t("Diesen nehmen", "Use this one")}</span>
            </button>
          {/each}
        {:else}
          <div class="ai-empty">
            <strong>{t("Drei Entwürfe auf einmal", "Three designs at once")}</strong>
            <span>{t(`Beschreib, was du sehen willst. Die KI kennt ${BLOCKS.length} Bausteine: von CPU-Kurve über Feierabend-Countdown bis Sonnenuntergang. Den Entwurf, der dir gefällt, passt du im Baukasten an.`, `Describe what you want to see. The AI knows ${BLOCKS.length} blocks, from a CPU graph to an end-of-day countdown to sunset. Fine-tune the design you like in the builder.`)}</span>
          </div>
        {/if}
      </div>
    </div>
  {:else}
    <div class="builder" in:paneIn>
      <!-- Links: das Widget so, wie es oben am Bildschirm haengen wird. -->
      <div class="left">
        <div class="screen" style:--accent={draft.accent}>
          <span class="edge-label">{t("Bildschirmkante", "Screen edge")}</span>
          <div class="holder" class:bump={bumping} class:launch={launching} bind:this={stageFace}>
            <WidgetFace widget={draft} {feed} />
          </div>
          {#if launching}<span class="ripple" aria-hidden="true"></span>{/if}
          <span class="slots" aria-label={t(`${draft.blocks.length} von ${room} Plätzen belegt`, `${draft.blocks.length} of ${room} slots used`)}>
            {#each Array.from({ length: room }, (_, i) => i) as i}<i class:on={i < draft.blocks.length}></i>{/each}
          </span>
        </div>
        <div class="name-row">
          <input class="name" bind:value={draft.name} maxlength="40" aria-label={t("Name des Widgets", "Widget name")} />
          <button class="primary" disabled={busy} onclick={save}>{busy ? t("Speichere …", "Saving …") : draft.id ? t("Speichern", "Save") : t("Aufhängen", "Hang it up")}</button>
        </div>
        {#if message}<p class="message" in:rise={{ y: 6 }}>{message}</p>{/if}
        <div class="left-scroll">
          <section class="look">
            <h3>{t("Aussehen", "Look")}</h3>
            <div class="shapes">
              {#each SHAPES as shape}
                <button class="shape" class:on={draft.shape === shape.id} onclick={() => setShape(shape.id)}>
                  <svg viewBox="0 0 48 28" aria-hidden="true">
                    {#if shape.id === "round"}<circle cx="24" cy="13" r="11" />{:else if shape.id === "pill"}<rect x="6" y="6" width="36" height="14" rx="7" />{:else}<rect x="8" y="3" width="32" height="22" rx="5" />{/if}
                  </svg>
                  <span>{shape.label}</span>
                  <small>{shape.room} {t("Plätze", "slots")}</small>
                </button>
              {/each}
            </div>
            <div class="pair">
              <div class="seg" use:lens={{ selector: "button.on" }}><span class="lens-thumb pick" aria-hidden="true"></span>
                {#each [["s", t("Klein", "Small")], ["m", t("Mittel", "Medium")], ["l", t("Groß", "Large")]] as const as [id, name]}
                  <button class:on={draft.size === id} onclick={() => (draft.size = id)}>{name}</button>
                {/each}
              </div>
              <div class="seg" use:lens={{ selector: "button.on" }}><span class="lens-thumb pick" aria-hidden="true"></span>
                <button class:on={draft.style === "hanging"} onclick={() => (draft.style = "hanging")} title={t("Hängt an der oberen Kante wie die Insel", "Hangs from the top edge like the island")}>{t("Hängend", "Hanging")}</button>
                <button class:on={draft.style === "floating"} onclick={() => (draft.style = "floating")} title={t("Schwebt mit kleinem Abstand unter der Kante", "Floats just below the edge")}>{t("Schwebend", "Floating")}</button>
              </div>
            </div>
            <div class="colors">
              {#each ACCENTS as color}
                <button class="color" class:on={draft.accent === color} style:background={color} onclick={() => (draft.accent = color)} aria-label="{t('Farbe', 'Color')} {color}"></button>
              {/each}
              <label class="color custom" title={t("Eigene Farbe", "Custom color")} style:background={ACCENTS.includes(draft.accent) ? "conic-gradient(#ff6961, #ffd60a, #30d158, #64d2ff, #bf5af2, #ff6961)" : draft.accent}>
                <input type="color" bind:value={draft.accent} aria-label={t("Eigene Farbe", "Custom color")} />
              </label>
            </div>
          </section>
        </div>
      </div>

      <!-- Rechts: Form, Optik, Farbe, Bausteine. -->
      <div class="right">
        <section>
          <h3>{t("Im Widget", "In the widget")} <small>{t(`${draft.blocks.length} von ${room}`, `${draft.blocks.length} of ${room}`)}</small></h3>
          {#if draft.blocks.length === 0}
            <p class="small-text">{t("Noch leer. Tippe unten auf einen Baustein, er fliegt hinein.", "Still empty. Tap a block below and it flies in.")}</p>
          {/if}
          <div class="blocks">
            {#each draft.blocks as block, i (i)}
              <div class="block" in:rise={{ y: 8 }}>
                <span class="bname">{label(block.kind)}</span>
                {#if block.kind === "worldclock"}
                  <select class="btext" bind:value={block.text}>
                    {#each ZONES as [zone, city]}<option value={zone}>{city}</option>{/each}
                  </select>
                {:else if block.kind === "countdown"}
                  {@const [name, date] = block.text.includes("|") ? block.text.split("|") : ["", block.text]}
                  <input class="btext" value={name} maxlength="30" placeholder={t("Name", "Name")} oninput={(e) => (block.text = `${e.currentTarget.value}|${date ?? ""}`)} />
                  <input class="btext date" type="date" value={date} oninput={(e) => (block.text = `${name}|${e.currentTarget.value}`)} />
                {:else if PARAM[block.kind]}
                  <input class="btext" bind:value={block.text} maxlength="40" placeholder={PARAM[block.kind]?.placeholder} />
                {/if}
                <input class="blabel" bind:value={block.label} maxlength="16" placeholder={t("Beschriftung", "Label")} title={t("Eigene Beschriftung (leer = Standard)", "Custom label (empty = default)")} />
                <button class="tiny" disabled={i === 0} onclick={() => moveBlock(i, -1)} aria-label={t("Nach vorn", "Move earlier")}>
                  <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M3 7.5l3-3 3 3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
                </button>
                <button class="tiny" disabled={i === draft.blocks.length - 1} onclick={() => moveBlock(i, 1)} aria-label={t("Nach hinten", "Move later")}>
                  <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M3 4.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
                </button>
                <button class="tiny" onclick={() => removeBlock(i)} aria-label={t("Entfernen", "Remove")}>
                  <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
                </button>
              </div>
            {/each}
          </div>
        </section>

        <section>
          <h3>{t("Bausteine", "Blocks")} <small>{BLOCKS.length}</small></h3>
          <div class="find">
            <input class="search" bind:value={query} placeholder={t("Suchen, z. B. Wind, Feierabend, Kurve", "Search, e.g. wind, workday, graph")} aria-label={t("Bausteine suchen", "Search blocks")} />
          </div>
          <div class="groups">
            {#each GROUPS as g}
              <button class:on={group === g} onclick={() => (group = g)}>{g}</button>
            {/each}
          </div>
          <div class="catalog">
            {#each catalog as item (item.kind)}
              {@const preview = viewOf({ kind: item.kind, text: PARAM[item.kind]?.initial ?? "" }, feed)}
              <button class="tile" disabled={draft.blocks.length >= room} onclick={(e) => addFrom(e, item.kind)} title={item.hint}>
                <span class="tname">{item.label}</span>
                <span class="tvalue" class:muted={preview.muted}>{preview.value || "–"}</span>
                {#if preview.pct !== null}<span class="tbar"><i style:width="{Math.min(100, Math.max(0, preview.pct))}%"></i></span>{/if}
              </button>
            {:else}
              <p class="small-text">{t("Nichts gefunden. Probier ein anderes Wort.", "Nothing found. Try another word.")}</p>
            {/each}
          </div>
        </section>
        <p class="foot">{t("Widgets verschwinden wie die Insel bei Vollbild und im Spielemodus. Ziehen verschiebt sie entlang der oberen Kante, Rechtsklick öffnet sie hier.", "Like the island, widgets hide in full screen and game mode. Drag to move them along the top edge; right-click opens them here.")}</p>
      </div>
    </div>
  {/if}
</div>

<style>
  .view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 22px;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 36px;
  }
  .segments {
    flex: none;
    display: flex;
    padding: 4px;
    border-radius: var(--r-full);
  }
  .segments button {
    position: relative;
    z-index: 1;
    width: 110px;
    height: 28px;
    border: 0;
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    cursor: pointer;
    transition: color var(--t-base) ease;
  }
  .segments button:last-of-type {
    width: 130px;
  }
  .segments button.on {
    color: #000;
  }
  .mine {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .mine-item {
    flex: none;
    display: flex;
    align-items: center;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .mine-item.current {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .mine-chip,
  .mine-x,
  .mine-new {
    border: 0;
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .mine-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 4px 0 10px;
  }
  .mine-chip small {
    font-size: var(--fs-caption);
    color: var(--warn);
  }
  .mine-x {
    height: 28px;
    padding: 0 8px 0 6px;
    color: var(--text-3);
  }
  .mine-x.danger {
    color: #ff6961;
    font-weight: var(--fw-semibold);
  }
  .mine-new {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.08);
  }
  .swatch {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }

  /* ---------- Baukasten ---------- */
  .builder {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 340px 1fr;
    gap: 22px;
    padding-bottom: 6px;
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .left-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
    padding: 2px 2px 8px;
  }
  .look {
    gap: 10px;
  }
  /* Ein Stueck Bildschirm: oben die Kante, an der das Widget haengt, dahinter ein ruhiger Hintergrund. */
  .screen {
    position: relative;
    height: 196px;
    border-radius: var(--r-lg);
    overflow: hidden;
    display: flex;
    justify-content: center;
    background:
      radial-gradient(80% 90% at 20% 100%, color-mix(in srgb, var(--accent) 35%, transparent), transparent 70%),
      radial-gradient(70% 80% at 100% 0%, rgba(94, 92, 230, 0.35), transparent 70%),
      linear-gradient(160deg, #1d2333, #101218);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08), inset 0 0 0 1px rgba(255, 255, 255, 0.05);
    transition: background var(--t-spring) ease;
  }
  .screen::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 2px;
    background: rgba(0, 0, 0, 0.9);
  }
  .edge-label {
    position: absolute;
    left: 12px;
    bottom: 10px;
    font-size: var(--fs-caption);
    color: rgba(235, 235, 245, 0.4);
  }
  .holder {
    position: relative;
    z-index: 1;
    transform-origin: top center;
    transition: transform var(--t-spring) var(--ease-spring);
  }
  .holder :global(.face) {
    transition: width var(--t-spring) var(--ease-spring), height var(--t-spring) var(--ease-spring), border-radius var(--t-slow) var(--ease-spring), margin var(--t-slow) var(--ease-spring);
  }
  .holder.bump {
    animation: bump var(--t-spring) var(--ease-spring);
  }
  @keyframes bump {
    0% { transform: scale(1); }
    35% { transform: scale(1.07); filter: drop-shadow(0 0 14px var(--accent)); }
    100% { transform: scale(1); }
  }
  .holder.launch {
    animation: launch 1100ms var(--ease-out);
  }
  @keyframes launch {
    0% { transform: translateY(0) scale(1); }
    30% { transform: translateY(14px) scale(0.96); }
    55% { transform: translateY(-6px) scale(1.04); filter: drop-shadow(0 0 22px var(--accent)); }
    100% { transform: none; filter: none; }
  }
  .ripple {
    position: absolute;
    top: 0;
    left: 50%;
    width: 30px;
    height: 30px;
    margin-left: -16px;
    margin-top: -16px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    animation: ripple 1000ms var(--ease-out) forwards;
    pointer-events: none;
  }
  @keyframes ripple {
    from { transform: scale(0.5); opacity: 0.9; }
    to { transform: scale(14); opacity: 0; }
  }
  .slots {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    gap: 4px;
  }
  .slots i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.18);
    transition: background var(--t-slow) ease, transform var(--t-slow) var(--ease-spring);
  }
  .slots i.on {
    background: var(--accent);
    transform: scale(1.15);
  }
  .name-row {
    display: flex;
    gap: 8px;
  }
  .name {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 12px;
    border-radius: var(--r-md);
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .name:focus,
  .search:focus,
  textarea:focus {
    outline: 2px solid var(--focus);
    outline-offset: -1px;
  }
  .right {
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 2px 6px 10px 0;
    scrollbar-width: thin;
    scrollbar-color: #2c2c2e transparent;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  h3 small {
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    color: var(--text-3);
  }
  .shapes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .shape {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 8px 6px 8px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
    color: var(--text-2);
    font: inherit;
    text-align: center;
    cursor: pointer;
    transition: box-shadow var(--t-base) ease, background var(--t-base) ease, transform var(--t-slow) var(--ease-spring);
  }
  .shape svg {
    width: 44px;
    height: 26px;
  }
  .shape svg :global(*) {
    fill: rgba(255, 255, 255, 0.18);
    transition: fill var(--t-base) ease;
  }
  .shape span {
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
  }
  .shape small {
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .shape:active {
    transform: scale(0.96);
  }
  .shape.on {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .shape.on svg :global(*) {
    fill: var(--accent);
  }
  .pair {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .seg {
    display: flex;
    padding: 2px;
    border-radius: var(--r-full);
    background: var(--well);
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.08), inset 0 1px 2px rgba(0, 0, 0, 0.35);
  }
  .seg button {
    position: relative;
    z-index: 1;
    height: 26px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-full);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
    transition: color var(--t-base) ease;
  }
  .seg button.on {
    color: #000;
    font-weight: var(--fw-semibold);
  }
  .colors {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 2px 0 0 2px;
  }
  .color {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.15);
    transition: transform var(--t-slow) var(--ease-spring), box-shadow var(--t-base) ease;
  }
  .color.on {
    transform: scale(1.12);
    box-shadow: 0 0 0 2px #000, 0 0 0 4px var(--text);
  }
  .color.custom {
    position: relative;
    overflow: hidden;
  }
  .color.custom input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .blocks {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .block {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 6px 6px 6px 12px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .bname {
    flex: 0 0 112px;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .btext,
  .blabel {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 8px;
    border-radius: var(--r-sm);
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(0, 0, 0, 0.35);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    color-scheme: dark;
  }
  .btext.date {
    flex: 1.4;
  }
  .blabel {
    flex: 0 1 120px;
  }
  .tiny {
    flex: none;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: var(--r-sm);
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-2);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .tiny:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.16);
    color: #fff;
  }
  .find {
    display: flex;
  }
  .search {
    flex: 1;
    height: 32px;
    padding: 0 12px;
    border-radius: var(--r-md);
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
  }
  .groups {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .groups button {
    height: 24px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
    transition: background var(--t-base) ease, color var(--t-base) ease;
  }
  .groups button.on {
    background: var(--accent);
    color: #000;
    font-weight: var(--fw-semibold);
  }
  .catalog {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 8px;
  }
  /* Jede Kachel zeigt den echten, aktuellen Wert des Bausteins. */
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 54px;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background var(--t-fast) ease, transform var(--t-slow) var(--ease-spring), box-shadow var(--t-fast) ease;
  }
  .tile:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.09);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14);
    transform: translateY(-2px);
  }
  .tile:active:not(:disabled) {
    transform: scale(0.95);
  }
  .tile:focus-visible,
  .shape:focus-visible,
  .color:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .tname {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .tvalue {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-variant-numeric: tabular-nums;
  }
  .tvalue.muted {
    color: var(--text-4);
  }
  .tbar {
    height: 3px;
    margin-top: 4px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.1);
    overflow: hidden;
  }
  .tbar i {
    display: block;
    height: 100%;
    background: var(--accent);
  }

  /* ---------- KI ---------- */
  .ai {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 340px 1fr;
    gap: 22px;
  }
  .ai-left {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .ai-left.working {
    justify-content: center;
  }
  .orb {
    border: 0;
    padding: 0;
    background: none;
    cursor: pointer;
    border-radius: 50%;
    margin: -6px 0 0;
    transition: transform var(--t-spring) var(--ease-spring);
  }
  .ai-left:not(.working) .orb:hover {
    transform: scale(1.04);
  }
  .ai-left.working .orb {
    transform: scale(1.1);
  }
  .orb.busy {
    cursor: progress;
  }
  .orb:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 4px;
  }
  .orb-hint {
    margin: 0;
    font-size: var(--fs-footnote);
    color: var(--text-3);
    text-align: center;
    transition: color var(--t-base) ease;
  }
  .orb-hint.live {
    color: #9fd8ff;
  }
  .asked {
    margin: 0;
    text-align: center;
    font-size: var(--fs-body);
    color: var(--text);
  }
  .write {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-end;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    min-width: 0;
    resize: none;
    line-height: 1.45;
    scrollbar-width: none;
    padding: 8px 12px;
    border-radius: var(--r-md);
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
  }
  .ideas {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: center;
  }
  .ideas button {
    height: 24px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    cursor: pointer;
  }
  .ideas button:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }
  .ai-right {
    min-height: 0;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    align-content: start;
    gap: 10px;
    padding-bottom: 8px;
    scrollbar-width: none;
  }
  .ai-empty {
    grid-column: 1 / -1;
    margin-top: 30px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 380px;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-3);
  }
  .ai-empty strong {
    font-size: var(--fs-headline);
    color: var(--text);
  }
  /* Platzhalter, waehrend die KI entwirft: Bildschirmkante mit schimmerndem Widget. */
  .skeleton {
    position: relative;
    height: 200px;
    border-radius: var(--r-lg);
    overflow: hidden;
    background: linear-gradient(160deg, #1a1f2b, #0e1015);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05);
    animation: breathe 1.6s ease-in-out infinite;
  }
  .sk-face {
    position: absolute;
    top: 0;
    left: 50%;
    height: 54px;
    transform: translateX(-50%);
    border-radius: 0 0 22px 22px;
    background: linear-gradient(100deg, #161618 30%, #2a2a2e 50%, #161618 70%);
    background-size: 300% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  .sk-line {
    position: absolute;
    left: 16px;
    right: 40%;
    bottom: 34px;
    height: 10px;
    border-radius: var(--r-xs);
    background: rgba(255, 255, 255, 0.07);
  }
  @keyframes shimmer {
    from { background-position: 100% 0; }
    to { background-position: -50% 0; }
  }
  @keyframes breathe {
    0%, 100% { opacity: 0.7; }
    50% { opacity: 1; }
  }
  .variant {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0 0 10px;
    border: 0;
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    transition: box-shadow var(--t-base) ease, transform var(--t-slow) var(--ease-spring);
  }
  .variant:hover {
    box-shadow: inset 0 0 0 1.5px var(--accent);
    transform: translateY(-3px);
  }
  .mini-screen {
    display: flex;
    justify-content: center;
    height: 130px;
    overflow: hidden;
    background:
      radial-gradient(80% 90% at 20% 100%, rgba(100, 210, 255, 0.18), transparent 70%),
      linear-gradient(160deg, #1d2333, #101218);
  }
  .vpreview {
    display: flex;
    justify-content: center;
    transform: scale(0.8);
    transform-origin: top center;
  }
  .vname {
    padding: 4px 12px 0;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .vmeta {
    padding: 0 12px;
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .vtake {
    padding: 2px 12px 0;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--accent);
  }

  .primary {
    height: 34px;
    padding: 0 18px;
    border: 0;
    border-radius: var(--r-lg);
    background: var(--accent);
    color: #000;
    font: inherit;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    cursor: pointer;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.45), 0 2px 6px rgba(0, 0, 0, 0.35);
    transition: transform var(--t-slow) var(--ease-spring), filter var(--t-fast) ease;
  }
  .primary:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .primary:active:not(:disabled) {
    transform: scale(0.95);
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .small-text,
  .message {
    margin: 0;
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-3);
  }
  .message {
    color: var(--text-2);
  }
  .foot {
    margin: 0;
    font-size: var(--fs-caption);
    line-height: 1.45;
    color: var(--text-4);
  }
</style>
