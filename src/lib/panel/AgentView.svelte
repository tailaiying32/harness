<script lang="ts">
  // One subagent in chat info, full size: its name and how it is doing, what
  // it was asked, its steps folded like the chat's (open one for its output),
  // what it wrote, and the answer it handed back. Stays at the bottom while
  // it runs, unless the user scrolls up.
  import { onMount, tick } from "svelte";
  import FindBar from "../FindBar.svelte";
  import { domSearcher } from "../find";
  import Markdown from "../chat/Markdown.svelte";
  import WorkRun from "../chat/WorkRun.svelte";
  import type { Chat } from "../state/chat.svelte";
  import { ago, runTime, type Agent, type AgentStep } from "../state/chatInfo.svelte";
  import type { ToolCall } from "../tools";
  import { MARKS } from "./agentMarks";

  let { chat, agent }: { chat: Chat; agent: Agent } = $props();

  // Runs of tool calls become one folded line, as in the chat.
  type Group = { kind: "run"; tools: ToolCall[] } | { kind: "text"; text: string };
  const groups = $derived.by(() => {
    const out: Group[] = [];
    for (const step of agent.steps as AgentStep[]) {
      if (step.type === "text") {
        out.push({ kind: "text", text: step.text });
        continue;
      }
      const last = out[out.length - 1];
      if (last?.kind === "run") last.tools.push(step);
      else out.push({ kind: "run", tools: [step] });
    }
    return out;
  });
  const lastGroup = $derived(groups[groups.length - 1]);
  const running = $derived(agent.state === "running");
  const tools = $derived(agent.steps.filter((s) => s.type === "tool").length);

  // The answer, unless it is just the last thing the agent wrote again.
  const answer = $derived.by(() => {
    const text = agent.answer.trim();
    const last = agent.steps[agent.steps.length - 1];
    return text && !(last?.type === "text" && last.text.trim() === text) ? text : "";
  });

  // The working mark and clock, as in the chat.
  const frames = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
  let ticks = $state(0);
  let now = $state(Date.now());
  $effect(() => {
    if (!running) return;
    const timer = setInterval(() => {
      ticks++;
      now = Date.now();
    }, 200);
    return () => clearInterval(timer);
  });

  const top = $derived.by(() => {
    const time = runTime(agent, now);
    const steps = `${tools} step${tools === 1 ? "" : "s"}`;
    if (running) {
      const last = agent.lastAt ? `last step ${ago(now - agent.lastAt)}` : "";
      return [time && `running ${time}`, steps, last].filter(Boolean).join(" · ");
    }
    const how = agent.state === "done" ? "done" : agent.state;
    return [time ? `${how} after ${time}` : how, steps].join(" · ");
  });

  let promptOpen = $state(false);

  let view = $state<HTMLElement>();
  let scroller: HTMLElement;
  let content: HTMLElement;
  let pinned = true;
  function onScroll() {
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
  }
  onMount(() => {
    const observer = new ResizeObserver(() => {
      if (pinned) scroller.scrollTop = scroller.scrollHeight;
    });
    observer.observe(content);
    void tick().then(() => (scroller.scrollTop = scroller.scrollHeight));
    return () => observer.disconnect();
  });
</script>

<div class="agent-view" bind:this={view}>
<FindBar root={view} searcher={() => (scroller ? domSearcher(scroller) : null)} />
<div class="scroll" bind:this={scroller} onscroll={onScroll}>
  <div class="content" bind:this={content}>
    <div class="head">
      <div class="row"><span class="marker {agent.state}">{MARKS[agent.state]}</span><span class="name">{agent.name}</span></div>
      <div class="row"><span class="marker"></span><span class="faint">{top}</span></div>
    </div>

    {#if agent.prompt}
      <div class="row">
        <span class="marker faint">&gt;</span>
        <button class="prompt" class:open={promptOpen} title={promptOpen ? "" : "Show all"} onclick={() => (promptOpen = !promptOpen)}>
          {agent.prompt}
        </button>
      </div>
    {/if}

    {#each groups as group, i (i)}
      {#if group.kind === "run"}
        <WorkRun
          tools={group.tools}
          root={chat.root}
          active={running && group === lastGroup}
          spinner={running && group === lastGroup ? frames[ticks % frames.length] : ""}
          elapsed={running && group === lastGroup && agent.lastAt ? Math.floor((now - agent.lastAt) / 1000) : 0}
        />
      {:else}
        <div class="row">
          <span class="marker"></span>
          <div class="body"><Markdown text={group.text} /></div>
        </div>
      {/if}
    {/each}

    {#if running && lastGroup?.kind !== "run"}
      <div class="row">
        <span class="marker live">{frames[ticks % frames.length]}</span>
        <span class="dim">{agent.live || "Working"}…</span>
      </div>
    {/if}

    {#if answer}
      <div class="row">
        <span class="marker faint">←</span>
        <div class="body"><Markdown text={answer} /></div>
      </div>
    {/if}
  </div>
</div>
</div>

<style>
  .agent-view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--pad-y) var(--pad-x);
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
  }
  .row {
    display: flex;
  }
  .marker {
    width: 2.5ch;
    flex-shrink: 0;
  }
  .marker.live {
    color: var(--accent);
  }
  .marker.running,
  .marker.done {
    color: var(--ok);
  }
  .marker.failed {
    color: var(--bad);
  }
  .marker.stopped {
    color: var(--faint);
  }
  .name {
    color: var(--bright);
  }
  .body {
    min-width: 0;
    flex: 1;
  }
  /* What it was asked: three lines, all of it on a click. */
  .prompt {
    flex: 1;
    min-width: 0;
    text-align: left;
    color: var(--dim);
    white-space: pre-wrap;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow: hidden;
  }
  .prompt.open {
    display: block;
  }
  .prompt:hover {
    color: var(--fg);
  }
  .dim {
    color: var(--dim);
  }
  .faint {
    color: var(--faint);
  }
</style>
