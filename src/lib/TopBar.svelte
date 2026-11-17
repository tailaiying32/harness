<script lang="ts">
  // The bar along the top of the chat and panel. On a Mac it sits in the window's
  // title bar, beside the window buttons, so it costs no room. Over the chat:
  // its title (click to rename), and the panel's five views as icons, pushed against the panel's
  // line. Clicking a view opens the panel on it; clicking the one on show hides
  // the panel. Over the open panel: that view's own row (see PanelBar), so the
  // panel has none of its own. Empty space drags the window.
  import PanelIcon from "./PanelIcon.svelte";
  import PanelBar from "./panel/PanelBar.svelte";
  import RenameField from "./RenameField.svelte";
  import type { AppState } from "./state/app.svelte";
  import type { PanelView } from "./state/chat.svelte";
  import { terminals } from "./state/terminals.svelte";

  let { app }: { app: AppState } = $props();

  const chat = $derived(app.current);
  const title = $derived(
    app.projects.flatMap((p) => p.sessions).find((s) => s.id === chat.sessionId)?.title ?? "",
  );
  const open = $derived(chat.panelOpen && chat.panelHasContent);

  /** The chat whose name is being edited here; switching chats ends it. */
  let renaming = $state<string | null>(null);

  async function finishRename(save: boolean, text: string) {
    const id = renaming;
    renaming = null;
    if (save && id) await app.rename(id, text);
  }

  // Side chats copy a Claude Code conversation, so Cursor chats have none.
  const views = $derived(
    (
      [
        // This chat's own things first, then the project's tools.
        { view: "info", name: "chat info" },
        { view: "docs", name: "docs" },
        { view: "side", name: "side chat" },
        { view: "terminal", name: "terminal" },
        { view: "browser", name: "browser" },
      ] as { view: PanelView; name: string }[]
    ).filter((v) => v.view !== "side" || chat.agent !== "cursor"),
  );

  function pick(view: PanelView) {
    if (open && chat.panelView === view) chat.panelOpen = false;
    else {
      chat.showView(view);
      if (view === "terminal") terminals.wantsKeys = true;
    }
  }
</script>

<!-- Laid out like the chat and panel below it (the chat takes the rest, the
     panel --panel-w), so the panel's dividing line runs straight up through the bar. -->
<header class="bar" data-tauri-drag-region>
  <span class="chat" data-tauri-drag-region>
    <!-- Only the name itself is clickable; the space after it still drags. -->
    <span class="title" data-tauri-drag-region>
      {#if renaming && renaming === chat.sessionId}
        <RenameField value={title} done={finishRename} />
      {:else if title}
        <button title="{title} (click to rename)" onclick={() => (renaming = chat.sessionId)}>{title}</button>
      {/if}
    </span>
    <span class="views">
      {#each views as { view, name } (view)}
        <button
          title={name}
          class:on={open && chat.panelView === view}
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
    display: flex;
    color: var(--dim);
  }
  .title button {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title button:hover {
    color: var(--fg);
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
  .views button:hover {
    color: var(--fg);
  }
  /* The view on show is bright, like the open chat in the sidebar. */
  .views button.on {
    color: var(--bright);
  }
  /* Over the open panel: exactly the panel's width and dividing line, no
     padding (PanelBar pads inside), so the line runs straight up. */
  .over-panel {
    width: var(--panel-w, 46%);
    box-sizing: border-box;
    flex-shrink: 0;
    min-width: 0;
    display: flex;
    align-items: center;
    border-left: 1px solid var(--line);
  }
  /* The sidebar put away: room for its show/hide button, keep awake's cup
     (and a Mac's window buttons) before the title. */
  :global(.bare) .chat {
    padding-left: calc(var(--toggle-x) + 2em + 2ch + var(--pad-x) * 0.8);
  }
  /* Full screen panel: as in a narrow window, below. */
  :global(.wide) .bar:has(.over-panel) .chat {
    flex: 0 0 auto;
  }
  :global(.wide) .bar:has(.over-panel) .title {
    display: none;
  }
  /* No line below to continue, so none here either. */
  :global(.wide) .over-panel {
    width: auto;
    flex: 1;
    border-left: 0;
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
      border-left: 0;
    }
  }
</style>
