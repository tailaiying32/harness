<script lang="ts">
  // The comments waiting to go, docked above what sends them (a doc's or
  // plan's foot, or the message box): one line with their count ("2
  // comments ▸") that unfolds every comment under it. Part of the page, not a
  // popup, so nothing is covered. Either part of a comment jumps to its text;
  // × removes it. Folded or open is kept across tabs, chats and restarts.
  import { loadPrefs, savePrefs } from "../themes";

  type Entry = { id: string; quote: string; text: string; gone?: boolean };
  let { items, onjump, ondrop }: { items: Entry[]; onjump: (id: string) => void; ondrop: (id: string) => void } = $props();

  let open = $state(loadPrefs().commentListOpen);

  function toggle() {
    open = !open;
    savePrefs({ commentListOpen: open });
  }

  const count = $derived(items.length === 1 ? "1 comment" : `${items.length} comments`);
</script>

<div class="dock">
  <button class="fold" onclick={toggle}>
    {count} <span class="arrow">{open ? "▾" : "▸"}</span>
  </button>
  {#if open}
    <div class="comment-list">
      {#each items as c (c.id)}
        <div class="comment-row">
          <button class="jump quote" disabled={c.gone} onclick={() => onjump(c.id)}>“{c.quote.replace(/\s+/g, " ").trim()}”</button>
          <div class="said-row">
            <button class="jump said" disabled={c.gone} onclick={() => onjump(c.id)}>{c.text}</button>
            <button class="drop" title="Remove" onclick={() => ondrop(c.id)}>×</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dock {
    /* Set apart from what is under it by space, not a rule. */
    margin-bottom: calc(var(--gap) * 1.5);
  }
  .fold {
    color: var(--dim);
  }
  .fold:hover {
    color: var(--fg);
  }
  .fold .arrow {
    color: var(--faint);
  }
  .comment-list {
    margin-top: var(--foot-gap);
    max-height: 30vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 0.6);
  }
  .comment-row {
    display: flex;
    flex-direction: column;
  }
  .jump {
    min-width: 0;
    text-align: left;
  }
  .quote {
    color: var(--faint);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .quote:disabled {
    text-decoration: line-through;
  }
  /* The comment, with its × on its first line. */
  .said-row {
    display: flex;
    gap: 1ch;
    align-items: flex-start;
  }
  .said {
    flex: 1;
    color: var(--fg);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .said:not(:disabled):hover,
  .drop:hover {
    color: var(--bright);
  }
  .drop {
    flex-shrink: 0;
    color: var(--faint);
  }
</style>
