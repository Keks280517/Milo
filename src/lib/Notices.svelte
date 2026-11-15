<script lang="ts">
  import { t } from "./i18n";
  // Alles, was gerade Aufmerksamkeit braucht, egal welcher Reiter offen ist:
  // Rueckfragen vor riskanten Aktionen und klingelnde Timer.
  import { app } from "./app.svelte";
</script>

{#if app.needsAttention}
  <div class="notices">
    {#each app.questions as question (question.id)}
      <div class="card" class:danger={question.danger} role="alertdialog" aria-label={question.title}>
        <div class="title">{question.title}</div>
        {#if question.detail}
          <div class="detail">{question.detail}</div>
        {/if}
        <div class="actions">
          <button class="ghost" onclick={() => app.answer(question, false)}>{t("Abbrechen", "Cancel")}</button>
          <button class="primary" onclick={() => app.answer(question, true)}>{question.action}</button>
        </div>
      </div>
    {/each}

    {#each app.ringing as timer (timer.id)}
      <div class="card ring" role="alert">
        <svg class="bell" viewBox="0 0 20 20" width="18" height="18" aria-hidden="true">
          <path d="M10 3.2a4.3 4.3 0 0 0-4.3 4.3v2.6L4.4 12.6h11.2l-1.3-2.5V7.5A4.3 4.3 0 0 0 10 3.2Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
          <path d="M8.3 15a1.8 1.8 0 0 0 3.4 0" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
        <div class="title grow">{t(`${timer.label} ist abgelaufen`, `${timer.label} is up`)}</div>
        <button class="ghost" onclick={() => app.extendTimer(timer, 300)} title={t("In fünf Minuten erneut klingeln", "Ring again in five minutes")}>+5 min</button>
        <button class="primary" onclick={() => app.dismissTimer(timer)}>{t("Stopp", "Stop")}</button>
      </div>
    {/each}

  </div>
{/if}

<style>
  .notices {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px 12px 8px;
    max-height: 55%;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .card {
    padding: 12px 14px;
    border-radius: var(--r-lg);
    background: var(--card);
    box-shadow: var(--card-edge);
    border: 1px solid var(--line);
    animation: pop var(--t-slow) cubic-bezier(0.3, 1.3, 0.4, 1) both;
  }
  .card.danger {
    border-color: #4a2623;
  }
  .card.ring {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 14px;
  }
  .bell {
    flex: none;
    color: var(--warn);
    transform-origin: 50% 15%;
    animation: swing 1.2s ease-in-out infinite;
  }
  .title {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    line-height: 1.35;
  }
  .grow {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail {
    margin-top: 4px;
    max-height: 96px;
    overflow-y: auto;
    color: var(--text-3);
    font-size: var(--fs-footnote);
    line-height: 1.45;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    scrollbar-width: thin;
    scrollbar-color: #3a3a3c transparent;
    user-select: text;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }
  button {
    font: inherit;
    border: 0;
    cursor: pointer;
  }
  .ghost,
  .primary {
    height: 30px;
    padding: 0 14px;
    border-radius: var(--r-lg);
    font-size: var(--fs-body);
    font-weight: var(--fw-medium);
    transition: background var(--t-fast) ease, transform var(--t-fast) ease;
  }
  .ghost {
    background: var(--chip);
    color: var(--text);
  }
  .ghost:hover {
    background: #2e2e32;
  }
  .primary {
    background: var(--text);
    color: #000;
  }
  .primary:hover {
    background: #fff;
  }
  .danger .primary {
    background: var(--bad);
    color: #fff;
  }
  .danger .primary:hover {
    background: #ff5d53;
  }
  .ghost:active,
  .primary:active {
    transform: scale(0.96);
  }


  @keyframes pop {
    from { opacity: 0; transform: translateY(-6px) scale(0.97); }
    to { opacity: 1; transform: none; }
  }
  @keyframes swing {
    0%, 62%, 100% { transform: rotate(0); }
    8% { transform: rotate(16deg); }
    20% { transform: rotate(-14deg); }
    32% { transform: rotate(10deg); }
    44% { transform: rotate(-6deg); }
    54% { transform: rotate(2deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .card, .bell { animation: none; }
  }
</style>
