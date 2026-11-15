<script lang="ts">
  // Inter: freie Schrift (OFL 1.1), liegt als Paket lokal vor und wird nicht aus dem Netz geladen.
  import "@fontsource-variable/inter/wght.css";
  import "@fontsource-variable/jetbrains-mono/wght.css";
  import "$lib/theme.css";
  import Island from "$lib/Island.svelte";
</script>

<Island />

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
    font-family: var(--font-sans);
    font-feature-settings: "cv11", "ss03";
    color: var(--text);
    user-select: none;
    -webkit-font-smoothing: antialiased;
  }
</style>
