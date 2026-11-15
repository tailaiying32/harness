<script lang="ts">
  // The open panel's own row, drawn in the top bar right above it, so the panel
  // needs no row of its own: docs' tabs, the side chat's name, or the browser's
  // address. What it doesn't fill stays empty for dragging the window.
  import BarIcon from "../BarIcon.svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { browserNavigate, browserReload } from "../ipc";
  import { NOTES_TAB, PLAN_TAB, type Chat } from "../state/chat.svelte";
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
  /** Typing a new address: double-click on it, or there's no page yet. */
  let typingUrl = $state(false);
  /** Lit for a moment after a click copies the address. */
  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  async function copy() {
    if (!page) return;
    try {
      await navigator.clipboard.writeText(page.url);
    } catch {
      return;
    }
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = false), 600);
  }

  function focusBox(node: HTMLInputElement) {
    if (typingUrl) node.focus();
  }

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

<!-- Full screen: the panel covers the chat (never the sidebar); again, or
     Escape, brings the chat back. On every view, just before its ×. -->
{#snippet widen()}
  <button
    class="icon"
    title={chat.panelWide ? "Bring the chat back (Esc)" : "Full screen"}
    onclick={() => (chat.panelWide = !chat.panelWide)}
  >
    <BarIcon name={chat.panelWide ? "narrow" : "widen"} />
  </button>
{/snippet}

<div class="row" data-tauri-drag-region>
  {#if chat.panelView === "browser"}
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
    <!-- One click copies the address; double-click types a new one. -->
    {#if page && !typingUrl}
      <button
        class="address shown"
        class:copied
        title="Click to copy, double-click to type an address"
        onclick={copy}
        ondblclick={() => (typingUrl = true)}>{page.url}</button
      >
    {:else}
    <input
      class="address"
      use:focusBox
      bind:value={address}
      onfocus={(e) => {
        editing = true;
        (e.target as HTMLInputElement).select();
      }}
      onblur={() => {
        editing = false;
        typingUrl = false;
      }}
      onkeydown={go}
      spellcheck="false"
      autocomplete="off"
      placeholder="Type a url"
    />
    {/if}
    <span class="spacer" data-tauri-drag-region></span>
    <!-- With the panel's other buttons at the right, not after the address. -->
    <button
      class="icon"
      class:off={!page}
      title="Open in your browser"
      onclick={() => page && void openUrl(page.url).catch(() => {})}
    >
      <BarIcon name="external" />
    </button>
    {@render widen()}
    <button class="icon" title="Close the browser" onclick={() => chat.closeBrowser()}>
      <BarIcon name="close" />
    </button>
  {:else if chat.panelView === "side" && chat.side}
    <span class="name on">{chat.side.title || "side chat"}</span>
    <span class="spacer" data-tauri-drag-region></span>
    {@render widen()}
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
    {@render widen()}
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
  .icon {
    display: flex;
  }
  .reload.off,
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
  /* The address as text: only as wide as it is (the rest drags the window),
     one line, cut short with an ellipsis. */
  .address.shown {
    flex: 0 1 auto;
    max-width: 48ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  .address.shown:hover {
    color: var(--fg);
  }
  /* Copied: the accent for a moment, the only sign. */
  .address.shown.copied {
    color: var(--accent);
  }
  .address:focus {
    color: var(--bright);
  }
  .address::placeholder {
    color: var(--faint);
  }
</style>
