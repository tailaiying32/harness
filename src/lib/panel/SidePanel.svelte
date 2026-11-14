<script lang="ts">
  // The side panel: a tabbed space beside the chat. One tab for the plan and
  // one for each markdown file the chat wrote. The plan's answer buttons live
  // here, at the foot of its tab, while Claude waits. Mouse only, like every
  // approval: no key approves a plan.
  //
  // The tab strip holds what the chat produced. Things that aren't the chat's
  // (a terminal, later) go at the right end of the bar, before the close mark.
  import { contentClick } from "../clicks";
  import Markdown from "../chat/Markdown.svelte";
  import { PLAN_TAB, planOutcome, type Chat } from "../state/chat.svelte";
  import { baseName, planOf } from "../tools";

  let { chat }: { chat: Chat } = $props();

  const active = $derived(chat.activeTab);
  const plan = $derived(chat.plan);
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
    if (chat.docs.filter((doc) => baseName(doc.path) === name).length < 2) return name;
    return path.split(/[\\/]/).slice(-2).join("/");
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

<aside class="panel">
  <div class="bar">
    <div class="tabs">
      {#if plan}
        <button class:on={active === PLAN_TAB} onclick={() => chat.openPanel(PLAN_TAB)}>
          {#if chat.planPending}<span class="accent">●</span>{/if}
          plan
        </button>
      {/if}
      {#each chat.docs as doc (doc.path)}
        <button class:on={active === doc.path} title={doc.path} onclick={() => chat.openPanel(doc.path)}>
          {label(doc.path)}
        </button>
      {/each}
    </div>
    <button class="close" title="Close the panel" onclick={() => (chat.panelOpen = false)}>×</button>
  </div>

  <!-- Each tab starts at its top. -->
  {#key active}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="body" onclick={contentClick}>
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
    width: 46%;
    flex-shrink: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--line);
  }
  /* Too narrow for both: the panel takes the chat's place (see App.svelte). */
  @media (max-width: 1100px) {
    .panel {
      width: auto;
      flex: 1;
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
  .tabs button {
    flex-shrink: 0;
  }
  .bar button:hover {
    color: var(--fg);
  }
  .bar button.on {
    color: var(--bright);
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
