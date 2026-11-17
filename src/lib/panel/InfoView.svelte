<script lang="ts">
  // Chat info's page: how full the context is, Claude's task list,
  // the chat's subagents (running and finished) and what runs in the
  // background. Sections with nothing in them are left out. Clicking a
  // subagent shows it full size (see AgentView); so does picking it in the
  // top bar's ▾ menu (see PanelBar), and "chat info" there comes back here.
  import type { Chat } from "../state/chat.svelte";
  import { ago, batchLabel, QUIET_MS, runTime, span, tokens, type Agent } from "../state/chatInfo.svelte";
  import { past, present } from "../tools";
  import { MARKS } from "./agentMarks";

  let { chat }: { chat: Chat } = $props();
  const info = $derived(chat.info);

  // Times move once a second while something runs.
  let now = $state(Date.now());
  $effect(() => {
    if (info.running === 0 && info.background.length === 0) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const finished = $derived(info.agents.length - info.running);
  const context = $derived(info.context);
  const share = $derived(context && context.max > 0 ? Math.min(1, context.used / context.max) : 0);

  const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

  /** What the agent did last, in the chat's words. */
  function lastStep(agent: Agent): string {
    if (agent.state === "running" && agent.live) return agent.live;
    if (agent.state === "failed" && agent.answer) return agent.answer.trim().split("\n")[0];
    for (let i = agent.steps.length - 1; i >= 0; i--) {
      const step = agent.steps[i];
      if (step.type === "tool") return capital(step.result === undefined && agent.state === "running" ? present(step) : past(step));
    }
    if (agent.state === "running") return "Starting";
    return agent.answer || agent.steps.length > 0 ? "Answered" : "";
  }

  /** How long since the last step (running) or the end (finished); "" when not known. */
  function since(agent: Agent): string {
    const at = agent.state === "running" ? agent.lastAt : agent.endedAt;
    return at ? ago(now - at) : "";
  }

  const quiet = (agent: Agent) =>
    agent.state === "running" && agent.lastAt !== undefined && now - agent.lastAt > QUIET_MS;
</script>

<div class="page">
  {#if context}
    <section>
      <div class="head">context</div>
      <div class="meter">
        <div class="bar"><div class="fill" style:width="{share * 100}%"></div></div>
        <span class="dim">{tokens(context.used)} of {tokens(context.max)} · {Math.round(share * 100)}%</span>
      </div>
      {#each context.parts as part (part.name)}
        <div class="pair faint"><span>{part.name}</span><span>{tokens(part.tokens)}</span></div>
      {/each}
    </section>
  {/if}

  {#if info.tasks.length}
    <section>
      <div class="head">tasks</div>
      <div>
        {#each info.tasks as task (task.id)}
          <div class="line">
            <span class="mark {task.status}">{task.status === "completed" ? "✓" : task.status === "in_progress" ? "●" : "○"}</span>
            <span class="task {task.status}">{task.text}</span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if info.agents.length}
    <section>
      <div class="head pair">
        <span>subagents</span>
        <span>{[info.running && `${info.running} running`, finished && `${finished} finished`].filter(Boolean).join(" · ")}</span>
      </div>
      <!-- By batch, newest first, each under a short label for what it is
           doing (the message it answers until that comes); the same
           grouping as the top bar's ▾ menu. -->
      {#each info.batches as batch (batch.batch)}
      <div class="agents">
        <div class="ask faint" title={batch.ask.trim() || undefined}>{batchLabel(batch)}</div>
        {#each batch.agents as agent (agent.toolId)}
          <button class="agent" onclick={() => info.show(agent.toolId)}>
            <span class="mark {agent.state}">{MARKS[agent.state]}</span>
            <span class="lines">
              <span class="pair">
                <span class="name" class:live={agent.state === "running"}>{agent.name}</span>
                <span class="faint">{runTime(agent, now)}</span>
              </span>
              <span class="pair">
                <span class="step" class:bad={agent.state === "failed"}>{lastStep(agent)}</span>
                <span class="faint" class:quiet={quiet(agent)}>{since(agent)}</span>
              </span>
            </span>
          </button>
        {/each}
      </div>
      {/each}
    </section>
  {/if}

  {#if info.background.length}
    <section>
      <div class="head">in the background</div>
      <div>
        {#each info.background as task (task.id)}
          <div class="line">
            <span class="mark running">●</span>
            <span class="pair grow">
              <span class="name">{task.description || task.kind}</span>
              <span class="faint">{task.since ? span(now - task.since) : ""}</span>
            </span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if !context && !info.tasks.length && !info.agents.length && !info.background.length}
    <div class="faint">nothing yet</div>
  {/if}
</div>

<style>
  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--pad-y) var(--pad-x);
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 2.2);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 0.6);
  }
  .head {
    color: var(--faint);
  }
  /* Text on the left, a time or size on the right. */
  .pair {
    display: flex;
    gap: 2ch;
    min-width: 0;
  }
  .pair > :first-child {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pair > :last-child {
    margin-left: auto;
    flex-shrink: 0;
  }
  .grow {
    flex: 1;
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 2ch;
  }
  .bar {
    flex: 1;
    height: 4px;
    background: var(--line);
  }
  .fill {
    height: 100%;
    background: var(--dim);
  }
  .line {
    display: flex;
  }
  .mark {
    width: 2.5ch;
    flex-shrink: 0;
  }
  .mark.running,
  .mark.done,
  .mark.completed,
  .mark.in_progress {
    color: var(--ok);
  }
  .mark.failed {
    color: var(--bad);
  }
  .mark.stopped,
  .mark.pending {
    color: var(--faint);
  }
  .task.completed {
    color: var(--dim);
  }
  .task.in_progress {
    color: var(--bright);
  }
  .agents {
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 0.7);
  }
  .agents + .agents {
    margin-top: calc(var(--gap) * 0.8);
  }
  /* What a batch was for: one faint line, cut short. */
  .ask {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .agent {
    display: flex;
    text-align: left;
    width: 100%;
  }
  .agent:hover .name {
    color: var(--bright);
  }
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    color: var(--fg);
  }
  .name.live {
    color: var(--bright);
  }
  .step {
    color: var(--dim);
  }
  .dim {
    color: var(--dim);
    flex-shrink: 0;
  }
  .faint {
    color: var(--faint);
  }
  .bad {
    color: var(--bad);
  }
  /* No new step for a while: worth a look. */
  .quiet {
    color: var(--accent);
  }
</style>
