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

  onMount(async () => {
    const { createLiveMarkdown } = await import("./liveMarkdown");
    if (gone || !host) return;
    view = createLiveMarkdown(host, {
      text,
      placeholder,
      onchange: (t) => onchange(t),
      onblur: () => ondone?.(),
    });
    if (autofocus) view.focus();
  });

  // Text changed from outside (a new plan arrived): take it, unless the user
  // is writing here.
  $effect(() => {
    const t = text;
    if (!view || view.hasFocus || t === view.state.doc.toString()) return;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: t } });
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
