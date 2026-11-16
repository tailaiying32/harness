<script lang="ts">
  // The side panel's views as small icons: a document, a chat bubble, a globe,
  // a terminal window, and for chat info a small tree: the chat and the work
  // it handed out.
  // Drawn like the branch mark (16 × 16, 1.4 stroke, one text line tall), so
  // they look the same in every font and take the colour of the text around them.
  // Each fills the box about as fully as the branch mark does (roughly 1.5 to
  // 14.5 each way); drawn smaller, they read as smaller icons beside it.
  import type { PanelView } from "./state/chat.svelte";

  let { view }: { view: PanelView } = $props();
</script>

<svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
  {#if view === "docs"}
    <path d="M9.75 1.5H3.75a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h8.5a1 1 0 0 0 1-1V5z" />
    <path d="M9.75 1.5V5h3.5M5.25 8.5h5.5M5.25 11.25h5.5" />
  {:else if view === "side"}
    <path d="M1.75 2.75a1 1 0 0 1 1-1h10.5a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H6.75l-3.5 2.75v-2.75h-.5a1 1 0 0 1-1-1z" />
  {:else if view === "terminal"}
    <rect x="1.5" y="2.25" width="13" height="11.5" rx="1" />
    <path d="M4.5 5.75l2.5 2.25-2.5 2.25M8.75 10.5h3" />
  {:else if view === "info"}
    <!-- Rounded elbows, no sharp corners; the two children well apart. -->
    <circle cx="3.75" cy="3.25" r="1.75" />
    <path class="round" d="M3.75 5V10.25a2.5 2.5 0 0 0 2.5 2.5h4M3.75 5a2.25 2.25 0 0 0 2.25 2.25h4.25" />
    <circle cx="12.5" cy="7.25" r="1.75" />
    <circle cx="12.5" cy="12.75" r="1.75" />
  {:else}
    <circle cx="8" cy="8" r="6.25" />
    <path d="M1.75 8h12.5M8 1.75c1.75 1.75 2.6 3.9 2.6 6.25S9.75 12.5 8 14.25M8 1.75C6.25 3.5 5.4 5.65 5.4 8s.85 4.5 2.6 6.25" />
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
  .round {
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
