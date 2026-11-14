<script lang="ts">
  // Projects and their chats: bare titles under each folder, a mark for what
  // each chat is doing. Nothing here redraws while a chat streams.
  import type { AppState, Dot } from "./state/app.svelte";
  import { shortPath } from "./paths";

  let { app, onsettings }: { app: AppState; onsettings: () => void } = $props();

  const glyph: Record<Dot, string> = { working: "●", waiting: "●", unseen: "●", idle: "○" };
  const meaning: Record<Dot, string> = {
    working: "Working",
    waiting: "Needs your answer",
    unseen: "Finished since you last looked",
    idle: "",
  };

  /** The chat waiting for a second click before it is deleted. */
  let deleting = $state<string | null>(null);

  async function remove(id: string) {
    deleting = null;
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
            {#if deleting === session.id}
              <span class="ask">delete?</span>
              <button class="danger" onclick={() => remove(session.id)}>yes</button>
              <button onclick={() => (deleting = null)}>no</button>
            {:else}
              <button class="open" onclick={() => app.open(session.id)} title={session.title}>
                <span class="dot {dot}" title={meaning[dot]}>{glyph[dot]}</span>
                <span class="title">{session.title || "untitled"}</span>
              </button>
              <button class="quiet" title="Delete this chat" onclick={() => (deleting = session.id)}>×</button>
            {/if}
          </div>
        {/each}
      </div>
    {/each}
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
  .dot.working {
    color: var(--ok);
  }
  .dot.waiting {
    color: var(--accent);
  }
  .dot.unseen {
    color: var(--fg);
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
  /* Lines up with the titles, past the mark's column. */
  .ask {
    flex: 1;
    padding-left: 2ch;
    color: var(--dim);
  }
  .chat button:not(.open):not(.quiet) {
    color: var(--dim);
  }
  .chat button.danger {
    color: var(--bad);
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
