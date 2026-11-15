<script lang="ts">
  // Entwicklungsseite fuer den Charakter. Aufruf im Browser: http://localhost:1420/lab
  //
  //   /lab                              Buehne plus alle Stimmungen und Arm-Varianten, live
  //   /lab?t=2.5&freeze=1               Standbild nach 2,5 s
  //   /lab?shot=success&st=0.3          Geste nach 0,3 s
  //   /lab?size=30                      Koerpergroesse
  //   /lab?strip=success,wave&n=12&dt=0.08   Bildstreifen: n Bilder im Abstand dt je Stimmung/Geste
  //   /lab?strip=...&arm=hands          Bildstreifen mit anderer Arm-Variante
  //   /lab?strip=...&t0=1.5             Bildstreifen erst nach 1,5 s in der Stimmung beginnen
  import "$lib/theme.css";
  import { onMount } from "svelte";
  import CharacterView from "$lib/character/Character.svelte";
  import StageView from "$lib/character/Stage.svelte";
  import { Character, type ArmStyle, type Mood, type OneShot } from "$lib/character/engine";

  const moods: Mood[] = [
    "idle", "listening", "thinking", "answering", "working", "searching", "fileHover", "fileOut",
    "timer", "ringing", "sleeping", "waking", "music", "collab", "walk", "dash", "sit", "doze", "push",
  ];
  const shots: OneShot[] = [
    "success", "error", "gulp", "wave", "hop", "stretch", "lookAround", "tilt", "perk", "nod",
    "appear", "vanish", "handoff", "kick", "startle", "giggle", "shrug",
  ];
  const styles: { id: ArmStyle; label: string }[] = [
    { id: "capsule", label: "Kapsel-Arme" },
    { id: "hands", label: "Schwebende Kugelhände" },
    { id: "spring", label: "Federlinie mit Kugelhand" },
  ];

  const params = new URLSearchParams(typeof location === "undefined" ? "" : location.search);
  const frozen = params.has("freeze");
  const strips = (params.get("strip") ?? "").split(",").filter(Boolean);
  const stripArm = (params.get("arm") ?? "capsule") as ArmStyle;
  const frames = Number(params.get("n") ?? 12);
  const frameStep = Number(params.get("dt") ?? 0.08);
  const stripStart = Number(params.get("t0") ?? 0);

  let size = $state(Number(params.get("size") ?? 64));
  let refs = $state<Record<string, CharacterView | undefined>>({});
  let stripCanvas = $state<HTMLCanvasElement>();
  let stage = $state<StageView>();
  let stageMood = $state<Mood>("idle");
  let liveliness = $state(2);

  function playAll(kind: OneShot) {
    for (const ref of Object.values(refs)) ref?.play(kind);
    stage?.play(kind);
  }

  /** Zeichnet je Eintrag eine Zeile mit aufeinanderfolgenden Bildern, um Bewegungsablaeufe zu pruefen. */
  function drawStrips(canvas: HTMLCanvasElement) {
    const cellW = size * 2.6, cellH = size * 2.5;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = frames * cellW * dpr;
    canvas.height = strips.length * cellH * dpr;
    canvas.style.width = frames * cellW + "px";
    canvas.style.height = strips.length * cellH + "px";
    const g = canvas.getContext("2d")!;
    g.scale(dpr, dpr);
    strips.forEach((name, row) => {
      const character = new Character({ armStyle: stripArm });
      character.idleGestures = false;
      const mood = moods.includes(name as Mood) ? (name as Mood) : null;
      const pace = mood === "dash" ? 300 : 32;
      const step = (seconds: number) => {
        for (let left = seconds; left > 1e-6; left -= 1 / 60) {
          const dt = Math.min(left, 1 / 60);
          if (character.mood === "walk" || character.mood === "dash") character.step += (Math.PI * pace * dt) / (34 * 0.36);
          character.update(dt);
        }
      };
      if (mood) {
        step(1);
        character.setMood(mood);
      } else {
        step(2);
        character.play(name as OneShot);
      }
      step(stripStart);
      const floor = row * cellH + cellH * 0.86;
      g.strokeStyle = "#1c1c1e";
      g.beginPath();
      g.moveTo(0, floor + 0.5);
      g.lineTo(frames * cellW, floor + 0.5);
      g.stroke();
      for (let i = 0; i < frames; i++) {
        character.draw(g, i * cellW + cellW / 2, floor, size);
        g.fillStyle = "#6e6e73";
        g.font = "10px Segoe UI";
        g.fillText(i === 0 ? name : (stripStart + i * frameStep).toFixed(2), i * cellW + 4, row * cellH + 12);
        step(frameStep);
      }
    });
  }

  onMount(() => {
    if (strips.length && stripCanvas) {
      drawStrips(stripCanvas);
      return;
    }
    const all = Object.values(refs);
    for (const ref of all) ref?.advance(Number(params.get("t") ?? 0));
    const shot = params.get("shot") as OneShot | null;
    if (shot && shots.includes(shot)) {
      for (const ref of all) {
        ref?.play(shot);
        ref?.advance(Number(params.get("st") ?? 0.2));
      }
    }
    for (const ref of all) ref?.redraw();
  });
