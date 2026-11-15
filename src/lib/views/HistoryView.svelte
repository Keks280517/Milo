<script lang="ts">
  import { t, locale } from "../i18n";
  // Verlauf: was Milo wann getan hat (Werkzeuge, Schnellbefehle, Dateien, Claude-Freigaben), mit
  // Angaben, Dauer und Ergebnis. Wo es sicher geht, laesst sich ein Eintrag rueckgaengig machen.
  // Das Protokoll liegt nur lokal und wird nach 30 Tagen geloescht.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { inTauri, type Action } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoActions } from "../demoData";

  let actions = $state<Action[]>([]);
  let filter = $state<"all" | "tools" | "files" | "claude" | "errors">("all");
  let open = $state<number | null>(null);
  let busy = $state<number | null>(null);
  let message = $state("");

  async function load() {
    if (tour.demo === "history") {
      actions = demoActions();
      return;
    }
    if (!inTauri) return;
    try {
      actions = await invoke<Action[]>("action_log", { limit: 300 });
    } catch (err) {
      message = String(err);
    }
  }

  onMount(() => {
    load();
    const timer = window.setInterval(load, 5000);
    return () => clearInterval(timer);
  });

  const LABELS: Record<string, string> = {
    cloud: "Gemini", claude: "Claude", file_in: t("Datei", "File"), file_out: t("Datei", "File"), file_delete: t("Datei", "File"), file_move: t("Datei", "File"), undo: t("Rückgängig", "Undo"),
    system_power: "System", set_volume: t("Lautstärke", "Volume"), media_control: t("Wiedergabe", "Playback"), open_app: t("Programm", "App"), window_control: t("Fenster", "Window"),
    workspace: t("Arbeitsplatz", "Workspace"), browser_click: "Browser", browser_type: "Browser", browser_navigate: "Browser", browser_key: "Browser",
    web_research: t("Recherche", "Research"), deep_research: t("Recherche", "Research"), get_weather: t("Wetter", "Weather"), add_note: t("Notiz", "Note"), search_notes: t("Notiz", "Note"), run_script: t("Skript", "Script"),
    play_youtube: "YouTube", look_at_screen: t("Bildschirm", "Screen"), save_screenshot: t("Bildschirm", "Screen"), clipboard: t("Zwischenablage", "Clipboard"), set_timer: "Timer",
  };
  const label = (tool: string) => LABELS[tool] ?? tool.replace(/_/g, " ");

  function group(tool: string): "files" | "claude" | "tools" {
    if (tool.startsWith("file") || tool === "undo") return "files";
    if (tool === "claude" || tool === "cloud") return "claude";
    return "tools";
  }

  const shown = $derived(
    actions.filter((a) => filter === "all" || (filter === "errors" ? !a.ok : group(a.tool) === filter)),
  );

  /** Nach Tagen gruppiert, neueste zuerst. */
  const days = $derived.by(() => {
    const out: { day: string; items: Action[] }[] = [];
    for (const action of shown) {
      const day = action.at.slice(0, 10);
      if (out.at(-1)?.day !== day) out.push({ day, items: [] });
      out.at(-1)!.items.push(action);
    }
    return out;
  });

  function dayName(day: string): string {
    const today = new Date();
    const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    if (day === iso(today)) return t("Heute", "Today");
    const yesterday = new Date(today);
    yesterday.setDate(today.getDate() - 1);
    if (day === iso(yesterday)) return t("Gestern", "Yesterday");
    const [y, m, d] = day.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(locale, { weekday: "long", day: "numeric", month: "long" });
  }

  function took(ms: number): string {
    if (!ms) return "";
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toLocaleString(locale, { maximumFractionDigits: 1 })} s`;
  }

  async function undo(action: Action) {
    busy = action.id;
    message = "";
    try {
      message = await invoke<string>("undo_action", { id: action.id });
    } catch (err) {
      message = String(err);
    } finally {
      busy = null;
      load();
    }
  }

  const FILTERS = [["all", t("Alles", "All")], ["tools", "Milo"], ["files", t("Dateien", "Files")], ["claude", "Claude & Gemini"], ["errors", t("Fehler", "Errors")]] as const;
</script>

<div class="view">
  <div class="filters">
    {#each FILTERS as [id, name]}
      <button class:on={filter === id} onclick={() => (filter = id)}>{name}</button>
    {/each}
  </div>
  {#if message}<p class="message">{message}</p>{/if}

  {#if days.length === 0}
    <p class="hint">{t("Noch nichts protokolliert. Sobald Milo etwas für dich erledigt, steht es hier.", "Nothing logged yet. As soon as Milo does something for you, it shows up here.")}</p>
  {/if}
  {#each days as { day, items } (day)}
    <section class="day">
      <div class="kicker">{dayName(day)}</div>
      <ul>
        {#each items as action (action.id)}
          <li class:failed={!action.ok} class:undone={action.undone}>
            <button class="row" onclick={() => (open = open === action.id ? null : action.id)}>
              <span class="time">{action.at.slice(11, 16)}</span>
              <span class="tool">{label(action.tool)}</span>
              <span class="summary">{action.summary}</span>
              {#if action.undone}<span class="chip">{t("rückgängig", "undone")}</span>{:else if !action.ok}<span class="chip bad">{t("Fehler", "Error")}</span>{/if}
            </button>
            {#if open === action.id}
              <div class="more">
                {#if action.detail}<code>{action.detail}</code>{/if}
                <div class="facts">
                  <span>{t(`${action.at.slice(11)} Uhr`, action.at.slice(11))}</span>
                  {#if took(action.ms)}<span>{t("dauerte", "took")} {took(action.ms)}</span>{/if}
                  <span>{action.tool}</span>
                </div>
              </div>
            {/if}
            {#if action.undoable}
              <button class="undo" disabled={busy !== null} onclick={() => undo(action)}>{busy === action.id ? "…" : t("Rückgängig", "Undo")}</button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/each}
  <p class="foot">{t("Nur auf diesem PC gespeichert, 30 Tage lang. Passwörter stehen nie im Verlauf.", "Stored only on this PC, for 30 days. Passwords never appear in the history.")}</p>
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
  .filters {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .filters button {
    height: 26px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--chip);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .filters button.on {
    background: var(--text);
    color: #000;
  }
  .day {
    border-radius: var(--r-md);
    background: var(--card);
    box-shadow: var(--card-edge);
    padding: 10px 6px 6px;
  }
  .kicker {
    padding: 0 6px 4px;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    position: relative;
    border-radius: var(--r-sm);
  }
  li:hover {
    background: #17171a;
  }
  .row {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: grid;
    grid-template-columns: 38px 84px 1fr auto;
    gap: 8px;
    align-items: baseline;
    padding: 6px 92px 6px 6px;
    font-size: var(--fs-footnote);
    color: var(--text-2);
    cursor: pointer;
  }
  .time {
    font-size: var(--fs-footnote);
    color: var(--text-4);
  }
  .tool {
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: var(--accent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .failed .tool { color: #ff6961; }
  .undone .summary { color: #6e6e73; text-decoration: line-through; }
  .chip {
    padding: 0 6px;
    border-radius: var(--r-xs);
    background: var(--chip);
    color: var(--text-3);
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
  }
  .chip.bad {
    background: #2a1212;
    color: #ff6961;
  }
  .more {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0 8px 8px 52px;
  }
  code {
    padding: 6px 8px;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: var(--fs-caption);
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .facts {
    display: flex;
    gap: 10px;
    font-size: var(--fs-caption);
    color: var(--text-4);
  }
  .undo {
    position: absolute;
    top: 3px;
    right: 6px;
    height: 22px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-sm);
    background: var(--chip-strong);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .undo:hover { background: #3a3a3c; }
  .undo:disabled { opacity: 0.5; cursor: default; }
  .hint,
  .foot,
  .message {
    margin: 0;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .message {
    color: var(--text-2);
  }
  /* Leerer Verlauf: mittig wie die anderen Leer-Zustaende, nicht oben links angeklebt. */
  .hint {
    padding: 48px 0 6px;
    text-align: center;
    font-size: var(--fs-body);
  }
  .foot {
    text-align: center;
    font-size: var(--fs-caption);
    color: var(--text-4);
  }
</style>
