<script lang="ts">
  // The docs button at the end of the docs view's row, and its menu: this
  // chat's notes, plan and docs, and the project's docs (shared by all its
  // chats), read fresh each time it opens. A row opens its doc; the row under
  // the pointer shows save (chat docs), rename and delete. A doc can be
  // dragged to the other group (saved to the project, or moved back into the
  // chat), onto the tab row (opened), or into the message box (added like a
  // dropped file).
  import BarIcon from "../BarIcon.svelte";
  import RenameField from "../RenameField.svelte";
  import { dropFiles, type DocEntry } from "../ipc";
  import { NOTES_TAB, PLAN_TAB, type Chat } from "../state/chat.svelte";

  let { chat }: { chat: Chat } = $props();

  type Group = "chat" | "project";
  /** Where a dragged doc would land. */
  type Target = Group | "tabs" | "composer";

  let open = $state(false);
  let box = $state<HTMLElement>();
  let lists = $state<{ chat: DocEntry[]; project: DocEntry[] }>({ chat: [], project: [] });
  /** The doc being renamed, or asked about deleting, by path. */
  let renaming = $state<string | null>(null);
  let deleting = $state<string | null>(null);
  /** A click away while renaming: the rename is kept (by the field's blur), then the menu closes. */
  let closeAfterRename = false;

  async function reload() {
    lists = await chat.listDocs();
  }

  function toggle() {
    if (open) return close();
    open = true;
    void reload();
  }

  function close() {
    open = false;
    renaming = null;
    deleting = null;
  }

  // A click outside the menu, or Escape, closes it.
  $effect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (box?.contains(e.target as Node)) return;
      if (renaming) closeAfterRename = true;
      else close();
    };
    const escape = (e: KeyboardEvent) => {
      // The rename field takes its own Escape.
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (deleting) deleting = null;
      else close();
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", escape);
    };
  });

  function show(tab: string) {
    if (dragged) return;
    close();
    if (tab === NOTES_TAB || tab === PLAN_TAB) chat.openPanel(tab);
    else void chat.openFile(tab);
  }

  /** A name without its ending, for renaming. */
  const stem = (name: string) => name.replace(/\.(md|markdown)$/i, "");

  async function finishRename(path: string, name: string, save: boolean, text: string) {
    renaming = null;
    if (save && text.trim() && text.trim() !== stem(name)) {
      await chat.renameDoc(path, text);
      await reload();
    }
    if (closeAfterRename) {
      closeAfterRename = false;
      close();
    }
  }

  async function saveToProject(path: string) {
    await chat.flushTyping?.();
    await chat.saveToProject(path);
    await reload();
  }

  async function saveNotes() {
    await chat.flushTyping?.();
    await chat.saveNotesToProject();
    await reload();
  }

  async function moveToChat(path: string) {
    await chat.moveToChat(path);
    await reload();
  }

  async function remove(path: string) {
    deleting = null;
    await chat.deleteDoc(path);
    await reload();
  }

  // ---- Dragging a doc ----

  /** The doc pressed on, until the pointer moves far enough to drag it. */
  let pressed: { doc: DocEntry; from: Group; x: number; y: number } | null = null;
  let drag = $state<{ doc: DocEntry; from: Group; x: number; y: number; over: Target | null } | null>(null);
  /** A drag just ended: the click that follows it opens nothing. */
  let dragged = false;

  const hints: Record<Target, string> = {
    project: "save to project",
    chat: "move to this chat",
    tabs: "open",
    composer: "add to message",
  };

  function press(e: PointerEvent, doc: DocEntry, from: Group) {
    if (e.button !== 0 || renaming || deleting) return;
    pressed = { doc, from, x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", cancel);
  }

  function targetAt(x: number, y: number, from: Group): Target | null {
    const at = document.elementFromPoint(x, y);
    if (!at) return null;
    const group = at.closest("[data-doc-group]")?.getAttribute("data-doc-group") as Group | undefined;
    if (group && box?.contains(at)) return group === from ? null : group;
    if (box?.contains(at)) return null;
    if (at.closest("[data-doc-drop]")) return "composer";
    if (at.closest("[data-doc-tabs]")) return "tabs";
    return null;
  }

  function move(e: PointerEvent) {
    if (!pressed) return;
    if (!drag && Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 4) return;
    const { doc, from } = pressed;
    drag = { doc, from, x: e.clientX, y: e.clientY, over: targetAt(e.clientX, e.clientY, from) };
  }

  function stop() {
    pressed = null;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", release);
    window.removeEventListener("pointercancel", cancel);
  }

  function cancel() {
    stop();
    drag = null;
  }

  function release() {
    stop();
    const done = drag;
    drag = null;
    if (!done) return;
    dragged = true;
    setTimeout(() => (dragged = false));
    const path = done.doc.path;
    if (done.over === "project") void saveToProject(path);
    else if (done.over === "chat") void moveToChat(path);
    else if (done.over === "tabs") {
      close();
      void chat.openFile(path);
    } else if (done.over === "composer") {
      close();
      dropFiles([path]);
    }
  }
</script>

{#snippet row(doc: DocEntry, from: Group)}
  {#if renaming === doc.path}
    <div class="row editing">
      <RenameField value={stem(doc.name)} done={(save, text) => finishRename(doc.path, doc.name, save, text)} />
    </div>
  {:else if deleting === doc.path}
    <div class="row editing" role="alertdialog">
      <span class="name question">delete {doc.name}?</span>
      <span class="actions shown">
        <button class="danger" onclick={() => remove(doc.path)}>yes</button>
        <button onclick={() => (deleting = null)}>no</button>
      </span>
    </div>
  {:else}
    <div class="row" class:held={drag?.doc.path === doc.path}>
      <button
        class="name"
        role="menuitem"
        title={doc.path}
        onclick={() => show(doc.path)}
        onpointerdown={(e) => press(e, doc, from)}>{doc.name}</button
      >
      <span class="actions">
        {#if from === "chat"}
          <button disabled={chat.busy} onclick={() => saveToProject(doc.path)}>save</button>
        {/if}
        <button onclick={() => (renaming = doc.path)}>rename</button>
        <button class="danger" onclick={() => (deleting = doc.path)}>delete</button>
      </span>
    </div>
  {/if}
{/snippet}

<span class="picker" bind:this={box}>
  <button class="icon" class:open title="Docs" aria-haspopup="menu" aria-expanded={open} onclick={toggle}>
    <BarIcon name="docs" />
  </button>
  {#if open}
    <div class="menu" role="menu" class:dragging={drag}>
      <div class="group" class:target={drag?.over === "chat"} data-doc-group="chat">
        <div class="heading">this chat</div>
        <div class="row">
          <button class="name" role="menuitem" onclick={() => show(NOTES_TAB)}>notes</button>
          {#if chat.notes?.trim()}
            <span class="actions">
              <button disabled={chat.busy} onclick={saveNotes}>save</button>
            </span>
          {/if}
        </div>
        {#if chat.planTab}
          <div class="row">
            <button class="name" role="menuitem" onclick={() => show(PLAN_TAB)}>plan</button>
          </div>
        {/if}
        {#each lists.chat as doc (doc.path)}
          {@render row(doc, "chat")}
        {/each}
      </div>
      {#if lists.project.length || drag?.from === "chat"}
        <div class="group" class:target={drag?.over === "project"} data-doc-group="project">
          <div class="heading">project</div>
          {#each lists.project as doc (doc.path)}
            {@render row(doc, "project")}
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</span>

<!-- The doc being dragged, under the pointer, and what dropping it does. -->
{#if drag}
  <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 8}px">
    {drag.doc.name}{#if drag.over}<span class="hint">· {hints[drag.over]}</span>{/if}
  </div>
{/if}

<style>
  .picker {
    position: relative;
    flex-shrink: 0;
    align-self: stretch;
    display: flex;
    align-items: center;
    /* The bar's gap is 2ch; the tabs beside it sit 3ch apart. */
    margin-right: 1ch;
  }
  .icon {
    display: flex;
  }
  .icon:hover {
    color: var(--fg);
  }
  .icon.open {
    color: var(--bright);
  }
  .menu {
    position: absolute;
    top: calc(100% + var(--gap) * 0.4);
    left: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 0.6);
    padding: calc(var(--gap) * 0.4) 0;
    min-width: 30ch;
    max-width: 56ch;
    max-height: 60vh;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--line);
    white-space: nowrap;
    user-select: none;
    -webkit-user-select: none;
  }
  .heading {
    padding: 0 2ch;
    color: var(--faint);
  }
  /* The group a dragged doc would land in. */
  .group.target {
    background: var(--line);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2ch;
    padding: 0 2ch;
  }
  .row:hover:not(.editing) {
    background: var(--line);
  }
  .dragging .row:hover {
    background: none;
  }
  .row.held .name {
    color: var(--faint);
  }
  .name {
    flex: 1;
    min-width: 0;
    text-align: left;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row:hover .name {
    color: var(--bright);
  }
  .question {
    color: var(--fg);
  }
  /* Save, rename, delete: on the row under the pointer only. */
  .actions {
    flex-shrink: 0;
    display: flex;
    gap: 2ch;
    visibility: hidden;
  }
  .row:hover .actions,
  .actions.shown {
    visibility: visible;
  }
  .dragging .actions {
    visibility: hidden;
  }
  .actions button {
    color: var(--dim);
  }
  .actions button:hover:not(:disabled) {
    color: var(--fg);
  }
  .actions .danger,
  .actions .danger:hover:not(:disabled) {
    color: var(--bad);
  }
  .ghost {
    position: fixed;
    z-index: 100;
    pointer-events: none;
    padding: 0 1ch;
    color: var(--bright);
    background: var(--panel);
    border: 1px solid var(--line);
    white-space: nowrap;
  }
  .hint {
    margin-left: 1ch;
    color: var(--dim);
  }
</style>
