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

  type Question = {
    question: string;
    header?: string;
    multiSelect?: boolean;
    options?: { label: string; description?: string }[];
  };
  const questions = $derived.by((): Question[] => {
    const input = item.input;
    if (input && typeof input === "object" && "questions" in input && Array.isArray(input.questions)) {
      return input.questions as Question[];
    }
    return [];
  });

  // What is picked so far, per question: option labels, plus free text.
  let picked = $state<Record<number, string[]>>({});
  let other = $state<Record<number, string>>({});

  function pick(index: number, label: string, multi: boolean) {
    const now = picked[index] ?? [];
    if (multi) {
      picked[index] = now.includes(label) ? now.filter((l) => l !== label) : [...now, label];
    } else {
      picked[index] = [label];
      other[index] = "";
    }
  }

  function typeOther(index: number, text: string, multi: boolean) {
    other[index] = text;
    // For a pick-one question, typing an answer replaces the picked option.
    if (!multi && text.trim()) picked[index] = [];
  }

  /** The answer to one question, the way Claude Code wants it: labels joined by ", ". */
  function answerTo(index: number): string {
    const typed = (other[index] ?? "").trim();
    return [...(picked[index] ?? []), ...(typed ? [typed] : [])].join(", ");
  }

  const complete = $derived(questions.length > 0 && questions.every((_, i) => answerTo(i) !== ""));

  function sendAnswers() {
    const answers = Object.fromEntries(questions.map((q, i) => [q.question, answerTo(i)]));
    void chat.answer(item, "allow", undefined, answers);
  }

  const outcome = $derived.by(() => {
    if (item.cancelled) return "cancelled";
    if (item.approval === "plan") {
      if (item.answered === "allow") return "approved";
      if (item.answered === "always_allow") return "approved · auto-accepting edits";
      return "kept planning";
    }
    if (item.approval === "question") return item.answered === "deny" ? "skipped" : "answered";
    if (item.answered === "allow") return "allowed";
    if (item.answered === "always_allow") return "allowed for this chat";
    return "denied";
  });
  const closed = $derived(Boolean(item.answered) || Boolean(item.cancelled));
</script>

{#if closed && item.approval === "tool"}
  <!-- Once answered, a tool approval shrinks to one line; the folded run shows the work. -->
  <div class="done">
    <span class:denied={item.answered === "deny"}>{outcome}</span> · {item.toolName}
    {target.split(/[\\/]/).pop()}
  </div>
{:else}
<div class="approval" class:closed>
  {#if item.approval === "plan"}
    <div class="head"><span class="accent">Plan</span> · ready for your review</div>
    <Markdown text={planText} />
  {:else if item.approval === "question"}
    <div class="head"><span class="accent">Question</span></div>
    {#each questions as q, i}
      {@const multi = Boolean(q.multiSelect)}
      <div class="question">
        <div class="bright">{q.question}{#if multi && !closed}<span class="faint detail">· pick any</span>{/if}</div>
        {#if closed}
          {#if item.answers?.[q.question]}
            <div class="dim">→ {item.answers[q.question]}</div>
          {/if}
        {:else}
          {#each q.options ?? [] as option}
            {@const on = (picked[i] ?? []).includes(option.label)}
            <button class="option" class:on onclick={() => pick(i, option.label, multi)}>
              <span class="box">{multi ? (on ? "[x]" : "[ ]") : on ? "(•)" : "( )"}</span>
              <span>
                {option.label}{#if option.description}<span class="faint detail">· {option.description}</span>{/if}
              </span>
            </button>
          {/each}
          <label class="option" class:on={(other[i] ?? "").trim() !== ""}>
            <span class="box">{multi ? ((other[i] ?? "").trim() ? "[x]" : "[ ]") : (other[i] ?? "").trim() ? "(•)" : "( )"}</span>
            <input
              value={other[i] ?? ""}
              oninput={(e) => typeOther(i, e.currentTarget.value, multi)}
              placeholder="something else…"
            />
          </label>
        {/if}
      </div>
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
    {#if closed}
      <span class="faint">{outcome}</span>
    {:else if item.approval === "plan"}
      <button class="primary" onclick={() => chat.answer(item, "allow")}>Approve plan</button>
      {#if item.alwaysLabel}
        <button onclick={() => chat.answer(item, "always_allow")}>{item.alwaysLabel}</button>
      {/if}
      <button onclick={() => chat.answer(item, "deny", "Keep planning.")}>Keep planning</button>
    {:else if item.approval === "question"}
      <button class="primary" disabled={!complete} onclick={sendAnswers}>Send answers</button>
      <button
        onclick={() => chat.answer(item, "deny", "The user skipped these questions. Use your best judgment.")}
      >
        Skip
      </button>
    {:else}
      <button class="primary" onclick={() => chat.answer(item, "allow")}>Allow</button>
      {#if item.alwaysLabel}
        <button class="always" onclick={() => chat.answer(item, "always_allow")}>{item.alwaysLabel}</button>
      {/if}
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
  .approval.closed {
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
  .dim {
    color: var(--dim);
  }
  .faint {
    color: var(--faint);
  }
  .detail {
    margin-left: 1ch;
  }
  .command {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .question {
    display: flex;
    flex-direction: column;
  }
  .question + .question {
    margin-top: 0.6em;
  }
  .option {
    display: flex;
    gap: 1ch;
    text-align: left;
    color: var(--fg);
    cursor: pointer;
  }
  .option:hover,
  .option.on {
    color: var(--bright);
  }
  .box {
    flex-shrink: 0;
    white-space: pre;
    color: var(--faint);
  }
  .option.on .box {
    color: var(--accent);
  }
  .option input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--bright);
    caret-color: var(--accent);
  }
  .option input::placeholder {
    color: var(--faint);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0 3ch;
    margin-top: 0.4em;
  }
  .actions button:hover {
    color: var(--bright);
  }
  /* A rule can be long; keep it to one line. */
  .always {
    max-width: 44ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    padding: 0 1ch;
  }
  .actions .primary:hover {
    color: var(--bg);
  }
  .primary:disabled {
    background: var(--line);
    color: var(--faint);
    cursor: default;
  }
  .actions .primary:disabled:hover {
    color: var(--faint);
  }
</style>
