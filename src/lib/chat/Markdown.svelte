<script module lang="ts">
  // A press inside a message is held: the text under it must not be redrawn
  // until it lets go. Streaming replaces the last block's elements, and a
  // drag selection that reaches into them is left half painted, or stuck.
  let held = $state(false);
  if (typeof window !== "undefined") {
    const end = () => (held = false);
    window.addEventListener(
      "pointerdown",
      (e) => {
        if (e.button === 0 && e.target instanceof Element && e.target.closest(".md")) held = true;
      },
      true,
    );
    window.addEventListener("pointerup", end, true);
    window.addEventListener("pointercancel", end, true);
  }
</script>

<script lang="ts">
  // Formatted text, drawn block by block: while text streams in, only the
  // last block changes, so the others are drawn once and left alone (comments
  // on them stay put; see chatComments.ts). Each block is one element, a
  // direct child of `.md`, as when the text was drawn whole.
  import { renderBlocks } from "../markdown";

  let { text }: { text: string } = $props();
  const blocks = $derived(renderBlocks(text));
  let shown = $state.raw<string[]>([]);
  $effect.pre(() => {
    const next = blocks;
    if (!held) shown = next;
  });
</script>

<!-- Safe: see the note at the top of markdown.ts. -->
<div class="md">{#each shown as html, i (i)}{@html html}{/each}</div>
