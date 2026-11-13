<script lang="ts">
  // Spike window: one chat, plain text. The real chat view, sidebar and side
  // panel come in later steps of PLAN.md.
  import { onMount, tick } from "svelte";
  import { homeDir } from "@tauri-apps/api/path";
  import {
    answerApproval,
    autotest,
    autotestReport,
    checkSetup,
    interrupt,
    sendMessage,
    subscribe,
    type AgentEvent,
    type ApprovalKind,
    type Decision,
    type SessionState,
    type SetupInfo,
  } from "./lib/ipc";

  type Item =
    | { id: number; type: "user"; text: string }
    | { id: number; type: "text"; messageId: string; text: string; streaming: boolean }
    | { id: number; type: "tool"; toolId: string; name: string; input: unknown; result?: string; isError?: boolean }
    | {
        id: number;
        type: "approval";
        requestId: string;
        approval: ApprovalKind;
        toolName: string;
        title: string;
        input: unknown;
        answered?: Decision;
      }
    | { id: number; type: "error"; text: string };

  type TextItem = Extract<Item, { type: "text" }>;

  const sessionId = crypto.randomUUID();
  let nextId = 0;

  let setup = $state<SetupInfo | null>(null);
  let projectPath = $state("");
  let draft = $state("");
  let items = $state<Item[]>([]);
  let status = $state<SessionState | "none">("none");
  let model = $state<string | null>(null);
  let scroller: HTMLElement;

  const busy = $derived(status === "starting" || status === "working" || status === "waiting_approval");

  // Live text arrives in many small pieces. Collect them and apply once per frame.
  const pendingText = new Map<string, { messageId: string; text: string }>();
  const drafts = new Map<string, TextItem>();
  let frame = 0;

  function push(item: Omit<Item, "id"> & Record<string, unknown>): Item {
    items.push({ ...item, id: nextId++ } as Item);
    return items[items.length - 1];
  }

  function draftFor(key: string, messageId: string): TextItem {
    const existing = drafts.get(key);
    if (existing) return existing;
    const created = push({ type: "text", messageId, text: "", streaming: true }) as TextItem;
    drafts.set(key, created);
    return created;
  }

  function flushText() {
    frame = 0;
    if (pendingText.size === 0) return;
    const pinned = isPinned();
    for (const [key, piece] of pendingText) {
      draftFor(key, piece.messageId).text += piece.text;
    }
    pendingText.clear();
    if (pinned) scrollToEnd();
  }

  function isPinned() {
    return !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
  }

  async function scrollToEnd() {
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight });
  }

  function resultText(content: unknown): string {
    if (typeof content === "string") return content;
    if (Array.isArray(content)) {
      return content.map((c) => (c && typeof c === "object" && "text" in c ? String(c.text) : "")).join("\n");
    }
    return content == null ? "" : JSON.stringify(content);
  }

  function handle(event: AgentEvent) {
    const pinned = isPinned();
    switch (event.kind) {
      case "session_started":
        model = event.model;
        break;
      case "status":
        status = event.state;
        break;
      case "text_delta": {
        const key = `${event.message_id}:${event.block_index}`;
        const piece = pendingText.get(key);
        if (piece) piece.text += event.text;
        else pendingText.set(key, { messageId: event.message_id, text: event.text });
        if (!frame) frame = requestAnimationFrame(flushText);
        return;
      }
      case "assistant_block": {
        if (event.parent_tool_id) break; // subagent traffic: not shown in the spike
        flushText();
        const block = event.block;
        if (block.type === "text") {
          // The finished block is the final truth; replace the live draft with it.
          const live = items.find(
            (i) => i.type === "text" && i.streaming && i.messageId === event.message_id,
          ) as (Item & { type: "text" }) | undefined;
          if (live) {
            live.text = block.text;
            live.streaming = false;
          } else {
            push({ type: "text", messageId: event.message_id, text: block.text, streaming: false });
          }
        } else if (block.type === "tool_call") {
          push({ type: "tool", toolId: block.tool_id, name: block.name, input: block.input });
        }
        break;
      }
      case "tool_result": {
        if (event.parent_tool_id) break;
        const tool = items.find((i) => i.type === "tool" && i.toolId === event.tool_id) as
          | (Item & { type: "tool" })
          | undefined;
        if (tool) {
          tool.result = resultText(event.content);
          tool.isError = event.is_error;
        }
        break;
      }
      case "approval_request":
        push({
          type: "approval",
          requestId: event.request_id,
          approval: event.approval,
          toolName: event.tool_name,
          title: event.title,
          input: event.input,
        });
        if (selfTesting) answer(items[items.length - 1] as Extract<Item, { type: "approval" }>, "allow");
        break;
      case "turn_ended":
        flushText();
        for (const i of items) if (i.type === "text") i.streaming = false;
        drafts.clear();
        turnDone?.();
        break;
      case "error":
        push({ type: "error", text: event.message });
        break;
    }
    if (pinned) scrollToEnd();
  }

  async function send() {
    const text = draft.trim();
    if (!text || !setup?.logged_in) return;
    draft = "";
    push({ type: "user", text });
    scrollToEnd();
    try {
      await sendMessage(sessionId, projectPath.trim(), text);
    } catch (e) {
      push({ type: "error", text: String(e) });
    }
  }

  async function answer(item: Item & { type: "approval" }, decision: Decision, message?: string) {
    try {
      await answerApproval(sessionId, item.requestId, decision, message);
      item.answered = decision;
    } catch (e) {
      push({ type: "error", text: String(e) });
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    }
  }

  function planText(input: unknown): string {
    return input && typeof input === "object" && "plan" in input ? String(input.plan) : "";
  }

  onMount(async () => {
    await subscribe((envelope) => {
      if (envelope.session_id === sessionId) handle(envelope.event);
    });
    projectPath = await homeDir();
    setup = await checkSetup();
    runSelfTest();
  });

  // Self-test: send scripted messages through this same window, allow every
  // approval, then write out what the window ended up showing.
  let selfTesting = false;
  let turnDone: (() => void) | null = null;

  async function runSelfTest() {
    const test = await autotest();
    if (!test) return;
    selfTesting = true;
    projectPath = test.folder;
    const started = performance.now();
    for (const message of test.messages) {
      const done = new Promise<void>((resolve) => (turnDone = resolve));
      draft = message;
      await send();
      await done;
    }
    await tick();
    await autotestReport(
      JSON.stringify(
        {
          seconds: Math.round((performance.now() - started) / 100) / 10,
          model,
          items: items.map((i) => ({ ...i })),
          shown: scroller.innerText,
        },
        null,
        2,
      ),
    );
  }
