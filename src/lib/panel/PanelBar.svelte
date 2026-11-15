<script lang="ts">
  // The open panel's own row, drawn in the top bar right above it, so the panel
  // needs no row of its own: docs' tabs, the side chat's name, the browser's
  // address, or the terminal's tabs. What it doesn't fill stays empty for
  // dragging the window.
  import BarIcon from "../BarIcon.svelte";
  import { browserNavigate, browserReload } from "../ipc";
  import { shortBranch } from "../paths";
  import { NOTES_TAB, PLAN_TAB, type Chat } from "../state/chat.svelte";
  import { terminals } from "../state/terminals.svelte";
  import { baseName } from "../tools";

  let { chat }: { chat: Chat } = $props();

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

  // ---- Docs ----
  const active = $derived(chat.activeTab);

  /** A file's name; with its folder when two tabs share a name. */
  function label(path: string): string {
    const name = baseName(path);
    if (chat.openDocs.filter((doc) => baseName(doc.path) === name).length < 2) return name;
    return path.split(/[\\/]/).slice(-2).join("/");
  }

  // ---- Browser ----
  const page = $derived(chat.browser);
  let address = $state("");
  let editing = $state(false);

  // The address follows the page, except while the user types in it.
  $effect(() => {
    const url = page?.url;
    if (!editing) address = url ?? "";
  });

  async function go(e: KeyboardEvent) {
    const box = e.target as HTMLInputElement;
    if (e.key === "Escape") {
      address = page?.url ?? "";
      box.blur();
      return;
    }
    if (e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    chat.browserProblem = "";
    try {
      await browserNavigate(chat.sessionId, address);
      box.blur();
    } catch (err) {
      chat.browserProblem = String(err);
    }
  }
</script>

<div class="row" data-tauri-drag-region bind:this={row}>
  {#if chat.panelView === "terminal"}
    <div class="tabs">
      {#each termTabs as tab (tab.id)}
        <span class="tab" class:on={tab.id === termShown?.id}>
          <button title={tab.running ?? tab.title} onclick={() => terminals.pick(project, tab.id)}>{tab.title}</button>
          <button class="shut" title="Close this tab" onclick={(e) => shut(e, tab.id)}>×</button>
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
    <!-- Before the address, like most browsers. Kept in place (just hidden)
         until a page is open, so the address doesn't jump right. -->
    <button
      class="icon reload"
      class:off={!page}
      class:busy={page?.loading}
      title="Reload"
      onclick={() => void browserReload(chat.sessionId).catch(() => {})}
    >
      <BarIcon name="reload" />
    </button>
    <input
      class="address"
      bind:value={address}
      onfocus={(e) => {
        editing = true;
        (e.target as HTMLInputElement).select();
      }}
      onblur={() => (editing = false)}
      onkeydown={go}
      spellcheck="false"
      autocomplete="off"
      placeholder="Type a url"
    />
    <span class="spacer" data-tauri-drag-region></span>
    <button class="icon" title="Close the browser" onclick={() => chat.closeBrowser()}>
      <BarIcon name="close" />
    </button>
  {:else if chat.panelView === "side" && chat.side}
    <span class="name on">{chat.side.title || "side chat"}</span>
    <span class="spacer" data-tauri-drag-region></span>
    <button class="icon" title="End the side chat; it is forgotten" onclick={() => chat.endSide()}>
      <BarIcon name="close" />
    </button>
  {:else}
    <!-- Many tabs scroll sideways. -->
    <div class="tabs">
      <!-- The chat's notes: always there, always first, never closed. -->
      <span class="tab" class:on={active === NOTES_TAB}>
        <button onclick={() => chat.openPanel(NOTES_TAB)}>notes</button>
      </span>
      {#if chat.planTab}
        <span class="tab" class:on={active === PLAN_TAB}>
          <button onclick={() => chat.openPanel(PLAN_TAB)}>plan</button>
          <!-- A plan that is waiting for an answer stays put. -->
          {#if !chat.planPending}
            <button class="shut" title="Close this tab" onclick={() => chat.closeTab(PLAN_TAB)}>×</button>
          {/if}
        </span>
      {/if}
      {#each chat.openDocs as doc (doc.path)}
        <span class="tab" class:on={active === doc.path}>
          <button title={doc.path} onclick={() => chat.openPanel(doc.path)}>{label(doc.path)}</button>
          <button class="shut" title="Close this tab" onclick={() => chat.closeTab(doc.path)}>×</button>
        </span>
      {/each}
    </div>
    <span class="spacer" data-tauri-drag-region></span>
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
  .name.on {
    color: var(--bright);
  }
  /* A side chat's name is a few words; keep it to one line. */
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
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
  .reload.off {
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
  /* As wide as an address usually needs; the rest drags the window. */
  .address {
    flex: 0 1 48ch;
    min-width: 12ch;
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
