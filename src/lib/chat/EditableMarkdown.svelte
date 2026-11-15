<script lang="ts">
  // Markdown you can write in: shown formatted, and plain markdown text to edit
  // once clicked. Clicking a link follows it instead. Escape, Cmd+Enter or
  // clicking away shows it formatted again. Empty text starts out editing.
  import { tick } from "svelte";
  import Markdown from "./Markdown.svelte";

  let {
    text,
    onchange,
    ondone,
    placeholder = "",
  }: {
    text: string;
    /** Every keystroke's text. */
    onchange: (text: string) => void;
    /** Editing ended (Escape, Cmd+Enter, clicked away). */
    ondone?: () => void;
    placeholder?: string;
  } = $props();

  let editing = $state(false);
  let box = $state<HTMLTextAreaElement>();
  const writing = $derived(editing || text.trim() === "");

  function grow() {
    if (!box) return;
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
  }

  async function edit(e: MouseEvent) {
    // A link is followed, not edited (the panel's click handler opens it).
    if ((e.target as HTMLElement).closest("a")) return;
    if (window.getSelection()?.toString()) return; // selecting text to copy
    editing = true;
    await tick();
    grow();
    box?.focus();
    box?.setSelectionRange(box.value.length, box.value.length);
  }

  function finish() {
    editing = false;
    ondone?.();
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape" || (e.key === "Enter" && (e.metaKey || e.ctrlKey))) {
      e.preventDefault();
      box?.blur();
    }
  }

  $effect(() => {
    // Grow to fit when the text arrives or changes from outside.
    void text;
    if (box) queueMicrotask(grow);
  });
</script>

{#if writing}
  <textarea
    bind:this={box}
    value={text}
    onfocus={() => (editing = true)}
    oninput={(e) => {
      editing = true;
      onchange((e.target as HTMLTextAreaElement).value);
      grow();
    }}
    onblur={finish}
    onkeydown={key}
    {placeholder}
    spellcheck="false"
  ></textarea>
{:else}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="shown" onclick={edit}>
    <Markdown {text} />
  </div>
{/if}

<style>
  textarea {
    display: block;
    width: 100%;
    min-height: 12em;
    resize: none;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--fg);
    line-height: inherit;
    caret-color: var(--accent);
    overflow: hidden;
  }
  textarea::placeholder {
    color: var(--faint);
  }
  .shown {
    cursor: text;
  }
</style>
