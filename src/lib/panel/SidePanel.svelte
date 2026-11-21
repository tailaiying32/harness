<script lang="ts">
  // The side panel: a space beside the chat, in one of five views picked in
  // the top bar, which also holds the view's own row: docs' tabs, the side
  // chat's name, the browser's address, the terminal's tabs (see PanelBar). Terminal:
  // the project's shells (see TerminalView). Browser: a localhost
  // page Claude can drive too (see BrowserView). Docs: one tab for the plan and one for each markdown file
  // the chat wrote or a link opened, edited in place and saved as the user
  // types, like the notes; the plan's answer buttons live at the foot
  // of its tab while Claude waits (mouse only, like every approval: no key
  // approves a plan). Side chat: one tab for the side conversation, its own
  // message box at the foot. Chat info: the chat's context, tasks and
  // subagents (see InfoView), or one subagent full size (see AgentView).
  // Showing and hiding the panel is the top bar's.
  import { contentClick } from "../clicks";
  import ChatView from "../chat/ChatView.svelte";
  import LiveMarkdown from "../chat/LiveMarkdown.svelte";
  import BrowserView from "./BrowserView.svelte";
  import TerminalView from "./TerminalView.svelte";
  import InfoView from "./InfoView.svelte";
  import AgentView from "./AgentView.svelte";
  import Markdown from "../chat/Markdown.svelte";
  import { focusEnd } from "../chat/editing";
  import { onDestroy, tick, untrack } from "svelte";
  import { NOTES_TAB, PLAN_TAB, planOutcome, type Chat } from "../state/chat.svelte";
  import { folderOf } from "../paths";
  import { applyPanelWidth, DEFAULT_PANEL_WIDTH, loadPrefs, savePrefs } from "../themes";
  import { planOf } from "../tools";
  import { inOrder } from "../chat/comments";
  import CommentDock from "../chat/CommentDock.svelte";
  import FindBar from "../FindBar.svelte";
  import { domSearcher } from "../find";

  // `covered`: something is drawn over the panel (settings, an image), so the
  // browser's page, which would sit on top of it, is put out of sight.
  let { chat, covered = false }: { chat: Chat; covered?: boolean } = $props();

  // A relative link in a document starts from that document's folder, as in
  // any markdown viewer; in the plan, from the folder Claude works in.
  const onClick = contentClick({
    base: () => (!chat.activeTab || chat.activeTab === PLAN_TAB || chat.activeTab === NOTES_TAB ? chat.root : folderOf(chat.activeTab)),
    openDoc: (path) => void chat.openFile(path),
  });

  const active = $derived(chat.activeTab);
  const plan = $derived(chat.planTab ? chat.plan : null);

  // ---- Notes and documents: saved a moment after typing stops, when
  // editing ends, and before typing starts in another tab ----
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let unsaved: { tab: string; text: string } | null = null;

  // A tab on show is read from disk, so it is the file as it is now before
  // the user edits it.
  $effect(() => {
    const tab = active;
    untrack(() => {
      if (tab === NOTES_TAB) void chat.loadNotes();
      else if (tab && tab !== PLAN_TAB) void chat.refreshDoc(tab);
    });
  });

  /** What a tab shows now; null if it is gone. */
  function shownText(tab: string): string | null {
    return tab === NOTES_TAB ? chat.notes : (chat.docs.find((doc) => doc.path === tab)?.text ?? null);
  }

  function typeIn(tab: string, text: string) {
    if (unsaved && unsaved.tab !== tab) flush();
    if (tab === NOTES_TAB) chat.notes = text;
    else {
      const doc = chat.docs.find((d) => d.path === tab);
      if (doc) doc.text = text;
    }
    unsaved = { tab, text };
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 400);
  }

  function flush(): Promise<void> | undefined {
    clearTimeout(saveTimer);
    const edit = unsaved;
    unsaved = null;
    // Claude wrote the file since this was typed: its text is on disk and in
    // the editor now, so saving this would quietly undo it.
    if (!edit || edit.text !== shownText(edit.tab)) return;
    return edit.tab === NOTES_TAB ? chat.saveNotes(edit.text) : chat.saveDoc(edit.tab, edit.text);
  }

  onDestroy(() => void flush());

  // The docs menu saves typing first, so a doc is renamed or moved as it stands.
  $effect(() => {
    const owner = chat;
    owner.flushTyping = flush;
    return () => {
      if (owner.flushTyping === flush) owner.flushTyping = null;
    };
  });

  /** Typing is saved first, so the doc (or the notes) moves as it stands. */
  async function saveToProject() {
    const tab = active;
    await flush();
    if (tab === NOTES_TAB) await chat.saveNotesToProject();
    else await chat.saveToProject(tab);
  }

  /** The waiting plan as it stands: the user's edit, or Claude's text. */
  const planText = $derived(plan ? chat.planText : "");

  // ---- Comments (see comments.ts): on a doc or a waiting plan, sent together ----

  /** The editor on show, for jumping to a comment from the list. */
  let editor = $state<LiveMarkdown>();
  /** The tab's text, for finding in a plan that is only shown. */
  let docBody = $state<HTMLElement>();
  /** The tab's comments not sent yet; the notes take none. */
  const pending = $derived(active === NOTES_TAB ? [] : chat.pendingComments(active));
  /** A doc (not the plan or the notes) with comments waiting to go. */
  const docPending = $derived(active !== PLAN_TAB && pending.length > 0);

  function jump(id: string) {
    editor?.focusComment(id);
  }

  function dropComment(id: string) {
    chat.setComments(active, chat.commentsFor(active).filter((x) => x.id !== id));
  }

  /** Typing is saved first, so the agent reads the doc the comments are on. */
  async function sendComments() {
    const tab = active;
    await flush();
    await chat.sendComments(tab);
  }

  const text = $derived(
    active === PLAN_TAB
      ? plan
        ? planOf(plan.input).text
        : ""
      : (chat.docs.find((doc) => doc.path === active)?.text ?? ""),
  );

  // The panel's left edge drags to set its width, kept as a share of the
  // area beside the sidebar (the panel's parent), since that is what the
  // width's percentage is measured against. The chat keeps at least
  // MIN_CHAT px and the panel MIN_PANEL px. Dragging SNAP px past the chat's
  // limit makes the panel full screen; SNAP px past the panel's own limit
  // hides it. Both live: back over the line (button still held) undoes it,
  // and only letting go settles it. Either way the panel keeps its width
  // from before the drag, for when it is back.
  const MIN_CHAT = 360;
  const MIN_PANEL = 280;
  const SNAP = 48;
  let panel = $state<HTMLElement>();
  let share = 0;
  let before = 0;
  /** The chat-and-panel area, kept for the whole drag: the panel itself goes
      away (and a new one comes) if the drag hides it and brings it back. */
  let area: HTMLElement | null = null;

  // Measured against the chat-and-panel area, which stays put even while
  // full screen hides the chat (and with it the edge being dragged, so the
  // drag listens on the window).
  function drag(e: PointerEvent) {
    const r = area?.getBoundingClientRect();
    if (!r) return;
    const wide = e.clientX < r.left + MIN_CHAT - SNAP;
    const hidden = e.clientX > r.right - MIN_PANEL + SNAP;
    chat.panelWide = wide;
    chat.panelOpen = !hidden;
    if (wide || hidden) {
      share = 0;
      applyPanelWidth(before);
      return;
    }
    const edge = Math.max(r.left + MIN_CHAT, Math.min(e.clientX, r.right - MIN_PANEL));
    share = (r.right - edge) / r.width;
    applyPanelWidth(share);
  }

  function endDrag() {
    window.removeEventListener("pointermove", drag);
    window.removeEventListener("pointerup", endDrag);
    window.removeEventListener("pointercancel", endDrag);
    document.documentElement.classList.remove("resizing");
    if (share) savePrefs({ panelWidth: share });
  }

  function startDrag(e: PointerEvent) {
    e.preventDefault();
    area = panel?.parentElement ?? null;
    // The set width, not the measured one: a narrow window shows the panel
    // across the whole window whatever its setting.
    before = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--panel-w")) / 100 || DEFAULT_PANEL_WIDTH;
    share = 0;
    // Keeps the resize cursor while the pointer is off the edge.
    document.documentElement.classList.add("resizing");
    window.addEventListener("pointermove", drag);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
  }

  /** Double-click the edge: back to the usual width. */
  function resetWidth() {
    applyPanelWidth(DEFAULT_PANEL_WIDTH);
    savePrefs({ panelWidth: DEFAULT_PANEL_WIDTH });
  }

  // The panel is rebuilt for each chat, so what is typed in its boxes lives on
  // the chat (planNote; a side chat's own draft), like the main message box's draft.
  let box = $state<HTMLTextAreaElement>();

  function grow() {
    if (!box) return;
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
  }

  // The side chat's own message box. Enter sends; Shift+Enter starts a new line.
  let askBox = $state<HTMLTextAreaElement>();

  // A box shown again with kept text is sized to fit it.
  $effect(() => {
    if (box) untrack(grow);
  });
  $effect(() => {
    if (askBox) untrack(growAsk);
  });
  const side = $derived(chat.panelView === "side" ? chat.side : null);

  // A queued question taken back to edit lands in this box (see Composer).
  $effect(() => {
    if (!side?.focusDraft) return;
    void tick().then(() => {
      if (!askBox) return;
      growAsk();
      focusEnd(askBox);
    });
  });

  function growAsk() {
    if (!askBox) return;
    askBox.style.height = "auto";
    askBox.style.height = `${askBox.scrollHeight}px`;
  }

  function askKey(e: KeyboardEvent) {
    if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
    e.preventDefault();
    if (!side || !side.draft.trim()) return;
    void side.send(side.draft);
    side.draft = "";
    queueMicrotask(growAsk);
  }

  // Keep planning says what the user typed, an edited plan and the comments
  // (see Chat.keepPlanningMessage).
  function keepPlanning() {
    if (!plan) return;
    void chat.answer(plan, "deny", chat.keepPlanningMessage());
    chat.planNote = "";
  }
