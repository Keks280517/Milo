<script lang="ts" module>
  import { t, locale } from "../i18n";
  /** Miniaturen bleiben fuer die Sitzung im Speicher (Pfad + Zeitstempel). */
  const thumbs = new Map<string, Promise<string>>();
</script>

<script lang="ts">
  // Eine Datei oder ein Ordner auf dem PC (Downloads, Ordner-Ansicht). Laesst sich in andere
  // Programme oder auf einen Ordner-Knopf in Milo ziehen, in der Vorschau ansehen und oeffnen.
  import { startDrag } from "@crabnebula/tauri-plugin-drag";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { invoke } from "@tauri-apps/api/core";
  import { app, fileSize, inTauri, type DiskEntry, type Place } from "../app.svelte";

  interface Props {
    entry: DiskEntry;
    places: Place[];
    onopen: (entry: DiskEntry) => void;
    onpreview: (entry: DiskEntry) => void;
  }
  let { entry, places, onopen, onpreview }: Props = $props();

  const COLORS: Record<string, string> = {
    text: "#64d2ff", pdf: "#ff6961", office: "#5e9eff", image: "#bf5af2", video: "#ff9f0a", audio: "#30d158", archive: "#ffd60a", program: "#8e8e93", other: "#8e8e93",
  };
  const ext = $derived(entry.dir ? "" : (entry.name.split(".").pop() ?? "").slice(0, 4).toUpperCase());
  /** Ordner, in dem der Eintrag liegt (klein geschrieben, zum Vergleichen). */
  const parent = $derived(entry.path.toLowerCase().replace(/[\\/][^\\/]*$/, ""));
  const visual = $derived(["image", "pdf", "video"].includes(entry.kind) && !entry.partial);
  let thumb = $state("");
  let row: HTMLElement;
  let menu = $state(false);
  let error = $state("");
  let pressed: { x: number; y: number } | null = null;

  // Miniatur erst holen, wenn die Zeile sichtbar wird.
  $effect(() => {
    if (!visual || !inTauri || !row) return;
    const key = `${entry.path}|${entry.modifiedMs}`;
    const observer = new IntersectionObserver((seen) => {
      if (!seen.some((s) => s.isIntersecting)) return;
      observer.disconnect();
      if (!thumbs.has(key)) thumbs.set(key, invoke<string>("browse_thumb", { path: entry.path, size: 72 }).catch(() => ""));
      thumbs.get(key)!.then((url) => (thumb = url));
    });
    observer.observe(row);
    return () => observer.disconnect();
  });

  function age(ms: number): string {
    const min = Math.round((Date.now() - ms) / 60000);
    if (min < 1) return t("gerade eben", "just now");
    if (min < 60) return t(`vor ${min} Min`, `${min} min ago`);
    if (min < 24 * 60) return t(`vor ${Math.round(min / 60)} Std`, `${Math.round(min / 60)} h ago`);
    return new Date(ms).toLocaleDateString(locale, { day: "numeric", month: "short", year: new Date(ms).getFullYear() === new Date().getFullYear() ? undefined : "numeric" });
  }

  function dragIcon(): string {
    const canvas = document.createElement("canvas");
    canvas.width = 48;
    canvas.height = 56;
    const g = canvas.getContext("2d")!;
    g.fillStyle = entry.dir ? "#64d2ff" : "#f5f5f7";
    g.beginPath();
    g.roundRect(4, 2, 40, 52, 8);
    g.fill();
    if (!entry.dir) {
      g.fillStyle = COLORS[entry.kind] ?? "#8e8e93";
      g.beginPath();
      g.roundRect(9, 30, 30, 15, 4);
      g.fill();
      g.fillStyle = "#fff";
      g.font = "bold 10px Segoe UI";
      g.textAlign = "center";
      g.fillText(ext || "DATEI", 24, 41);
    }
    return canvas.toDataURL("image/png");
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button === 0 && !(e.target as HTMLElement).closest("button")) pressed = { x: e.clientX, y: e.clientY };
  }
  async function onPointerMove(e: PointerEvent) {
    if (!pressed || !inTauri || entry.partial || Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 6) return;
    pressed = null;
    app.handing = true;
    app.handingPaths = [entry.path];
    try {
      // Nach draussen: kopieren. Auf einen Ordner-Knopf in Milo: verschieben (siehe Insel).
      await startDrag({ item: [entry.path], icon: dragIcon(), mode: "copy" }, () => {
        app.handing = false;
        app.handingPaths = [];
      });
    } catch (err) {
      app.handing = false;
      app.handingPaths = [];
      error = t("Ziehen hat nicht geklappt: ", "Dragging didn't work: ") + String(err);
    }
  }

  async function send(target: Place, copy: boolean) {
    menu = false;
    error = "";
    try {
      await app.transferFiles([entry.path], target.path, copy);
    } catch (err) {
      error = String(err);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="entry"
  class:partial={entry.partial}
  bind:this={row}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={() => (pressed = null)}
  onpointerleave={() => { pressed = null; menu = false; }}
  onclick={(e) => { if (!(e.target as HTMLElement).closest("button")) (entry.dir ? onopen(entry) : onpreview(entry)); }}
  ondblclick={() => !entry.dir && onopen(entry)}
  title={entry.dir ? t("Öffnen · auf einen Ordner-Knopf ziehen zum Verschieben", "Open · drag onto a folder button to move") : t("Klick: Vorschau · Doppelklick: öffnen · Ziehen: in ein Programm oder auf einen Ordner-Knopf", "Click: preview · Double-click: open · Drag: into an app or onto a folder button")}
>
  {#if entry.dir}
    <span class="folder"></span>
  {:else if thumb}
    <img class="thumb" src={thumb} alt="" />
  {:else}
    <span class="doc" style:--tint={COLORS[entry.kind] ?? "#8e8e93"}>{ext}</span>
  {/if}
  <span class="lines">
    <span class="name">{entry.name}</span>
    <span class="meta">
      {#if entry.partial}<span class="loading">{t("wird heruntergeladen …", "downloading …")}</span>{:else}{entry.dir ? t("Ordner", "Folder") : fileSize(entry.size)} · {age(entry.modifiedMs)}{/if}
    </span>
    {#if error}<span class="meta error">{error}</span>{/if}
  </span>
  {#if !entry.partial}
    <span class="actions">
      <button onclick={() => (menu = !menu)} title={t("Verschieben oder kopieren", "Move or copy")}>{t("Nach …", "To …")}</button>
      <button onclick={() => revealItemInDir(entry.path)} title={t("Im Explorer zeigen", "Show in Explorer")}>{t("Zeigen", "Show")}</button>
    </span>
  {/if}
  {#if menu}
    <div class="menu">
      {#each places.filter((p) => p.path.toLowerCase() !== parent) as place}
        <div class="target">
          <span class="pname">{place.name}</span>
          <button onclick={() => send(place, false)}>{t("Verschieben", "Move")}</button>
          <button onclick={() => send(place, true)}>{t("Kopieren", "Copy")}</button>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .entry {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 8px;
    border-radius: var(--r-md);
    cursor: default;
    transition: background var(--t-fast) ease;
    user-select: none;
  }
  .entry:hover {
    background: var(--card);
    box-shadow: var(--card-edge);
  }
  .entry.partial {
    opacity: 0.7;
  }
  .doc,
  .thumb,
  .folder {
    flex: none;
    width: 30px;
    height: 36px;
    border-radius: var(--r-sm);
  }
  .doc {
    background: var(--text);
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding-bottom: 6px;
    box-sizing: border-box;
    font-size: 8px;
    font-weight: var(--fw-bold);
    letter-spacing: 0.02em;
    color: var(--tint);
  }
  .thumb {
    object-fit: cover;
    background: var(--chip);
  }
  .folder {
    height: 26px;
    margin: 6px 0;
    border-radius: 3px 7px 7px 7px;
    background: linear-gradient(var(--accent), #3aa8d8);
    position: relative;
  }
  .folder::before {
    content: "";
    position: absolute;
    top: -4px;
    left: 0;
    width: 13px;
    height: 6px;
    border-radius: 3px 3px 0 0;
    background: var(--accent);
  }
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    font-size: var(--fs-body);
    font-weight: var(--fw-medium);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font-size: var(--fs-footnote);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta.error {
    color: #ff8a80;
    white-space: normal;
  }
  .loading {
    color: var(--accent);
  }
  .actions {
    display: flex;
    gap: 6px;
    opacity: 0;
    transition: opacity var(--t-fast) ease;
  }
  .entry:hover .actions {
    opacity: 1;
  }
  button {
    flex: none;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--chip);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
  }
  button:hover {
    background: #2e2e32;
  }
  .menu {
    position: absolute;
    z-index: 5;
    top: calc(100% - 4px);
    right: 8px;
    min-width: 260px;
    max-height: 220px;
    overflow-y: auto;
    padding: 6px;
    border-radius: var(--r-md);
    background: var(--chip);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.6), inset 0 0 0 1px #2c2c2e;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .target {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px 4px 8px;
    border-radius: var(--r-sm);
  }
  .target:hover {
    background: #26262a;
  }
  .pname {
    flex: 1;
    font-size: var(--fs-footnote);
    color: var(--text);
  }
  .target button {
    height: 24px;
    background: var(--chip-strong);
  }
</style>
