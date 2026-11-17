<script lang="ts">
  // A list of picks that opens above the message box while a word is typed
  // (`@` for a chat, `/` for a command). The best is at the bottom, next to
  // the box, and the list goes up from there: Up moves away from the box,
  // Down back, stopping at the ends. Rows are drawn a batch at a time as the
  // highlight or a scroll nears the top. The box keeps the keys; this draws
  // the list and follows the pointer.
  import { tick } from "svelte";

  type Row = { key: string; label: string; detail?: string };

  let {
    rows,
    row = $bindable(0),
    onPick,
    label,
    wide = false,
  }: {
    /** Best first. */
    rows: Row[];
    /** The highlighted row: 0 is the best. */
    row: number;
    onPick: (i: number) => void;
    label: string;
    /** Rows have a detail after the label; the list takes the box's width. */
    wide?: boolean;
  } = $props();

  const BATCH = 30;
  let shown = $state(BATCH);
  let list = $state<HTMLElement>();
  const drawn = $derived(rows.slice(0, shown));

  /** The most rows in view at once; more scroll. */
  const VISIBLE = 8;

  // A new list (something was typed) starts from the bottom again. Only
  // when the rows themselves change: the same rows given again leave the
  // list where it is.
  const keys = $derived(rows.map((r) => r.key).join("\n"));
  $effect(() => {
    void keys;
    shown = BATCH;
    void tick().then(() => {
      if (list) list.scrollTop = list.scrollHeight;
    });
  });

  // Exactly VISIBLE rows tall when there are more, whatever the text size.
  $effect(() => {
    void drawn.length;
    const first = list?.querySelector("button");
    if (!list || !first) return;
    const pad = parseFloat(getComputedStyle(list).paddingTop) + parseFloat(getComputedStyle(list).paddingBottom);
    const border = list.offsetHeight - list.clientHeight;
    list.style.maxHeight = `${first.offsetHeight * VISIBLE + pad + border}px`;
  });

  /** Draws the next batch above the others, keeping the rows in view still. */
  async function more() {
    if (!list || shown >= rows.length) return;
    const before = list.scrollHeight;
    shown += BATCH;
    await tick();
    list.scrollTop += list.scrollHeight - before;
  }

  // The highlight stays in view, and nearing the last drawn row draws more.
  $effect(() => {
    const at = Math.min(row, drawn.length - 1);
    if (at >= shown - 5) void more();
    // The best row takes the list to its very bottom, padding and all.
    if (at <= 0 && list) list.scrollTop = list.scrollHeight;
    else list?.querySelector(`[data-i="${at}"]`)?.scrollIntoView({ block: "nearest" });
  });

  function onScroll() {
    const first = list?.firstElementChild as HTMLElement | null;
    if (list && list.scrollTop < (first?.offsetHeight ?? 0) * 2) void more();
  }

  // Scrolling with the pointer still moves the rows under it; the row it
  // lands on is the highlighted one, as a move would make it. A wheel or
  // trackpad scroll only: the list scrolling itself to follow the keys
  // leaves the highlight where the keys put it.
  let pointer: { x: number; y: number } | null = null;
  function onWheel() {
    requestAnimationFrame(() => {
      if (!pointer) return;
      const under = document.elementFromPoint(pointer.x, pointer.y)?.closest<HTMLElement>("[data-i]");
      if (under && list?.contains(under)) row = Number(under.dataset.i);
    });
  }
</script>

<!-- Pressing a row keeps the cursor in the box: the pick is typed in there. -->
<!-- svelte-ignore a11y_interactive_supports_focus: the box keeps the focus and the keys -->
<div
  class="menu"
  class:wide
  role="listbox"
  aria-label={label}
  bind:this={list}
  onscroll={onScroll}
  onwheel={onWheel}
  onpointermove={(e) => (pointer = { x: e.clientX, y: e.clientY })}
  onpointerleave={() => (pointer = null)}
>
  {#each drawn as _, j (drawn[drawn.length - 1 - j].key)}
    {@const i = drawn.length - 1 - j}
    {@const r = drawn[i]}
    <button
      role="option"
      data-i={i}
      aria-selected={i === row}
      class:on={i === row}
      onpointermove={(e) => {
        // Only a real move: the web view also sends moves with the pointer
        // still, when rows scroll or change under it, which would pull the
        // highlight back from where the keys put it. (The list's own handler
        // records the position after this one.)
        if (!pointer || (pointer.x === e.clientX && pointer.y === e.clientY)) return;
        if (row !== i) row = i;
      }}
      onpointerdown={(e) => {
        e.preventDefault();
        onPick(i);
      }}
    >
      <span class="label">{r.label}</span>{#if r.detail}<span class="detail">{r.detail}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .menu {
    box-sizing: border-box;
    position: absolute;
    left: 0;
    bottom: calc(100% + var(--gap) * 0.4);
    z-index: 10;
    display: flex;
    flex-direction: column;
    max-width: 100%;
    overflow-y: auto;
    padding: calc(var(--gap) * 0.4) 0;
    background: var(--panel);
    border: 1px solid var(--line);
  }
  .menu.wide {
    width: 100%;
  }
  button {
    display: flex;
    gap: 2ch;
    flex-shrink: 0;
    min-width: 0;
    text-align: left;
    padding: 0 2ch;
    color: var(--dim);
    white-space: nowrap;
  }
  button.on {
    color: var(--accent);
  }
  .label {
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .detail {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--faint);
  }
</style>
