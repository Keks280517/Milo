<script lang="ts">
  import { t } from "../i18n";
  // Timer-Ansicht: alle laufenden und klingelnden Timer gross, dazu eine Schnellwahl fuer neue.
  import { app, clock, inTauri, type Alarm } from "../app.svelte";
  import { tour } from "../tourState.svelte";
  import { demoAlarms, demoTimers } from "../demoData";
  import TimerCard from "../cards/TimerCard.svelte";

  const presets = [1, 3, 5, 10, 15, 30];
  let label = $state("");
  let minutes = $state<number | null>(null);

  // Klingelnde zuerst, danach nach Restzeit.
  // Tour: Beispiel-Timer und -Wecker in der echten Ansicht.
  const demo = $derived(tour.demo === "timer");
  // Beispiel-Timer einmal anlegen, damit er gleichmaessig herunterzaehlt.
  const demoList = demoTimers();
  const timers = $derived(demo ? demoList : app.timers);
  const alarms = $derived(demo ? demoAlarms() : app.settings.alarms);
  const sorted = $derived([...timers].sort((a, b) => Number(b.ringing) - Number(a.ringing) || a.dueMs - b.dueMs));

  // ---------- Fokus ----------
  const focusLeft = $derived(app.focus ? Math.max(0, app.focus.endMs - app.clockNow) : 0);

  // ---------- Wecker ----------
  const DAYS = [t("Mo", "Mon"), t("Di", "Tue"), t("Mi", "Wed"), t("Do", "Thu"), t("Fr", "Fri"), t("Sa", "Sat"), t("So", "Sun")];
  let alarmTime = $state("07:00");
  let alarmDays = $state<number[]>([0, 1, 2, 3, 4]);
  let alarmLabel = $state("");
  let alarmError = $state("");
  function toggleDay(day: number) {
    alarmDays = alarmDays.includes(day) ? alarmDays.filter((d) => d !== day) : [...alarmDays, day].sort();
  }
  function daysText(alarm: Alarm): string {
    const d = [...alarm.days].sort().join(",");
    if (!d) return t("einmalig", "once");
    if (d === "0,1,2,3,4") return t("werktags", "weekdays");
    if (d === "5,6") return t("Wochenende", "weekends");
    if (d === "0,1,2,3,4,5,6") return t("täglich", "every day");
    return alarm.days.map((x) => DAYS[x]).join(", ");
  }
  async function addAlarm() {
    alarmError = "";
    if (!/^\d{1,2}:\d{2}$/.test(alarmTime)) return;
    try {
      if (inTauri) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("alarm_add", { time: alarmTime, days: alarmDays, label: alarmLabel });
      } else {
        app.settings = { ...app.settings, alarms: [...app.settings.alarms, { id: String(Date.now()), time: alarmTime, days: alarmDays, label: alarmLabel, enabled: true }] };
      }
      alarmLabel = "";
    } catch (err) {
      alarmError = String(err);
    }
  }
  function setAlarm(id: string, patch: Partial<Alarm> | null) {
    const alarms = patch === null ? app.settings.alarms.filter((a) => a.id !== id) : app.settings.alarms.map((a) => (a.id === id ? { ...a, ...patch } : a));
    app.saveSettings({ alarms });
  }

  function start(mins: number) {
    if (!(mins > 0)) return;
    app.startTimer(label, Math.round(mins * 60));
    label = "";
    minutes = null;
  }
</script>

