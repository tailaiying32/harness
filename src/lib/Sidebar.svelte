<script lang="ts">
  // Projects and their chats: bare titles under each folder, a mark for what
  // each chat is doing. Nothing here redraws while a chat streams.
  import type { AppState, Dot } from "./state/app.svelte";
  import BranchMark from "./BranchMark.svelte";
  import { prLabel, shortPath } from "./paths";

  let { app, onsettings }: { app: AppState; onsettings: () => void } = $props();

  const glyph: Record<Dot, string> = { working: "●", waiting: "●", unseen: "●", idle: "○" };
  const meaning: Record<Dot, string> = {
    working: "Working",
    waiting: "Needs your answer",
    unseen: "Finished since you last looked",
    idle: "",
  };

  /** The trash list is showing. */
  let trashOpen = $state(false);

  /** How long a trashed chat has left before it is deleted for good. */
  const TRASH_DAYS = 30;
  function daysLeft(trashedAt: number | null): string {
    const left = Math.ceil(TRASH_DAYS - (Date.now() - (trashedAt ?? Date.now())) / 86_400_000);
    return left <= 1 ? "last day" : `${left}d left`;
  }

  /** The chat being renamed, and the text typed so far. */
  let renaming = $state<string | null>(null);
  let draft = $state("");

  function startRename(id: string, title: string) {
    draft = title;
    renaming = id;
  }

  async function finishRename(save: boolean) {
    const id = renaming;
    renaming = null;
    if (save && id) await app.rename(id, draft);
  }

  /** Puts the cursor in the box with the whole name selected. */
  function focusAll(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  // One click: the trash is the safety net.
  async function remove(id: string) {
    await app.remove(id);
  }
</script>

<aside>
  <div class="list">
    {#each app.projects as project (project.id)}
      <div>
        <div class="row folder">
          <span class="path" title={project.path}>{shortPath(project.path, app.home, 22)}/</span>
          <button class="quiet" title="New chat in this folder" onclick={() => app.newChat(project.path)}>+</button>
        </div>
        {#each project.sessions as session (session.id)}
          {@const dot = app.dot(session.id)}
          <div class="row chat" class:active={session.id === app.current.sessionId}>
            {#if renaming === session.id}
              <span class="dot {dot}">{glyph[dot]}</span>
              <input
                class="rename"
                use:focusAll
                bind:value={draft}
                onblur={() => finishRename(true)}
                onkeydown={(e) => {
                  if (e.key === "Enter") finishRename(true);
                  else if (e.key === "Escape") finishRename(false);
                }}
              />
            {:else}
              <button class="open" ondblclick={() => startRename(session.id, session.title)} onclick={() => app.open(session.id)} title="{session.title} (double-click to rename)">
                <span class="dot {dot}" title={meaning[dot]}>{glyph[dot]}</span>
                <span class="title">{session.title || "untitled"}</span>
              </button>
              <!-- The branch mark and delete share one slot: the mark normally,
                   delete while the pointer is on the row. -->
              <span class="end">
                {#if session.branch}
                  <span class="branch" title={prLabel(session.pr, session.branch)}><BranchMark pr={session.pr} /></span>
                {/if}
                <button class="quiet" title="Move this chat to the trash" onclick={() => remove(session.id)}>×</button>
              </span>
            {/if}
          </div>
        {/each}
      </div>
    {/each}
    {#if app.trash.length > 0}
      <div class="trash">
        <button class="row folder" onclick={() => (trashOpen = !trashOpen)}>
          <span class="path">trash ({app.trash.length})</span>
          <span>{trashOpen ? "−" : "+"}</span>
        </button>
        {#if trashOpen}
          {#each app.trash as session (session.id)}
            <div class="row chat" class:active={session.id === app.current.sessionId}>
              <button class="open" onclick={() => app.open(session.id)} title="{session.title} (send a message to restore it)">
                <span class="dot">·</span>
                <span class="title">{session.title || "untitled"}</span>
              </button>
              <!-- Days left normally; "restore" while the pointer is on the row. -->
              <span class="end wide">
                <span class="left">{daysLeft(session.trashed_at)}</span>
                <button class="quiet" title="Restore this chat" onclick={() => app.restore(session.id)}>restore</button>
              </span>
            </div>
          {/each}
          <div class="row chat">
            <button class="empty" onclick={() => app.emptyTrash()}>empty trash</button>
          </div>
        {/if}
      </div>
    {/if}
    {#if app.problem}<div class="problem">{app.problem}</div>{/if}
  </div>

  <div class="foot">
    <button onclick={() => app.newChat()}>+ new chat</button>
    <button onclick={onsettings}>settings</button>
  </div>
</aside>

<style>
  aside {
    width: var(--side);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    padding: var(--pad-y) calc(var(--pad-x) * 0.6);
    border-right: 1px solid var(--line);
    min-height: 0;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 1.6);
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 1ch;
  }
  .folder {
    color: var(--faint);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chat {
    margin-top: calc(var(--gap) * 0.4);
    color: var(--dim);
  }
  .chat.active {
    color: var(--bright);
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    text-align: left;
    color: inherit;
  }
  .open:hover {
    color: var(--fg);
  }
  .chat.active .open:hover {
    color: var(--bright);
  }
  .dot {
    width: 2ch;
    flex-shrink: 0;
    color: var(--faint);
  }
  /* Same meaning in every theme: grey working, yellow needs you, blue
     finished since you last looked. Each theme supplies its own blue and yellow. */
  .dot.working {
    color: var(--dim);
  }
  .dot.waiting {
    color: var(--yellow);
  }
  .dot.unseen {
    color: var(--blue);
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Shown only while the pointer is on the row, so titles stay bare. */
  .quiet {
    color: var(--faint);
    visibility: hidden;
  }
  .row:hover .quiet {
    visibility: visible;
  }
  .quiet:hover {
    color: var(--fg);
  }
  .end {
    flex-shrink: 0;
    display: grid;
    width: 1.5ch;
    justify-items: end;
  }
  .end > * {
    grid-area: 1 / 1;
  }
  .end.wide {
    width: auto;
  }
  .row:hover .branch,
  .row:hover .left {
    visibility: hidden;
  }
  .trash {
    margin-top: auto;
  }
  .trash .folder {
    width: 100%;
    text-align: left;
  }
  .trash .folder:hover {
    color: var(--fg);
  }
  .left {
    flex-shrink: 0;
    color: var(--faint);
  }
  .empty {
    color: var(--faint);
  }
  .empty:hover {
    color: var(--fg);
  }
  .empty {
    padding-left: 2ch;
  }
  /* Edited in place: the title's own text with a thin underline, none of the
     system's field box or blue focus ring. */
  .rename {
    flex: 1;
    min-width: 0;
    color: var(--bright);
    background: none;
    border: 0;
    border-bottom: 1px solid var(--dim);
    border-radius: 0;
    padding: 0;
    /* The row's gap sits between mark and field; titles have none. */
    margin: 0 0 0 -1ch;
    outline: 0;
    appearance: none;
    line-height: inherit;
  }
  .problem {
    color: var(--bad);
    overflow-wrap: anywhere;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    margin-top: var(--gap);
  }
  .foot button {
    color: var(--faint);
  }
  .foot button:hover {
    color: var(--fg);
  }
  /* A narrow window is all chat. */
  @media (max-width: 720px) {
    aside {
      display: none;
    }
  }
</style>
