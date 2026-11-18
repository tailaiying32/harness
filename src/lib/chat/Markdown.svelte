<script lang="ts">
  // Formatted text, drawn block by block: while text streams in, only the
  // last block changes, so the others are drawn once and left alone (comments
  // on them stay put; see chatComments.ts). Each block is one element, a
  // direct child of `.md`, as when the text was drawn whole.
  import { renderBlocks } from "../markdown";

  let { text }: { text: string } = $props();
  const blocks = $derived(renderBlocks(text));
</script>

<!-- Safe: see the note at the top of markdown.ts. -->
<div class="md">{#each blocks as html, i (i)}{@html html}{/each}</div>
