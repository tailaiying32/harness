<script lang="ts">
  // The message box: a prompt marker, a plain text area, and one faint line of
  // context under it. Enter sends; Shift+Enter adds a line.
  import { onMount, tick } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import BranchMark from "../BranchMark.svelte";
  import { pickFolder, type Mode, type ModelInfo } from "../ipc";
  import { prLabel, prShort, shortPath } from "../paths";
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

  // Effort lives in the model menu. A level picked here is for this chat only;
  // new chats start on Claude Code's own default for their model.
  const picked = $derived(modelOptions.find((m) => m.value === chat.modelChoice));
  const efforts = $derived(picked?.efforts ?? []);
  const effort = $derived(
    efforts.includes(chat.effortChoice) ? chat.effortChoice : (picked?.default_effort ?? null),
  );

  // The line under the box has two menus, one open at a time: the chat's
  // pull requests, and the model.
  let open = $state<"prs" | "model" | null>(null);
  let menus = $state<Record<string, HTMLElement | undefined>>({});
  const toggle = (which: "prs" | "model") => (open = open === which ? null : which);

  // Any click outside the open menu, or Escape, closes it.
  $effect(() => {
    const menu = open && menus[open];
    if (!menu) return;
    const away = (e: PointerEvent) => {
      if (!menu.contains(e.target as Node)) open = null;
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        open = null;
        box.focus();
      }
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", escape);
    };
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
        rows="1"
        placeholder="Message Claude Code"
      ></textarea>
      {#if chat.busy}
        <button class="stop" onclick={() => chat.stop()}>stop</button>
      {/if}
    </div>

    <div class="meta">
      <!-- Laid out like the Claude desktop app: on a new chat the folder picker
           sits at the left; once the chat has started it is gone (the sidebar
           shows the folder). Model and mode sit at the right. -->
      {#if !chat.started}
        <button class="folder" onclick={changeFolder} title="Choose the folder this chat works in">
          {folder}
        </button>
      {:else if chat.prs.length}
        <!-- The chat's pull requests: one mark and a count; the menu lists
             them. A worktree with no pull request yet shows nothing. -->
        <span class="picker" bind:this={menus.prs}>
          <button
            class="prs"
            onclick={() => toggle("prs")}
            title={prLabel(chat.prs)}
            aria-haspopup="menu"
            aria-expanded={open === "prs"}
          >
            <BranchMark prs={chat.prs} />
            {prShort(chat.prs)}
          </button>
          {#if open === "prs"}
            <div class="menu left" role="menu">
              {#each chat.prs as pr (pr.url)}
                <button role="menuitem" class="row" title="Open on GitHub" onclick={() => openUrl(pr.url)}>
                  <span class="pr {pr.state}">#{pr.number} {pr.state}</span>
                  <span>{pr.branch}</span>
                </button>
              {/each}
              {#if chat.workPath}
                <div class="where" title="This chat works in its own worktree">{shortPath(chat.workPath, home, 60)}</div>
              {/if}
            </div>
          {/if}
        </span>
      {/if}
      <span class="spacer"></span>
      <span class="picker" bind:this={menus.model}>
        <button onclick={() => toggle("model")} title="Model and effort" aria-haspopup="menu" aria-expanded={open === "model"}>
          {picked?.name ?? chat.modelChoice}{#if effort}&ensp;{effort}{/if}
        </button>
        {#if open === "model"}
          <div class="menu" role="menu">
            {#each modelOptions as m (m.value)}
              <button
                role="menuitemradio"
                aria-checked={m.value === chat.modelChoice}
                class:on={m.value === chat.modelChoice}
                title={m.detail ?? ""}
                onclick={() => pickModel(m.value)}>{m.name}</button
              >
            {/each}
            {#if efforts.length}
              <div class="efforts" role="group" aria-label="Effort">
                <span>effort</span>
                {#each efforts as level (level)}
                  <button
                    role="menuitemradio"
                    aria-checked={level === effort}
                    class:on={level === effort}
                    onclick={() => chat.changeEffort(level)}>{level}</button
                  >
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </span>
      <button onclick={nextMode} title="Switch mode">{modeLabel}</button>
    </div>
  </div>
</div>

<style>
  .composer {
    padding: 0 var(--pad-x) var(--foot-y);
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
    margin-top: var(--foot-gap);
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
  .meta .prs {
    display: flex;
    align-items: center;
    gap: 1ch;
  }
  .pr.open {
    color: var(--ok);
  }
  .pr.draft {
    color: var(--dim);
  }
  .pr.merged {
    color: var(--purple);
  }
  .pr.closed {
    color: var(--bad);
  }
  .meta .spacer {
    flex: 1 1 0;
    min-width: 0;
  }
  .meta button:not(:disabled):hover {
    color: var(--fg);
  }
  /* A menu opens upward from its label: the model's right-aligned with it,
     the pull requests' left-aligned. A model click picks and leaves the menu
     open, so effort can be set in the same visit. */
  .picker {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + var(--gap) * 0.4);
    z-index: 10;
    display: flex;
    flex-direction: column;
    max-height: 60vh;
    overflow-y: auto;
    padding: calc(var(--gap) * 0.4) 0;
    background: var(--panel);
    border: 1px solid var(--line);
  }
  .menu button {
    text-align: left;
    padding: 0 2ch;
    color: var(--dim);
  }
  .menu button.on {
    color: var(--accent);
  }
  .menu.left {
    right: auto;
    left: 0;
  }
  .menu .row {
    display: flex;
    gap: 2ch;
  }
  /* Wide enough for "#123 merged", so the branches line up. */
  .menu .row .pr {
    min-width: 11ch;
  }
  .menu .row:hover {
    color: var(--fg);
  }
  .where {
    margin-top: calc(var(--gap) * 0.4);
    padding: calc(var(--gap) * 0.4) 2ch 0;
    border-top: 1px solid var(--line);
    color: var(--faint);
  }
  .efforts {
    display: flex;
    gap: 1.5ch;
    margin-top: calc(var(--gap) * 0.4);
    padding: calc(var(--gap) * 0.4) 2ch 0;
    border-top: 1px solid var(--line);
    color: var(--faint);
  }
  .efforts button {
    padding: 0;
  }
</style>
