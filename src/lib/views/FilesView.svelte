<script lang="ts">
  import { lens } from "$lib/lens";
  import { t } from "../i18n";
  // Dateien-Ansicht mit drei Bereichen: Milos eigene Ablage (alles, was Milo bekommen hat),
  // die Downloads des PCs und eine Ordner-Ansicht mit Favoriten. Dateien lassen sich auf die
  // Ordner-Knoepfe oben ziehen (verschieben) oder ueber „Nach …“ verschieben und kopieren.
  import EmptyState from "../EmptyState.svelte";
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { app, fileSize, inTauri, type DiskEntry, type Place } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoEntries, demoPlaces } from "../demoData";
  import FileRow from "../cards/FileRow.svelte";
  import DiskRow from "../cards/DiskRow.svelte";

  type Section = "milo" | "downloads" | "folders";
  let section = $state<Section>("downloads");
  let places = $state<Place[]>([]);
  let folder = $state("");
  let entries = $state<DiskEntry[]>([]);
  let loading = $state(false);
  let error = $state("");
  let preview = $state<{ entry: DiskEntry; image: string; text: string; failed: boolean } | null>(null);

  const downloads = $derived(places.find((p) => p.builtin && p.name === "Downloads")?.path ?? "");
  const current = $derived(section === "downloads" ? downloads : section === "folders" ? folder : "");

  async function loadPlaces() {
    if (tour.demo === "files") {
      places = demoPlaces();
      return;
    }
    if (!inTauri) return;
    places = await invoke<Place[]>("browse_places");
    if (!folder) folder = places.find((p) => p.name === "Dokumente" || p.name === "Documents")?.path ?? places[0]?.path ?? "";
  }

  async function load() {
    if (tour.demo === "files") {
      entries = demoEntries();
      return;
    }
    if (!inTauri || !current) return;
    loading = entries.length === 0;
    error = "";
    try {
      entries = await invoke<DiskEntry[]>("browse_list", { path: current, newestFirst: section === "downloads" });
    } catch (err) {
      error = String(err);
      entries = [];
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadPlaces();
    let stop: (() => void) | undefined;
    if (inTauri) listen<string>("milo://browse", ({ payload }) => payload.toLowerCase() === current.toLowerCase() && load()).then((fn) => (stop = fn));
    return () => {
      stop?.();
      if (inTauri) invoke("browse_watch", { path: null });
    };
  });

  // Neu laden bei Wechsel des Ordners oder nach einem Verschieben; den Ordner beobachten, solange er offen ist.
  $effect(() => {
    void app.filesChanged;
    const path = current;
    if (!path) return;
    untrack(() => {
      entries = [];
      preview = null;
      load();
    });
    if (!inTauri || tour.demo === "files") return;
    invoke("browse_watch", { path });
    const ping = window.setInterval(() => invoke("browse_watch", { path }), 4000);
    return () => clearInterval(ping);
  });

  function openEntry(entry: DiskEntry) {
    if (entry.dir) {
      section = "folders";
      folder = entry.path;
    } else {
      invoke("browse_open", { path: entry.path }).catch((err) => (error = String(err)));
    }
  }

  async function showPreview(entry: DiskEntry) {
    preview = { entry, image: "", text: "", failed: false };
    try {
      if (entry.kind === "text") {
        const text = await invoke<string>("browse_text", { path: entry.path });
        if (preview?.entry === entry) preview.text = text;
      } else {
        const image = await invoke<string>("browse_thumb", { path: entry.path, size: 512 });
        if (preview?.entry === entry) preview.image = image;
      }
    } catch {
      if (preview?.entry === entry) preview.failed = true;
    }
  }

  /** Brotkrumen der Ordner-Ansicht: ab dem naechsten bekannten Ordner. */
  const crumbs = $derived.by(() => {
    if (section !== "folders" || !folder) return [];
    const base = places.filter((p) => folder.toLowerCase().startsWith(p.path.toLowerCase())).sort((a, b) => b.path.length - a.path.length)[0];
    const start = base ? base.path : folder.split("\\")[0];
    const rest = folder.slice(start.length).split("\\").filter(Boolean);
    const out = [{ name: base?.name ?? start, path: start }];
    let path = start;
    for (const part of rest) {
      path = `${path}\\${part}`;
      out.push({ name: part, path });
    }
    return out;
  });

  async function addFavorite() {
    places = await invoke<Place[]>("browse_add_place", { path: folder });
  }

  async function removeFavorite(place: Place) {
    places = await invoke<Place[]>("browse_remove_place", { path: place.path });
  }

  const isFavorite = $derived(places.some((p) => p.path.toLowerCase() === folder.toLowerCase()));
  const partials = $derived(entries.filter((e) => e.partial).length);
