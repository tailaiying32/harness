<script lang="ts">
  // Projects and their chats: bare titles under each folder, a mark for what
  // each chat is doing. Nothing here redraws while a chat streams.
  import type { AppState, Dot } from "./state/app.svelte";
  import type { SessionRow } from "./ipc";
  import BranchMark from "./BranchMark.svelte";
  import RenameField from "./RenameField.svelte";
  import { prLabel, shortPath } from "./paths";
  import { loadPrefs, savePrefs } from "./themes";

  let { app, onsettings }: { app: AppState; onsettings: () => void } = $props();

  const glyph: Record<Dot, string> = { working: "●", waiting: "●", unseen: "●", idle: "○" };
  const meaning: Record<Dot, string> = {
    working: "Working",
    waiting: "Needs your answer",
    unseen: "Finished since you last looked",
    idle: "",
  };

  /** Each project shows this many of its most recently used chats. */
  const CAP = 8;

  /** How many hidden chats each project shows, in steps of CAP; until the app quits. */
  let extra = $state<Record<string, number>>({});

  /** Folders folded down to their name (plus any chat that needs you); kept across quits. */
  let folded = $state<string[]>(loadPrefs().foldedProjects);

  // ---- Project order ----
  // Fixed, set by dragging a folder name. A project seen for the first time
  // goes on top (it was just used); one with no chats left drops out.
  let order = $state<string[]>(loadPrefs().projectOrder);
  // A project's folder moved: its place and its fold go to the new path. The
  // saved settings were changed already; this is the copy kept here. Before
  // the effect below, which drops a path that is no longer a project.
  $effect(() => {
    if (!app.moved) return;
    const prefs = loadPrefs();
    order = prefs.projectOrder;
    folded = prefs.foldedProjects;
  });
  $effect(() => {
    // Before the first load the list is empty; pruning then would wipe the saved order.
    if (!app.loaded) return;
    const paths = app.projects.map((p) => p.path);
    const next = [...paths.filter((p) => !order.includes(p)), ...order.filter((p) => paths.includes(p))];
    if (next.join("\n") !== order.join("\n")) {
      order = next;
      savePrefs({ projectOrder: order });
    }
  });
  const projects = $derived(
    [...app.projects].sort((a, b) => {
      const i = order.indexOf(a.path);
      const j = order.indexOf(b.path);
      return (i < 0 ? -1 : i) - (j < 0 ? -1 : j);
    }),
  );

  /** The folder being pressed; it starts dragging once the pointer moves a few pixels. */
  let pressed: { path: string; y: number } | null = null;
  let dragging = $state<string | null>(null);
  /** Set by a drag, so the click that ends it doesn't fold the folder too. */
  let dragged = false;
  const blocks: Record<string, HTMLElement> = $state({});

  function press(e: PointerEvent, path: string) {
    if (e.button !== 0) return;
    pressed = { path, y: e.clientY };
    dragged = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  // Live: the folder moves among the others as the pointer passes their middles.
  function drag(e: PointerEvent) {
    if (!pressed) return;
    if (!dragging && Math.abs(e.clientY - pressed.y) < 4) return;
    dragging = pressed.path;
    dragged = true;
    const others = order.filter((p) => p !== dragging);
    const to = others.filter((p) => {
      const r = blocks[p]?.getBoundingClientRect();
      return r && e.clientY > r.top + r.height / 2;
    }).length;
    const next = [...others.slice(0, to), dragging, ...others.slice(to)];
    if (next.join("\n") !== order.join("\n")) order = next;
  }

  function drop() {
    if (dragging) savePrefs({ projectOrder: order });
    pressed = null;
    dragging = null;
  }

  /** The folder name folds one step: back to the first CAP chats, then away. */
  function fold(project: { id: string; path: string }) {
    if (dragged) {
      dragged = false;
      return;
    }
    if (extra[project.id]) {
      extra[project.id] = 0;
      return;
    }
    folded = folded.includes(project.path) ? folded.filter((p) => p !== project.path) : [...folded, project.path];
    savePrefs({ foldedProjects: folded });
  }

  /** Shown even in a folded folder: the open chat, and ones working, waiting or finished unseen. */
  function needsYou(session: SessionRow): boolean {
    return session.id === app.current.sessionId || app.dot(session.id) !== "idle";
  }

  /** Shown even past the cap: those, and chats with a pull request still open. */
  function keep(session: SessionRow): boolean {
    return needsYou(session) || session.prs.some((pr) => pr.state === "open" || pr.state === "draft");
  }

  /** The chat being renamed. */
  let renaming = $state<string | null>(null);

  async function finishRename(save: boolean, text: string) {
    const id = renaming;
    renaming = null;
    if (save && id) await app.rename(id, text);
  }

  /** The chat asking "delete? yes / no" before it goes to the trash. */
  let deleting = $state<string | null>(null);

  // Asks first: the trash is tucked away in settings, so a misclick is hard to spot.
  async function remove(id: string) {
    deleting = null;
    await app.remove(id);
  }
</script>

<aside>
  <!-- On a Mac: the strip by the window buttons drags the window, like a title bar. -->
  <div class="drag" data-tauri-drag-region></div>
  <div class="list">
    {#each projects as project (project.id)}
      {@const isFolded = folded.includes(project.path)}
      {@const capped = project.sessions.filter((s, i) => (isFolded ? needsYou(s) : i < CAP || keep(s)))}
      {@const hidden = isFolded ? [] : project.sessions.filter((s) => !capped.includes(s))}
      {@const opened = hidden.slice(0, extra[project.id] ?? 0)}
      {@const shown = opened.length ? project.sessions.filter((s) => capped.includes(s) || opened.includes(s)) : capped}
      <div bind:this={blocks[project.path]} class:dragging={dragging === project.path}>
        <div class="row folder">
          <!-- Click folds a step; drag moves the folder among the others. The
               caret points down while open, right once folded to the name. -->
          <button
            class="path"
            title="{project.path} (click to fold, drag to move)"
            onclick={() => fold(project)}
            onpointerdown={(e) => press(e, project.path)}
            onpointermove={drag}
            onpointerup={drop}
            onpointercancel={drop}
          >
            <svg class="caret" class:shut={isFolded} viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg>
            <span class="name" class:missing={project.missing}>{shortPath(project.path, app.home, 20)}/</span>
          </button>
          {#if project.missing}
            <!-- The folder was moved or renamed: point at where it is, and its chats follow. -->
            <button class="locate" title="{project.path} is gone. Pick where it is now." onclick={() => app.locate(project.path)}>locate</button>
          {:else}
            <button class="quiet" title="New chat in this folder" onclick={() => app.newChat(project.path)}>+</button>
          {/if}
        </div>
        {#each shown as session (session.id)}
          {@const dot = app.dot(session.id)}
          <div class="row chat" class:active={session.id === app.current.sessionId}>
            {#if deleting === session.id}
              <span class="ask">delete?</span>
              <button class="danger" onclick={() => remove(session.id)}>yes</button>
              <button class="no" onclick={() => (deleting = null)}>no</button>
            {:else if renaming === session.id}
              <span class="dot {dot}">{glyph[dot]}</span>
              <RenameField class="in-row" value={session.title} done={finishRename} />
            {:else}
              <button class="open" ondblclick={() => (renaming = session.id)} onclick={() => app.open(session.id)} title="{session.title} (double-click to rename)">
                <span class="dot {dot}" title={meaning[dot]}>{glyph[dot]}</span>
                <span class="title">{session.title || "untitled"}</span>
              </button>
              <!-- The pull request mark and delete share one slot: the mark
                   normally, delete while the pointer is on the row. A chat's
                   worktree alone gets no mark; only a pull request does. One
                   mark stands for all of a chat's pull requests. -->
              <span class="end">
                {#if session.prs.length}
                  <span class="branch" title={prLabel(session.prs)}><BranchMark prs={session.prs} /></span>
                {/if}
                <button class="quiet" title="Move this chat to the trash" onclick={() => (deleting = session.id)}>×</button>
              </span>
            {/if}
          </div>
        {/each}
        <!-- Folding back is the folder name's job, so this is the only label. -->
        {#if opened.length < hidden.length}
          <button class="row more" onclick={() => (extra[project.id] = opened.length + CAP)}>show more</button>
        {/if}
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
    padding: var(--pad-y) calc(var(--pad-x) * 0.6) var(--foot-y);
    border-right: 1px solid var(--line);
    min-height: 0;
    /* The top strip, in line with the top bar, holds the show/hide button
       (see App.svelte) and, on a Mac, the window buttons. */
    position: relative;
    padding-top: calc(var(--bar) + var(--pad-y) * 0.4);
  }
  .drag {
    display: none;
  }
  :global(.mac) .drag {
    display: block;
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: var(--bar);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* Scrolling down only: a pixel of overhang must not add a sideways bar. */
    overflow-x: hidden;
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
  /* In the dots' column, so the folder name lines up with the chat titles. */
  .caret {
    /* Centred on the dots below, which fill the column's first 1ch. */
    width: 1.4ch;
    height: 1.4ch;
    margin: 0 0.8ch 0 -0.2ch;
    flex-shrink: 0;
    align-self: center;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
  }
  .caret.shut {
    transform: rotate(-90deg);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A project whose folder is gone, and the way to point at where it went. */
  .name.missing {
    color: var(--faint);
  }
  .locate {
    flex-shrink: 0;
    color: var(--dim);
    text-decoration: underline;
    text-underline-offset: 0.2em;
  }
  .locate:hover {
    color: var(--fg);
  }
  .dragging .folder {
    color: var(--fg);
    cursor: grabbing;
  }
  .path:hover {
    color: var(--fg);
  }
  .path {
    display: flex;
    align-items: baseline;
    text-align: left;
    color: inherit;
    touch-action: none;
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
    /* As wide as the branch mark (1em), which is wider than 1.5ch in monospace. */
    width: max(1.5ch, 1em);
    justify-items: end;
  }
  .end > * {
    grid-area: 1 / 1;
  }
  .row:hover .branch {
    visibility: hidden;
  }
  .ask {
    flex: 1;
    padding-left: 2ch;
    color: var(--dim);
  }
  .no {
    color: var(--dim);
  }
  .no:hover {
    color: var(--fg);
  }
  .danger {
    color: var(--bad);
  }
  /* The row's gap sits between mark and field; titles have none. */
  .row :global(.in-row) {
    margin-left: -1ch;
  }
  /* Lined up with the titles, as faint as the folder path. */
  .more {
    margin-top: calc(var(--gap) * 0.4);
    padding-left: 2ch;
    color: var(--faint);
  }
  .more:hover {
    color: var(--fg);
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
