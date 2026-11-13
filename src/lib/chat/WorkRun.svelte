<script lang="ts">
  // A run of tool calls, folded into one line. Clicking opens it to one line
  // per call, with the diff under each edit. While the run is still going the
  // line shows the current step and how long the turn has taken.
  import { diffLines, editCounts, past, present, skipped, title, type ToolCall } from "../tools";
  import Clipped from "./Clipped.svelte";

  let {
    tools,
    root = "",
    active = false,
    spinner = "",
    elapsed = 0,
  }: { tools: ToolCall[]; root?: string; active?: boolean; spinner?: string; elapsed?: number } = $props();

  let open = $state(false);

  const running = $derived(tools.find((tool) => tool.result === undefined));
  // A call the user declined is not a failure, and plans and questions report
  // their outcome on their own card.
  const failed = $derived(tools.some((tool) => tool.isError && !tool.declined));
  const allSkipped = $derived(tools.every(skipped));
  const counts = $derived(editCounts(tools));
  const summary = $derived.by(() => {
    const text = tools.map(past).join(", ");
    return text.charAt(0).toUpperCase() + text.slice(1);
  });

  /** Tools whose output is worth showing when the run is opened. */
  const showsOutput = (tool: ToolCall) =>
    !tool.declined && (tool.isError || ["Bash", "Grep", "Glob", "WebSearch"].includes(tool.name));
  const outputLines = (tool: ToolCall) => (tool.result ?? "").trimEnd().split("\n").map((text) => ({ text }));
</script>

<div>
  <button class="fold" onclick={() => (open = !open)} aria-expanded={open}>
    <span class="marker" class:live={active} class:bad={!active && failed}>
      {active ? spinner : failed ? "!" : allSkipped ? "–" : "✓"}
    </span>
    <span class="summary">
      {#if active && running}{present(running)}…{:else}{summary}{/if}
      {#if counts.added > 0}<span class="ok">+{counts.added}</span>{/if}
      {#if counts.removed > 0}<span class="bad">-{counts.removed}</span>{/if}
    </span>
    {#if active}<span class="faint">{elapsed}s</span>{/if}
    <span class="faint">{open ? "▾" : "▸"}</span>
  </button>

  {#if open}
    <div class="details">
      {#each tools as tool (tool.toolId)}
        {@const name = title(tool, root)}
        {@const diff = diffLines(tool)}
        <div>
          <div class="call">
            <span class="verb">{name.verb}</span>
            <span class="target">{name.target}</span>
            {#if tool.result === undefined}<span class="faint">running</span>{/if}
            {#if skipped(tool)}<span class="faint">not run</span>{:else if tool.isError && !tool.declined}<span class="bad">failed</span>{/if}
          </div>
          {#if diff && diff.length > 0}
            <Clipped lines={diff} max={24} />
          {/if}
          {#if tool.result && showsOutput(tool)}
            <Clipped lines={outputLines(tool)} max={12} />
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .fold {
    display: flex;
    width: 100%;
    text-align: left;
    color: var(--dim);
    gap: 1ch;
  }
  .marker {
    width: 1.5ch;
    flex-shrink: 0;
    color: var(--ok);
  }
  .marker.live {
    color: var(--accent);
  }
  .summary {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ok {
    color: var(--ok);
  }
  .bad {
    color: var(--bad);
  }
  .faint {
    color: var(--faint);
    flex-shrink: 0;
  }
  .details {
    display: flex;
    flex-direction: column;
    gap: 0.4em;
    padding-left: 2.5ch;
    margin-top: 0.3em;
    color: var(--dim);
  }
  .call {
    display: flex;
    gap: 1ch;
  }
  .verb {
    flex-shrink: 0;
  }
  .target {
    color: var(--fg);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
