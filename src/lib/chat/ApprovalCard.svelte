<script lang="ts">
  // The one boxed element in the chat. Mouse only: no key approves or denies,
  // so a stray keypress can never approve a command.
  import type { ApprovalItem, Chat } from "../state/chat.svelte";
  import { diffLines, title } from "../tools";
  import Clipped from "./Clipped.svelte";
  import Markdown from "./Markdown.svelte";

  let { item, chat }: { item: ApprovalItem; chat: Chat } = $props();

  const call = $derived({ toolId: "", name: item.toolName, input: item.input });
  const diff = $derived(diffLines(call));
  const target = $derived(title(call, chat.projectPath).target);

  const planText = $derived(
    item.input && typeof item.input === "object" && "plan" in item.input ? String(item.input.plan) : "",
  );

  type Question = { question: string; options?: { label: string; description?: string }[] };
  const questions = $derived.by((): Question[] => {
    const input = item.input;
    if (input && typeof input === "object" && "questions" in input && Array.isArray(input.questions)) {
      return input.questions as Question[];
    }
    return [];
  });

  const outcome: Record<string, string> = {
    allow: "allowed",
    always_allow: "allowed for this chat",
    deny: "denied",
  };
</script>

{#if item.answered && item.approval === "tool"}
  <!-- Once answered, a tool approval shrinks to one line; the folded run shows the work. -->
  <div class="done">
    <span class:denied={item.answered === "deny"}>{outcome[item.answered]}</span> · {item.toolName}
    {target.split(/[\\/]/).pop()}
  </div>
{:else}
<div class="approval" class:answered={item.answered}>
  {#if item.approval === "plan"}
    <div class="head"><span class="accent">Plan</span> · ready for your review</div>
    <Markdown text={planText} />
  {:else if item.approval === "question"}
    <div class="head"><span class="accent">Question</span></div>
    {#each questions as q}
      <div class="bright">{q.question}</div>
      {#each q.options ?? [] as option}
        <div class="option"><span class="faint">-</span> {option.label}</div>
      {/each}
    {/each}
  {:else}
    <div class="head"><span class="accent">{item.toolName}</span> · {item.title}</div>
    {#if item.toolName === "Bash"}
      <div class="bright command">$ {target}</div>
    {:else if target}
      <div class="bright command">{target}</div>
    {/if}
    {#if diff && diff.length > 0}
      <Clipped lines={diff} max={16} />
    {/if}
  {/if}

  <div class="actions">
    {#if item.answered}
      <span class="faint">{item.approval === "plan" && item.answered === "deny" ? "kept planning" : outcome[item.answered]}</span>
    {:else if item.approval === "plan"}
      <button class="primary" onclick={() => chat.answer(item, "allow")}>Approve plan</button>
      <button onclick={() => chat.answer(item, "deny", "Keep planning.")}>Keep planning</button>
    {:else if item.approval === "question"}
      <!-- Choosing an answer isn't built yet; see PLAN.md step 4. -->
      <button
        onclick={() =>
          chat.answer(item, "deny", "Answering questions isn't supported in this build yet. Use your best judgment.")}
      >
        Skip
      </button>
    {:else}
      <button class="primary" onclick={() => chat.answer(item, "allow")}>Allow</button>
      <button onclick={() => chat.answer(item, "always_allow")}>Always allow</button>
      <button onclick={() => chat.answer(item, "deny")}>Deny</button>
    {/if}
  </div>
</div>
{/if}

<style>
  .approval {
    border: 1px solid var(--accent);
    padding: 0.8em 1.25em;
    display: flex;
    flex-direction: column;
    gap: 0.4em;
  }
  .approval.answered {
    border-color: var(--line);
  }
  .done {
    color: var(--faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .denied {
    color: var(--bad);
  }
  .head {
    color: var(--dim);
  }
  .accent {
    color: var(--accent);
  }
  .bright {
    color: var(--bright);
  }
  .faint {
    color: var(--faint);
  }
  .command {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .option {
    display: flex;
    gap: 1ch;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 3ch;
    margin-top: 0.4em;
  }
  .actions button:hover {
    color: var(--bright);
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    padding: 0 1ch;
  }
  .actions .primary:hover {
    color: var(--bg);
  }
</style>
