<script lang="ts">
  // The chat column: messages, folded tool runs, approvals. Pinned to the
  // bottom like a terminal unless the user scrolls up.
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { Chat, Item, ToolItem } from "../state/chat.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import Markdown from "./Markdown.svelte";
  import WorkRun from "./WorkRun.svelte";

  let { chat, problem }: { chat: Chat; problem: string | null } = $props();

  let scroller: HTMLElement;
  let content: HTMLElement;
  let pinned = true;

  // Runs of tool calls become one group. This only recomputes when an item is
  // added, not while text streams into an existing one.
  type Group =
    | { kind: "run"; id: number; tools: ToolItem[] }
    | { kind: "item"; id: number; item: Exclude<Item, ToolItem> };

  const groups = $derived.by(() => {
    const out: Group[] = [];
    for (const item of chat.items) {
      if (item.type === "tool") {
        const last = out[out.length - 1];
        if (last?.kind === "run") last.tools.push(item);
        else out.push({ kind: "run", id: item.id, tools: [item] });
      } else {
        out.push({ kind: "item", id: item.id, item });
      }
    }
    return out;
  });

  const lastGroup = $derived(groups[groups.length - 1]);
  const working = $derived(chat.busy && chat.status !== "waiting_approval");
  /** Show a line of its own when Claude is working but nothing on screen says so. */
  const idleLine = $derived(
    working &&
      lastGroup?.kind !== "run" &&
      !(lastGroup?.kind === "item" && lastGroup.item.type === "text" && lastGroup.item.streaming),
  );

  // The working indicator: one changing character and the seconds so far.
  // It ticks a few times a second and touches only the line that shows it.
  const frames = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
  let ticks = $state(0);
  $effect(() => {
    if (!working) return;
    const timer = setInterval(() => ticks++, 200);
    return () => clearInterval(timer);
  });
  const spinner = $derived(frames[ticks % frames.length]);
  const elapsed = $derived.by(() => {
    void ticks;
    return chat.turnStartedAt === null ? 0 : Math.max(0, Math.floor((Date.now() - chat.turnStartedAt) / 1000));
  });

  function onScroll() {
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
  }

  // Sending a message always jumps back to the bottom.
  $effect(() => {
    if (chat.items[chat.items.length - 1]?.type === "user") {
      pinned = true;
      scroller.scrollTop = scroller.scrollHeight;
    }
  });

  onMount(() => {
    const observer = new ResizeObserver(() => {
      if (pinned) scroller.scrollTop = scroller.scrollHeight;
    });
    observer.observe(content);
    return () => observer.disconnect();
  });

  async function onClick(e: MouseEvent) {
    const target = e.target as HTMLElement;

    const copy = target.closest<HTMLElement>("[data-copy]");
    if (copy) {
      const code = copy.closest(".code")?.querySelector("pre")?.textContent ?? "";
      try {
        await navigator.clipboard.writeText(code);
        copy.textContent = "copied";
        setTimeout(() => (copy.textContent = "copy"), 1200);
      } catch {
        copy.textContent = "couldn't copy";
      }
      return;
    }

    // Links open in the default browser, never inside this window.
    const link = target.closest("a");
    if (link?.href) {
      e.preventDefault();
      void openUrl(link.href);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scroll" bind:this={scroller} onscroll={onScroll} onclick={onClick}>
  <div class="content" bind:this={content}>
    {#if problem}
      <div class="row"><span class="marker bad">!</span><div class="body bad">{problem}</div></div>
    {/if}

    {#each groups as group (group.id)}
      {#if group.kind === "run"}
        <div class="block">
          <WorkRun
            tools={group.tools}
            root={chat.projectPath}
            active={working && group === lastGroup}
            spinner={working && group === lastGroup ? spinner : ""}
            elapsed={working && group === lastGroup ? elapsed : 0}
          />
        </div>
      {:else if group.item.type === "user"}
        <div class="row block">
          <span class="marker accent">&gt;</span>
          <div class="body user">{group.item.text}</div>
        </div>
      {:else if group.item.type === "text"}
        <div class="row block">
          <span class="marker"></span>
          <div class="body"><Markdown text={group.item.text} /></div>
        </div>
      {:else if group.item.type === "thinking"}
        <div class="row block">
          <span class="marker"></span>
          <details class="body thinking">
            <summary>Thinking</summary>
            <div class="thought">{group.item.text}</div>
          </details>
        </div>
      {:else if group.item.type === "approval"}
        <div class="row block">
          <span class="marker"></span>
          <div class="body"><ApprovalCard item={group.item} {chat} /></div>
        </div>
      {:else if group.item.type === "note"}
        <div class="row block">
          <span class="marker"></span>
          <div class="body faint">{group.item.text}</div>
        </div>
      {:else}
        <div class="row block">
          <span class="marker bad">!</span>
          <div class="body bad pre">{group.item.text}</div>
        </div>
      {/if}
    {/each}

    {#if idleLine}
      <div class="row">
        <span class="marker accent">{spinner}</span>
        <div class="body dim">Working… <span class="faint">{elapsed}s</span></div>
      </div>
    {/if}
  </div>
</div>

<style>
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--pad-y) var(--pad-x);
    display: flex;
    flex-direction: column;
  }
  .content {
    width: 100%;
    max-width: var(--col);
    /* Short chats sit at the bottom, above the message box. */
    margin: auto auto 0;
    display: flex;
    flex-direction: column;
    gap: var(--gap);
  }
  /* Let the browser skip layout for blocks that are off screen. */
  .block {
    content-visibility: auto;
    contain-intrinsic-size: auto 2em;
  }
  .row {
    display: flex;
  }
  .marker {
    width: 2.5ch;
    flex-shrink: 0;
  }
  .body {
    min-width: 0;
    flex: 1;
  }
  .user {
    color: var(--bright);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .accent {
    color: var(--accent);
  }
  .bad {
    color: var(--bad);
  }
  .dim {
    color: var(--dim);
  }
  .faint {
    color: var(--faint);
  }
  .thinking {
    color: var(--dim);
  }
  .thinking summary {
    cursor: pointer;
    list-style: none;
  }
  .thinking summary::-webkit-details-marker {
    display: none;
  }
  .thought {
    white-space: pre-wrap;
    margin-top: 0.3em;
    padding-left: 2ch;
    border-left: 1px solid var(--line);
  }
</style>
