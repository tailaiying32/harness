<script lang="ts">
  // The bar along the top of the chat and panel. On a Mac it sits in the window's
  // title bar, beside the window buttons, so it costs no room. The chat's title
  // on the left; over the open panel, its views and ▸ to hide it; with the
  // panel closed, only ◂ to bring it back. Its empty space drags the window.
  import type { AppState } from "./state/app.svelte";

  let { app }: { app: AppState } = $props();

  const chat = $derived(app.current);
  const title = $derived(
    app.projects.flatMap((p) => p.sessions).find((s) => s.id === chat.sessionId)?.title ?? "",
  );
  const open = $derived(chat.panelOpen && chat.panelHasContent);

  /**
   * Brings the panel back on the view it had, or one that still has something;
   * with nothing anywhere, on the browser's empty address line.
   */
  function reopen() {
    const has = { docs: chat.tabCount > 0, side: chat.side !== null, browser: chat.browserOn };
    if (!has[chat.panelView]) {
      const next = (["docs", "side", "browser"] as const).find((view) => has[view]);
      if (!next) return chat.openBrowser();
      chat.panelView = next;
    }
    chat.panelOpen = true;
  }
</script>

<!-- Laid out like the chat and panel below it (the chat takes the rest, the
     panel --panel-w and no less), so the panel's dividing line runs straight up
     through the bar. -->
<header class="bar" data-tauri-drag-region>
  <span class="title" data-tauri-drag-region>{title}</span>
  <!-- Sized exactly like the panel (its width plus a 1px left line, no padding);
       the padding is inside. -->
  <span class="views" class:open>
    <span class="inner" data-tauri-drag-region>
    {#if open}
      {#if chat.tabCount > 0}
        <button class:on={chat.panelView === "docs"} onclick={() => chat.showView("docs")}>
          {#if chat.planPending}<span class="accent">●</span>{/if}
          docs
        </button>
      {/if}
      <button class:on={chat.panelView === "side"} onclick={() => chat.showView("side")}>side chat</button>
      <button class:on={chat.panelView === "browser"} onclick={() => chat.showView("browser")}>browser</button>
      <span class="spacer" data-tauri-drag-region></span>
      <button title="Hide the panel" onclick={() => (chat.panelOpen = false)}>▸</button>
    {:else}
      <!-- Closed: one mark brings the panel back. (A plan opens the panel by
           itself; one left waiting after hiding it shows in the chat, as does
           a page Claude opened.) With nothing in it, the mark opens the
           browser's address line. -->
      <button title="Show the panel" onclick={reopen}>◂</button>
    {/if}
    </span>
  </span>
</header>

<style>
  .bar {
    height: var(--bar);
    flex-shrink: 0;
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--line);
    color: var(--faint);
    white-space: nowrap;
    user-select: none;
    -webkit-user-select: none;
  }
  .title {
    flex: 1;
    min-width: 0;
    line-height: var(--bar);
    padding: 0 var(--pad-x);
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--dim);
  }
  .views {
    flex-shrink: 0;
    display: flex;
  }
  .inner {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 3ch;
    padding: 0 var(--pad-x);
  }
  /* Over the open panel: the panel's width, edge and dividing line. */
  .views.open {
    width: var(--panel-w, 46%);
    min-width: 0;
    border-left: 1px solid var(--line);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  /* Too narrow for both: the panel takes the chat's place (see App.svelte). */
  @media (max-width: 1100px) {
    .views.open {
      width: auto;
      border-left: 0;
    }
  }
  button:hover {
    color: var(--fg);
  }
  button.on {
    color: var(--bright);
  }
  .accent {
    color: var(--accent);
  }
</style>
