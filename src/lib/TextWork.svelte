<script lang="ts">
  import { t, locale } from "./i18n";
  // Text-Werkzeuge: der markierte Text (oder die Zwischenablage) und was Milo damit tun soll.
  import { app } from "./app.svelte";

  const ACTIONS = [
    { id: "fix", label: t("Korrigieren", "Fix") },
    { id: "formal", label: t("Förmlicher", "More formal") },
    { id: "casual", label: t("Lockerer", "More casual") },
    { id: "shorter", label: t("Kürzer", "Shorter") },
    { id: "english", label: t("Englisch", "English") },
    { id: "german", label: t("Deutsch", "German") },
    { id: "summary", label: t("Zusammenfassen", "Summarize") },
    { id: "explain", label: t("Erklären", "Explain") },
  ];

  let copied = $state(false);

  async function copyResult() {
    await navigator.clipboard.writeText(app.textResult).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<div class="panel" role="dialog" aria-label={t("Text-Werkzeuge", "Text tools")}>
  <div class="top">
    <span class="kicker">{app.textOffer?.fromSelection ? t("Markierter Text", "Selected text") : t("Text aus der Zwischenablage", "Text from the clipboard")}{app.textOffer && app.textOffer.chars > 400 ? ` · ${app.textOffer.chars.toLocaleString(locale)} ${t("Zeichen", "characters")}` : ""}</span>
    <button class="close" onclick={() => app.textCancel()} aria-label={t("Schließen", "Close")}>×</button>
  </div>
  {#if app.textOffer}
    <p class="preview">{app.textOffer.text}{app.textOffer.chars > 400 ? " …" : ""}</p>
  {/if}
  {#if app.textResult}
    <pre class="result">{app.textResult}</pre>
    <div class="row">
      <span class="note">{t("Liegt in der Zwischenablage.", "It's on the clipboard.")}</span>
      <button onclick={copyResult}>{copied ? t("Kopiert", "Copied") : t("Nochmal kopieren", "Copy again")}</button>
      <button class="primary" onclick={() => app.textCancel()}>{t("Fertig", "Done")}</button>
    </div>
  {:else if app.textOffer}
    <div class="actions">
      {#each ACTIONS as action}
        <button disabled={app.textBusy} onclick={() => app.textTransform(action.id)}>{action.label}</button>
      {/each}
    </div>
    {#if app.textBusy}<p class="note">{t("Gemini arbeitet …", "Gemini is working …")}</p>{/if}
  {/if}
  {#if app.textError}<p class="error">{app.textError}</p>{/if}
</div>

<style>
  .panel {
    margin: 0 12px 8px;
    padding: 10px 12px;
    border-radius: var(--r-md);
    background: #15161a;
    box-shadow: inset 0 0 0 1px #2a2c34;
    display: flex;
    flex-direction: column;
    gap: 8px;
    animation: drop var(--t-base) cubic-bezier(0.3, 1.3, 0.4, 1) both;
  }
  .top {
    display: flex;
    align-items: center;
  }
  .kicker {
    flex: 1;
    font-size: var(--fs-caption);
    font-weight: var(--fw-semibold);
    color: var(--text-3);
  }
  .close {
    all: unset;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-3);
    cursor: pointer;
    font-size: var(--fs-headline);
  }
  .close:hover {
    background: var(--chip-strong);
    color: var(--text);
  }
  .preview {
    margin: 0;
    max-height: 64px;
    overflow: hidden;
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-2);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--chip-strong);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-medium);
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: #3a3a3c;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .primary {
    background: var(--text);
    color: #000;
  }
  .primary:hover {
    background: #fff;
  }
  .result {
    margin: 0;
    padding: 8px 10px;
    max-height: 160px;
    overflow: auto;
    border-radius: var(--r-sm);
    background: var(--card);
    box-shadow: var(--card-edge);
    font: inherit;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text);
    white-space: pre-wrap;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .note {
    flex: 1;
    margin: 0;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .error {
    margin: 0;
    font-size: var(--fs-footnote);
    color: #ff6961;
  }
  @keyframes drop {
    from { opacity: 0; transform: translateY(-6px); }
    to { opacity: 1; transform: none; }
  }
</style>
