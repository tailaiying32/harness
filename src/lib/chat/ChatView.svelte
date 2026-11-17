<script lang="ts">
  // The chat column: messages, folded tool runs, approvals. Pinned to the
  // bottom like a terminal unless the user scrolls up.
  import { onMount, tick, untrack } from "svelte";
  import { contentClick } from "../clicks";
  import { PLAN_TAB, planOutcome, withWords, type Chat, type Item, type ToolItem, type UserItem } from "../state/chat.svelte";
  import { indent, newLine, outdent, type Edit } from "./editing";
  import ApprovalCard from "./ApprovalCard.svelte";
  import FileWord from "./FileWord.svelte";
  import ImageWord from "./ImageWord.svelte";
  import SignInCard from "./SignInCard.svelte";
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

  // A waiting message opened for editing in place (double-click). Its keys
  // work as in the message box: Enter sends, Shift+Enter adds a line (the
  // next list item in a list), Tab indents. Escape puts the original back.
  function editBox(box: HTMLTextAreaElement) {
    window.getSelection()?.removeAllRanges();
    fit(box);
    box.focus();
    box.setSelectionRange(box.value.length, box.value.length);
  }

  function fit(box: HTMLTextAreaElement) {
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
  }

  async function applyEdit(box: HTMLTextAreaElement, item: UserItem, edit: Edit) {
    if (!item.editing) return;
    const text = item.editing.text;
    item.editing.text = text.slice(0, edit.from) + edit.insert + text.slice(edit.to);
    await tick();
    box.setSelectionRange(edit.selStart, edit.selEnd);
    fit(box);
  }

  function editKey(e: KeyboardEvent, item: UserItem) {
    if (e.isComposing || !item.editing) return;
    const box = e.currentTarget as HTMLTextAreaElement;
    const { selectionStart: start, selectionEnd: end } = box;
    if (e.key === "Escape") {
      e.preventDefault();
      chat.finishEdit(item, false);
    } else if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      chat.finishEdit(item, true);
    } else if (e.key === "Enter") {
      const edit = newLine(item.editing.text, start, end);
      if (!edit) return;
      e.preventDefault();
      void applyEdit(box, item, edit);
    } else if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      const edit = e.shiftKey ? outdent(item.editing.text, start, end) : indent(item.editing.text, start, end);
      if (edit) void applyEdit(box, item, edit);
    }
  }

  /** Only the latest page line offers to open the browser, so older lines stay quiet. */
  const latestPage = $derived.by(() => {
    for (let i = chat.items.length - 1; i >= 0; i--) if (chat.items[i].type === "page") return chat.items[i].id;
    return undefined;
  });

  /** The page line whose address was just copied, lit for a moment (as in the panel's address). */
  let copiedPage = $state<number | undefined>();
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyPage(id: number, url: string) {
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      return;
    }
    copiedPage = id;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copiedPage = undefined), 600);
  }

  // Only the latest part of a chat is drawn. A long chat would otherwise cost
  // hundreds of megabytes; earlier messages are drawn when the user scrolls up.
  const PAGE = 60;
  /** How many groups at the top are not drawn. */
  let hidden = $state(untrack(() => Math.max(0, groups.length - PAGE)));
  const drawn = $derived(hidden > 0 ? groups.slice(hidden) : groups);

  // While the user stays at the bottom of a growing chat, let the top go again.
  $effect(() => {
    if (pinned && groups.length - hidden > PAGE * 2) hidden = groups.length - PAGE;
  });

  /** Draws one more page above, keeping what's on screen where it is. */
  let drawingEarlier = false;
  async function showEarlier() {
    if (hidden === 0 || drawingEarlier) return;
    drawingEarlier = true;
    const before = scroller.scrollHeight;
    hidden = Math.max(0, hidden - PAGE);
    await tick();
    scroller.scrollTop += scroller.scrollHeight - before;
    drawingEarlier = false;
  }
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

  // The view lets go of the bottom only when it moves up. Blocks that were
  // off screen are measured roughly until drawn, so the page can grow under a
  // view that hasn't moved; that must not count as scrolling away.
  let lastTop = 0;
  function onScroll() {
    const distance = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
    if (distance < 40) pinned = true;
    else if (scroller.scrollTop < lastTop) pinned = false;
    lastTop = scroller.scrollTop;
    if (scroller.scrollTop < 300) void showEarlier();
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
    // The message box growing a line makes the view shorter; stay at the end.
    observer.observe(scroller);
    // A chat opens at its end.
    void tick().then(() => {
      if (pinned && scroller) scroller.scrollTop = scroller.scrollHeight;
    });
    return () => observer.disconnect();
  });

  // Files with a tab in the side panel; their names in a folded run open it.
  const docPaths = $derived(chat.docs.map((doc) => doc.path));

  // Relative links in the chat start from the folder Claude works in (its worktree, if any).
  const onClick = contentClick({ base: () => chat.root, openDoc: (path) => void chat.openFile(path) });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scroll" bind:this={scroller} onscroll={onScroll} onclick={onClick}>
  <div class="content" bind:this={content}>
    {#if problem}
      <div class="row"><span class="marker bad">!</span><div class="body bad">{problem}</div></div>
    {/if}

    {#if hidden > 0}
      <button class="earlier" onclick={showEarlier}>↑ show earlier messages</button>
    {/if}

    {#each drawn as group (group.id)}
      {#if group.kind === "run"}
        <div class="block">
          <WorkRun
            tools={group.tools}
            root={chat.root}
            active={working && group === lastGroup}
            spinner={working && group === lastGroup ? spinner : ""}
            elapsed={working && group === lastGroup ? elapsed : 0}
            docs={docPaths}
            onopen={(path) => chat.openPanel(path)}
            onimage={(image) => chat.openImage(image)}
            onagent={(id) => chat.openInfo(id)}
          />
        </div>
      {:else if group.item.type === "user" && group.item.editing}
        {@const item = group.item}
        <div class="row block mine">
          <textarea
            class="body user edit"
            rows="1"
            bind:value={group.item.editing.text}
            use:editBox
            oninput={(e) => fit(e.currentTarget)}
            onkeydown={(e) => editKey(e, item)}
          ></textarea>
        </div>
      {:else if group.item.type === "user"}
        {@const item = group.item}
        <div class="row block mine">
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="body user" ondblclick={() => chat.editQueued(item)}>{#each withWords(group.item.text, group.item.images) as piece, i (i)}{#if piece.image}<ImageWord image={piece.image} onopen={(image) => chat.openImage(image)} />{:else if piece.file}<FileWord word={piece.text} path={piece.file} onopen={(path) => chat.openDropped(path)} />{:else if piece.url}<a class="link" href={piece.url}>{piece.text}</a>{:else}{piece.text}{/if}{/each}</div>
          <!-- Claude reads a message sent mid-turn after its current step,
               unless the user would rather cut that step short. -->
          {#if group.item.queued}
            <button class="now" title="Stop the current turn and send this now" onclick={() => chat.stop()}>
              send now
            </button>
          {/if}
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
      {:else if group.item.type === "approval" && group.item.approval === "plan"}
        <!-- The plan itself is in the side panel; the chat keeps one line, and its
             link is the only cue that a plan waits (no dot, no colour). -->
        {@const plan = group.item}
        {@const waiting = !plan.answered && !plan.cancelled}
        {@const latest = plan.requestId === chat.plan?.requestId}
        <div class="row block">
          <span class="marker"></span>
          <div class="body" class:faint={!waiting}>
            {#if waiting}
              <span class="dim">Plan · ready for your review</span>
            {:else}
              Plan · {planOutcome(plan)}
            {/if}
            {#if latest}
              <span class:dim={waiting}>·</span>
              <button class="open" class:waiting onclick={() => chat.openPanel(PLAN_TAB)}>open plan</button>
            {/if}
          </div>
        </div>
      {:else if group.item.type === "page"}
        <!-- The page itself is in the side panel's browser; the chat keeps one line. -->
        {@const page = group.item}
        <div class="row block">
          <span class="marker"></span>
          <div class="body faint page">
            <span>Page ·&nbsp;</span><button
              class="address"
              class:copied={copiedPage === page.id}
              title="Click to copy"
              onclick={() => copyPage(page.id, page.url)}>{page.url.replace(/^https?:\/\//, "")}</button>
            {#if page.id === latestPage}
              <span>&nbsp;·&nbsp;</span><button class="open" onclick={() => chat.showPage(page.url)}>open browser</button>
            {/if}
          </div>
        </div>
      {:else if group.item.type === "approval"}
        <div class="row block">
          <span class="marker"></span>
          <div class="body"><ApprovalCard item={group.item} {chat} /></div>
        </div>
      {:else if group.item.type === "signin"}
        <div class="row block">
          <span class="marker"></span>
          <div class="body"><SignInCard item={group.item} {chat} /></div>
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
        <div class="body dim">
          {chat.compacting ? "Compacting…" : "Working…"} <span class="faint">{elapsed}s</span>
        </div>
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
  .earlier {
    align-self: flex-start;
    margin-left: 2.5ch;
    color: var(--faint);
  }
  .earlier:hover {
    color: var(--fg);
  }
  .marker {
    width: 2.5ch;
    flex-shrink: 0;
  }
  .body {
    min-width: 0;
    flex: 1;
  }
  /* The user's messages: a block in the panel colour on the right, so they
     are easy to find when scrolling back. The text stays left aligned and may
     take most of the column, since the background already sets it apart. */
  .mine {
    flex-direction: column;
    align-items: flex-end;
  }
  .user {
    flex: 0 1 auto;
    max-width: 85%;
    padding: 0.7em 1.8ch;
    border-radius: 6px;
    background: var(--panel);
    color: var(--bright);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  /* Web addresses in the user's messages, like links in Claude's. A click
     opens them in the default browser (see clicks.ts). */
  .user .link {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  /* A waiting message being edited: the same block, now a text box as wide
     as a message may grow. */
  .edit {
    width: 85%;
    box-sizing: border-box;
    resize: none;
    border: 0;
    outline: 0;
    font: inherit;
    line-height: inherit;
    max-height: 40vh;
    caret-color: var(--accent);
  }
  /* Under a message Claude hasn't taken in yet. */
  .now {
    margin-top: 0.2em;
    color: var(--dim);
  }
  .now:hover {
    color: var(--bright);
  }
  /* More air around them than between Claude's blocks (this adds to the
     normal gap), so each one opens a new turn. Two in a row stay together. */
  .mine:not(:first-child),
  .mine + :not(.mine) {
    margin-top: 1.2em;
  }
  .mine + .mine {
    margin-top: 0;
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
  .open {
    text-decoration: underline;
    text-underline-offset: 0.2em;
  }
  /* A page line stays on one line: a long address is cut short with "…"
     (a click still copies all of it) and "open browser" stays in view. */
  .page {
    display: flex;
    white-space: nowrap;
  }
  .page > * {
    flex-shrink: 0;
  }
  .page .address {
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* A page line's address: plain text that copies on click. */
  .address:hover {
    color: var(--fg);
  }
  .address.copied {
    color: var(--accent);
  }
  .open.waiting {
    color: var(--bright);
  }
  .open:hover {
    color: var(--fg);
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
