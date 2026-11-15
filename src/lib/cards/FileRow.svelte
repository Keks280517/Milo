<script lang="ts">
  import { t } from "../i18n";
  // Eine Datei aus Milos Ablage. Sie laesst sich mit der Maus in andere Programme ziehen
  // (Explorer, Browser, Mail, Discord ...), im Ordner zeigen und in der Ablage loeschen.
  import { startDrag } from "@crabnebula/tauri-plugin-drag";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { app, fileSize, inTauri, niceDate, type StoredFile } from "../app.svelte";

  interface Props {
    file: StoredFile;
    /** Mit Knoepfen zum Zeigen und Loeschen (Dateien-Ansicht). */
    manage?: boolean;
  }
  let { file, manage = false }: Props = $props();

  const COLORS: Record<StoredFile["kind"], string> = { text: "#64d2ff", pdf: "#ff6961", office: "#5e9eff", image: "#bf5af2", other: "#8e8e93" };
  const ext = $derived((file.name.split(".").pop() ?? "").slice(0, 4).toUpperCase());
  let removing = $state(false);
  let error = $state("");
  let pressed: { x: number; y: number } | null = null;

  /** Kleines Dateisymbol als Bild, das beim Ziehen am Mauszeiger haengt. */
  function dragIcon(): string {
    const canvas = document.createElement("canvas");
    canvas.width = 48;
    canvas.height = 56;
    const g = canvas.getContext("2d")!;
    g.fillStyle = "#f5f5f7";
    g.beginPath();
    g.roundRect(4, 2, 40, 52, 8);
    g.fill();
    g.fillStyle = COLORS[file.kind];
    g.beginPath();
    g.roundRect(9, 30, 30, 15, 4);
    g.fill();
    g.fillStyle = "#fff";
    g.font = "bold 10px Segoe UI";
    g.textAlign = "center";
    g.fillText(ext || "DATEI", 24, 41);
    return canvas.toDataURL("image/png");
  }

  // Gezogen wird erst nach ein paar Pixeln Bewegung, damit ein einfacher Klick nichts ausloest.
  function onPointerDown(e: PointerEvent) {
    if (e.button === 0 && !(e.target as HTMLElement).closest("button")) pressed = { x: e.clientX, y: e.clientY };
  }
  async function onPointerMove(e: PointerEvent) {
    if (!pressed || !inTauri || Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 6) return;
    pressed = null;
    app.handing = true;
    try {
      await startDrag({ item: [file.path], icon: dragIcon(), mode: "copy" }, () => (app.handing = false));
    } catch (err) {
      app.handing = false;
      error = t("Ziehen hat nicht geklappt: ", "Dragging didn't work: ") + String(err);
    }
  }

  async function remove() {
    if (!removing) {
      // Erst beim zweiten Klick wird geloescht.
      removing = true;
      return;
    }
    try {
      await app.deleteFile(file);
    } catch (err) {
      error = String(err);
      removing = false;
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="file"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={() => (pressed = null)}
  onpointerleave={() => (pressed = null)}
  title={t("Zum Weitergeben in ein anderes Programm ziehen", "Drag into another app to hand it over")}
>
  <span class="doc" style:--tint={COLORS[file.kind]}>{ext}</span>
  <span class="lines">
    <span class="name">{file.name}</span>
    <span class="meta">
      {fileSize(file.size)} · {niceDate(file.addedAt)}{file.chars === 0 ? ` · ${t("ohne lesbaren Text", "no readable text")}` : ""}
    </span>
    {#if error}<span class="meta error">{error}</span>{/if}
  </span>
  {#if manage}
    <button onclick={() => revealItemInDir(file.path)} title={t("Im Ordner zeigen", "Show in folder")}>{t("Zeigen", "Show")}</button>
    <button class:danger={removing} onclick={remove} onmouseleave={() => (removing = false)} title={t("Milos Kopie löschen; dein Original bleibt", "Delete Milo's copy; your original stays")}>
      {removing ? t("Wirklich?", "Sure?") : t("Löschen", "Delete")}
    </button>
  {/if}
</div>

<style>
  .file {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 8px;
    border-radius: var(--r-md);
    cursor: grab;
    transition: background var(--t-fast) ease;
    user-select: none;
  }
  .file:hover {
    background: var(--card);
    box-shadow: var(--card-edge);
  }
  .file:active {
    cursor: grabbing;
  }
  .doc {
    flex: none;
    width: 30px;
    height: 36px;
    border-radius: var(--r-sm);
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
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    font-size: var(--fs-callout);
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
  button {
    flex: none;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--chip);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
    transition: background var(--t-fast) ease, color var(--t-fast) ease;
  }
  button:hover {
    background: #2e2e32;
  }
  button.danger {
    background: var(--bad);
    color: #fff;
  }
</style>
