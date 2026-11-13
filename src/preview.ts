// A stand-in for the core, so the window can be looked at in a plain browser
// with sample data: `npm run dev`, then open /preview.html. For working on the
// look only. It is not part of the built app (the build starts from index.html).

import { mount } from "svelte";
import type { AgentEvent, ProjectRow, SessionRow } from "./lib/ipc";
import App from "./App.svelte";

type Callback = (message: unknown) => void;
const callbacks = new Map<number, Callback>();
let nextCallback = 1;
let channel = 0;
let index = 0;

/** Delivers an event the way the core's channel does. */
function emit(session_id: string, event: AgentEvent) {
  callbacks.get(channel)?.({ message: { session_id, event }, index: index++ });
}

const home = "/Users/you";
const now = Date.now();
const row = (id: string, project_path: string, title: string, minutesAgo: number): SessionRow => ({
  id,
  project_path,
  title,
  mode: "default",
  model: "default",
  agent_session_id: "agent-" + id,
  last_used_at: now - minutesAgo * 60_000,
});

let projects: ProjectRow[] = [
  {
    id: "p1",
    path: `${home}/code/harness`,
    sessions: [
      row("saved", `${home}/code/harness`, "Add a sidebar with saved chats", 2),
      row("working", `${home}/code/harness`, "Why is the idle timer late when hidden?", 9),
      row("waiting", `${home}/code/harness`, "Bump dependencies and run the tests", 30),
      row("unseen", `${home}/code/harness`, "Tidy the footer so it stays on one line", 55),
    ],
  },
  {
    id: "p2",
    path: `${home}/code/a-rather-long-folder-name/website`,
    sessions: [
      row("old1", `${home}/code/a-rather-long-folder-name/website`, "Fix the broken image links on the pricing page", 600),
      row("old2", `${home}/code/a-rather-long-folder-name/website`, "Draft the launch post", 2000),
      row("long", `${home}/code/a-rather-long-folder-name/website`, "A very long chat (300 turns)", 3000),
    ],
  },
];

const text = (message_id: string, body: string): AgentEvent => ({
  kind: "assistant_block",
  message_id,
  block: { type: "text", text: body },
  parent_tool_id: null,
});
const tool = (tool_id: string, name: string, input: unknown): AgentEvent => ({
  kind: "assistant_block",
  message_id: "m-" + tool_id,
  block: { type: "tool_call", tool_id, name, input },
  parent_tool_id: null,
});
const result = (tool_id: string, content: string, is_error = false): AgentEvent => ({
  kind: "tool_result",
  tool_id,
  content,
  is_error,
  parent_tool_id: null,
});
const ended: AgentEvent = { kind: "turn_ended", ok: true, duration_ms: 4200 };

const savedChat: AgentEvent[] = [
  { kind: "user_message", text: "Add a sidebar with saved chats. Keep the look." },
  { kind: "session_started", agent_session_id: "agent-saved", model: "claude-fable-5-2", mode: "default" },
  tool("t1", "Read", { file_path: `${home}/code/harness/src/App.svelte` }),
  result("t1", "…"),
  tool("t2", "Grep", { pattern: "projectPath" }),
  result("t2", "src/lib/state/chat.svelte.ts:63\nsrc/lib/chat/Composer.svelte:39"),
  tool("t3", "Edit", {
    file_path: `${home}/code/harness/src/App.svelte`,
    old_string: "<main>\n  <ChatView />",
    new_string: "<div class=\"app\">\n  <Sidebar {app} />\n  <main>\n    <ChatView />",
  }),
  {
    kind: "approval_request",
    request_id: "r1",
    approval: "tool",
    tool_name: "Edit",
    title: "src/App.svelte",
    input: { file_path: `${home}/code/harness/src/App.svelte`, old_string: "<main>", new_string: "<div class=\"app\">" },
    tool_id: "t3",
    always_label: "Allow all edits this chat",
  },
  { kind: "approval_answered", request_id: "r1", decision: "always_allow", answers: null },
  { kind: "mode_changed", mode: "acceptEdits" },
  result("t3", "ok"),
  text(
    "m1",
    "# The sidebar is in\n\nChats are grouped by **project folder**, newest first.\n\n## What changed\n\n- A small database holds projects, chats and their history.\n- Reopening a chat *replays* its saved events.\n- [ ] Still to do: the side panel.\n\n```ts\nconst chat = Chat.fromSaved(row, events, live);\napp.open(chat.sessionId);\n```\n\nRun `npm run tauri dev` to try it.",
  ),
  ended,
  { kind: "user_message", text: "Run the tests too." },
  tool("t4", "Bash", { command: "cargo test", description: "Run the core's tests" }),
  {
    kind: "approval_request",
    request_id: "r2",
    approval: "tool",
    tool_name: "Bash",
    title: "Run the core's tests",
    input: { command: "cargo test" },
    tool_id: "t4",
    always_label: "Always allow cargo test",
  },
  { kind: "approval_answered", request_id: "r2", decision: "allow", answers: null },
  result("t4", "running 29 tests\ntest result: ok. 29 passed; 0 failed"),
  text("m2", "All 29 tests pass."),
  ended,
];