<div class="view">
  {#each sorted as timer (timer.id)}
    <TimerCard id={timer.id} timer={demo ? timer : undefined} big={sorted.length === 1} />
  {/each}

  <section class="new focus" class:running={!!app.focus}>
    <h3>{t("Fokus", "Focus")}</h3>
    {#if app.focus}
      <div class="focus-now">
        <span class="focus-time">{clock(focusLeft)}</span>
        <span class="focus-what">{app.focus.phase === "focus" ? t("Konzentriert arbeiten – Milo ist still", "Deep work – Milo stays quiet") : t("Pause – kurz aufstehen, Wasser trinken", "Break – stand up, drink some water")}</span>
        <button onclick={() => app.stopFocus()}>{t("Beenden", "End")}</button>
      </div>
    {:else}
      <p class="hint">{t("Eine Runde konzentriert arbeiten, danach eine kurze Pause. Milo bleibt in der Zeit still, die Pille zeigt die Restzeit. Auch per Sprache: „Fokus 25 Minuten“.", "Work in focus for one round, then take a short break. Milo stays quiet meanwhile and the pill shows the time left. Also by voice: \"Focus 25 minutes\".")}</p>
      <div class="presets">
        {#each [15, 25, 50] as mins}
          <button onclick={() => app.startFocus(mins)}>{mins} min</button>
        {/each}
      </div>
    {/if}
  </section>

  <section class="new alarms">
    <h3>{t("Wecker", "Alarms")}</h3>
    {#each alarms as alarm (alarm.id)}
      <div class="alarm" class:off={!alarm.enabled}>
        <span class="alarm-time">{alarm.time}</span>
        <span class="alarm-what">{alarm.label || t("Wecker", "Alarm")}<small>{daysText(alarm)}</small></span>
        <button class="toggle" class:on={alarm.enabled} onclick={() => setAlarm(alarm.id, { enabled: !alarm.enabled })} role="switch" aria-checked={alarm.enabled} aria-label={t("Wecker an oder aus", "Alarm on or off")}><i></i></button>
        <button class="x" onclick={() => setAlarm(alarm.id, null)} aria-label={t("Wecker löschen", "Delete alarm")}>×</button>
      </div>
    {/each}
    <form onsubmit={(e) => { e.preventDefault(); addAlarm(); }}>
      <input class="clock" type="time" bind:value={alarmTime} aria-label={t("Weckzeit", "Alarm time")} />
      <input class="grow" bind:value={alarmLabel} placeholder={t("Wofür? (optional)", "What for? (optional)")} maxlength="40" spellcheck="false" />
      <button type="submit" class="primary">{t("Stellen", "Set")}</button>
    </form>
    <div class="days" role="group" aria-label={t("Wochentage", "Weekdays")}>
      {#each DAYS as day, i}
        <button class:on={alarmDays.includes(i)} onclick={() => toggleDay(i)} aria-pressed={alarmDays.includes(i)}>{day}</button>
      {/each}
      <small>{alarmDays.length ? "" : t("keiner = einmalig", "none = once")}</small>
    </div>
    {#if alarmError}<p class="hint error">{alarmError}</p>{/if}
  </section>

  <section class="new">
    <h3>{t("Neuer Timer", "New timer")}</h3>
    <div class="presets">
      {#each presets as mins}
        <button onclick={() => start(mins)}>{mins} min</button>
      {/each}
    </div>
    <form onsubmit={(e) => { e.preventDefault(); start(minutes ?? 0); }}>
      <input class="grow" bind:value={label} placeholder={t("Wofür? (optional)", "What for? (optional)")} maxlength="40" spellcheck="false" />
      <input class="mins" type="number" min="1" max="10080" bind:value={minutes} placeholder={t("Minuten", "Minutes")} />
      <button type="submit" class="primary" disabled={!minutes || minutes <= 0}>Start</button>
    </form>
  </section>
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
  }
  .new.focus {
    margin-top: auto;
  }
  .new.alarms,
  .new.alarms ~ .new {
    margin-top: 0;
  }
  .toggle {
    width: 38px;
    height: 22px;
    padding: 2px;
    border-radius: var(--r-full);
    background: #3a3a3e;
    display: flex;
  }
  .toggle i {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--text);
    transition: transform var(--t-fast) ease;
  }
  .toggle.on {
    background: var(--ok);
  }
  .toggle.on i {
    transform: translateX(16px);
  }
  .new.focus.running {
    box-shadow: var(--card-edge), inset 0 0 0 1px rgba(255, 159, 10, 0.35);
  }
  .focus-now {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .focus-time {
    font-size: var(--fs-large);
    font-weight: var(--fw-bold);
    font-variant-numeric: tabular-nums;
    color: #ff9f0a;
  }
  .focus-what {
    flex: 1;
    font-size: var(--fs-footnote);
    color: var(--text-2);
  }
  .hint {
    margin: 0 0 8px;
    font-size: var(--fs-footnote);
    line-height: 1.45;
    color: var(--text-3);
  }
  .hint.error {
    color: #ff6961;
    margin: 6px 0 0;
  }
  .alarm {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 0;
  }
  .alarm.off {
    opacity: 0.5;
  }
  .alarm-time {
    font-size: var(--fs-title);
    font-weight: var(--fw-bold);
    font-variant-numeric: tabular-nums;
  }
  .alarm-what {
    flex: 1;
    display: flex;
    flex-direction: column;
    font-size: var(--fs-body);
  }
  .alarm-what small {
    font-size: var(--fs-caption);
    color: var(--text-3);
  }
  .alarm .x {
    padding: 0 10px;
    background: none;
    color: var(--text-3);
  }
  .clock {
    width: 104px;
    color-scheme: dark;
  }
  .days {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 6px;
  }
  .days button {
    height: 26px;
    padding: 0 8px;
    font-size: var(--fs-footnote);
    color: var(--text-3);
  }
  .days button.on {
    background: var(--text);
    color: #000;
  }
  .days small {
    font-size: var(--fs-caption);
    color: var(--text-4);
  }
  .new {
    margin-top: auto;
    padding: 12px 14px 14px;
    border-radius: var(--r-lg);
    background: var(--card);
    box-shadow: var(--card-edge);
    border: 1px solid var(--line);
  }
  h3 {
    margin: 0 0 8px;
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    color: var(--text);
    letter-spacing: var(--tr-headline);
  }
  .presets {
    display: flex;
    gap: 6px;
  }
  .presets button {
    flex: 1;
  }
  form {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  button,
  input {
    height: 32px;
    border: 0;
    border-radius: var(--r-lg);
    background: var(--chip);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
  }
  button {
    padding: 0 12px;
    font-weight: var(--fw-medium);
    cursor: pointer;
    font-variant-numeric: tabular-nums;
    transition: background var(--t-fast) ease, transform var(--t-fast) ease, opacity var(--t-fast) ease;
  }
  button:hover:not(:disabled) {
    background: #2e2e32;
  }
  button:active:not(:disabled) {
    transform: scale(0.96);
  }
  button.primary {
    background: var(--text);
    color: #000;
    padding: 0 18px;
  }
  button.primary:hover:not(:disabled) {
    background: #fff;
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  input {
    padding: 0 12px;
    outline: 0;
    min-width: 0;
    background: #19191c;
    border: 1px solid var(--line);
    box-sizing: border-box;
  }
  input:focus {
    border-color: #3a3a3e;
  }
  input::placeholder {
    color: var(--text-4);
  }
  .grow {
    flex: 1;
  }
  .mins {
    width: 96px;
  }
</style>
