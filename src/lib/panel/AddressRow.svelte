<script lang="ts">
  // The browser's address row, under the top bar's tabs: ↻, the address of
  // the tab on show, and a way out to the user's own browser.
  import BarIcon from "../BarIcon.svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { browserNavigate, browserReload } from "../ipc";
  import type { Chat } from "../state/chat.svelte";

  let { chat }: { chat: Chat } = $props();

  const page = $derived(chat.browser);
  let address = $state("");
  let editing = $state(false);
  /** The click that focused the box; its mouseup would otherwise undo select-all. */
  let focusClick = false;
  /** The last mousedown was that focusing click. */
  let justFocused = false;

  /**
   * Click handling: the first click selects the whole address. A second click
   * places the cursor, even a quick one, which the system counts as a
   * double-click and would turn into a word selection.
   */
  function pressAddress(e: MouseEvent) {
    const box = e.currentTarget as HTMLInputElement;
    if (document.activeElement !== box) {
      focusClick = justFocused = true;
      return;
    }
    if (justFocused && e.detail === 2) {
      e.preventDefault();
      const at = offsetAt(box, e.clientX);
      box.setSelectionRange(at, at);
    }
    justFocused = false;
  }

  /** The text position nearest to `x` in a one-line box, measured with the box's own font. */
  function offsetAt(box: HTMLInputElement, x: number): number {
    const style = getComputedStyle(box);
    const pen = document.createElement("canvas").getContext("2d");
    if (!pen) return box.value.length;
    pen.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    const target = x - box.getBoundingClientRect().left - parseFloat(style.paddingLeft) + box.scrollLeft;
    let before = 0;
    for (let i = 1; i <= box.value.length; i++) {
      const width = pen.measureText(box.value.slice(0, i)).width;
      if (width >= target) return target - before < width - target ? i - 1 : i;
      before = width;
    }
    return box.value.length;
  }

  // A new empty tab puts the cursor in the address box.
  let addressBox = $state<HTMLInputElement>();
  $effect(() => {
    if (chat.focusAddress && addressBox) addressBox.focus();
  });

  // The address follows the page, except while the user types in it.
  $effect(() => {
    const url = page?.url;
    if (!editing) address = url ?? "";
  });

  async function go(e: KeyboardEvent) {
    const box = e.target as HTMLInputElement;
    if (e.key === "Escape") {
      // Used here: it doesn't also end a full screen panel.
      e.preventDefault();
      address = page?.url ?? "";
      box.blur();
      return;
    }
    if (e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    chat.browserProblem = "";
    try {
      await browserNavigate(chat.sessionId, page?.id ?? null, address);
      box.blur();
    } catch (err) {
      chat.browserProblem = String(err);
    }
  }
</script>

<!-- Marked so the tab keys know the browser has the keys (see tabKeys.ts). -->
<div class="row" data-tabs="browser">
  <!-- Before the address, like most browsers. Left out until the tab has
       a page, so an empty tab starts with the address box, not a gap. -->
  {#if page?.url}
    <button
      class="icon reload"
      class:busy={page.loading}
      title="Reload"
      onclick={() => void browserReload(chat.sessionId, page.id).catch(() => {})}
    >
      <BarIcon name="reload" />
    </button>
  {/if}
  <!-- One click selects the whole address, ready to type over or copy. -->
  <input
    class="address"
    bind:this={addressBox}
    bind:value={address}
    onmousedown={pressAddress}
    onmouseup={(e) => {
      if (focusClick) e.preventDefault();
      focusClick = false;
    }}
    onfocus={(e) => {
      editing = true;
      e.currentTarget.select();
    }}
    onblur={() => (editing = false)}
    onkeydown={go}
    spellcheck="false"
    autocomplete="off"
    placeholder="Type a url"
  />
    <button
    class="icon"
    class:off={!page?.url}
    title="Open in your browser"
    onclick={() => page?.url && void openUrl(page.url).catch(() => {})}
  >
    <BarIcon name="external" />
  </button>
</div>

<style>
  .row {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 2ch;
    padding: calc(var(--gap) * 0.5) var(--pad-x);
    border-bottom: 1px solid var(--line);
    color: var(--faint);
  }
  button:hover {
    color: var(--fg);
  }
  .icon {
    display: flex;
  }
  .icon.off {
    visibility: hidden;
  }
  /* Turns while the page loads. */
  .reload.busy :global(svg) {
    animation: turn 0.9s linear infinite;
  }
  @keyframes turn {
    to {
      transform: rotate(360deg);
    }
  }
  /* Fills the row up to the buttons, so anywhere there is clickable. A long
     address is cut short with an ellipsis. */
  .address {
    flex: 1;
    min-width: 12ch;
    text-overflow: ellipsis;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--dim);
    caret-color: var(--accent);
  }
  .address:focus {
    color: var(--bright);
  }
  .address::placeholder {
    color: var(--faint);
  }
</style>