/** Three hundred short turns, for checking that a long chat stays light. */
const longChat: AgentEvent[] = Array.from({ length: 300 }, (_, n) => [
  { kind: "user_message", text: `Question ${n + 1}` } as AgentEvent,
  tool(`long-${n}`, "Read", { file_path: `${home}/code/site/page${n}.ts` }),
  result(`long-${n}`, "…"),
  text(`long-m${n}`, `## Answer ${n + 1}\n\nA paragraph with **bold** text and \`code\`.\n\n- one\n- two`),
  ended,
]).flat();

/** A scripted answer to any message typed in the preview. */
async function play(id: string) {
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  emit(id, { kind: "session_started", agent_session_id: "agent-" + id, model: "claude-fable-5-2", mode: "default" });
  emit(id, { kind: "status", state: "working" });
  await wait(900);
  const words = "Here is a **sample answer**, streamed a few words at a time so the live text can be checked. ".split(" ");
  for (const word of words) {
    emit(id, { kind: "text_delta", message_id: "live", block_index: 0, text: word + " " });
    await wait(60);
  }
  emit(id, text("live", words.join(" ").trim()));
  emit(id, tool("live-t", "Bash", { command: "npm test", description: "Run the tests" }));
  emit(id, {
    kind: "approval_request",
    request_id: "live-r",
    approval: "tool",
    tool_name: "Bash",
    title: "Run the tests",
    input: { command: "npm test" },
    tool_id: "live-t",
    always_label: "Always allow npm test",
  });
  emit(id, { kind: "status", state: "waiting_approval" });
}

function finish(id: string) {
  emit(id, { kind: "status", state: "working" });
  emit(id, result("live-t", "12 passed"));
  emit(id, text("live-2", "Done. The tests pass."));
  emit(id, ended);
  emit(id, { kind: "status", state: "idle" });
}

type Args = Record<string, any>;

const commands: Record<string, (args: Args) => unknown> = {
  subscribe: (args) => {
    channel = args.onEvent.id;
    // Give the sidebar one of each mark.
    setTimeout(() => {
      emit("working", { kind: "status", state: "working" });
      emit("waiting", { kind: "status", state: "waiting_approval" });
      emit("unseen", ended);
    }, 50);
  },
  check_setup: () => ({ binary: "claude", logged_in: true, subscription: "max", problem: null }),
  sidebar: () => projects,
  load_session: (args) => ({
    session: projects.flatMap((p) => p.sessions).find((s) => s.id === args.sessionId),
    events: args.sessionId === "saved" ? savedChat : args.sessionId === "long" ? longChat : [{ kind: "user_message", text: "(sample chat)" }, text("m", "Nothing saved for this one in the preview."), ended],
    live: false,
  }),
  delete_session: (args) => {
    projects = projects
      .map((p) => ({ ...p, sessions: p.sessions.filter((s) => s.id !== args.sessionId) }))
      .filter((p) => p.sessions.length > 0);
  },
  send_message: (args) => {
    const known = projects.flatMap((p) => p.sessions).some((s) => s.id === args.sessionId);
    if (!known) {
      const project = projects.find((p) => p.path === args.projectPath) ?? { id: args.projectPath, path: args.projectPath, sessions: [] };
      project.sessions = [row(args.sessionId, args.projectPath, args.text.slice(0, 60), 0), ...project.sessions];
      projects = [project, ...projects.filter((p) => p !== project)];
    }
    void play(args.sessionId);
  },
  answer_approval: (args) => finish(args.sessionId),
  interrupt: (args) => {
    emit(args.sessionId, { kind: "interrupted" });
    emit(args.sessionId, { kind: "turn_ended", ok: false, duration_ms: null });
    emit(args.sessionId, { kind: "status", state: "idle" });
  },
  set_mode: (args) => emit(args.sessionId, { kind: "mode_changed", mode: args.mode }),
  set_model: () => undefined,
  list_models: () => [
    { value: "default", name: "Default (Fable 5.2)", detail: null },
    { value: "opus", name: "Opus 5.5", detail: null },
    { value: "haiku", name: "Haiku 5.5", detail: null },
  ],
  close_session: () => undefined,
  autotest: () => null,
  "plugin:path|resolve_directory": () => home,
  "plugin:dialog|open": () => null,
  "plugin:opener|open_url": () => undefined,
};

(window as any).__TAURI_INTERNALS__ = {
  transformCallback(callback: Callback) {
    const id = nextCallback++;
    callbacks.set(id, callback);
    return id;
  },
  unregisterCallback(id: number) {
    callbacks.delete(id);
  },
  async invoke(command: string, args: Args = {}) {
    const handler = commands[command];
    if (!handler) throw new Error(`The preview has no stand-in for "${command}"`);
    return handler(args);
  },
};

mount(App, { target: document.getElementById("app")! });
