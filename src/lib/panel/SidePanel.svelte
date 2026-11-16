<script lang="ts">
  // The side panel: a space beside the chat, in one of four views picked in
  // the top bar, which also holds the view's own row: docs' tabs, the side
  // chat's name, the browser's address, the terminal's tabs (see PanelBar). Terminal:
  // the project's shells (see TerminalView). Browser: a localhost
  // page Claude can drive too (see BrowserView). Docs: one tab for the plan and one for each markdown file
  // the chat wrote or a link opened, edited in place and saved as the user
  // types, like the notes; the plan's answer buttons live at the foot
  // of its tab while Claude waits (mouse only, like every approval: no key
  // approves a plan). Side chat: one tab for the side conversation, its own
  // message box at the foot. Showing and hiding the panel is the top bar's.
  import { contentClick } from "../clicks";
  import ChatView from "../chat/ChatView.svelte";
  import LiveMarkdown from "../chat/LiveMarkdown.svelte";
  import BrowserView from "./BrowserView.svelte";
  import TerminalView from "./TerminalView.svelte";
  import Markdown from "../chat/Markdown.svelte";
  import { onDestroy, untrack } from "svelte";
  import { NOTES_TAB, PLAN_TAB, planOutcome, type Chat } from "../state/chat.svelte";
  import { folderOf } from "../paths";
  import { applyPanelWidth, DEFAULT_PANEL_WIDTH, savePrefs } from "../themes";
  import { planOf } from "../tools";

  // `covered`: something is drawn over the panel (settings, an image), so the
  // browser's page, which would sit on top of it, is put out of sight.
  let { chat, covered = false }: { chat: Chat; covered?: boolean } = $props();

  // A relative link in a document starts from that document's folder, as in
  // any markdown viewer; in the plan, from the folder Claude works in.
  const onClick = contentClick({
    base: () => (chat.activeTab === PLAN_TAB || chat.activeTab === NOTES_TAB ? chat.root : folderOf(chat.activeTab)),
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
      else if (tab !== PLAN_TAB) void chat.refreshDoc(tab);
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

  function flush() {
    clearTimeout(saveTimer);
    const edit = unsaved;
    unsaved = null;
    // Claude wrote the file since this was typed: its text is on disk and in
    // the editor now, so saving this would quietly undo it.
    if (!edit || edit.text !== shownText(edit.tab)) return;
    void (edit.tab === NOTES_TAB ? chat.saveNotes(edit.text) : chat.saveDoc(edit.tab, edit.text));
  }

  onDestroy(flush);

  /** The waiting plan as it stands: the user's edit, or Claude's text. */
  const planText = $derived(plan ? (chat.planDraft ?? planOf(plan.input).text) : "");

  /** The user's edit to send with Approve, if they changed anything. */
  function editedPlan(): string | undefined {
    if (!plan || chat.planDraft === null) return undefined;
    return chat.planDraft === planOf(plan.input).text ? undefined : chat.planDraft;
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
  let panel: HTMLElement;
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
    area = panel.parentElement;
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

  let feedback = $state("");
  let box = $state<HTMLTextAreaElement>();

  function grow() {
    if (!box) return;
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
  }

  // The side chat's own message box. Enter sends; Shift+Enter starts a new line.
  let ask = $state("");
  let askBox = $state<HTMLTextAreaElement>();
  const side = $derived(chat.panelView === "side" ? chat.side : null);

  function growAsk() {
    if (!askBox) return;
    askBox.style.height = "auto";
    askBox.style.height = `${askBox.scrollHeight}px`;
  }

  function askKey(e: KeyboardEvent) {
    if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
    e.preventDefault();
    if (!side || !ask.trim()) return;
    void side.send(ask);
    ask = "";
    queueMicrotask(growAsk);
  }

  // An edited plan goes back with the answer: whole, so Claude sees exactly
  // what the user changed.
  function keepPlanning() {
    if (!plan) return;
    const edited = editedPlan();
    const note = feedback.trim() || "Keep planning.";
    void chat.answer(plan, "deny", edited ? `I edited the plan; here is my version:\n\n${edited}\n\n${note}` : note);
    feedback = "";
  }
</script>

<aside class="panel" bind:this={panel}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" title="Drag to resize; double-click to reset" onpointerdown={startDrag} ondblclick={resetWidth}></div>
  {#if chat.panelView === "terminal"}
    <TerminalView {chat} {covered} />
  {:else if chat.panelView === "browser"}
    <BrowserView {chat} {covered} />
  {:else if side}
    <div class="side">
      <ChatView chat={side} problem={null} />
    </div>
    <div class="foot">
      <div class="feedback">
        <span class="marker">&gt;</span>
        <!-- svelte-ignore a11y_autofocus -->
        <textarea
          bind:this={askBox}
          bind:value={ask}
          oninput={growAsk}
          onkeydown={askKey}
          rows="1"
          autofocus
          placeholder="ask on the side"
        ></textarea>
      </div>
    </div>
  {:else}

  <!-- Each tab starts at its top. -->
  {#key active}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="body" onclick={onClick}>
      {#if active === NOTES_TAB}
        {#if chat.notes !== null}
          <LiveMarkdown text={chat.notes} onchange={(t) => typeIn(NOTES_TAB, t)} ondone={flush} placeholder="notes" autofocus={chat.notes === ""} />
        {/if}
      {:else if active === PLAN_TAB && plan && chat.planPending}
        <!-- A waiting plan can be edited; Approve sends the edit. -->
        <LiveMarkdown text={planText} onchange={(t) => (chat.planDraft = t)} />
      {:else if active === PLAN_TAB}
        <Markdown {text} />
      {:else}
        <!-- A document: the file itself, saved as the user types. -->
        {@const tab = active}
        <LiveMarkdown {text} onchange={(t) => typeIn(tab, t)} ondone={flush} />
      {/if}
    </div>
  {/key}

  {#if active === PLAN_TAB && plan}
    <div class="foot">
      {#if chat.planPending}
        <div class="feedback">
          <span class="marker">&gt;</span>
          <textarea
            bind:this={box}
            bind:value={feedback}
            oninput={grow}
            rows="1"
            placeholder="what should change? (optional)"
          ></textarea>
        </div>
        <div class="actions">
          <button class="primary" onclick={() => chat.answer(plan, "allow", undefined, undefined, editedPlan())}>Approve plan</button>
          {#if plan.alwaysLabel}
            <button onclick={() => chat.answer(plan, "always_allow", undefined, undefined, editedPlan())}>{plan.alwaysLabel}</button>
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
  .actions button:hover {
    color: var(--bright);
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    padding: 0 1ch;
  }
  .actions .primary:hover {
    color: var(--bg);
  }
  .faint {
    color: var(--faint);
  }
</style>
