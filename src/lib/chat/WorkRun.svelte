<script lang="ts">
  // A run of tool calls, folded into one line. Clicking opens it to one line
  // per call, with the diff under each edit. While the run is still going the
  // line shows the current step and how long the turn has taken.
  import {
    awaitsAnswer,
    baseName,
    diffLines,
    docPath,
    editCounts,
    past,
    pending,
    present,
    showsOutput,
    skipped,
    title,
    type ToolCall,
  } from "../tools";
  import Clipped from "./Clipped.svelte";

  let {
    tools,
    root = "",
    active = false,
    spinner = "",
    elapsed = 0,
    docs = [],
    onopen,
  }: {
    tools: ToolCall[];
    root?: string;
    active?: boolean;
    spinner?: string;
    elapsed?: number;
    /** Files that have a tab in the side panel. */
    docs?: string[];
    /** Opens a file's tab. */
    onopen?: (path: string) => void;
  } = $props();

  let open = $state(false);

  const running = $derived(tools.find((tool) => tool.result === undefined));
  // A call the user declined is not a failure, and plans and questions report
  // their outcome on their own card.
  const failed = $derived(tools.some((tool) => tool.isError && !tool.declined));
  const allSkipped = $derived(tools.every(skipped));
  const counts = $derived(editCounts(tools));
  // When nothing is running but a call has no result yet, it is waiting for
  // the user's answer on the card below; don't say it ran.
  const waiting = $derived(!active && tools.some(awaitsAnswer));
  // The summary in pieces, so the name of a file with a tab can be a link to it.
  const summary = $derived.by(() => {
    const parts: { text: string; doc?: string }[] = [];
    tools.forEach((tool, i) => {
      let text = (i > 0 ? ", " : "") + (!active && awaitsAnswer(tool) ? pending(tool) : past(tool));
      if (i === 0) text = text.charAt(0).toUpperCase() + text.slice(1);
      const path = docPath(tool);
      const name = baseName(path);
      if (path && !tool.declined && docs.includes(path) && text.endsWith(name)) {
        parts.push({ text: text.slice(0, -name.length) }, { text: name, doc: path });
      } else {
        parts.push({ text });
      }
    });
    return parts;
  });

  function toggle(e: KeyboardEvent) {
    if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
    e.preventDefault();
    open = !open;
  }

  function openDoc(e: MouseEvent, path: string) {
    e.stopPropagation();
    onopen?.(path);
  }

  const outputLines = (tool: ToolCall) => (tool.result ?? "").trimEnd().split("\n").map((text) => ({ text }));
</script>

<div>
  <!-- Not a <button>: the names of documents inside it are buttons of their own. -->
  <div class="fold" role="button" tabindex="0" onclick={() => (open = !open)} onkeydown={toggle} aria-expanded={open}>
    <span class="marker" class:live={active} class:bad={!active && failed}>
      {active ? spinner : failed ? "!" : waiting ? "·" : allSkipped ? "–" : "✓"}
    </span>
    <span class="summary">
      {#if active && running}{present(running)}…{:else}{#each summary as part}{#if part.doc}{@const path = part.doc}<button class="doc" title="Open in the side panel" onclick={(e) => openDoc(e, path)}>{part.text}</button>{:else}{part.text}{/if}{/each}{/if}
      {#if counts.added > 0}<span class="ok">+{counts.added}</span>{/if}
      {#if counts.removed > 0}<span class="bad">-{counts.removed}</span>{/if}
    </span>
    {#if active}<span class="faint">{elapsed}s</span>{/if}
    <span class="faint">{open ? "▾" : "▸"}</span>
  </div>

  {#if open}
    <div class="details">
      {#each tools as tool (tool.toolId)}
        {@const name = title(tool, root)}
        {@const diff = diffLines(tool)}
        <div>
          <div class="call">
            <span class="verb">{name.verb}</span>
            {#if docs.includes(docPath(tool)) && !tool.declined}
              <button class="target doc" title="Open in the side panel" onclick={(e) => openDoc(e, docPath(tool))}>
                {name.target}
              </button>
            {:else}
              <span class="target">{name.target}</span>
            {/if}
            {#if tool.result === undefined}<span class="faint">{active ? "running" : "waiting"}</span>{/if}
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
    cursor: pointer;
  }
  .doc {
    text-decoration: underline;
    text-underline-offset: 0.2em;
  }
  .doc:hover {
    color: var(--fg);
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
