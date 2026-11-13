<script lang="ts">
  // The message box: a prompt marker, a plain text area, and one faint line of
  // context under it. Enter sends; Shift+Enter adds a line.
  import { tick } from "svelte";
  import { pickFolder, type Mode } from "../ipc";
  import type { Chat } from "../state/chat.svelte";
  import { applySize, applyTheme, savePrefs, sizes, themes, type Size } from "../themes";

  let {
    chat,
    ready,
    home,
    theme = $bindable(),
    size = $bindable(),
  }: { chat: Chat; ready: boolean; home: string; theme: string; size: Size } = $props();

  let draft = $state("");
  let box: HTMLTextAreaElement;

  const modes: { id: Mode; label: string }[] = [
    { id: "default", label: "ask before edits" },
    { id: "acceptEdits", label: "auto-accept edits" },
    { id: "plan", label: "plan" },
  ];
  const modeLabel = $derived(modes.find((m) => m.id === chat.mode)?.label ?? chat.mode);

  function nextMode() {
    const index = modes.findIndex((m) => m.id === chat.mode);
    void chat.changeMode(modes[(index + 1) % modes.length].id);
  }

  const folder = $derived.by(() => {
    const path = chat.projectPath;
    const base = home.replace(/[\\/]$/, "");
    const short = base && path.startsWith(base) ? "~" + path.slice(base.length) : path;
    if (short.length <= 40) return short;
    // Too long for one line: keep the end, which says the most.
    return "…/" + short.split(/[\\/]/).filter(Boolean).slice(-2).join("/");
  });

  async function changeFolder() {
    const picked = await pickFolder(chat.projectPath || undefined);
    if (picked) chat.projectPath = picked;
  }

  const model = $derived(chat.model?.replace(/^claude-/, "") ?? "");

  const usage = $derived.by(() => {
    if (!chat.usage) return "";
    const names: Record<string, string> = { seven_day: "weekly", five_hour: "5-hour" };
    const name = names[chat.usage.window ?? ""];
    const percent = Math.round(chat.usage.utilization * 100);
    return name ? `${percent}% of ${name} limit` : `${percent}% of limit`;
  });

  function grow() {
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
  }

  async function send() {
    const text = draft;
    if (!text.trim() || !ready) return;
    draft = "";
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

  function pickTheme(id: string) {
    theme = id;
    applyTheme(id);
    savePrefs({ theme, size });
  }

  function pickSize(value: Size) {
    size = value;
    applySize(value);
    savePrefs({ theme, size });
  }
</script>

<div class="composer">
  <div class="inner">
    <div class="prompt">
      <span class="marker">&gt;</span>
      <textarea
        bind:this={box}
        bind:value={draft}
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
      <button onclick={nextMode} title="Switch mode">{modeLabel}</button>
      {#if model}<span>{model}</span>{/if}
      <button onclick={changeFolder} disabled={chat.started} title={chat.started ? "" : "Choose the folder"}>
        {folder}
      </button>
      <span class="spacer"></span>
      {#if usage}<span>{usage}</span>{/if}
      <!-- Theme and size live here until there is a settings page. -->
      <select value={theme} onchange={(e) => pickTheme(e.currentTarget.value)} aria-label="Theme">
        {#each themes as t (t.id)}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
      <select value={size} onchange={(e) => pickSize(e.currentTarget.value as Size)} aria-label="Size">
        {#each sizes as s (s)}
          <option value={s}>{s}</option>
        {/each}
      </select>
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
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 3ch;
    margin-top: calc(var(--gap) * 0.6);
    color: var(--faint);
  }
  .spacer {
    flex: 1;
  }
  .meta button:not(:disabled):hover,
  .meta select:hover {
    color: var(--fg);
  }
  select {
    appearance: none;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--faint);
    cursor: pointer;
  }
</style>
