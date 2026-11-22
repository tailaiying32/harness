<script lang="ts">
  // The theme picker in the top bar: a menu drawn like the app's own, never a
  // native one. It stays open after a pick, so themes can be tried in a row.
  import { themes, type Theme } from "../src/lib/themeData";
  import { manualPick, pick, shownTheme, watchTheme } from "./look";

  let { quiet = false }: { quiet?: boolean } = $props();

  let shown = $state(shownTheme().id);
  let auto = $state(manualPick() === null);
  watchTheme((theme) => {
    shown = theme.id;
    auto = manualPick() === null;
  });
  let open = $state(false);
  let root: HTMLElement;

  const label = (t: Theme) => t.name.toLowerCase();
  const current = $derived(themes.find((t) => t.id === shown) ?? themes[0]);

  function outside(e: PointerEvent) {
    if (open && !root.contains(e.target as Node)) open = false;
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") open = false;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={key} onblur={() => (open = false)} />

<span class="theme" class:quiet bind:this={root}>
  <button class:on={open} aria-haspopup="menu" aria-expanded={open} onclick={() => (open = !open)}>
    {label(current)}{#if !quiet}{" "}<span class="caret">{open ? "▴" : "▾"}</span>{/if}
  </button>
  {#if open}
    <div class="menu" role="menu">
      <button role="menuitemradio" aria-checked={auto} class:picked={auto} onclick={() => pick("auto")}>
        <span class="swatch split"></span>
        auto (system)
      </button>
      {#each themes as t (t.id)}
        <button role="menuitemradio" aria-checked={!auto && t.id === shown} class:picked={!auto && t.id === shown} onclick={() => pick(t.id)}>
          <span class="swatch" style:background={t.bg} style:border-color={t.faint}><span style:background={t.accent}></span></span>
          {label(t)}
        </button>
      {/each}
    </div>
  {/if}
</span>

<style>
  .theme {
    position: relative;
  }
  .theme > button {
    color: var(--dim);
  }
  .theme.quiet > button {
    color: var(--faint);
  }
  .theme > button:hover,
  .theme > button.on {
    color: var(--fg);
  }
  .theme.quiet > button:hover,
  .theme.quiet > button.on {
    color: var(--dim);
  }
  .caret {
    color: var(--faint);
  }
  /* As the app's menus: a tinted box with a hairline edge, rows of plain text. */
  .menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    max-height: 70vh;
    overflow-y: auto;
    padding: 6px 0;
    background: var(--panel);
    border: 1px solid var(--line);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 1.5ch;
    padding: 0 2ch;
    color: var(--dim);
    text-align: left;
    white-space: nowrap;
  }
  .menu button:hover {
    color: var(--fg);
  }
  .menu button.picked {
    color: var(--accent);
  }
  /* A theme at a glance: its background, with a block of its accent. */
  .swatch {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 2.4ch;
    height: 1em;
    border: 1px solid;
  }
  .swatch span {
    width: 1ch;
    height: 0.45em;
  }
  .swatch.split {
    background: linear-gradient(90deg, #111210 50%, #fcfcfb 50%);
    border-color: var(--faint);
  }
</style>