</script>

<div class="view">
  <div class="top">
    <div class="segments" use:lens={{ selector: "button.on" }}><span class="lens-thumb pick" aria-hidden="true"></span>
      <button class:on={section === "milo"} onclick={() => (section = "milo")}>Milo</button>
      <button class:on={section === "downloads"} onclick={() => (section = "downloads")}>Downloads{#if partials > 0 && section === "downloads"}<span class="dot"></span>{/if}</button>
      <button class:on={section === "folders"} onclick={() => (section = "folders")}>{t("Ordner", "Folders")}</button>
    </div>
    {#if current}<button class="ghost" onclick={() => revealItemInDir(current)} title={t("Im Explorer öffnen", "Open in Explorer")}>Explorer</button>{/if}
  </div>

  {#if section !== "milo" && places.length > 0}
    <div class="places" aria-label={t("Ordner zum Hinspringen und Hineinziehen", "Folders to jump to and drop into")}>
      {#each places as place (place.path)}
        <button
          class="place"
          class:here={place.path.toLowerCase() === current.toLowerCase()}
          class:target={app.dropFolder === place.path}
          data-drop-folder={place.path}
          onclick={() => { section = place.name === "Downloads" && place.builtin ? "downloads" : "folders"; if (section === "folders") folder = place.path; }}
          oncontextmenu={(e) => { if (!place.builtin) { e.preventDefault(); removeFavorite(place); } }}
          title={place.builtin ? t(`${place.path} · Dateien hierher ziehen zum Verschieben`, `${place.path} · Drag files here to move them`) : t(`${place.path} · Rechtsklick: aus den Favoriten nehmen`, `${place.path} · Right-click: remove from favorites`)}
        >{place.name}</button>
      {/each}
    </div>
  {/if}

  {#if section === "folders" && crumbs.length > 0}
    <div class="crumbs">
      {#each crumbs as crumb, i (crumb.path)}
        {#if i > 0}<span class="sep">›</span>{/if}
        <button class="crumb" data-drop-folder={crumb.path} class:target={app.dropFolder === crumb.path} onclick={() => (folder = crumb.path)}>{crumb.name}</button>
      {/each}
      {#if !isFavorite}<button class="ghost small" onclick={addFavorite} title={t("Als Ordner-Knopf oben anheften", "Pin as a folder button at the top")}>＋ {t("Favorit", "Favorite")}</button>{/if}
    </div>
  {/if}

  <div class="list">
    {#if section === "milo"}
      {#each app.files as file (file.id)}
        <FileRow {file} manage />
      {:else}
        <EmptyState kind="files" title={t("Noch nichts abgelegt", "Nothing on the shelf yet")} text={t("Zieh eine Datei auf die Insel, dann bewahre ich eine Kopie auf und kann Fragen dazu beantworten.", "Drag a file onto the island and I'll keep a copy and can answer questions about it.")} />
      {/each}
      {#if app.files.length > 0}
        <p class="foot">{t("Das sind Milos Kopien. Deine Originale bleiben, wo sie sind.", "These are Milo's copies. Your originals stay where they are.")}</p>
      {/if}
    {:else if error}
      <p class="hint">{error}</p>
    {:else if loading}
      <p class="hint">{t("Lade …", "Loading …")}</p>
    {:else}
      {#each entries as entry (entry.path)}
        <DiskRow {entry} {places} onopen={openEntry} onpreview={showPreview} />
      {:else}
        <EmptyState kind="downloads" title={section === "downloads" ? t("Keine neuen Downloads", "No new downloads") : t("Dieser Ordner ist leer", "This folder is empty")} text={section === "downloads" ? t("Was du herunterlädst, erscheint hier, und du kannst es direkt in die Insel ziehen.", "What you download shows up here, and you can drag it straight into the island.") : ""} />
      {/each}
    {/if}
  </div>

  {#if preview}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="preview" onclick={(e) => e.target === e.currentTarget && (preview = null)}>
      <div class="sheet">
        <div class="phead">
          <span class="pname" title={preview.entry.path}>{preview.entry.name}</span>
          <span class="psize">{fileSize(preview.entry.size)}</span>
          <button class="ghost" onclick={() => preview && openEntry(preview.entry)}>{t("Öffnen", "Open")}</button>
          <button class="ghost" onclick={() => preview && revealItemInDir(preview.entry.path)}>{t("Zeigen", "Show")}</button>
          <button class="close" onclick={() => (preview = null)} aria-label={t("Vorschau schließen", "Close preview")}>✕</button>
        </div>
        <div class="pbody">
          {#if preview.image}
            <img src={preview.image} alt={preview.entry.name} />
          {:else if preview.text}
            <pre>{preview.text}</pre>
          {:else if preview.failed}
            <p class="hint">{t("Für diese Datei gibt es keine Vorschau. Doppelklick öffnet sie.", "There's no preview for this file. Double-click to open it.")}</p>
          {:else}
            <p class="hint">{t("Lade Vorschau …", "Loading preview …")}</p>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<svelte:window onkeydown={(e) => e.key === "Escape" && preview && (preview = null)} />

<style>
  .view {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 10px 0;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 2px;
  }
  .segments {
    display: flex;
    padding: 2px;
    border-radius: var(--r-full);
    background: var(--well);
    box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.08), inset 0 1px 2px rgba(0, 0, 0, 0.35);
  }
  .segments button {
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
  .segments button.on {
    color: #000;
    font-weight: var(--fw-semibold);
  }
  .dot {
    position: absolute;
    top: 5px;
    right: 4px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .ghost {
    height: 26px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--chip);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
  }
  .ghost:hover {
    background: var(--chip-strong);
    color: var(--text);
  }
  .ghost.small {
    height: 22px;
    margin-left: auto;
    font-size: var(--fs-caption);
  }
  .places {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    scrollbar-width: none;
    padding: 0 2px;
  }
  .place {
    flex: none;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--card);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
    box-shadow: inset 0 0 0 1px #1f1f22;
    transition: background var(--t-fast) ease, box-shadow var(--t-fast) ease, transform var(--t-fast) ease;
  }
  .place:hover {
    background: var(--chip);
  }
  .place.here {
    color: var(--accent);
    box-shadow: inset 0 0 0 1px #1d4a5c;
  }
  .place.target,
  .crumb.target {
    background: #0f3242;
    color: var(--text);
    box-shadow: inset 0 0 0 1.5px var(--accent);
    transform: scale(1.06);
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    padding: 0 4px;
    font-size: var(--fs-footnote);
    color: var(--text-4);
  }
  .crumb {
    border: 0;
    padding: 2px 6px;
    border-radius: var(--r-xs);
    background: none;
    color: var(--text-2);
    font: inherit;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 160px;
  }
  .crumb:hover {
    background: var(--chip);
  }
  .crumb:last-of-type {
    color: var(--text);
    font-weight: var(--fw-semibold);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding-bottom: 12px;
  }
  .list::-webkit-scrollbar {
    width: 10px;
  }
  .list::-webkit-scrollbar-thumb {
    border: 3px solid transparent;
    border-radius: var(--r-xs);
    background: #2c2c2e content-box;
  }
  .hint {
    margin: auto;
    max-width: 380px;
    text-align: center;
    font-size: var(--fs-body);
    line-height: 1.5;
    color: var(--text-3);
  }
  .foot {
    margin: 8px 8px 0;
    font-size: var(--fs-footnote);
    color: var(--text-4);
  }
  .preview {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 10px;
    background: rgba(0, 0, 0, 0.55);
    animation: fade var(--t-fast) ease-out;
  }
  .sheet {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-lg);
    background: var(--card);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6), inset 0 0 0 1px #2c2c2e;
    overflow: hidden;
  }
  .phead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .pname {
    flex: 1;
    min-width: 0;
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .psize {
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .close {
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--chip-strong);
    color: var(--text);
    cursor: pointer;
  }
  .pbody {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 10px;
    overflow: auto;
  }
  .pbody img {
    max-width: 100%;
    max-height: 100%;
    border-radius: var(--r-sm);
    object-fit: contain;
  }
  .pbody pre {
    align-self: stretch;
    width: 100%;
    margin: 0;
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-word;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }
  @keyframes fade {
    from { opacity: 0; }
  }
</style>
