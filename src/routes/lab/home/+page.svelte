<script lang="ts">
  // Entwicklungsseite fuer die Startseite: die Uebersicht in der Groesse der offenen Insel
  // (klein, mittel, gross), mit Beispieldaten. Aufruf: http://localhost:1420/lab/home?demo
  // Parameter: cards=media,agenda,... sizes=media:s,agenda:l ... panel=0|1|2
  import "$lib/theme.css";
  import { app } from "$lib/app.svelte";
  import HomeView from "$lib/views/HomeView.svelte";

  const PANELS = [{ w: 760, h: 420 }, { w: 880, h: 460 }, { w: 1000, h: 480 }];
  const params = new URLSearchParams(typeof location === "undefined" ? "" : location.search);
  const panel = PANELS[Number(params.get("panel") ?? 1)] ?? PANELS[1];
  const cards = params.get("cards")?.split(",").filter(Boolean);
  const sizes = Object.fromEntries((params.get("sizes") ?? "").split(",").filter(Boolean).map((s) => s.split(":")));
  app.settings = { ...app.settings, homeCards: cards ?? app.settings.homeCards, homeSizes: sizes };
  app.media = { title: "Windows down, the city’s glowing", artist: "Nova Lane", album: "Golden Hour", subtitle: "", app: "Spotify", playing: true, positionMs: 42000, durationMs: 195000, atMs: Date.now(), canNext: true, canPrevious: true, canSeek: true, cover: "" };
  app.weather = { place: "Berlin", region: "", now: { temp: 6, feels: 3, code: 63, text: "Regen", wind: 12, humidity: 80, isDay: true }, hours: [], days: [{ date: "2026-11-15", label: "So", min: 2, max: 8, code: 63, text: "", rain: 70, wind: 12, sunrise: "", sunset: "" }] };
  // Oben ~58 px Kopfzeile, unten ~62 px Wuerfelleiste: der Rest gehoert der Uebersicht.
  const height = panel.h - 120;
</script>

<main style:width="{panel.w}px">
  <div class="area" style:height="{height}px" data-lab-home>
    <HomeView />
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    background: linear-gradient(120deg, #6a2a3a, #2a3a6a);
    font-family: var(--font, system-ui, sans-serif);
  }
  main {
    margin: 20px auto;
    padding: 58px 0 62px;
    border-radius: 0 0 36px 36px;
    background: #050506;
  }
  .area {
    display: flex;
    flex-direction: column;
  }
</style>
