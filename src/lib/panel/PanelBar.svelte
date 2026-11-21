<script lang="ts">
  // The open panel's own row, drawn in the top bar right above it, so the panel
  // needs no row of its own: docs' tabs, the side chat's name, the browser's
  // address, the terminal's tabs, or chat info's tabs. What it doesn't fill stays empty for
  // dragging the window.
  import { tick } from "svelte";
  import BarIcon from "../BarIcon.svelte";
  import DocsMenu from "./DocsMenu.svelte";
  import { MARKS } from "./agentMarks";
  import { batchLabel } from "../state/chatInfo.svelte";
  import { shortBranch } from "../paths";
  import { NOTES_TAB, PLAN_TAB, type Chat } from "../state/chat.svelte";
  import { terminals } from "../state/terminals.svelte";
  import { baseName } from "../tools";
  import { TabDrag } from "./tabDrag.svelte";

  let { chat }: { chat: Chat } = $props();

  /** Dragging a tab along its row (every row but chat info's). */
  const drag = new TabDrag();

  // ---- Terminal (the project's tabs) ----
  const project = $derived(chat.projectPath.trim());
  const termTabs = $derived(terminals.tabs(project));
  const termShown = $derived(terminals.shown(project));
  /** The chat's worktree, offered in the + menu while it is there. */
  const worktree = $derived(
    chat.workPath && chat.wtStatus === "live" && chat.branch ? { path: chat.workPath, label: shortBranch(chat.branch) } : null,
  );
  let row = $state<HTMLElement>();
  let menuOpen = $state(false);
  let menuBox = $state<HTMLElement>();
  let askBox = $state<HTMLElement>();
  /** Where the "close it?" box sits: under its tab, kept inside the row. */
  let askX = $state(0);
  let askLeft = $state(0);
  $effect(() => {
    const box = askBox;
    const x = askX;
    if (!box || !row) return;
    askLeft = Math.max(0, Math.min(x, row.clientWidth - box.offsetWidth));
  });
  const asking = $derived(
    terminals.confirming && termTabs.some((t) => t.id === terminals.confirming?.id) ? terminals.confirming : null,
  );
  const askingName = $derived(asking ? (termTabs.find((t) => t.id === asking.id)?.title ?? "") : "");

  async function newTab(where?: { path: string; label: string }) {
    menuOpen = false;
    try {
      await terminals.open(project, where);
    } catch (e) {
      chat.showError(String(e));
    }
  }

  function shut(e: MouseEvent, id: string) {
    const tab = (e.currentTarget as HTMLElement).closest(".tab");
    if (tab && row) askX = tab.getBoundingClientRect().left - row.getBoundingClientRect().left;
    void terminals.askClose(id);
  }

  // A click outside the open menu or question, or Escape, closes it (a
  // question answered this way keeps the tab).
  $effect(() => {
    const box = menuOpen ? menuBox : asking ? askBox : null;
    if (!box) return;
    const dismiss = () => {
      menuOpen = false;
      terminals.confirming = null;
    };
    const away = (e: PointerEvent) => {
      if (!box.contains(e.target as Node)) dismiss();
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", escape);
    };
  });

  // ---- Side chats ----
  /**
   * Each side chat's tab name: its title, with " 2", " 3" where two share one,
   * counted in the order they were started, wherever they sit in the row.
   */
  const sideTabs = $derived.by(() => {
    const seen = new Map<string, number>();
    const names = new Map<string, string>();
    for (const side of [...chat.sides].sort((a, b) => a.born - b.born)) {
      const base = side.title || "side chat";
      const n = (seen.get(base) ?? 0) + 1;
      seen.set(base, n);
      names.set(side.sessionId, n > 1 ? `${base} ${n}` : base);
    }
    return chat.sides.map((side) => ({ id: side.sessionId, name: names.get(side.sessionId)! }));
  });

  // ---- Chat info: the subagent picked last, and a menu of them all ----
  let agentMenu = $state(false);
  let agentPicker = $state<HTMLElement>();
  function pickAgent(id: string) {
    agentMenu = false;
    chat.info.show(id);
  }
  // A click outside the menu, or Escape, closes it.
  $effect(() => {
    const box = agentMenu ? agentPicker : null;
    if (!box) return;
    const away = (e: PointerEvent) => {
      if (!box.contains(e.target as Node)) agentMenu = false;
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape") agentMenu = false;
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", escape);
    };
  });

  // ---- Docs ----
  const active = $derived(chat.activeTab);

  // A tab opened or picked while the row overflows is scrolled into sight,
  // in every view. "nearest" leaves a tab that is already in sight alone.
  $effect(() => {
    void [chat.panelView, chat.panelOpen, active, chat.side?.sessionId, page?.id, termShown?.id];
    void tick().then(() => row?.querySelector(".tabs .tab.on")?.scrollIntoView({ block: "nearest", inline: "nearest" }));
  });

  /** A file's name; with its folder when two tabs share a name. */
  function label(path: string): string {
    const name = baseName(path);
    if (chat.openDocs.filter((doc) => baseName(doc.path) === name).length < 2) return name;
    return path.split(/[\\/]/).slice(-2).join("/");
  }

  // ---- Browser (its address row is in BrowserView) ----
  const page = $derived(chat.browser);

  /** A tab's name: the page's title, else its host, else "new tab". */
  function pageLabel(tab: { url: string; title: string }): string {
    if (tab.title) return tab.title;
    try {
      return new URL(tab.url).host || "new tab";
    } catch {
      return "new tab";
    }
  }
