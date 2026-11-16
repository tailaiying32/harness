<script lang="ts">
  // Markdown written in place, shown formatted as you type (see liveMarkdown.ts).
  // The editor loads the first time one is shown; until then, the text shows
  // formatted as usual.
  import { onDestroy, onMount } from "svelte";
  import type { EditorView } from "@codemirror/view";
  import Markdown from "./Markdown.svelte";

  let {
    text,
    onchange,
    ondone,
    placeholder = "",
    autofocus = false,
  }: {
    text: string;
    /** Every edit's text. */
    onchange: (text: string) => void;
    /** The editor lost focus (clicked away, or Escape). */
    ondone?: () => void;
    placeholder?: string;
    /** Put the cursor in it once loaded, e.g. for empty notes. */
    autofocus?: boolean;
  } = $props();

  let host = $state<HTMLElement>();
  let view = $state<EditorView>();
  let gone = false;
  /** A change from outside is being put in: not an edit to report back. */
  let taking = false;

  onMount(async () => {
    const { createLiveMarkdown } = await import("./liveMarkdown");
    if (gone || !host) return;
    view = createLiveMarkdown(host, {
      text,
      placeholder,
      onchange: (t) => taking || onchange(t),
      onblur: () => ondone?.(),
    });
    if (autofocus) view.focus();
  });

  // Text changed from outside (a new plan arrived, Claude wrote the notes):
  // take it, even while the user is writing here. Only the changed middle is
  // replaced, so the cursor stays where it was in the text around it. Not
  // reported as an edit: saving it back could undo a later write by Claude.
  $effect(() => {
    const next = text;
    if (!view) return;
    const now = view.state.doc.toString();
    if (next === now) return;
    let from = 0;
    while (from < now.length && from < next.length && now[from] === next[from]) from++;
    let end = 0;
    while (end < now.length - from && end < next.length - from && now[now.length - 1 - end] === next[next.length - 1 - end]) end++;
    taking = true;
    try {
      view.dispatch({ changes: { from, to: now.length - end, insert: next.slice(from, next.length - end) } });
    } finally {
      taking = false;
    }
  });

  onDestroy(() => {
    gone = true;
    view?.destroy();
  });
</script>

{#if !view}
  <Markdown {text} />
{/if}
<div class="live" class:loading={!view} bind:this={host}></div>

<style>
  .loading {
    display: none;
  }
</style>
