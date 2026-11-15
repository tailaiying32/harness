<script lang="ts">
  // The side panel: a space beside the chat, in one of three views picked in
  // the top bar, which also holds the view's own row: docs' tabs, the side
  // chat's name, the browser's address (see PanelBar). Browser: a localhost
  // page Claude can drive too (see BrowserView). Docs: one tab for the plan and one for each markdown file
  // the chat wrote or a link opened; the plan's answer buttons live at the foot
  // of its tab while Claude waits (mouse only, like every approval: no key
  // approves a plan). Side chat: one tab for the side conversation, its own
  // message box at the foot. Showing and hiding the panel is the top bar's.
  import { contentClick } from "../clicks";
  import ChatView from "../chat/ChatView.svelte";
  import LiveMarkdown from "../chat/LiveMarkdown.svelte";
  import BrowserView from "./BrowserView.svelte";
  import Markdown from "../chat/Markdown.svelte";
  import { onDestroy } from "svelte";
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

  // ---- Notes: saved a moment after typing stops, and when editing ends ----
  let notesTimer: ReturnType<typeof setTimeout> | undefined;
  let notesUnsaved: string | null = null;

  $effect(() => {
    if (active === NOTES_TAB) void chat.loadNotes();
  });

  function typeNotes(text: string) {
    chat.notes = text;
    notesUnsaved = text;
    clearTimeout(notesTimer);
    notesTimer = setTimeout(flushNotes, 400);
  }

  function flushNotes() {
    clearTimeout(notesTimer);
    if (notesUnsaved === null) return;
    const text = notesUnsaved;
    notesUnsaved = null;
    void chat.saveNotes(text);
  }

  onDestroy(flushNotes);

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
  // window. The chat keeps at least MIN_CHAT px and the panel MIN_PANEL px.
  const MIN_CHAT = 360;
  const MIN_PANEL = 280;
  let panel: HTMLElement;
  let share = 0;

  function drag(e: PointerEvent) {
    const chatLeft = panel.previousElementSibling?.getBoundingClientRect().left ?? 0;
    const width = window.innerWidth;
    const edge = Math.max(chatLeft + MIN_CHAT, Math.min(e.clientX, width - MIN_PANEL));
    share = (width - edge) / width;
    applyPanelWidth(share);
  }

  function startDrag(e: PointerEvent) {
    e.preventDefault();
    const grip = e.currentTarget as HTMLElement;
    grip.setPointerCapture(e.pointerId);
    grip.onpointermove = drag;
    grip.onpointerup = grip.onpointercancel = () => {
      grip.onpointermove = grip.onpointerup = grip.onpointercancel = null;
      if (share) savePrefs({ panelWidth: share });
    };
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
  {#if chat.panelView === "browser"}
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
          <LiveMarkdown text={chat.notes} onchange={typeNotes} ondone={flushNotes} placeholder="notes" autofocus={chat.notes === ""} />
        {/if}
      {:else if active === PLAN_TAB && plan && chat.planPending}
        <!-- A waiting plan can be edited; Approve sends the edit. -->
        <LiveMarkdown text={planText} onchange={(t) => (chat.planDraft = t)} />
      {:else}
        <Markdown {text} />
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
  /* Too narrow for both: the panel takes the chat's place (see App.svelte). */
  @media (max-width: 1100px) {
    .panel {
      width: auto;
      flex: 1;
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
