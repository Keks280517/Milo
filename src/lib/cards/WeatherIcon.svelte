<script lang="ts">
  // Wettersymbol, selbst gezeichnet (keine Bilddateien). Auf Wunsch leicht animiert.
  import { sky } from "../weather";

  interface Props {
    code: number;
    isDay?: boolean;
    size?: number;
    animated?: boolean;
  }
  let { code, isDay = true, size = 24, animated = false }: Props = $props();

  const kind = $derived(sky(code));
  /** Sonne oder Mond sind zu sehen: allein oder hinter einer Wolke. */
  const light = $derived(kind === "clear" || kind === "partly");
  const cloud = $derived(kind !== "clear");
  const wet = $derived(kind === "rain" || kind === "drizzle");
</script>

<svg viewBox="0 0 32 32" width={size} height={size} class:animated aria-hidden="true">
  {#if light}
    <g transform={kind === "partly" ? "translate(5 -3) scale(0.78)" : ""}>
      {#if isDay}
        <g class="rays" stroke="#ffd60a" stroke-width="2" stroke-linecap="round">
          {#each [0, 45, 90, 135, 180, 225, 270, 315] as angle}
            <line x1="16" y1="3.2" x2="16" y2="6.2" transform="rotate({angle} 16 16)" />
          {/each}
        </g>
        <circle cx="16" cy="16" r="6.4" fill="#ffd60a" />
      {:else}
        <path d="M20.5 5.6a10.6 10.6 0 1 0 6 18.4A9.2 9.2 0 0 1 20.5 5.6Z" fill="#f2f2f7" />
      {/if}
    </g>
  {/if}
  {#if cloud}
    <g class="cloud">
      <path
        d="M9.6 24.6a5.1 5.1 0 0 1-.6-10.2 7.1 7.1 0 0 1 13.7-1.5 5.9 5.9 0 0 1 .5 11.7Z"
        fill={kind === "cloudy" || kind === "partly" || kind === "fog" ? "#ffffff" : "#dfe5ee"}
        transform={kind === "partly" ? "translate(-1.5 2.5) scale(0.92)" : wet || kind === "snow" || kind === "storm" ? "translate(0 -3)" : ""}
      />
    </g>
  {/if}
  {#if wet}
    <g class="drops" stroke="#64d2ff" stroke-width="2" stroke-linecap="round">
      {#each kind === "rain" ? [10, 16, 22] : [12, 20] as x, i}
        <line x1={x} y1="24.5" x2={x - 1.4} y2={kind === "rain" ? 29 : 27} style:animation-delay="{i * 0.22}s" />
      {/each}
    </g>
  {:else if kind === "snow"}
    <g class="drops" fill="#ffffff">
      {#each [10, 16, 22] as x, i}
        <circle cx={x} cy={i === 1 ? 28 : 25.6} r="1.5" style:animation-delay="{i * 0.3}s" />
      {/each}
    </g>
  {:else if kind === "storm"}
    <path class="bolt" d="M16.8 20.2l-4.2 5.6h3l-1.4 4.6 5-6.4h-3.1l1.6-3.8Z" fill="#ffd60a" />
  {:else if kind === "fog"}
    <g stroke="#ffffff" stroke-width="2" stroke-linecap="round" opacity="0.75">
      <line x1="7" y1="27.2" x2="25" y2="27.2" />
      <line x1="10" y1="30.4" x2="22" y2="30.4" />
    </g>
  {/if}
</svg>

<style>
  svg {
    display: block;
    flex: none;
    overflow: visible;
  }
  .animated .rays {
    transform-origin: 16px 16px;
    animation: spin 24s linear infinite;
  }
  .animated .cloud {
    animation: drift 7s ease-in-out infinite;
  }
  .animated .drops > * {
    animation: fall 1.1s linear infinite;
  }
  .animated .bolt {
    animation: flash 2.8s ease-in-out infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  @keyframes drift {
    0%, 100% { transform: translateX(-0.7px); }
    50% { transform: translateX(0.7px); }
  }
  @keyframes fall {
    0% { transform: translateY(-2px); opacity: 0; }
    30% { opacity: 1; }
    100% { transform: translateY(3px); opacity: 0; }
  }
  @keyframes flash {
    0%, 78%, 100% { opacity: 1; }
    84% { opacity: 0.25; }
    90% { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .animated * { animation: none !important; }
  }
</style>
