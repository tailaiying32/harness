<script lang="ts">
  // The open panel's own row, drawn in the top bar right above it, so the panel
  // needs no row of its own: docs' tabs, the side chat's name, or the browser's
  // address. What it doesn't fill stays empty for dragging the window.
  import { browserNavigate, browserReload } from "../ipc";
  import { PLAN_TAB, type Chat } from "../state/chat.svelte";
  import { baseName } from "../tools";

  let { chat }: { chat: Chat } = $props();

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

<div class="row" data-tauri-drag-region>
  {#if chat.panelView === "browser"}
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
      placeholder="localhost:5173"
    />
    <span class="spacer" data-tauri-drag-region></span>
    {#if page}
      <button title="Reload" onclick={() => void browserReload(chat.sessionId).catch(() => {})}>
        {page.loading ? "…" : "↻"}
      </button>
    {/if}
    <button title="Close the browser" onclick={() => chat.closeBrowser()}>×</button>
  {:else if chat.panelView === "side" && chat.side}
    <span class="name on">{chat.side.title || "side chat"}</span>
    <span class="spacer" data-tauri-drag-region></span>
    <button title="End the side chat; it is forgotten" onclick={() => chat.endSide()}>×</button>
  {:else}
    <!-- Many tabs scroll sideways. -->
    <div class="tabs">
      {#if chat.planTab}
        <span class="tab" class:on={active === PLAN_TAB}>
          <button onclick={() => chat.openPanel(PLAN_TAB)}>
            {#if chat.planPending}<span class="accent">●</span>{/if}
            plan
          </button>
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
  .accent {
    color: var(--accent);
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