</script>

<main>
  <header>
    <label>
      Folder
      <input bind:value={projectPath} spellcheck="false" disabled={items.length > 0} />
    </label>
    <span class="status" class:busy>
      {status === "none" ? "ready" : status.replace("_", " ")}{model ? ` · ${model}` : ""}
    </span>
  </header>

  {#if setup?.problem}
    <p class="notice">{setup.problem}</p>
  {/if}

  <section class="chat" bind:this={scroller}>
    {#each items as item (item.id)}
      {#if item.type === "user"}
        <div class="msg user">{item.text}</div>
      {:else if item.type === "text"}
        <div class="msg assistant">{item.text}</div>
      {:else if item.type === "tool"}
        <details class="tool" class:failed={item.isError}>
          <summary>{item.name}{item.result === undefined ? " …" : ""}</summary>
          <pre>{JSON.stringify(item.input, null, 2)}</pre>
          {#if item.result !== undefined}<pre>{item.result}</pre>{/if}
        </details>
      {:else if item.type === "approval"}
        <div class="approval">
          {#if item.approval === "plan"}
            <strong>Plan</strong>
            <div class="plan">{planText(item.input)}</div>
          {:else if item.approval === "question"}
            <strong>Claude has a question</strong>
            <pre>{JSON.stringify(item.input, null, 2)}</pre>
          {:else}
            <strong>{item.toolName}</strong> <span>{item.title}</span>
            <pre>{JSON.stringify(item.input, null, 2)}</pre>
          {/if}

          {#if item.answered}
            <em>{item.answered.replace("_", " ")}</em>
          {:else if item.approval === "plan"}
            <div class="row">
              <button class="primary" onclick={() => answer(item, "allow")}>Approve plan</button>
              <button onclick={() => answer(item, "deny", "Keep planning.")}>Keep planning</button>
            </div>
          {:else if item.approval === "question"}
            <div class="row">
              <button
                onclick={() =>
                  answer(item, "deny", "Questions aren't supported in this build yet. Use your best judgment.")}
              >
                Skip
              </button>
            </div>
          {:else}
            <div class="row">
              <button class="primary" onclick={() => answer(item, "allow")}>Allow</button>
              <button onclick={() => answer(item, "always_allow")}>Always allow</button>
              <button onclick={() => answer(item, "deny")}>Deny</button>
            </div>
          {/if}
        </div>
      {:else}
        <div class="msg error">{item.text}</div>
      {/if}
    {/each}
  </section>

  <footer>
    <textarea
      bind:value={draft}
      onkeydown={onKey}
      rows="3"
      placeholder="Message Claude Code. Enter to send, Shift+Enter for a new line."
    ></textarea>
    {#if busy}
      <button onclick={() => interrupt(sessionId)}>Stop</button>
    {:else}
      <button class="primary" onclick={send} disabled={!draft.trim() || !setup?.logged_in}>Send</button>
    {/if}
  </footer>
</main>

<style>
  :global(:root) {
    --bg: #ffffff;
    --fg: #1c1c1e;
    --muted: #6b6b70;
    --line: #e3e3e6;
    --soft: #f4f4f6;
    --accent: #c2603c;
    --error: #b3261e;
    color-scheme: light dark;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #1b1b1d;
      --fg: #ececee;
      --muted: #9a9aa0;
      --line: #333336;
      --soft: #252528;
      --accent: #e0825f;
      --error: #f2867f;
    }
  }
  :global(html, body, #app) {
    height: 100%;
    margin: 0;
  }
  :global(body) {
    background: var(--bg);
    color: var(--fg);
    font: 15px/1.55 -apple-system, "Segoe UI", system-ui, sans-serif;
  }
  main {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
    color: var(--muted);
  }
  header label {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  header input {
    flex: 1;
    font: inherit;
    color: var(--fg);
    background: var(--soft);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 4px 8px;
  }
  .status.busy {
    color: var(--accent);
  }
  .notice {
    margin: 0;
    padding: 8px 16px;
    background: var(--soft);
    color: var(--error);
  }
  .chat {
    flex: 1;
    overflow-y: auto;
    padding: 20px 16px;
  }
  .chat > :global(*) {
    max-width: 760px;
    margin: 0 auto 14px;
    /* Let the browser skip layout for blocks that are off screen. */
    content-visibility: auto;
    contain-intrinsic-size: auto 60px;
  }
  .msg {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .msg.user {
    background: var(--soft);
    border-radius: 12px;
    padding: 10px 14px;
  }
  .msg.error {
    color: var(--error);
  }
  .tool,
  .approval {
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 8px 12px;
    font-size: 13px;
  }
  .tool summary {
    cursor: pointer;
    color: var(--muted);
  }
  .tool.failed summary {
    color: var(--error);
  }
  .approval {
    border-color: var(--accent);
  }
  .approval span,
  .approval em {
    color: var(--muted);
  }
  .plan {
    white-space: pre-wrap;
    margin: 8px 0;
  }
  pre {
    margin: 8px 0 0;
    padding: 8px;
    background: var(--soft);
    border-radius: 6px;
    font: 12px/1.45 ui-monospace, "SF Mono", Consolas, monospace;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 240px;
    overflow-y: auto;
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  footer {
    display: flex;
    gap: 10px;
    align-items: flex-end;
    padding: 12px 16px 16px;
    border-top: 1px solid var(--line);
  }
  textarea {
    flex: 1;
    resize: none;
    font: inherit;
    color: var(--fg);
    background: var(--soft);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px 12px;
  }
  textarea:focus,
  header input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  button {
    font: inherit;
    color: var(--fg);
    background: var(--soft);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 7px 14px;
    cursor: pointer;
  }
  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
