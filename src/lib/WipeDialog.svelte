<script lang="ts">
  import { t, lang } from "./i18n";
  // "Milo loeschen": zwei Warnstufen, die zweite mit einem Code zum Abtippen.
  // Erst danach wird wirklich etwas entfernt; Abbrechen ist jederzeit moeglich.
  import { invoke } from "@tauri-apps/api/core";
  import { app } from "./app.svelte";

  let step = $state(1);
  let typed = $state("");
  let error = $state("");
  let busy = $state(false);
  // Jedes Mal ein neuer vierstelliger Code, damit niemand ihn aus Gewohnheit eintippt.
  const code = String(1000 + Math.floor(Math.random() * 9000));

  function cancel() {
    app.wiping = false;
  }

  async function wipe() {
    if (typed.trim() !== code || busy) return;
    busy = true;
    try {
      await invoke("wipe_everything");
    } catch (err) {
      error = String(err);
      busy = false;
    }
  }
</script>

<div class="card" role="alertdialog" aria-label={t("Milo löschen", "Delete Milo")}>
  {#if step === 1}
    <div class="title">{t("Milo wirklich löschen?", "Really delete Milo?")}</div>
    <div class="detail">
      {t("Das entfernt Milo von diesem PC: das Programm, alle Notizen und alles Gemerkte, Milos Kopien deiner Dateien, Timer, Einstellungen, den Autostart, den Gemini-Schlüssel und die Verbindung zu Claude. Deine eigenen Dateien bleiben unberührt. Das lässt sich nicht rückgängig machen.", "This removes Milo from this PC: the app, all notes and everything it remembered, Milo's copies of your files, timers, settings, autostart, the Gemini key and the connection to Claude. Your own files stay untouched. This can't be undone.")}
    </div>
    <div class="actions">
      <button class="ghost" onclick={cancel}>{t("Abbrechen", "Cancel")}</button>
      <button class="danger" onclick={() => (step = 2)}>{t("Ja, weiter", "Yes, continue")}</button>
    </div>
  {:else}
    <div class="title">{t("Letzte Bestätigung", "Final confirmation")}</div>
    <div class="detail">
      {#if lang === "en"}Type the code <strong>{code}</strong> to delete Milo for good.{:else}Tippe den Code <strong>{code}</strong> ein, um Milo endgültig zu löschen.{/if}
    </div>
    {#if error}<div class="note error">{error}</div>{/if}
    <div class="actions">
      <!-- svelte-ignore a11y_autofocus -->
      <input class="code" bind:value={typed} inputmode="numeric" maxlength="4" placeholder={t("Code", "Code")} autofocus />
      <button class="ghost" onclick={cancel}>{t("Abbrechen", "Cancel")}</button>
      <button class="danger" onclick={wipe} disabled={typed.trim() !== code || busy}>{t("Endgültig löschen", "Delete for good")}</button>
    </div>
  {/if}
</div>

<style>
  .card {
    margin: 2px 12px 8px;
    padding: 12px 16px;
    border-radius: var(--r-lg);
    background: #1a1011;
    border: 1px solid #5a2622;
    animation: pop var(--t-slow) cubic-bezier(0.3, 1.3, 0.4, 1) both;
  }
  .title {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
  }
  .detail {
    margin-top: 6px;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-2);
  }
  .detail strong {
    font-size: var(--fs-callout);
    letter-spacing: 0.12em;
    color: #fff;
  }
  .note {
    margin-top: 6px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .note.error {
    color: #ff8a80;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .code {
    width: 86px;
    height: 30px;
    margin-right: auto;
    padding: 0 12px;
    box-sizing: border-box;
    border-radius: var(--r-lg);
    border: 1px solid #5a2622;
    outline: 0;
    background: #0d0808;
    color: #fff;
    font: inherit;
    font-size: var(--fs-callout);
    letter-spacing: 0.14em;
    font-variant-numeric: tabular-nums;
  }
  button {
    height: 30px;
    padding: 0 14px;
    border: 0;
    border-radius: var(--r-lg);
    font: inherit;
    font-size: var(--fs-body);
    font-weight: var(--fw-medium);
    cursor: pointer;
    transition: background var(--t-fast) ease, opacity var(--t-fast) ease;
  }
  .ghost {
    background: #2a2224;
    color: var(--text);
  }
  .ghost:hover {
    background: #382d30;
  }
  .danger {
    background: var(--bad);
    color: #fff;
  }
  .danger:hover:not(:disabled) {
    background: #ff5d53;
  }
  .danger:disabled {
    opacity: 0.35;
    cursor: default;
  }
  @keyframes pop {
    from { opacity: 0; transform: translateY(-6px) scale(0.97); }
    to { opacity: 1; transform: none; }
  }
</style>
