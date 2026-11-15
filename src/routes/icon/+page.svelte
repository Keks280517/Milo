<script lang="ts">
  // Zeichnet das App-Symbol (1024 x 1024) mit dem echten Charakter. Nur zum Erzeugen der Symboldateien:
  //   scripts\shot-page.ps1 -Url http://localhost:1420/icon -Out app-icon.png -Width 1024 -Height 1024
  //   npx tauri icon app-icon.png
  import { onMount } from "svelte";
  import { Character } from "$lib/character/engine";

  let canvas: HTMLCanvasElement;

  onMount(() => {
    const g = canvas.getContext("2d")!;
    const milo = new Character();
    milo.setMood("idle");
    milo.idleGestures = false;
    // Einschwingen lassen, damit die Federn ruhig stehen.
    for (let i = 0; i < 240; i++) milo.update(1 / 60);
    g.clearRect(0, 0, 1024, 1024);
    // Schwarze, weich gerundete Flaeche wie die Insel.
    const r = 230;
    g.fillStyle = "#000";
    g.beginPath();
    g.roundRect(40, 40, 944, 944, r);
    g.fill();
    milo.draw(g, 512, 720, 420);
  });
</script>

<canvas bind:this={canvas} width="1024" height="1024"></canvas>

<style>
  :global(html, body) {
    margin: 0;
    background: transparent;
  }
  canvas {
    display: block;
  }
</style>
