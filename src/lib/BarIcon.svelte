<script lang="ts">
  // The panel row's reload and close as small icons, drawn like the panel's
  // view icons (see PanelIcon: 16 × 16, 1.4 stroke, one text line tall), so
  // they sit beside them at the same size instead of as small font glyphs.
  let { name }: { name: "reload" | "close" } = $props();
</script>

<svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
  {#if name === "reload"}
    <path d="M14 8a6 6 0 1 1-6-6c1.7 0 3.3.7 4.5 1.8L14 5.3" />
    <path d="M14 1.75V5.3h-3.55" />
  {:else}
    <path d="M3.25 3.25l9.5 9.5M12.75 3.25l-9.5 9.5" />
  {/if}
</svg>

<style>
  .icon {
    width: 1em;
    height: 1em;
    flex-shrink: 0;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }
</style>
