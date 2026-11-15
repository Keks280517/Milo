<script lang="ts">
  // Erklaerkasten der Tour. Er sitzt unterhalb der Insel, nicht in ihr, damit alles, was die Tour in
  // der Insel zeigt, frei sichtbar bleibt. Dazu: Fortschritt, Zurueck/Weiter, Ueberspringen mit
  // Rueckfrage und am Ende "Geschafft!".
  import { fade, fly, scale } from "svelte/transition";
  import { t } from "./i18n";
  import { app } from "./app.svelte";
  import { tour, tourSteps } from "./tourState.svelte";

  const steps = $derived(tourSteps(app.settings.name));
  const step = $derived(steps[Math.min(tour.index, steps.length - 1)]);
  /** Der Kasten rueckt leicht zum erklaerten Bereich (hoechstens 140 px), der Pfeil zeigt genau hin. */
  const shift = $derived(tour.done || tour.confirmSkip ? 0 : Math.max(-140, Math.min(140, tour.focusX * 0.4)));
  const arrow = $derived(Math.max(-220, Math.min(220, tour.focusX - shift)));

  function next() {
    tour.go(tour.index + 1, steps.length);
  }
  function back() {
    tour.go(tour.index - 1, steps.length);
  }
  function finish() {
    tour.end();
    app.tab = "chat";
  }

  function onKey(e: KeyboardEvent) {
    if (tour.confirmSkip) {
      if (e.key === "Escape") tour.confirmSkip = false;
      return;
    }
    if (e.key === "ArrowRight" || e.key === "Enter") {
      e.preventDefault();
      if (tour.done) finish();
      else next();
    } else if (e.key === "ArrowLeft") back();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="tour-card-wrap" style:--shift="{shift}px" style:--arrow="{arrow}px" role="dialog" aria-modal="true" aria-label={t("Einführung", "Introduction")} in:fly={{ y: -10, duration: 320 }}>
  {#if tour.confirmSkip}
    <div class="box" in:scale={{ start: 0.94, duration: 200 }}>
      <h3>{t("Wirklich überspringen?", "Really skip?")}</h3>
      <p>{t(`Die Tour lässt sich später in den Einstellungen unter „Über ${app.settings.name}“ jederzeit wieder starten.`, `You can start the tour again any time in the settings under "About ${app.settings.name}".`)}</p>
      <div class="foot">
        <span class="grow"></span>
        <button class="ghost-btn" onclick={() => (tour.confirmSkip = false)}>{t("Nein, weiter ansehen", "No, keep watching")}</button>
        <button class="primary" onclick={finish}>{t("Ja, überspringen", "Yes, skip")}</button>
      </div>
    </div>
  {:else if tour.done}
    <div class="box done" in:scale={{ start: 0.85, duration: 480 }}>
      <div class="burst" aria-hidden="true">
        {#each Array.from({ length: 16 }, (_, i) => i) as i}<i style:--a="{i * (360 / 16)}deg" style:--d="{i * 25}ms"></i>{/each}
      </div>
      <h2>{t("Super, geschafft!", "Awesome, all done!")}</h2>
      <p>{t(`Du kennst jetzt alles Wichtige. Die Tour kannst du jederzeit in den Einstellungen unter „Über ${app.settings.name}“ noch einmal ansehen.`, `You now know everything important. You can watch the tour again any time in the settings under "About ${app.settings.name}".`)}</p>
      <button class="primary" onclick={finish}>{t("Los geht's", "Let's go")}</button>
    </div>
  {:else if step}
    {#key tour.index}
      <div class="box pointing" in:fade={{ duration: 220 }}>
        <div class="dots" aria-hidden="true">
          {#each steps as _, i}<span class:on={i === tour.index} class:past={i < tour.index}></span>{/each}
          <span class="count">{tour.index + 1} / {steps.length}</span>
        </div>
        <h3>{step.title}</h3>
        <p>{step.text}</p>
        {#if step.note}<p class="note">{step.note}</p>{/if}
        <div class="foot">
          <button class="skip" onclick={() => (tour.confirmSkip = true)}>{t("Überspringen", "Skip")}</button>
          <span class="grow"></span>
          {#if tour.index > 0}<button class="ghost-btn" onclick={back}>{t("Zurück", "Back")}</button>{/if}
          <button class="primary" onclick={next}>{tour.index === steps.length - 1 ? t("Fertig", "Finish") : t("Weiter", "Next")}</button>
        </div>
      </div>
    {/key}
  {/if}
</div>

<style>
  .tour-card-wrap {
    position: absolute;
    top: calc(100% + 14px);
    left: 50%;
    transform: translateX(calc(-50% + var(--shift, 0px)));
    transition: transform var(--t-spring) var(--ease-spring, cubic-bezier(0.3, 1.3, 0.4, 1));
    width: min(520px, 92vw);
    z-index: 70;
    color: var(--text);
  }
  .box {
    position: relative;
    box-sizing: border-box;
    padding: 14px 18px 12px;
    border-radius: var(--r-lg);
    background: #121216;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1), inset 0 1px 0 rgba(255, 255, 255, 0.08), 0 18px 44px rgba(0, 0, 0, 0.55);
  }
  /* Kleiner Pfeil oben, der auf den erklaerten Bereich zeigt. */
  .box.pointing::before {
    content: "";
    position: absolute;
    top: -7px;
    left: calc(50% + var(--arrow, 0px) - 7px);
    width: 14px;
    height: 14px;
    border-radius: 3px;
    background: rgba(18, 18, 22, 0.97);
    box-shadow: inset 1px 1px 0 rgba(255, 255, 255, 0.1);
    transform: rotate(45deg);
    transition: left var(--t-spring) var(--ease-spring, ease);
  }
  .dots {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 8px;
  }
  .dots span {
    width: 5px;
    height: 5px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.16);
    transition: width var(--t-base) ease, background var(--t-base) ease;
  }
  .dots span.past {
    background: rgba(255, 255, 255, 0.4);
  }
  .dots span.on {
    width: 16px;
    background: var(--accent);
  }
  .dots .count {
    width: auto;
    height: auto;
    margin-left: auto;
    background: none;
    font-size: var(--fs-caption);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  h3 {
    margin: 0 0 6px;
    font-size: var(--fs-headline);
    font-weight: var(--fw-bold);
  }
  p {
    margin: 0;
    font-size: var(--fs-body);
    line-height: 1.5;
    color: var(--text-2);
  }
  .note {
    margin-top: 8px;
    padding: 8px 10px;
    border-radius: var(--r-md);
    background: rgba(232, 135, 93, 0.13);
    color: #f5c2a8;
    font-size: var(--fs-footnote);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }
  .grow {
    flex: 1;
  }
  button {
    font: inherit;
    cursor: pointer;
  }
  .primary,
  .ghost-btn {
    height: 32px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--r-md);
    font-size: var(--fs-body);
    font-weight: var(--fw-semibold);
  }
  .primary {
    background: var(--text);
    color: #000;
  }
  .ghost-btn {
    background: rgba(255, 255, 255, 0.09);
    color: var(--text);
  }
  .skip {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-3);
    font-size: var(--fs-footnote);
  }
  .skip:hover {
    color: var(--text);
  }
  button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .done {
    text-align: center;
    padding: 24px 22px 18px;
  }
  .done h2 {
    margin: 0 0 8px;
    font-size: var(--fs-title);
  }
  .done p {
    margin: 0 0 16px;
  }
  .burst {
    position: absolute;
    left: 50%;
    top: 4px;
    width: 0;
    height: 0;
  }
  .burst i {
    position: absolute;
    width: 6px;
    height: 14px;
    border-radius: 3px;
    background: var(--accent);
    opacity: 0;
    animation: pop 1100ms cubic-bezier(0.2, 0.8, 0.3, 1) var(--d) forwards;
  }
  .burst i:nth-child(3n) { background: #ffd60a; }
  .burst i:nth-child(3n + 1) { background: #ff6b9d; }
  @keyframes pop {
    0% { opacity: 1; transform: rotate(var(--a)) translateY(0) scale(0.6); }
    100% { opacity: 0; transform: rotate(var(--a)) translateY(-80px) scale(1); }
  }
  @media (prefers-reduced-motion: reduce) {
    .tour-card-wrap,
    .box.pointing::before {
      transition: none;
    }
    .burst i {
      animation: none;
    }
  }
</style>
