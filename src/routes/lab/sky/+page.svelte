<script lang="ts">
  // Entwicklungsseite fuer den Wetter-Himmel: alle Wetterlagen bei Tag und Nacht nebeneinander.
  // Aufruf im Browser: http://localhost:1420/lab/sky
  import "$lib/theme.css";
  import Sky from "$lib/Sky.svelte";
  import { conditionText } from "$lib/weather";

  const CODES = [0, 2, 3, 45, 53, 63, 75, 95];
</script>

<main>
  {#each [true, false] as isDay}
    {#each CODES as code}
      <div class="tile">
        <Sky {code} {isDay} />
        <b>{conditionText(code, isDay)}</b>
        <small>{isDay ? "Tag" : "Nacht"} · {code}</small>
      </div>
    {/each}
  {/each}
</main>

<style>
  :global(body) {
    margin: 0;
    background: #000;
  }
  main {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    padding: 10px;
  }
  .tile {
    position: relative;
    isolation: isolate;
    height: 170px;
    border-radius: 18px;
    overflow: hidden;
    padding: 12px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    color: #fff;
    font-family: system-ui, sans-serif;
    text-shadow: 0 1px 8px rgba(0, 0, 0, 0.35);
  }
  small {
    opacity: 0.75;
  }
</style>
