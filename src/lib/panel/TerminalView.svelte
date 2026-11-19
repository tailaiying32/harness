<script lang="ts">
  // The panel's terminal view: the project's tab on show, filling the panel.
  // Its tabs, + and × are in the top bar (see PanelBar). The tab's xterm lives
  // in the terminals store and is moved in here, so it keeps its screen when
  // the panel is made again for another chat. Opening the view on a project
  // with no tabs opens one; closing the last tab leaves the view.
  import { onDestroy, onMount } from "svelte";
  import { terminalPlace, terminalResize } from "../ipc";
  import type { Chat } from "../state/chat.svelte";
  import { terminals } from "../state/terminals.svelte";

  let { chat, covered }: { chat: Chat; covered: boolean } = $props();

  const project = $derived(chat.projectPath.trim());
  const tab = $derived(terminals.shown(project));
  let slot = $state<HTMLElement>();
  /** The first look at the project's tabs is done (and a first tab opened if it had none). */
  let ready = $state(false);
  let problem = $state("");
  let pageShown = $state(document.visibilityState === "visible");
  let gone = false;

  onMount(async () => {
    try {
      const tabs = await terminals.fetch(project);
      if (tabs.length === 0 && !gone) await terminals.open(project);
    } catch (e) {
      problem = String(e);
    }
    ready = true;
  });

  // The last tab closed (by ×, or its shell exited): leave the view.
  $effect(() => {
    if (ready && !problem && terminals.tabs(project).length === 0) chat.leaveTerminal();
  });

  /** Fits the xterm to the slot and tells the shell its size. */
  let lastSize = "";
  async function fit() {
    const current = tab;
    if (!current || !slot) return;
    const made = await terminals.xterm(current.id);
    if (gone || tab?.id !== current.id) return;
    // Not laid out (the panel is hidden): no size to tell the shell yet.
    if (!made.fit.proposeDimensions()) return;
    made.fit.fit();
    const { cols, rows } = made.term;
    terminals.size = { cols, rows };
    const key = `${current.id} ${cols} ${rows}`;
    if (key === lastSize) return;
    lastSize = key;
    void terminalResize(current.id, cols, rows).catch(() => {});
  }

  // Show the tab: move its xterm into the slot.
  $effect(() => {
    const current = tab;
    const into = slot;
    if (!current || !into) return;
    void terminals.xterm(current.id).then((made) => {
      if (gone || tab?.id !== current.id) return;
      if (made.host.parentElement !== into) into.replaceChildren(made.host);
      void fit();
    });
  });

  // The theme or text size changed: the same panel now holds a different number of columns.
  $effect(() => {
    if (terminals.restyled) void fit();
  });

  // An action that should put the keys in the terminal (the icon, a tab, +, Cmd+T).
  $effect(() => {
    const current = tab;
    if (!terminals.wantsKeys || !current) return;
    terminals.wantsKeys = false;
    void terminals.xterm(current.id).then((made) => {
      if (!gone && tab?.id === current.id) made.term.focus();
    });
  });

  $effect(() => {
    if (!slot) return;
    const watch = new ResizeObserver(() => void fit());
    watch.observe(slot);
    const seen = () => (pageShown = document.visibilityState === "visible");
    document.addEventListener("visibilitychange", seen);
    return () => {
      watch.disconnect();
      document.removeEventListener("visibilitychange", seen);
    };
  });

  // Which tab is on screen: a tab on screen never counts as unused.
  $effect(() => {
    void terminalPlace(tab?.id ?? null, !covered && pageShown).catch(() => {});
  });

  onDestroy(() => {
    gone = true;
    void terminalPlace(null, false).catch(() => {});
    // The xterm is kept (in the store) for the next time the tab shows.
    slot?.replaceChildren();
  });
</script>

{#if problem}
  <div class="problem">{problem}</div>
{/if}
<div class="slot" bind:this={slot}></div>

<style>
  .problem {
    padding: calc(var(--gap) * 0.5) var(--pad-x);
    border-bottom: 1px solid var(--line);
    color: var(--bad);
  }
  /* The terminal fills the panel, with a little room round the text. */
  .slot {
    flex: 1;
    min-height: 0;
    padding: calc(var(--gap) * 0.6) calc(var(--pad-x) * 0.6);
    background: var(--bg);
  }
  .slot :global(.terminal-host) {
    width: 100%;
    height: 100%;
  }
  .slot :global(.xterm) {
    height: 100%;
  }
  .slot :global(.xterm-viewport) {
    background: var(--bg) !important;
  }
</style>