</script>

<main>
  {#if strips.length}
    <canvas bind:this={stripCanvas}></canvas>
  {:else}
    <h1>Milo – Charakter-Labor</h1>

    <h2>Bühne (so groß wie die geschlossene Insel)</h2>
    <div class="pill">
      <StageView
        bind:this={stage}
        width={420} height={64}
        layout={{ left: 0, right: 420, floor: 55, roamMin: 48, roamMax: 372, size: 34 }}
        mood={stageMood} {liveliness} paused={frozen}
      />
    </div>
    <div class="bar">
      <label>Stimmung
        <select bind:value={stageMood}>
          {#each moods as mood}<option>{mood}</option>{/each}
        </select>
      </label>
      {#each ["ball", "friend", "plant", "stars", "push", "sit"] as const as what}
        <button onclick={() => stage?.perform(what)}>{what}</button>
      {/each}
      <label>Lebhaftigkeit <input type="range" min="0" max="2" step="1" bind:value={liveliness} /> {["ruhig", "normal", "verspielt"][liveliness]}</label>
    </div>

    <h2>Gesten</h2>
    <div class="bar">
      <label>Größe <input type="range" min="24" max="120" bind:value={size} /> {size}px</label>
      {#each shots as kind}
        <button onclick={() => playAll(kind)}>{kind}</button>
      {/each}
    </div>

    {#each styles as style}
      <h2>{style.label}</h2>
      <div class="grid">
        {#each moods as mood}
          <figure>
            <CharacterView
              bind:this={refs[`${style.id}-${mood}`]}
              {mood} {size} armStyle={style.id}
              width={size * 2.8} height={size * 2.6}
              level={mood === "music" ? 0.5 : 0}
              urgency={mood === "timer" ? 1 : 0}
              idleGestures={false} paused={frozen}
            />
            <figcaption>{mood}</figcaption>
          </figure>
        {/each}
        <figure>
          <CharacterView
            bind:this={refs[`${style.id}-helper`]}
            mood="collab" {size} armStyle={style.id} facing={-1}
            accent={["#64d2ff", "#bf5af2"]}
            width={size * 2.8} height={size * 2.6} idleGestures={false} paused={frozen}
          />
          <figcaption>Cloud-Helfer</figcaption>
        </figure>
      </div>
    {/each}
  {/if}
</main>

<style>
  :global(html), :global(body) {
    margin: 0;
    background: #101012;
    color: var(--text);
    font-family: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  }
  main { padding: 20px 28px 60px; }
  h1 { font-size: 20px; margin: 0 0 14px; }
  h2 { font-size: 14px; font-weight: 600; color: #a1a1a6; margin: 26px 0 8px; }
  .bar { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; font-size: 13px; margin-top: 10px; }
  .bar button, .bar select {
    padding: 6px 12px; border-radius: 999px; border: 1px solid #2c2c2e;
    background: #1c1c1e; color: inherit; font: inherit; cursor: pointer;
  }
  .pill {
    position: relative; width: 420px; height: 64px; overflow: hidden;
    background: #000; border-radius: 0 0 30px 30px;
  }
  .grid { display: flex; flex-wrap: wrap; gap: 4px; }
  figure { margin: 0; text-align: center; background: #000; border-radius: 12px; }
  figcaption { font-size: 11px; color: #8e8e93; padding-bottom: 6px; }
</style>
