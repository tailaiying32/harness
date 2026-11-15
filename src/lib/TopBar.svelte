<script lang="ts">
  // The bar along the top of the chat and panel. On a Mac it sits in the window's
  // title bar, beside the window buttons, so it costs no room. Over the chat:
  // its title, and the panel's three views as icons, pushed against the panel's
  // line. Clicking a view opens the panel on it; clicking the one on show hides
  // the panel. Over the open panel: that view's own row (see PanelBar), so the
  // panel has none of its own. Empty space drags the window.
  import PanelIcon from "./PanelIcon.svelte";
  import PanelBar from "./panel/PanelBar.svelte";
  import type { AppState } from "./state/app.svelte";
  import type { PanelView } from "./state/chat.svelte";

  let { app }: { app: AppState } = $props();

  const chat = $derived(app.current);
  const title = $derived(
    app.projects.flatMap((p) => p.sessions).find((s) => s.id === chat.sessionId)?.title ?? "",
  );
  const open = $derived(chat.panelOpen && chat.panelHasContent);

  const views: { view: PanelView; name: string }[] = [
    { view: "docs", name: "docs" },
    { view: "side", name: "side chat" },
    { view: "browser", name: "browser" },
  ];

  /** Docs has nothing to show until Claude writes a plan or a document, or a link opens one. */
  const empty = (view: PanelView) => view === "docs" && chat.tabCount === 0;

  function pick(view: PanelView) {
    if (open && chat.panelView === view) chat.panelOpen = false;
    else if (!empty(view)) chat.showView(view);
  }
</script>

<!-- Laid out like the chat and panel below it (the chat takes the rest, the
     panel --panel-w), so the panel's dividing line runs straight up through the bar. -->
<header class="bar" data-tauri-drag-region>
  <span class="chat" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>{title}</span>
    <span class="views">
      {#each views as { view, name } (view)}
        <button
          title={name}
          class:on={open && chat.panelView === view}
          class:empty={empty(view)}
          onclick={() => pick(view)}
        >
          <PanelIcon {view} />
        </button>
      {/each}
    </span>
  </span>
  {#if open}
    <span class="over-panel" data-tauri-drag-region>
      <PanelBar {chat} />
    </span>
  {/if}
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
  .chat {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 3ch;
    padding: 0 var(--pad-x);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--dim);
  }
  .views {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 1.6ch;
  }
  .views button {
    display: flex;
  }
  /* The view on show is in the accent colour, like the chosen model in the
     model menu; a plain bright icon stood out more than anything else in the
     bar. Pointing at one shows the same colour. */
  .views button.on,
  .views button:hover {
    color: var(--accent);
  }
  .views button.empty {
    opacity: 0.45;
    cursor: default;
  }
  .views button.empty:hover {
    color: var(--faint);
  }
  /* Over the open panel: exactly the panel's width and dividing line, no
     padding (PanelBar pads inside), so the line runs straight up. */
  .over-panel {
    width: var(--panel-w, 46%);
    flex-shrink: 0;
    min-width: 0;
    display: flex;
    align-items: center;
    border-left: 1px solid var(--line);
  }
  /* Too narrow for both: the panel takes the chat's place (see App.svelte);
     the chat's part keeps only the icons. */
  @media (max-width: 1100px) {
    .bar:has(.over-panel) .chat {
      flex: 0 0 auto;
    }
    .bar:has(.over-panel) .title {
      display: none;
    }
    .over-panel {
      width: auto;
      flex: 1;
    }
  }
</style>
