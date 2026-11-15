<script lang="ts">
  // Lebendiger Leer-Zustand: ein kleines animiertes Bild in der Farbe der Ansicht, ein Satz und
  // auf Wunsch Knoepfe. Ersetzt die grauen Einzeiler wie „Keine Downloads.“.
  import type { Snippet } from "svelte";

  type Kind = "files" | "downloads" | "weather" | "system" | "claude" | "notes" | "music" | "timer" | "agenda";
  interface Props {
    kind: Kind;
    title: string;
    text?: string;
    /** Kompakt fuer Karten der Uebersicht. */
    small?: boolean;
    children?: Snippet;
  }
  let { kind, title, text = "", small = false, children }: Props = $props();

  const HUE: Record<Kind, string> = {
    files: "#5ac8fa", downloads: "#64d2ff", weather: "#7fb6ff", system: "#30d158", claude: "#f5a46f",
    notes: "#ffd60a", music: "#ff6fae", timer: "#ff9f0a", agenda: "#7d7aff",
  };
</script>

<div class="empty" class:small style:--hue={HUE[kind]}>
  <div class="art {kind}" aria-hidden="true">
    {#if kind === "music"}
      <span class="vinyl"><i></i></span>
    {:else if kind === "timer"}
      <span class="dial"><i class="hand"></i></span>
    {:else if kind === "weather"}
      <span class="sun"></span><span class="cloud"></span>
    {:else if kind === "claude"}
      <span class="buddy"><i></i><i></i></span><span class="z">z</span><span class="z two">z</span>
    {:else if kind === "system"}
      <span class="bars"><i></i><i></i><i></i><i></i></span>
    {:else if kind === "agenda"}
      <span class="cal"><b></b><i></i></span>
    {:else if kind === "notes"}
      <span class="paper"><i></i><i></i><i></i></span>
    {:else}
      <span class="folder"><i></i></span>
    {/if}
  </div>
  <strong>{title}</strong>
  {#if text}<p>{text}</p>{/if}
  {#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    max-width: 380px;
    text-align: center;
  }
  .empty.small {
    margin: 0;
    gap: 4px;
    align-items: flex-start;
    text-align: left;
  }
  strong {
    font-size: var(--fs-callout);
    font-weight: var(--fw-semibold);
    color: var(--text);
  }
  .small strong {
    font-size: var(--fs-callout);
  }
  p {
    margin: 0;
    font-size: var(--fs-footnote);
    line-height: 1.5;
    color: var(--text-3);
  }
  .actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: center;
    margin-top: 4px;
  }
  .small .actions {
    justify-content: flex-start;
  }
  .art {
    position: relative;
    width: 72px;
    height: 72px;
    display: grid;
    place-items: center;
  }
  .small .art {
    width: 44px;
    height: 44px;
  }
  .art > span {
    position: relative;
  }
  /* Platte, die sich langsam dreht */
  .vinyl {
    width: 70%;
    height: 70%;
    border-radius: 50%;
    background: radial-gradient(circle, var(--hue) 0 16%, #111 17% 19%, transparent 20%), repeating-radial-gradient(circle, #121214 0 1.4px, #1f1f22 1.4px 2.8px);
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.5);
    animation: spin 6s linear infinite;
  }
  .vinyl i {
    position: absolute;
    top: 12%;
    left: 48%;
    width: 4%;
    height: 22%;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.25);
  }
  /* Zifferblatt mit kreisendem Zeiger */
  .dial {
    width: 66%;
    height: 66%;
    border-radius: 50%;
    border: 3px solid color-mix(in srgb, var(--hue) 80%, #fff);
  }
  .hand {
    position: absolute;
    left: 50%;
    top: 14%;
    width: 3px;
    height: 36%;
    margin-left: -1.5px;
    border-radius: 2px;
    background: #fff;
    transform-origin: 50% 100%;
    animation: spin 4s linear infinite;
  }
  /* Sonne hinter ziehender Wolke */
  .sun {
    position: absolute !important;
    width: 42%;
    height: 42%;
    top: 16%;
    right: 16%;
    border-radius: 50%;
    background: radial-gradient(circle at 40% 35%, #fff6c9, var(--warn) 60%, #ff9f0a);
  }
  .cloud {
    position: absolute !important;
    width: 62%;
    height: 30%;
    bottom: 22%;
    left: 12%;
    border-radius: var(--r-lg);
    background: linear-gradient(180deg, #fff, #cfd8e6);
    animation: drift 4s ease-in-out infinite;
  }
  .cloud::before {
    content: "";
    position: absolute;
    width: 50%;
    height: 120%;
    left: 18%;
    bottom: 40%;
    border-radius: 50%;
    background: #fff;
  }
  @keyframes drift {
    0%, 100% { transform: translateX(-4px); }
    50% { transform: translateX(6px); }
  }
  /* Schlafendes Claude-Wuerfelchen */
  .buddy {
    width: 52%;
    height: 46%;
    border-radius: var(--r-sm);
    background: linear-gradient(180deg, #f8b892, #e7895a);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 22%;
    animation: snooze 3s ease-in-out infinite;
  }
  .buddy i {
    width: 18%;
    height: 3px;
    border-radius: 2px;
    background: #3a2216;
  }
  .z {
    position: absolute !important;
    right: 8%;
    top: 4%;
    font-size: var(--fs-body);
    font-weight: var(--fw-bold);
    color: var(--hue);
    animation: float 3s ease-in-out infinite;
  }
  .z.two {
    right: 0;
    top: -8%;
    font-size: var(--fs-caption);
    animation-delay: 1.4s;
  }
  @keyframes snooze {
    0%, 100% { transform: scaleY(1); }
    50% { transform: scaleY(0.94) translateY(1px); }
  }
  @keyframes float {
    0% { transform: translateY(6px); opacity: 0; }
    40% { opacity: 1; }
    100% { transform: translateY(-10px); opacity: 0; }
  }
  /* Balken, die sanft auf und ab gehen */
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 4px;
    height: 50%;
  }
  .bars i {
    width: 7px;
    height: 100%;
    border-radius: 3px;
    background: var(--hue);
    transform-origin: bottom;
    animation: level 1.6s ease-in-out infinite;
  }
  .bars i:nth-child(2) { animation-delay: -0.4s; }
  .bars i:nth-child(3) { animation-delay: -0.9s; }
  .bars i:nth-child(4) { animation-delay: -1.2s; }
  @keyframes level {
    0%, 100% { transform: scaleY(0.35); }
    50% { transform: scaleY(1); }
  }
  .cal {
    width: 56%;
    height: 56%;
    border-radius: var(--r-sm);
    background: var(--text);
    overflow: hidden;
  }
  .cal b {
    display: block;
    height: 30%;
    background: var(--hue);
  }
  .cal i {
    position: absolute;
    left: 30%;
    top: 46%;
    width: 40%;
    height: 34%;
    border-radius: var(--r-xs);
    background: color-mix(in srgb, var(--hue) 35%, #fff);
  }
  .paper {
    width: 50%;
    height: 62%;
    border-radius: var(--r-xs);
    background: #fffbe6;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 14%;
    padding: 0 16%;
    box-sizing: border-box;
    transform: rotate(-6deg);
  }
  .paper i {
    height: 2px;
    border-radius: 1px;
    background: #c9b25a;
  }
  .folder {
    width: 60%;
    height: 44%;
    border-radius: 4px 8px 8px 8px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--hue) 80%, #fff), var(--hue));
  }
  .folder::before {
    content: "";
    position: absolute;
    left: 0;
    top: -16%;
    width: 42%;
    height: 22%;
    border-radius: 4px 4px 0 0;
    background: color-mix(in srgb, var(--hue) 70%, #fff);
  }
  .folder i {
    position: absolute;
    left: 50%;
    top: -60%;
    width: 30%;
    height: 50%;
    margin-left: -15%;
    border-radius: 3px;
    background: #fff;
    animation: drop 2.4s ease-in-out infinite;
  }
  @keyframes drop {
    0% { transform: translateY(-6px); opacity: 0; }
    30% { opacity: 1; }
    70% { transform: translateY(10px); opacity: 1; }
    100% { transform: translateY(14px); opacity: 0; }
  }
  @keyframes bob {
    0%, 100% { transform: translateY(0) rotate(var(--r, 0deg)); }
    50% { transform: translateY(-3px) rotate(var(--r, 0deg)); }
  }
  .paper {
    --r: -6deg;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .art * { animation: none !important; }
  }
</style>
