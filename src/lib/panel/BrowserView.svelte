<script lang="ts">
  // The panel's browser view: a tab for the page, an address line, and an
  // empty space the page itself sits over. The page is a separate web view the
  // core holds (one per chat, localhost only), placed over `slot` whenever
  // this view is on screen and nothing is drawn over it; otherwise the core
  // parks it out of sight at the same size, so Claude still sees it as shown.
  import { onDestroy } from "svelte";
  import { browserNavigate, browserPlace, browserReload, type BrowserRect } from "../ipc";
  import type { Chat } from "../state/chat.svelte";

  let { chat, covered }: { chat: Chat; covered: boolean } = $props();

  const page = $derived(chat.browser);
  let address = $state("");
  let editing = $state(false);
  let problem = $state("");
  let slot = $state<HTMLElement>();

  // The address line follows the page, except while the user types in it.
  $effect(() => {
    const url = page?.url;
    if (!editing && url !== undefined) address = url;
  });

  /** The tab's name: the page's title, or its host and port. */
  const name = $derived.by(() => {
    if (!page) return "browser";
    if (page.title) return page.title;
    try {
      return new URL(page.url).host;
    } catch {
      return page.url;
    }
  });

  async function go(e: KeyboardEvent) {
    if (e.key === "Escape") {
      address = page?.url ?? "";
      (e.target as HTMLInputElement).blur();
      return;
    }
    if (e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    problem = "";
    try {
      await browserNavigate(chat.sessionId, address);
      (e.target as HTMLInputElement).blur();
    } catch (err) {
      problem = String(err);
    }
  }

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

<div class="bar">
  <div class="tabs">
    <span class="tab on">
      <button class="name" title={page?.url ?? ""}>{name}</button>
      <button class="shut" title="Close the browser" onclick={() => chat.closeBrowser()}>×</button>
    </span>
  </div>
</div>
<div class="address">
  <input
    bind:value={address}
    onfocus={(e) => {
      editing = true;
      (e.target as HTMLInputElement).select();
    }}
    onblur={() => (editing = false)}
    onkeydown={go}
    spellcheck="false"
    autocomplete="off"
    placeholder="localhost:5173"
  />
  {#if page}
    <button title="Reload" onclick={() => void browserReload(chat.sessionId).catch(() => {})}>
      {page.loading ? "…" : "↻"}
    </button>
  {/if}
</div>
{#if problem}
  <div class="problem">{problem}</div>
{/if}
<div class="slot" bind:this={slot}></div>

<style>
  .bar {
    display: flex;
    gap: 3ch;
    padding: calc(var(--pad-y) * 0.6) var(--pad-x) calc(var(--gap) * 0.7);
    border-bottom: 1px solid var(--line);
    color: var(--faint);
    white-space: nowrap;
  }
  .tabs {
    display: flex;
    gap: 3ch;
    min-width: 0;
    flex: 1;
  }
  .tab {
    display: flex;
    gap: 1.5ch;
    min-width: 0;
  }
  .bar button:hover {
    color: var(--fg);
  }
  .name {
    max-width: 40ch;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--bright);
  }
  .shut {
    visibility: hidden;
  }
  .tab:hover .shut,
  .tab.on .shut {
    visibility: visible;
  }
  .address {
    display: flex;
    gap: 2ch;
    padding: calc(var(--gap) * 0.5) var(--pad-x);
    border-bottom: 1px solid var(--line);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--dim);
    caret-color: var(--accent);
  }
  input:focus {
    color: var(--bright);
  }
  input::placeholder {
    color: var(--faint);
  }
  .address button {
    color: var(--faint);
  }
  .address button:hover {
    color: var(--fg);
  }
  .problem {
    padding: calc(var(--gap) * 0.5) var(--pad-x);
    color: var(--bad);
  }
  /* The page sits over this space. */
  .slot {
    flex: 1;
    min-height: 0;
  }
</style>
