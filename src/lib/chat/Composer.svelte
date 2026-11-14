<script lang="ts">
  // The message box: a prompt marker, a plain text area, and one faint line of
  // context under it. Enter sends; Shift+Enter adds a line.
  import { onMount, tick } from "svelte";
  import { pickFolder, type Mode, type ModelInfo } from "../ipc";
  import { shortPath } from "../paths";
  import type { Chat } from "../state/chat.svelte";
  import { savePrefs } from "../themes";
  import { indent, newLine, outdent, renumbered, type Edit } from "./editing";

  let {
    chat,
    ready,
    home,
    models,
    focus = 0,
  }: { chat: Chat; ready: boolean; home: string; models: ModelInfo[]; focus?: number } = $props();

  let box: HTMLTextAreaElement;
  let frame: HTMLElement;

  // Opening a chat puts the cursor in its message box, sized to any unsent text.
  onMount(() => {
    grow();
    box.focus();
  });

  // Asking for a new chat while already on an empty one changes nothing on
  // screen, so the cursor is put in the box by hand.
  $effect(() => {
    if (focus) box?.focus();
  });

  const modes: { id: Mode; label: string }[] = [
    { id: "default", label: "ask before edits" },
    { id: "acceptEdits", label: "auto-accept edits" },
    { id: "plan", label: "plan" },
    { id: "auto", label: "auto" },
  ];
  const modeLabel = $derived(modes.find((m) => m.id === chat.mode)?.label ?? chat.mode);

  function nextMode() {
    const index = modes.findIndex((m) => m.id === chat.mode);
    const mode = modes[(index + 1) % modes.length].id;
    void chat.changeMode(mode);
    // New chats start in the last mode picked. Plan is for one task, so it isn't kept.
    if (mode !== "plan") savePrefs({ mode });
  }

  const folder = $derived(shortPath(chat.projectPath, home));

  async function changeFolder() {
    const picked = await pickFolder(chat.projectPath || undefined);
    if (picked) chat.projectPath = picked;
  }

  // The list comes from Claude Code, so it shows real names ("Opus 5.5"). A
  // saved choice that is no longer listed still gets an entry, so it shows.
  const modelOptions = $derived(
    models.some((m) => m.value === chat.modelChoice)
      ? models
      : [{ value: chat.modelChoice, name: chat.modelChoice, detail: null }, ...models],
  );

  function pickModel(choice: string) {
    void chat.changeModel(choice);
    savePrefs({ model: choice });
  }

  const usage = $derived.by(() => {
    if (!chat.usage) return "";
    // Claude Code names the limit that applies to the current model, e.g.
    // "seven_day", "seven_day_overage_included" or "five_hour".
    const window = chat.usage.window ?? "";
    const name = window.startsWith("seven_day") ? "weekly" : window.startsWith("five_hour") ? "5-hour" : "";
    const percent = Math.round(chat.usage.utilization * 100);
    return name ? `${percent}% of ${name} limit` : `${percent}% of limit`;
  });

  // The box is measured by letting it collapse for a moment. The composer's
  // own height is held meanwhile: otherwise the chat above would stretch into
  // the freed space and back, and lose its place at the end on every key.
  function grow() {
    frame.style.height = `${frame.offsetHeight}px`;
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
    frame.style.height = "";
  }

  async function send() {
    const text = chat.draft;
    if (!text.trim() || !ready) return;
    chat.draft = "";
    await tick();
    grow();
    void chat.send(text);
  }

  /** Makes one replacement in the box the way typing would, so undo still works. */
  async function apply(asked: Edit) {
    // Numbered lists the edit disturbs are put back in order in the same step.
    const edit = renumbered(chat.draft, asked);
    box.setSelectionRange(edit.from, edit.to);
    const typed = edit.insert
      ? document.execCommand("insertText", false, edit.insert)
      : edit.from === edit.to || document.execCommand("delete");
    if (!typed) {
      chat.draft = chat.draft.slice(0, edit.from) + edit.insert + chat.draft.slice(edit.to);
      await tick();
    }
    box.setSelectionRange(edit.selStart, edit.selEnd);
    grow();
  }

  // The box is for writing, so its keys work as in a document: Tab indents
  // and Shift+Tab pulls back (Tab never leaves the box), and a new line in a
  // list starts the next item. Enter alone still sends.
  function onKey(e: KeyboardEvent) {
    if (e.isComposing) return;
    const { selectionStart: start, selectionEnd: end } = box;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    } else if (e.key === "Enter") {
      const edit = newLine(chat.draft, start, end);
      if (!edit) return;
      e.preventDefault();
      void apply(edit);
    } else if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      const edit = e.shiftKey ? outdent(chat.draft, start, end) : indent(chat.draft, start, end);
      if (edit) void apply(edit);
    }
  }
</script>

<div class="composer" bind:this={frame}>
  <div class="inner">
    <div class="prompt">
      <span class="marker">&gt;</span>
      <textarea
        bind:this={box}
        bind:value={chat.draft}
        oninput={grow}
        onkeydown={onKey}
        rows="2"
        placeholder="Message Claude Code"
      ></textarea>
      {#if chat.busy}
        <button class="stop" onclick={() => chat.stop()}>stop</button>
      {/if}
    </div>

    <div class="meta">
      <!-- Laid out like the Claude desktop app: on a new chat the folder picker
           sits at the left; once the chat has started it is gone (the sidebar
           shows the folder). Model, mode and usage sit at the right. -->
      {#if !chat.started}
        <button class="folder" onclick={changeFolder} title="Choose the folder this chat works in">
          {folder}
        </button>
      {/if}
      <span class="spacer"></span>
      <select value={chat.modelChoice} onchange={(e) => pickModel(e.currentTarget.value)} aria-label="Model">
        {#each modelOptions as m (m.value)}
          <option value={m.value} title={m.detail ?? ""}>{m.name}</option>
        {/each}
      </select>
      <button onclick={nextMode} title="Switch mode">{modeLabel}</button>
      {#if usage}<span>{usage}</span>{/if}
    </div>
  </div>
</div>

<style>
  .composer {
    padding: 0 var(--pad-x) calc(var(--pad-y) * 0.7);
  }
  .inner {
    max-width: var(--col);
    margin: 0 auto;
    border-top: 1px solid var(--line);
    padding-top: var(--gap);
  }
  .prompt {
    display: flex;
  }
  .marker {
    width: 2.5ch;
    flex-shrink: 0;
    color: var(--accent);
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
    max-height: 40vh;
    caret-color: var(--accent);
  }
  textarea::placeholder {
    color: var(--faint);
  }
  .stop {
    align-self: flex-start;
    color: var(--bad);
    margin-left: 2ch;
  }
  /* One line always: the folder gives way first when the window is narrow. */
  .meta {
    display: flex;
    align-items: center;
    gap: 3ch;
    white-space: nowrap;
    margin-top: calc(var(--gap) * 0.2);
    color: var(--faint);
  }
  .meta > * {
    flex-shrink: 0;
  }
  .meta .folder {
    flex-shrink: 1;
    min-width: 6ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta .spacer {
    flex: 1 1 0;
    min-width: 0;
  }
  .meta button:not(:disabled):hover,
  .meta select:hover {
    color: var(--fg);
  }
  select {
    appearance: none;
    /* As wide as the chosen option, not the longest one. */
    field-sizing: content;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--faint);
    cursor: pointer;
  }
</style>
