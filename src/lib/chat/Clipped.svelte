<script lang="ts">
  // A tinted block of lines (a diff or command output), cut when long.
  type Line = { text: string; sign?: "+" | "-" };

  let { lines, max = 20 }: { lines: Line[]; max?: number } = $props();
  let showAll = $state(false);
  const shown = $derived(showAll ? lines : lines.slice(0, max));
</script>

<div class="clipped">
  {#each shown as line}
    <div class="line" class:add={line.sign === "+"} class:del={line.sign === "-"}>
      {#if line.sign}<span class="sign">{line.sign}</span>{/if}<span>{line.text || " "}</span>
    </div>
  {/each}
  {#if !showAll && lines.length > max}
    <button class="more" onclick={() => (showAll = true)}>show {lines.length - max} more lines</button>
  {/if}
</div>

<style>
  .clipped {
    background: var(--panel);
    padding: 0.5em 1.25em;
    color: var(--dim);
    overflow-x: auto;
  }
  .line {
    display: flex;
    gap: 1.5ch;
    white-space: pre;
  }
  .sign {
    flex-shrink: 0;
  }
  .add {
    color: var(--ok);
  }
  .del {
    color: var(--bad);
  }
  .more {
    color: var(--faint);
    margin-top: 0.3em;
  }
  .more:hover {
    color: var(--fg);
  }
</style>
