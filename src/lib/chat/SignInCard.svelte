<script lang="ts">
  // Claude Code said it isn't signed in. One button runs its own sign-in in
  // the browser, then the message that failed is sent again.
  import type { Chat, SignInItem } from "../state/chat.svelte";

  let { item, chat }: { item: SignInItem; chat: Chat } = $props();
</script>

{#if item.state === "done"}
  <div class="faint">signed in</div>
{:else}
  <div class="card">
    <div class="bright">Claude Code isn't signed in</div>
    {#if item.state === "failed"}
      <div class="bad pre">{item.error}</div>
      <div class="dim">Or run <span class="bright">claude auth login</span> in a terminal, then send again.</div>
    {/if}
    <div class="actions">
      {#if item.state === "working"}
        <span class="faint">waiting for the browser…</span>
      {:else}
        <button class="primary" onclick={() => chat.signIn(item)}>Sign in</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Same box as an approval card. */
  .card {
    border: 1px solid var(--accent);
    padding: 0.8em 1.25em;
    display: flex;
    flex-direction: column;
    gap: 0.4em;
  }
  .bright {
    color: var(--bright);
  }
  .dim {
    color: var(--dim);
  }
  .faint {
    color: var(--faint);
  }
  .bad {
    color: var(--bad);
  }
  .pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    gap: 3ch;
    margin-top: 0.4em;
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    padding: 0 1ch;
  }
</style>
