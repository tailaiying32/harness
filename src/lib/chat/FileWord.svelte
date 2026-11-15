<script lang="ts">
  // A dropped file's word, `[report.pdf]`, in the accent colour. Clicking it
  // opens the file in its own app.
  let { word, path, onopen }: { word: string; path: string; onopen?: (path: string) => void } = $props();

  function open(e: MouseEvent) {
    e.stopPropagation();
    onopen?.(path);
  }
</script>

<button class="file-word" title={path} onclick={open}>{word}</button>

<style>
  .file-word {
    color: var(--accent);
    text-align: left;
    overflow-wrap: anywhere;
  }
  .file-word:hover {
    text-decoration: underline;
    text-underline-offset: 0.2em;
  }
</style>
