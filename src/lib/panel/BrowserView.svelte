<script lang="ts">
  // The panel's browser view: the address row, then the space the page itself
  // sits over. Its tabs are in the top bar (see PanelBar). The tab on show is a
  // separate web view the core holds (localhost only), placed over `slot` (or at its
  // top, shrunk, when Claude gave it a window size) whenever
  // this view is on screen and nothing is drawn over it; otherwise the core
  // parks it out of sight at the same size, so Claude still sees it as shown.
  import { onDestroy } from "svelte";
  import { browserPlace, type BrowserRect } from "../ipc";
  import AddressRow from "./AddressRow.svelte";
  import type { Chat } from "../state/chat.svelte";

  let { chat, covered }: { chat: Chat; covered: boolean } = $props();

  const page = $derived(chat.browser);
  let slot = $state<HTMLElement>();

  // ---- Placing the page over the slot ----
  const visible = $derived(!covered && page !== null);
  let frame = 0;
  let last = "";

  function place() {
    frame = 0;
    if (!slot || !page) return;
    const r = slot.getBoundingClientRect();
    const rect: BrowserRect = { x: r.left, y: r.top, width: r.width, height: r.height };
    const key = JSON.stringify([rect, visible]);
    if (key === last) return;
    last = key;
    void browserPlace(chat.sessionId, rect, visible).catch(() => {});
  }

  /** At most once a frame, however often the layout moves. */
  function schedule() {
    if (!frame) frame = requestAnimationFrame(place);
  }

  $effect(() => {
    // Re-place when the page appears or something is drawn over it.
    void visible;
    void page;
    schedule();
  });

  $effect(() => {
    if (!slot) return;
    const watch = new ResizeObserver(schedule);
    watch.observe(slot);
    // The slot can move without resizing (the panel's width drags its left edge).
    watch.observe(document.documentElement);
    window.addEventListener("resize", schedule);
    return () => {
      watch.disconnect();
      window.removeEventListener("resize", schedule);
    };
  });

  // Leaving the view (another view, another chat, the panel hidden): park the page.
  onDestroy(() => {
    if (frame) cancelAnimationFrame(frame);
    if (!slot || !last) return;
    const r = slot.getBoundingClientRect();
    void browserPlace(chat.sessionId, { x: r.left, y: r.top, width: r.width, height: r.height }, false).catch(() => {});
  });
</script>

<AddressRow {chat} />
<!-- An address that couldn't be opened, typed above. -->
{#if chat.browserProblem}
  <div class="problem">{chat.browserProblem}</div>
{/if}
<div class="slot" bind:this={slot}></div>

<style>
  .problem {
    padding: calc(var(--gap) * 0.5) var(--pad-x);
    border-bottom: 1px solid var(--line);
    color: var(--bad);
  }
  /* The page sits over this space: the whole panel. */
  .slot {
    flex: 1;
    min-height: 0;
  }
</style>
