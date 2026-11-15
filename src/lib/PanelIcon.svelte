<script lang="ts">
  // The side panel's views as small icons: a document, a chat bubble, a globe.
  // Drawn like the branch mark (16 × 16, 1.4 stroke, one text line tall), so
  // they look the same in every font and take the colour of the text around them.
  import type { PanelView } from "./state/chat.svelte";

  let { view }: { view: PanelView } = $props();
</script>

<svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
  {#if view === "docs"}
    <path d="M9.5 1.75H4.25a1 1 0 0 0-1 1v10.5a1 1 0 0 0 1 1h7.5a1 1 0 0 0 1-1V5z" />
    <path d="M9.5 1.75V5h3.25M5.75 8.25h4.5M5.75 10.75h4.5" />
  {:else if view === "side"}
    <path d="M2.25 3.75a1 1 0 0 1 1-1h9.5a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1H6.5l-3 2.5v-2.5h-.25a1 1 0 0 1-1-1z" />
  {:else}
    <circle cx="8" cy="8" r="6" />
    <path d="M2 8h12M8 2c1.7 1.7 2.5 3.8 2.5 6S9.7 12.3 8 14M8 2C6.3 3.7 5.5 5.8 5.5 8s.8 4.3 2.5 6" />
  {/if}
</svg>

<style>
  .icon {
    width: 1em;
    height: 1em;
    flex-shrink: 0;
    vertical-align: -0.15em;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }
</style>
