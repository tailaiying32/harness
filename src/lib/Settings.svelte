<script lang="ts">
  // Settings, drawn over the chat and side panel. A choice applies at once and
  // is remembered. Also home to things that aren't part of the work at hand,
  // like the trash.
  import type { AppState } from "./state/app.svelte";
  import { applyFontSize, applySize, applyTheme, fontSizes, savePrefs, sizes, themes, type Size } from "./themes";

  let {
    app,
    theme = $bindable(),
    size = $bindable(),
    fontSize = $bindable(),
    onclose,
  }: { app: AppState; theme: string; size: Size; fontSize: number; onclose: () => void } = $props();

  const groups = [
    { name: "dark", list: themes.filter((t) => t.dark) },
    { name: "light", list: themes.filter((t) => !t.dark) },
  ];

  function pickTheme(id: string) {
    theme = id;
    applyTheme(id);
    savePrefs({ theme: id });
  }

  function pickSize(value: Size) {
    size = value;
    applySize(value);
    savePrefs({ size: value });
  }

  /** How long a trashed chat has left before it is deleted for good. */
  const TRASH_DAYS = 30;
  function daysLeft(trashedAt: number | null): string {
    const left = Math.ceil(TRASH_DAYS - (Date.now() - (trashedAt ?? Date.now())) / 86_400_000);
    return left <= 1 ? "last day" : `${left}d left`;
  }

  /** "empty trash" was clicked once and is asking to be sure. */
  let emptying = $state(false);

  async function emptyTrash() {
    emptying = false;
    await app.emptyTrash();
  }

  function pickFontSize(px: number) {
    fontSize = px;
    applyFontSize(px);
    savePrefs({ fontSize: px });
  }
</script>

<div class="settings">
  <div class="inner">
    <div class="head">
      <span class="title">Settings</span>
      <button onclick={onclose}>close</button>
    </div>

    <section>
      <div class="label">Spacing</div>
      <div class="choices">
        {#each sizes as s (s)}
          <button class:on={s === size} onclick={() => pickSize(s)}>
            <span class="mark">{s === size ? "(•)" : "( )"}</span>{s}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="label">Font size</div>
      <div class="choices">
        {#each fontSizes as px (px)}
          <button class:on={px === fontSize} onclick={() => pickFontSize(px)}>
            <span class="mark">{px === fontSize ? "(•)" : "( )"}</span>{px}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="label">Theme</div>
      {#each groups as group (group.name)}
        <div class="sub">{group.name}</div>
        <div class="choices grid">
          {#each group.list as t (t.id)}
            <button class:on={t.id === theme} onclick={() => pickTheme(t.id)}>
              <span class="mark">{t.id === theme ? "(•)" : "( )"}</span>{t.name}
            </button>
          {/each}
        </div>
      {/each}
    </section>

    <section>
      <div class="label">Trash</div>
      {#if app.trash.length === 0}
        <div class="sub">empty. Deleted chats stay here {TRASH_DAYS} days.</div>
      {:else}
        <div class="sub">Deleted chats stay here {TRASH_DAYS} days. Open one to read it; send a message to restore it.</div>
        <div class="trash">
          {#each app.trash as session (session.id)}
            <div class="row">
              <button class="open" onclick={() => app.open(session.id)} title={session.title}>
                {session.title || "untitled"}
              </button>
              <span class="left">{daysLeft(session.trashed_at)}</span>
              <button class="act" onclick={() => app.restore(session.id)}>restore</button>
            </div>
          {/each}
        </div>
        <div class="empty">
          {#if emptying}
            <span class="warn">Delete {app.trash.length} {app.trash.length === 1 ? "chat" : "chats"} for good?</span>
            <button class="danger" onclick={emptyTrash}>yes</button>
            <button class="act" onclick={() => (emptying = false)}>no</button>
          {:else}
            <button class="act" onclick={() => (emptying = true)}>empty trash</button>
          {/if}
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .settings {
    position: absolute;
    inset: 0;
    /* Above the side panel's drag edge. */
    z-index: 2;
    background: var(--bg);
    overflow-y: auto;
    padding: var(--pad-y) var(--pad-x);
  }
  .inner {
    max-width: var(--col);
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 2);
  }
  .head {
    display: flex;
    justify-content: space-between;
    padding-bottom: var(--gap);
    border-bottom: 1px solid var(--line);
  }
  .title {
    color: var(--accent);
    font-weight: 700;
  }
  .head button {
    color: var(--faint);
  }
  .head button:hover {
    color: var(--fg);
  }
  .label {
    color: var(--bright);
    font-weight: 700;
    margin-bottom: calc(var(--gap) * 0.6);
  }
  .sub {
    color: var(--faint);
    margin-top: calc(var(--gap) * 0.8);
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 0 4ch;
  }
  .choices.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(24ch, 1fr));
    gap: 0 2ch;
  }
  .choices button {
    text-align: left;
    color: var(--fg);
  }
  .choices button:hover,
  .choices button.on {
    color: var(--bright);
  }
  .mark {
    color: var(--faint);
    margin-right: 1ch;
    white-space: pre;
  }
  .on .mark {
    color: var(--accent);
  }
  .trash {
    margin-top: calc(var(--gap) * 0.8);
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 2ch;
  }
  .open {
    flex: 1;
    min-width: 0;
    text-align: left;
    color: var(--dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .open:hover {
    color: var(--fg);
  }
  .left {
    flex-shrink: 0;
    color: var(--faint);
  }
  .act {
    flex-shrink: 0;
    color: var(--faint);
  }
  .act:hover {
    color: var(--fg);
  }
  .empty {
    display: flex;
    gap: 2ch;
    margin-top: calc(var(--gap) * 0.8);
  }
  .warn {
    color: var(--fg);
  }
  .danger {
    color: var(--bad);
  }
</style>
