<script lang="ts">
  // The user's working preferences file (~/.harness/preferences.md), which
  // every agent is given. Edited in place and saved as you type; chats started
  // after that get the new text.
  import { onDestroy } from "svelte";
  import LiveMarkdown from "./chat/LiveMarkdown.svelte";
  import { readPreferences, writePreferences } from "./ipc";

  /** The file's text; null until read. */
  let text = $state<string | null>(null);
  let problem = $state("");
  /** Typed but not saved yet. */
  let unsaved: string | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  async function load() {
    try {
      const read = await readPreferences();
      // Typing since the read started wins; it's saved next.
      if (unsaved === null) text = read;
    } catch (e) {
      problem = String(e);
    }
  }
  load();

  function typeIn(next: string) {
    text = next;
    unsaved = next;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 400);
  }

  async function flush() {
    clearTimeout(saveTimer);
    const edit = unsaved;
    unsaved = null;
    if (edit === null) return;
    try {
      await writePreferences(edit);
      problem = "";
    } catch (e) {
      problem = String(e);
    }
  }

  onDestroy(() => void flush());
</script>

<!-- Back in the app: an agent may have changed the file meanwhile. -->
<svelte:window onfocus={() => unsaved === null && load()} />

{#if text !== null}
  <LiveMarkdown {text} onchange={typeIn} ondone={flush} placeholder="preferences" autofocus={text === ""} />
{/if}
{#if problem}
  <div class="problem">{problem}</div>
{/if}

<style>
  .problem {
    color: var(--bad);
  }
</style>
