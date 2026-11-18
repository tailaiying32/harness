<script lang="ts">
  // The find box (see find.ts), at the top right of the view it searches.
  // Enter goes to the next match, Shift+Enter back; Escape closes it.
  import { onDestroy, tick } from "svelte";
  import { addSpot, closed, opened, type Searcher, type Spot } from "./find";

  let {
    root,
    searcher,
    main = false,
    onopen,
    onclose,
  }: {
    /** The view the box belongs to: Cmd+F after a click in it opens this box. */
    root: HTMLElement | undefined;
    /** What to search, asked each time the box opens. */
    searcher: () => Searcher | null;
    main?: boolean;
    /** Runs before searching, e.g. to draw a chat's earlier messages. */
    onopen?: () => Promise<void> | void;
    onclose?: () => void;
  } = $props();

  let open = $state(false);
  let query = $state("");
  let count = $state(0);
  let index = $state(0);
  let input = $state<HTMLInputElement>();
  let using: Searcher | null = null;
  let unwatch: (() => void) | undefined;
  /** Where the cursor was, to go back to on close. */
  let back: HTMLElement | null = null;

  const spot: Spot = { root: null as unknown as HTMLElement, open: show, close };

  $effect(() => {
    if (!root) return;
    spot.root = root;
    spot.main = main;
    return addSpot(spot);
  });

  async function show() {
    opened(spot);
    if (!open) {
      const active = document.activeElement;
      back = active instanceof HTMLElement && active !== document.body ? active : null;
      open = true;
      await onopen?.();
      using = searcher();
      unwatch = using?.watch?.(() => run(false));
      if (query) run(true);
    }
    await tick();
    input?.focus();
    input?.select();
  }

  /** Searches again; `jump` moves to the first match, otherwise the place is kept. */
  function run(jump: boolean) {
    if (!using) return;
    count = using.search(query);
    index = jump ? 0 : Math.min(index, Math.max(0, count - 1));
    if (count) using.show(index);
  }

  function step(by: number) {
    if (!count || !using) return;
    index = (index + by + count) % count;
    using.show(index);
  }

  function close() {
    if (!open) return;
    const handled = count > 0 && using?.finish?.(index);
    unwatch?.();
    unwatch = undefined;
    using?.clear();
    using = null;
    open = false;
    count = 0;
    closed(spot);
    onclose?.();
    if (!handled && back?.isConnected) back.focus({ preventScroll: true });
    back = null;
  }

  function onKey(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === "Enter" || (mod && e.key.toLowerCase() === "g")) {
      e.preventDefault();
      step(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  onDestroy(() => {
    unwatch?.();
    using?.clear();
    closed(spot);
  });
</script>

{#if open}
  <div class="find-bar">
    <input bind:this={input} bind:value={query} oninput={() => run(true)} onkeydown={onKey} placeholder="find" spellcheck="false" />
    <span class="count" class:none={query && !count}>{query ? (count ? `${index + 1}/${count}` : "0/0") : ""}</span>
    <button title="Previous (Shift+Enter)" disabled={!count} onclick={() => step(-1)}>↑</button>
    <button title="Next (Enter)" disabled={!count} onclick={() => step(1)}>↓</button>
    <button title="Close (Escape)" onclick={close}>×</button>
  </div>
{/if}

<style>
  .find-bar {
    position: absolute;
    /* At the top of the view, its right edge on the text's right edge (the
       view says where that is), not out by the window's corner. */
    top: calc(var(--pad-y) / 2);
    right: var(--find-right, var(--pad-x));
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 0.5ch;
    padding: 2px 4px 2px 1ch;
    background: var(--bg);
    border: 1px solid var(--line);
  }
  input {
    width: 18ch;
    background: none;
    border: none;
    outline: none;
    color: var(--fg);
    font: inherit;
    padding: 0;
  }
  input::placeholder {
    color: var(--faint);
  }
  .count {
    min-width: 5ch;
    text-align: right;
    color: var(--faint);
  }
  .count.none {
    color: var(--bad);
  }
  button {
    color: var(--dim);
    padding: 0 0.5ch;
  }
  button:hover:not(:disabled) {
    color: var(--fg);
  }
  button:disabled {
    color: var(--faint);
  }
</style>
