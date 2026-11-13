<script lang="ts">
  // Formatted text. Re-renders only when its own text changes, so finished
  // blocks render once and only the block being streamed is redone.
  import { renderMarkdown } from "../markdown";

  let { text }: { text: string } = $props();
  const html = $derived(renderMarkdown(text));
</script>

<!-- Safe: see the note at the top of markdown.ts. -->
<div class="md">{@html html}</div>