</script>

<!-- The same pair at the right of every view. Full screen: the panel covers
     the chat (never the sidebar); again, or Escape, brings the chat back. ×
     only hides the panel, like clicking the view's icon again; ending a side
     chat or closing the page is the small × by its name. -->
{#snippet corner()}
  <button
    class="icon"
    title={chat.panelWide ? "Exit full screen (Esc)" : "Full screen"}
    onclick={() => (chat.panelWide = !chat.panelWide)}
  >
    <BarIcon name={chat.panelWide ? "narrow" : "widen"} />
  </button>
  <button class="icon" title="Hide" onclick={() => (chat.panelOpen = false)}>
    <BarIcon name="close" />
  </button>
{/snippet}

<div class="row" data-tauri-drag-region bind:this={row}>
  {#if chat.panelView === "terminal"}
    <div class="tabs">
      {#each drag.arrange(termTabs, (t) => t.id) as tab (tab.id)}
        <span
          class="tab"
          class:on={tab.id === termShown?.id}
          class:held={tab.id === drag.id}
          data-tab={tab.id}
        >
          <button
            title={tab.running ?? tab.title}
            onclick={() => terminals.pick(project, tab.id)}
            onpointerdown={(e) =>
              drag.start(
                e,
                tab.id,
                () => termTabs.map((t) => t.id),
                (id, to) => terminals.move(project, id, to),
              )}
            >{#if tab.failed}<span class="done failed" title="Failed">●</span>{:else if tab.done}<span class="done" title="Finished">●</span>{/if}{tab.title}</button
          >
          <button class="shut" title="Close" onclick={(e) => shut(e, tab.id)}>×</button>
        </span>
      {/each}
    </div>
    <span class="picker" bind:this={menuBox}>
      <button
        class="plus"
        class:open={menuOpen}
        title="New tab"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onclick={() => (worktree ? (menuOpen = !menuOpen) : void newTab())}>+</button
      >
      {#if menuOpen}
        <div class="menu" role="menu">
          <button role="menuitem" onclick={() => newTab()}>new tab</button>
          {#if worktree}
            <button role="menuitem" onclick={() => newTab(worktree)}>new tab in {worktree.label}</button>
          {/if}
        </div>
      {/if}
    </span>
    <span class="spacer" data-tauri-drag-region></span>
    {@render corner()}
    {#if asking}
      <div class="ask" role="alertdialog" bind:this={askBox} style:left="{askLeft}px">
        <span class="question">{askingName} is running. Close it?</span>
        <span class="answers">
          <button class="danger" onclick={() => terminals.close(asking.id)}>close</button>
          <button class="keep" onclick={() => (terminals.confirming = null)}>keep</button>
        </span>
      </div>
    {/if}
  {:else if chat.panelView === "browser"}
    <!-- Marked so the tab keys know the browser has the keys (see tabKeys.ts). -->
    <div class="tabs pages" data-tabs="browser">
      {#each drag.arrange(chat.browserTabs, (t) => t.id) as tab (tab.id)}
        <span
          class="tab"
          class:on={tab.id === page?.id}
          class:held={tab.id === drag.id}
          data-tab={tab.id}
        >
          <button
            title={tab.url || "new tab"}
            onclick={() => chat.pickBrowserTab(tab.id)}
            onpointerdown={(e) =>
              drag.start(
                e,
                tab.id,
                () => chat.browserTabs.map((t) => t.id),
                (id, to) => chat.moveBrowserTab(id, to),
              )}>{pageLabel(tab)}</button
          >
          <button class="shut" title="Close" onclick={() => chat.closeBrowserTab(tab.id)}>×</button>
        </span>
      {/each}
    </div>
    <button class="plus" title="New tab" onclick={() => chat.newBrowserTab()}>+</button>
    <span class="spacer" data-tauri-drag-region></span>
    {@render corner()}
  {:else if chat.panelView === "info"}
    <!-- The overview, then the subagent picked last, with a menu of them all
         by batch, as the overview has them. -->
    <div class="info" data-tabs="info">
      <span class="tab" class:on={!chat.info.shownAgent}>
        <button onclick={() => chat.info.show(null)}>chat info</button>
      </span>
      {#if chat.info.agents.length}
        {@const picked = chat.info.pickedAgent}
        <span class="picker agents" bind:this={agentPicker}>
          <!-- Like a tab: from the overview the name goes to that agent; on it
               (or before any is picked), the name opens the menu. -->
          <button
            class="picked"
            class:on={!!chat.info.shownAgent}
            class:open={agentMenu}
            aria-haspopup="menu"
            aria-expanded={agentMenu}
            onclick={() => (picked && !chat.info.shownAgent ? chat.info.show(picked.toolId) : (agentMenu = !agentMenu))}
          >
            <span class="who">{picked?.name ?? "subagents"}</span>
          </button>
          {#if agentMenu}
            <div class="menu agent-menu" role="menu">
              {#each chat.info.batches as batch (batch.batch)}
                <div class="batch">{batchLabel(batch)}</div>
                {#each batch.agents as agent (agent.toolId)}
                  <button role="menuitem" class:on={agent === chat.info.shownAgent} onclick={() => pickAgent(agent.toolId)}>
                    <span class="mark {agent.state}">{MARKS[agent.state]}</span>
                    <span class="who">{agent.name}</span>
                  </button>
                {/each}
              {/each}
            </div>
          {/if}
        </span>
      {/if}
    </div>
    <span class="spacer" data-tauri-drag-region></span>
    {@render corner()}
  {:else if chat.panelView === "side" && chat.side}
    <div class="tabs">
      {#each drag.arrange(sideTabs, (t) => t.id) as { id, name } (id)}
        <span
          class="tab"
          class:on={id === chat.side.sessionId}
          class:held={id === drag.id}
          data-tab={id}
        >
          <button
            onclick={() => chat.pickSide(id)}
            onpointerdown={(e) =>
              drag.start(
                e,
                id,
                () => chat.sides.map((s) => s.sessionId),
                (id, to) => chat.moveSide(id, to),
              )}>{name}</button
          >
          <button class="shut" title="End" onclick={() => chat.endSide(id)}>×</button>
        </span>
      {/each}
    </div>
    <button class="plus" title="New side chat" onclick={() => chat.newSide()}>+</button>
    <span class="spacer" data-tauri-drag-region></span>
    {@render corner()}
  {:else}
    <DocsMenu {chat} />
    <!-- Many tabs scroll sideways. A doc dragged from the docs menu onto them opens. -->
    <div class="tabs" data-doc-tabs>
      <!-- The chat's notes: always first. Closed, the docs menu brings it back. -->
      {#if chat.notesTab}
        <span class="tab" class:on={active === NOTES_TAB}>
          <button onclick={() => chat.openPanel(NOTES_TAB)}>notes</button>
          <button class="shut" title="Close" onclick={() => chat.closeTab(NOTES_TAB)}>×</button>
        </span>
      {/if}
      {#if chat.planTab}
        <span class="tab" class:on={active === PLAN_TAB}>
          <button onclick={() => chat.openPanel(PLAN_TAB)}>plan</button>
          <!-- A plan that is waiting for an answer stays put. -->
          {#if !chat.planPending}
            <button class="shut" title="Close" onclick={() => chat.closeTab(PLAN_TAB)}>×</button>
          {/if}
        </span>
      {/if}
      <!-- Documents can be dragged along the row; notes and plan stay first. -->
      {#each drag.arrange(chat.openDocs, (d) => d.path) as doc (doc.path)}
        <span
          class="tab"
          class:on={active === doc.path}
          class:held={doc.path === drag.id}
          data-tab={doc.path}
        >
          <button
            title={doc.path}
            onclick={() => chat.openPanel(doc.path)}
            onpointerdown={(e) =>
              drag.start(
                e,
                doc.path,
                () => chat.openDocs.map((d) => d.path),
                (path, to) => chat.moveDoc(path, to),
              )}>{label(doc.path)}</button
          >
          <button class="shut" title="Close" onclick={() => chat.closeTab(doc.path)}>×</button>
        </span>
      {/each}
    </div>
    <span class="spacer" data-tauri-drag-region></span>
    {@render corner()}
  {/if}
</div>

<style>
  .row {
    position: relative;
    flex: 1;
    min-width: 0;
    align-self: stretch;
    display: flex;
    align-items: center;
    padding: 0 var(--pad-x);
    gap: 2ch;
    color: var(--faint);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  button:hover {
    color: var(--fg);
  }
  .tabs {
    display: flex;
    gap: 3ch;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    flex-shrink: 0;
    display: flex;
    gap: 1.5ch;
  }
  .tab.on button:first-child,
  .tab.held button:first-child {
    color: var(--bright);
  }
  /* A command finished in a tab off screen: the sidebar's blue "finished since you last looked". */
  .done {
    display: inline-block;
    width: 2ch;
    color: var(--blue);
  }
  /* An agent's tab whose last command failed: red until its next command. */
  .done.failed {
    color: var(--bad);
  }
  /* A tab's close mark shows on the tab in front, or under the pointer. */
  .shut {
    visibility: hidden;
  }
  .tab.on .shut,
  .tab:hover .shut {
    visibility: visible;
  }
  .icon {
    display: flex;
  }
  /* Chat info's subagents: a mark each, coloured by how it is doing; the
     one on show at full strength, the rest faded until pointed at. */
  .info {
    display: flex;
    align-items: center;
    gap: 3ch;
    min-width: 0;
  }
  .info .tab {
    flex-shrink: 0;
  }
  /* The picked subagent's name gives way before anything else in the row. */
  .picker.agents {
    flex-shrink: 1;
    min-width: 0;
    gap: 1ch;
    /* Its menu is placed against the whole row, so it stays inside the panel. */
    position: static;
  }
  .picked {
    display: flex;
    gap: 1ch;
    min-width: 0;
  }
  .picked.on,
  .picked.open,
  .agent-menu button.on {
    color: var(--bright);
  }
  .who {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Every subagent, by batch as on the overview; long lists scroll. */
  .menu.agent-menu {
    left: var(--pad-x);
    min-width: min(28ch, calc(100% - 2 * var(--pad-x)));
    max-width: min(60ch, calc(100% - 2 * var(--pad-x)));
    max-height: 60vh;
    overflow-y: auto;
  }
  .agent-menu button {
    min-width: 0;
  }
  .agent-menu button {
    display: flex;
  }
  .batch {
    padding: 0 2ch;
    color: var(--faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .batch:not(:first-child) {
    margin-top: calc(var(--gap) * 0.6);
  }
  .mark {
    width: 2.5ch;
    flex-shrink: 0;
  }
  .mark.running,
  .mark.done {
    color: var(--ok);
  }
  .mark.failed {
    color: var(--bad);
  }
  .mark.stopped {
    color: var(--faint);
  }
  /* A page's title can be long; its tab keeps to a few words. */
  .pages .tab button:first-child {
    max-width: 18ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* The terminal's + and its menu, which opens downward over the panel. */
  .picker {
    position: relative;
    flex-shrink: 0;
    align-self: stretch;
    display: flex;
    align-items: center;
  }
  .plus.open {
    color: var(--bright);
  }
  .menu,
  .ask {
    position: absolute;
    top: calc(100% + var(--gap) * 0.4);
    z-index: 10;
    display: flex;
    flex-direction: column;
    padding: calc(var(--gap) * 0.4) 0;
    background: var(--panel);
    border: 1px solid var(--line);
  }
  .menu {
    left: 0;
  }
  .menu button {
    text-align: left;
    padding: 0 2ch;
    color: var(--dim);
  }
  .menu button:hover {
    color: var(--fg);
  }
  /* "python3 is running. Close it?" under its tab. */
  .ask {
    top: calc(100% - var(--gap) * 0.2);
    padding: calc(var(--gap) * 0.5) 2ch;
    gap: calc(var(--gap) * 0.4);
  }
  .question {
    color: var(--fg);
  }
  .answers {
    display: flex;
    gap: 3ch;
  }
  .danger {
    color: var(--bad);
  }
  .keep {
    color: var(--dim);
  }
</style>
