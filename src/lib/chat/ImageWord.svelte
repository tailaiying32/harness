<script lang="ts">
  // An image's `[image n]` word, in the accent colour. Clicking it opens the
  // image over the chat.
  import type { ImageRef } from "../ipc";

  let { image, onopen }: { image: ImageRef; onopen?: (image: ImageRef) => void } = $props();

  function open(e: MouseEvent) {
    // Inside a folded work line, a click must not also fold or unfold it.
    e.stopPropagation();
    onopen?.(image);
  }
</script>

<button class="image-word" title="Open the image" onclick={open}>[image {image.n}]</button>

<style>
  .image-word {
    color: var(--accent);
    white-space: nowrap;
  }
  .image-word:hover {
    text-decoration: underline;
    text-underline-offset: 0.2em;
  }
</style>