</script>

<!-- The tab's comments, at the top of its foot: a line with their count,
     which unfolds the list. Either part of a comment jumps to its text. -->
{#snippet commentDock()}
  <CommentDock
    items={inOrder(pending).map((c) => ({ id: c.id, quote: c.quote, text: c.text, gone: c.from < 0 }))}
    onjump={jump}
    ondrop={dropComment}
  />
{/snippet}

<aside class="panel" bind:this={panel}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" onpointerdown={startDrag} ondblclick={resetWidth}></div>
  {#if chat.panelView === "terminal"}
    <TerminalView {chat} {covered} />
  {:else if chat.panelView === "browser"}
    <BrowserView {chat} {covered} />
  {:else if chat.panelView === "info"}
    <!-- Marked so the tab keys know chat info has the keys (see tabKeys.ts). -->
    <div class="tabbed" data-tabs="info">
      {#if chat.info.shownAgent}
        <!-- Keyed, so each agent opens at its newest step. -->
        {#key chat.info.shownAgent.toolId}
          <AgentView {chat} agent={chat.info.shownAgent} />
        {/key}
      {:else}
        <InfoView {chat} />
      {/if}
    </div>
  {:else if side}
    <!-- Marked so the tab keys know a side chat has the keys (see tabKeys.ts).
         Keyed, so switching tabs swaps the whole view, box and all. -->
    {#key side.sessionId}
      <div class="tabbed" data-tabs="side">
        <div class="side">
          <ChatView chat={side} problem={null} />
        </div>
        <div class="foot">
          <div class="feedback">
            <span class="marker">&gt;</span>
            <!-- svelte-ignore a11y_autofocus -->
            <textarea
              bind:this={askBox}
              bind:value={side.draft}
              oninput={growAsk}
              onkeydown={askKey}
              rows="1"
              autofocus
              placeholder="ask on the side"
            ></textarea>
          </div>
        </div>
      </div>
    {/key}
  {:else}

  <!-- Each tab starts at its top. -->
  {#key active}
    <!-- Find (Cmd+F) in the tab; a new tab starts with it closed. -->
    <FindBar root={panel} searcher={() => (editor ? editor.searcher() : docBody ? domSearcher(docBody) : null)} />
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="body" bind:this={docBody} onclick={onClick}>
      {#if !active}
        <!-- Every tab closed: nothing here; the docs menu opens one. -->
      {:else if active === NOTES_TAB}
        {#if chat.notes !== null}
          <LiveMarkdown bind:this={editor} text={chat.notes} onchange={(t) => typeIn(NOTES_TAB, t)} ondone={flush} placeholder="notes" autofocus={chat.notes === ""} />
        {/if}
      {:else if active === PLAN_TAB && plan && chat.planPending}
        <!-- A waiting plan can be edited; Approve sends the edit. -->
        <LiveMarkdown
          bind:this={editor}
          text={planText}
          onchange={(t) => (chat.planDraft = t)}
          comments={chat.commentsFor(PLAN_TAB)}
          oncomments={(list) => chat.setComments(PLAN_TAB, list)}
        />
      {:else if active === PLAN_TAB}
        <Markdown {text} />
      {:else}
        <!-- A document: the file itself, saved as the user types. -->
        {@const tab = active}
        <LiveMarkdown
          bind:this={editor}
          {text}
          onchange={(t) => typeIn(tab, t)}
          ondone={flush}
          comments={chat.commentsFor(tab)}
          oncomments={(list) => chat.setComments(tab, list)}
        />
      {/if}
    </div>
  {/key}

  {#if docPending}
    <!-- Comments waiting to go: sending them is the doc's main action now. -->
    <div class="foot">
      {@render commentDock()}
      <div class="actions">
        <button class="primary" onclick={sendComments}>Send comments</button>
        {#if chat.isChatDoc(active)}
          <button disabled={chat.busy} onclick={saveToProject}>Save to project</button>
        {/if}
      </div>
    </div>
  {:else if chat.isChatDoc(active) || (active === NOTES_TAB && chat.notes?.trim())}
    <!-- A doc in the chat's folder, and the notes, go with the chat unless saved to the project. -->
    <div class="foot">
      <div class="actions">
        <button class="primary" disabled={chat.busy} onclick={saveToProject}>Save to project</button>
      </div>
    </div>
  {/if}
  {#if active === PLAN_TAB && plan}
    <div class="foot">
      {#if chat.planPending}
        {#if pending.length}{@render commentDock()}{/if}
        <div class="feedback">
          <span class="marker">&gt;</span>
          <textarea
            bind:this={box}
            bind:value={chat.planNote}
            oninput={grow}
            rows="1"
            placeholder="what should change? (optional)"
          ></textarea>
        </div>
        <div class="actions">
          <button class="primary" onclick={() => chat.answer(plan, "allow", undefined, undefined, chat.approvedPlan())}>Approve plan</button>
          {#if plan.alwaysLabel}
            <button onclick={() => chat.answer(plan, "always_allow", undefined, undefined, chat.approvedPlan())}>{plan.alwaysLabel}</button>
          {/if}
          <button onclick={keepPlanning}>Keep planning</button>
        </div>
      {:else}
        <span class="faint">{planOutcome(plan)}</span>
      {/if}
    </div>
  {/if}
  {/if}
</aside>

<style>
  .panel {
    width: var(--panel-w, 46%);
    box-sizing: border-box;
    flex-shrink: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--line);
    position: relative;
  }
  /* The left edge, a little wider than the line it sits on, drags the width. */
  .grip {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -4px;
    width: 8px;
    cursor: col-resize;
    z-index: 1;
  }
  .grip:hover,
  .grip:active {
    background: linear-gradient(to right, transparent 3px, var(--faint) 3px, var(--faint) 5px, transparent 5px);
  }
  /* Full screen: the panel takes the chat's place, as in a narrow window.
     Its left line would sit right against the sidebar's (or the window's
     edge), so it goes. */
  :global(.wide) .panel {
    width: auto;
    flex: 1;
    border-left: 0;
  }
  :global(.wide) .grip {
    display: none;
  }
  /* Too narrow for both: the panel takes the chat's place (see App.svelte). */
  @media (max-width: 1100px) {
    .panel {
      width: auto;
      flex: 1;
      border-left: 0;
    }
    .grip {
      display: none;
    }
  }
  /* The side chat: the same view as the main chat, in the panel's space. */
  /* Lays out as if it weren't there. */
  .tabbed {
    display: contents;
  }
  .side {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--pad-y) var(--pad-x);
  }
  .foot {
    margin: 0 var(--pad-x) var(--foot-y);
    padding-top: var(--gap);
    border-top: 1px solid var(--line);
  }
  /* The side chat's box lines up with its text column, as the main one does. */
  .tabbed .foot {
    margin: 0 max(var(--pad-x), calc((100% - var(--col)) / 2)) var(--foot-y);
  }
  .feedback {
    display: flex;
  }
  .marker {
    width: 2.5ch;
    flex-shrink: 0;
    color: var(--faint);
  }
  textarea {
    flex: 1;
    min-width: 0;
    resize: none;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--bright);
    line-height: inherit;
    max-height: 30vh;
    caret-color: var(--accent);
  }
  textarea::placeholder {
    color: var(--faint);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0 3ch;
    margin-top: var(--foot-gap);
  }
  .actions button:not(.primary):hover {
    color: var(--bright);
  }
  /* Alone in its foot (a doc's save button): no gap under the line beyond the
     foot's own, as the plan's first row sits. */
  .foot > .actions:first-child {
    margin-top: 0;
  }
  .faint {
    color: var(--faint);
  }
</style>
