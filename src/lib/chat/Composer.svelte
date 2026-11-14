<script lang="ts">
  // The message box: a prompt marker, a plain text area, and one faint line of
  // context under it. Enter sends; Shift+Enter adds a line.
  import { onMount, tick } from "svelte";
  import { pickFolder, type Mode, type ModelInfo } from "../ipc";
  import { shortPath } from "../paths";
  import type { Chat } from "../state/chat.svelte";
  import { savePrefs } from "../themes";

  let { chat, ready, home, models }: { chat: Chat; ready: boolean; home: string; models: ModelInfo[] } = $props();

  let box: HTMLTextAreaElement;

  // Opening a chat puts the cursor in its message box, sized to any unsent text.
  onMount(() => {
    grow();
    box.focus();
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
    void chat.changeMode(modes[(index + 1) % modes.length].id);
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

  function grow() {
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
  }

  async function send() {
    const text = chat.draft;
    if (!text.trim() || !ready) return;
    chat.draft = "";
    await tick();
    grow();
    void chat.send(text);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void send();
    }
  }
</script>

<div class="composer">
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
