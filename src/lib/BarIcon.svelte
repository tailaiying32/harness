<script lang="ts">
  // The panel row's reload and close as small icons, drawn like the panel's
  // view icons (see PanelIcon: 16 × 16, 1.4 stroke, one text line tall), so
  // they sit beside them at the same size instead of as small font glyphs.
  let { name }: { name: "reload" | "close" | "sidebar" | "widen" | "narrow" | "external" | "awake" | "docs" } = $props();
</script>

<svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
  {#if name === "reload"}
    <path d="M14 8a6 6 0 1 1-6-6c1.7 0 3.3.7 4.5 1.8L14 5.3" />
    <path d="M14 1.75V5.3h-3.55" />
  {:else if name === "sidebar"}
    <!-- A window with its left strip ruled off. -->
    <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="1.5" />
    <path d="M6 2.75v10.5" />
  {:else if name === "widen"}
    <!-- Two corners pushed out. -->
    <path d="M9.5 2.25h4.25V6.5M6.5 13.75H2.25V9.5M13.75 2.25L9 7M2.25 13.75L7 9" />
  {:else if name === "narrow"}
    <!-- Two corners pulled in. -->
    <path d="M13.75 6.5H9.5V2.25M2.25 9.5H6.5v4.25M9.5 6.5l4.25-4.25M6.5 9.5l-4.25 4.25" />
  {:else if name === "external"}
    <!-- A box with an arrow leaving it. -->
    <path d="M12.75 9v3.75a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-8.5a1 1 0 0 1 1-1H7" />
    <path d="M9.5 2.25h4.25V6.5M13.75 2.25L7.5 8.5" />
  {:else if name === "docs"}
    <!-- A folder: the docs menu (the docs view's own icon is a page). -->
    <path d="M1.75 4.25a1 1 0 0 1 1-1h3.5l1.5 1.75h5.5a1 1 0 0 1 1 1v6.75a1 1 0 0 1-1 1H2.75a1 1 0 0 1-1-1z" />
  {:else if name === "awake"}
    <!-- A cup with three lines of steam: keep awake. The cup's bottom sits
         on the sidebar icon's bottom line, the steam inside the box's top. -->
    <path d="M1.75 5.75h9.5v4a3.5 3.5 0 0 1-3.5 3.5h-2.5a3.5 3.5 0 0 1-3.5-3.5z" />
    <path d="M11.25 6.75h.75a2.25 2.25 0 0 1 0 4.5h-.75" />
    <path stroke-linecap="round" d="M3.75 2.5v.75M6.5 1.75v1.5M9.25 2.5v.75" />
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
