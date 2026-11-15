<script lang="ts">
  // Anleitung zum Verbinden mit Claude Code: was man vorher braucht, was der Knopf macht und wie man
  // es wieder loest. Steht im Claude-Tab (solange nicht verbunden) und in den Einstellungen.
  import { t } from "../i18n";
  import { app } from "../app.svelte";
  import { untrack } from "svelte";

  let { open: startOpen = false }: { open?: boolean } = $props();
  // Nur der Anfangszustand kommt von aussen; danach klappt der Nutzer selbst.
  let open = $state(untrack(() => startOpen));
  const name = $derived(app.settings.name);
</script>

<div class="guide">
  <button class="head" onclick={() => (open = !open)} aria-expanded={open}>
    <span>{t("So richtest du es ein", "How to set it up")}</span>
    <span class="chev" class:open>›</span>
  </button>
  {#if open}
    <ol>
      <li>
        <strong>{t("Claude Code bereithalten", "Have Claude Code ready")}</strong>
        <span>{t("Entweder die Claude-Desktop-App mit dem Code-Tab oder Claude Code im Terminal. Melde dich dort mit deinem Claude-Konto an. Die Nutzungsgrenzen gibt es nur mit Pro- oder Max-Abo.", "Either the Claude desktop app with its Code tab or Claude Code in a terminal. Sign in there with your Claude account. Usage limits are only available with a Pro or Max plan.")}</span>
      </li>
      <li>
        <strong>{t("Node.js installieren (einmalig)", "Install Node.js (once)")}</strong>
        <span>{t(`${name} meldet sich über zwei kleine Skripte bei Claude, die mit Node.js laufen. Hol dir die LTS-Version von nodejs.org, falls du sie noch nicht hast.`, `${name} talks to Claude through two small scripts that run on Node.js. Get the LTS version from nodejs.org if you don't have it yet.`)}</span>
      </li>
      <li>
        <strong>{t("„Mit Claude Code verbinden“ klicken", "Click “Connect to Claude Code”")}</strong>
        <span>{t(`${name} trägt dabei Hooks (melden, was Claude gerade tut) und die Statuszeile (meldet die Limits) in deine Claude-Einstellungen ein: %USERPROFILE%\\.claude\\settings.json. Deine eigenen Einträge bleiben erhalten.`, `${name} adds hooks (report what Claude is doing) and the status line (reports limits) to your Claude settings: %USERPROFILE%\\.claude\\settings.json. Your own entries are kept.`)}</span>
      </li>
      <li>
        <strong>{t("Neue Sitzung starten", "Start a new session")}</strong>
        <span>{t("Claude liest die Einstellungen beim Start. Schon offene Sitzungen einmal neu starten, dann erscheint der orange Würfel und hier siehst du live mit.", "Claude reads its settings on start. Restart sessions that are already open; then the orange cube appears and you can follow along here.")}</span>
      </li>
    </ol>
    <p class="foot">{t(`Alles bleibt auf deinem PC; ${name} liest nur mit und macht Claude nicht langsamer. „Verbindung trennen“ entfernt die Einträge wieder vollständig.`, `Everything stays on your PC; ${name} only reads along and doesn't slow Claude down. “Disconnect” removes the entries again completely.`)}</p>
  {/if}
</div>

<style>
  .guide {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .head {
    all: unset;
    display: flex;
    justify-content: space-between;
    align-items: center;
    cursor: pointer;
    font-size: var(--fs-footnote);
    font-weight: var(--fw-semibold);
    color: #f5b08a;
  }
  .head:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
    border-radius: var(--r-xs);
  }
  .chev {
    display: inline-block;
    transition: transform var(--t-fast) ease;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  ol {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-3);
  }
  li strong {
    display: block;
    color: var(--text);
    font-weight: var(--fw-semibold);
  }
  .foot {
    margin: 0;
    font-size: var(--fs-caption);
    line-height: 1.45;
    color: var(--text-3);
  }
</style>
