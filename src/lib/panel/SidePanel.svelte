<script lang="ts">
  // The side panel: a tabbed space beside the chat. One tab for the plan and
  // one for each markdown file the chat wrote or a link opened. The plan's answer buttons live
  // here, at the foot of its tab, while Claude waits. Mouse only, like every
  // approval: no key approves a plan.
  //
  // The tab strip holds what the chat produced. Things that aren't the chat's
  // (a terminal, later) go at the right end of the bar, before the close mark.
  import { contentClick } from "../clicks";
  import Markdown from "../chat/Markdown.svelte";
  import { PLAN_TAB, planOutcome, type Chat } from "../state/chat.svelte";
  import { folderOf } from "../paths";
  import { applyPanelWidth, DEFAULT_PANEL_WIDTH, savePrefs } from "../themes";
  import { baseName, planOf } from "../tools";

  let { chat }: { chat: Chat } = $props();

  // A relative link in a document starts from that document's folder, as in
  // any markdown viewer; in the plan, from the project folder.
  const onClick = contentClick({
    base: () => (chat.activeTab === PLAN_TAB ? chat.projectPath : folderOf(chat.activeTab)),
    openDoc: (path) => void chat.openFile(path),
  });

  const active = $derived(chat.activeTab);
  const plan = $derived(chat.planTab ? chat.plan : null);
  const text = $derived(
    active === PLAN_TAB
      ? plan
        ? planOf(plan.input).text
        : ""
      : (chat.docs.find((doc) => doc.path === active)?.text ?? ""),
  );

  /** A file's name; with its folder when two tabs share a name. */
  function label(path: string): string {
    const name = baseName(path);
    if (chat.openDocs.filter((doc) => baseName(doc.path) === name).length < 2) return name;
    return path.split(/[\\/]/).slice(-2).join("/");
  }

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

  function keepPlanning() {
    if (!plan) return;
    void chat.answer(plan, "deny", feedback.trim() || "Keep planning.");
    feedback = "";
  }
</script>

<aside class="panel" bind:this={panel}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" title="Drag to resize; double-click to reset" onpointerdown={startDrag} ondblclick={resetWidth}></div>
  <div class="bar">
    <div class="tabs">
      {#if plan}
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
    <button class="close" title="Close the panel" onclick={() => (chat.panelOpen = false)}>▸</button>
  </div>

  <!-- Each tab starts at its top. -->
  {#key active}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="body" onclick={onClick}>
      <Markdown {text} />
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
          <button class="primary" onclick={() => chat.answer(plan, "allow")}>Approve plan</button>
          {#if plan.alwaysLabel}
            <button onclick={() => chat.answer(plan, "always_allow")}>{plan.alwaysLabel}</button>
          {/if}
          <button onclick={keepPlanning}>Keep planning</button>
        </div>
      {:else}
        <span class="faint">{planOutcome(plan)}</span>
      {/if}
    </div>
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
  .bar {
    display: flex;
    gap: 3ch;
    padding: calc(var(--pad-y) * 0.6) var(--pad-x) calc(var(--gap) * 0.7);
    border-bottom: 1px solid var(--line);
    color: var(--faint);
    white-space: nowrap;
  }
  /* Many tabs scroll sideways; the close mark stays put. */
  .tabs {
    display: flex;
    gap: 3ch;
    min-width: 0;
    flex: 1;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    flex-shrink: 0;
    display: flex;
    gap: 1.5ch;
  }
  .bar button:hover {
    color: var(--fg);
  }
  .tab.on button:first-child {
    color: var(--bright);
  }
  /* A tab's close mark shows on the tab in front, or under the pointer. */
  .shut {
    visibility: hidden;
  }
  .tab.on .shut,
  .tab:hover .shut {
    visibility: visible;
  }
  .close {
    flex-shrink: 0;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--pad-y) var(--pad-x);
  }
  .foot {
    margin: 0 var(--pad-x) calc(var(--pad-y) * 0.7);
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
    margin-top: calc(var(--gap) * 0.6);
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
  .accent {
    color: var(--accent);
  }
  .faint {
    color: var(--faint);
  }
</style>
