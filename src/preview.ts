// A stand-in for the core, so the window can be looked at in a plain browser
// with sample data: `npm run dev`, then open /preview.html. For working on the
// look only. It is not part of the built app (the build starts from index.html).

import { mount } from "svelte";
import type { AgentEvent, PrInfo, ProjectRow, SessionRow } from "./lib/ipc";
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
const part = (number: number, state: PrInfo["state"], branch: string): PrInfo => ({
  number,
  state,
  url: `https://github.com/you/harness/pull/${number}`,
  head: "abc",
  branch,
});
const row = (
  id: string,
  project_path: string,
  title: string,
  minutesAgo: number,
  branch: string | null = null,
  // One state makes a single pull request for `branch`; a list is used as given.
  pr: PrInfo["state"] | PrInfo[] | null = null,
): SessionRow => ({
  id,
  project_path,
  title,
  mode: "default",
  model: "default",
  effort: "default",
  agent_session_id: "agent-" + id,
  last_used_at: now - minutesAgo * 60_000,
  work_path: branch ? `${home}/Library/Application Support/harness/worktrees/${id}` : null,
  branch,
  wt_status: branch ? "live" : null,
  prs: !pr ? [] : typeof pr === "string" ? [part(40 + minutesAgo, pr, branch ?? "")] : pr,
  trashed_at: null,
});

/** A task split into pull requests: one landed, two open, the last built on the second. */
const split = [
  part(61, "merged", "harness/split-task-part-1"),
  part(62, "open", "harness/split-task-part-2"),
  part(63, "open", "harness/split-task-part-3"),
];

/** Chats in the trash, for the sidebar's trash list. */
let trash: SessionRow[] = [
  { ...row("trashed1", `${home}/code/harness`, "Try a blur on the side panel", 4000), trashed_at: now - 3 * 86_400_000 },
  { ...row("trashed2", `${home}/code/harness`, "Old spike: Electron comparison", 9000, "harness/old-spike-1a2b3c"), trashed_at: now - 29.5 * 86_400_000 },
];

let projects: ProjectRow[] = [
  {
    id: "p1",
    path: `${home}/code/harness`,
    sessions: [
      row("planning", `${home}/code/harness`, "Idle timeout for chat processes", 1),
      row("images", `${home}/code/harness`, "Settings list cut off", 1.5),
      row("saved", `${home}/code/harness`, "Add a sidebar with saved chats", 2, "harness/add-a-sidebar-9f2c1e", "open"),
      row("working", `${home}/code/harness`, "Why is the idle timer late when hidden?", 9, "harness/idle-timer-late-4be0a1"),
      row("waiting", `${home}/code/harness`, "Bump dependencies and run the tests", 30),
      row("unseen", `${home}/code/harness`, "Tidy the footer so it stays on one line", 55, "harness/tidy-the-footer-77d3aa", "merged"),
      row("looks", `${home}/code/harness`, "Move the idle timer to the core", 70, "harness/idle-timer-core-c01d5e", "draft"),
      row("split", `${home}/code/harness`, "Several pull requests per chat", 80, "harness/split-task-part-3", split),
      row("landed", `${home}/code/harness`, "Rename the settings keys", 90, null, [
        part(71, "merged", "harness/rename-keys-1"),
        part(72, "closed", "harness/rename-keys-2"),
        part(73, "closed", "harness/rename-keys-3"),
      ]),
      row("dropped", `${home}/code/harness`, "Try a native menu bar", 95, null, [
        part(74, "closed", "harness/native-menu-1"),
        part(75, "closed", "harness/native-menu-2"),
      ]),
    ],
  },
  {
    id: "p2",
    path: `${home}/code/a-rather-long-folder-name/website`,
    sessions: [
      row("old1", `${home}/code/a-rather-long-folder-name/website`, "Fix the broken image links on the pricing page", 600, "harness/fix-the-broken-image-0aa1b2", "closed"),
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

/** A chat where Claude asked for a worktree, waiting for the answer. */
const askingChat: AgentEvent[] = [
  { kind: "user_message", text: "Bump dependencies and run the tests." },
  { kind: "session_started", agent_session_id: "agent-waiting", model: "claude-fable-5-2", mode: "auto" },
  tool("w1", "Read", { file_path: `${home}/code/harness/package.json` }),
  result("w1", "…"),
  tool("w2", "EnterWorktree", { name: "bump dependencies" }),
  { kind: "worktree_ask", request_id: "wt-1", repo_root: `${home}/code/harness`, tool_id: "w2", name: "bump dependencies" },
];

/** A chat that moved to a worktree, then was put away and brought back. */
const movedChat: AgentEvent[] = [
  { kind: "user_message", text: "Why is the idle timer late when hidden?" },
  { kind: "session_started", agent_session_id: "agent-working", model: "claude-fable-5-2", mode: "auto" },
  tool("v1", "EnterWorktree", { name: "idle timer late" }),
  { kind: "worktree_ask", request_id: "wt-2", repo_root: `${home}/code/harness`, tool_id: "v1", name: "idle timer late" },
  { kind: "worktree_chosen", choice: "worktree" },
  { kind: "worktree_changed", status: "created", branch: "harness/idle-timer-late-4be0a1" },
  result("v1", "Harness is making this chat's worktree itself. Stop here; you will be told where to continue.", true),
  text("v2", "Moved. The timer runs in the window, which the system slows down when hidden."),
  ended,
  { kind: "worktree_changed", status: "put_away", branch: "harness/idle-timer-late-4be0a1" },
  { kind: "user_message", text: "Move it to the core then." },
  { kind: "worktree_changed", status: "restored", branch: "harness/idle-timer-late-4be0a1" },
  text("v3", "Done: the core now times it."),
  ended,
];

/** A chat with a pasted screenshot, and one a tool took. For images. */
const imagesChat: AgentEvent[] = [
  {
    kind: "user_message",
    text: "The theme list on the settings page gets cut off at the bottom. [image 1]",
    images: [{ n: 1, media_type: "image/png" }],
  },
  { kind: "session_started", agent_session_id: "agent-images", model: "claude-opus-5-5", mode: "default" },
  text("i1", "I see it: the list stops at Tokyo Night and there's no way to scroll. Checking the layout."),
  tool("it1", "Read", { file_path: `${home}/code/harness/src/lib/Settings.svelte` }),
  result("it1", "…"),
  text("i2", "The list had a fixed height taller than the window. It now fills the space left and scrolls. Here's the page after the change:"),
  tool("it2", "Edit", {
    file_path: `${home}/code/harness/src/lib/Settings.svelte`,
    old_string: "  height: 640px;",
    new_string: "  flex: 1;\n  min-height: 0;\n  overflow-y: auto;",
  }),
  result("it2", "ok"),
  tool("it3", "Read", { file_path: `${home}/code/harness/after.png` }),
  {
    kind: "tool_result",
    tool_id: "it3",
    content: [{ type: "image_ref", n: 2, media_type: "image/png" }],
    is_error: false,
    parent_tool_id: null,
  },
  ended,
];

/** Images pasted in the preview, by chat, numbered from 1 like the core does. */
const pasted = new Map<string, ArrayBuffer[]>();

/** A stand-in screenshot for the sample chat's images. */
async function sampleImage(n: number): Promise<ArrayBuffer> {
  const canvas = document.createElement("canvas");
  canvas.width = 1440;
  canvas.height = 900;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#111210";
  ctx.fillRect(0, 0, 1440, 900);
  ctx.fillStyle = "#1a1b17";
  ctx.fillRect(380, 200, 420, n === 1 ? 700 : 470);
  ctx.font = "20px Menlo, monospace";
  ["Harness Dark", "Harness Light", "Solarized Dark", "Solarized Light", "Gruvbox Dark", "Gruvbox Light", "Dracula", "Nord", "Tokyo Ni"]
    .slice(0, n === 1 ? 9 : 8)
    .forEach((name, i) => {
      ctx.fillStyle = i === 0 ? "#d9a35b" : "#cfccc2";
      ctx.fillText(name, 404, 245 + i * 72);
    });
  ctx.fillStyle = "#d9a35b";
  ctx.font = "bold 26px Menlo, monospace";
  ctx.fillText(n === 1 ? "Settings (before)" : "Settings (after)", 380, 120);
  const blob = await new Promise<Blob>((done) => canvas.toBlob((b) => done(b!), "image/png"));
  return blob.arrayBuffer();
}

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
    "# The sidebar is in\n\nChats are grouped by **project folder**, newest first.\n\n## What changed\n\n- A small database holds projects, chats and their history.\n- Reopening a chat *replays* its saved events.\n- [ ] Still to do: the side panel.\n\n```ts\nconst chat = Chat.fromSaved(row, events, live);\napp.open(chat.sessionId);\n```\n\nRun `npm run tauri dev` to try it. The plan is in [PLAN.md](PLAN.md) and the look in [the notes](docs/look.md#colours); the [Tauri docs](https://tauri.app) have more. A [missing file](gone.md) shows an error.",
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

// A chat in plan mode: it wrote two documents, then presented a plan that is
// still waiting for an answer. For the side panel.
const notesPath = `${home}/code/harness/docs/memory-notes.md`;
const testPlanPath = `${home}/code/harness/docs/test-plan.md`;
const planPath = `${home}/.claude/plans/idle-timeout.md`;
const notesText =
  "# Memory per chat\n\nWhat each part of the app holds, measured on this Mac with a release build and the window hidden.\n\n## Readings\n\n- A waiting chat's process holds about 260 MB.\n- After a few turns it rises to about 310 MB.\n- The app alone, with a short chat open: 105 to 160 MB.\n\n## Takeaway\n\nFour idle chats cost over 1 GB today. With `IDLE_TIMEOUT` at five minutes they cost nothing.\n\n| Part | Memory |\n|---|---|\n| App alone | 105 to 160 MB |\n| One chat's process | 260 to 310 MB |\n";
const planText = (revised: boolean) =>
  "# Idle timeout for chats\n\nClose a chat's process " +
  (revised ? "**ten** minutes" : "five minutes") +
  " after its last answer, and resume it on the next message.\n\n## What changes\n\n- Each session gets an idle timer, started when a turn ends.\n- A new message cancels the timer.\n- When it fires, the process is asked to exit. It finishes background work first.\n- The next message resumes the chat by its session ID.\n\n## Order of work\n\n1. Add the timer to the session manager in `sessions.rs`\n2. Close on fire, cancel on message\n3. Resume on the next send\n4. Test at 5 and 45 seconds with the window hidden\n\n```rust\nfn spawn_idle_timer(id: SessionId, after: Duration) -> JoinHandle<()> {\n    tokio::spawn(async move { sleep(after).await; close(id); })\n}\n```\n\n## Not covered\n\n- Chats with background work still running are left alone.\n- The timeout is fixed until the settings page has it.\n";
const planRequest = (request_id: string, tool_id: string, revised: boolean): AgentEvent => ({
  kind: "approval_request",
  request_id,
  approval: "plan",
  tool_name: "ExitPlanMode",
  title: "Plan",
  input: { plan: planText(revised), planFilePath: planPath },
  tool_id,
  always_label: "Approve, auto-accept edits",
});

const planningChat: AgentEvent[] = [
  { kind: "user_message", text: "Chats I've walked away from should stop using memory. Plan it out before changing anything." },
  { kind: "session_started", agent_session_id: "agent-planning", model: "claude-fable-5-2", mode: "plan" },
  text("p1", "I'll look at how chats are tracked, measure what an idle one holds, then write a plan."),
  tool("pt1", "Read", { file_path: `${home}/code/harness/src-tauri/src/sessions.rs` }),
  result("pt1", "…"),
  tool("pt2", "Grep", { pattern: "SessionState::Idle" }),
  result("pt2", "src-tauri/src/sessions.rs:88"),
  text("p2", "The session manager already knows when a turn ends, so the timer can start there. I wrote down what I measured, and a short list of what to test."),
  tool("pt3", "Bash", { command: "ps -o rss,command -ax | grep claude" }),
  result("pt3", "264112 claude -p"),
  tool("pt4", "Write", { file_path: notesPath, content: notesText }),
  result("pt4", "ok"),
  tool("pt5", "Write", { file_path: testPlanPath, content: "# What to test\n\n- [ ] The timer fires at 5 seconds\n- [ ] A message cancels it\n- [x] Resume works after an exit\n" }),
  result("pt5", "ok"),
  // The plan's own file: it becomes the plan tab, not a document tab.
  tool("pt6", "Write", { file_path: planPath, content: planText(false) }),
  result("pt6", "ok"),
  tool("pt7", "ExitPlanMode", { plan: planText(false), planFilePath: planPath }),
  planRequest("plan-r1", "pt7", false),
];

/** What the preview does when the sample plan is answered. */
async function afterPlan(id: string, decision: string) {
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  emit(id, { kind: "status", state: "working" });
  await wait(700);
  if (decision === "deny") {
    // Keep planning: a revised plan replaces the old one in the same tab.
    emit(id, result("pt7", "The user wants changes.", true));
    emit(id, text("p3", "Understood. I've moved the timeout to ten minutes."));
    emit(id, tool("pt8", "ExitPlanMode", { plan: planText(true), planFilePath: planPath }));
    emit(id, planRequest("plan-r2", "pt8", true));
    emit(id, { kind: "status", state: "waiting_approval" });
    return;
  }
  emit(id, { kind: "mode_changed", mode: decision === "always_allow" ? "acceptEdits" : "default" });
  emit(id, result("pt7", "Plan approved."));
  // An edit to a document the chat wrote: its tab follows.
  emit(id, tool("pt9", "Edit", { file_path: notesPath, old_string: "## Takeaway", new_string: "## Takeaway (after the change)" }));
  emit(id, result("pt9", "ok"));
  emit(id, text("p4", "The timer is in. I updated the notes with the new readings."));
  emit(id, ended);
  emit(id, { kind: "status", state: "idle" });
}

/** Short and long messages from both sides, for judging how the user's messages stand out. */
const looksChat: AgentEvent[] = [
  { kind: "user_message", text: "Why is the idle timer late when the window is hidden?" },
  tool("k1", "Grep", { pattern: "setTimeout" }),
  result("k1", "src/lib/state/app.svelte.ts:112"),
  tool("k2", "Read", { file_path: `${home}/code/harness/src/lib/state/app.svelte.ts` }),
  result("k2", "…"),
  text(
    "k-m1",
    "The timer runs in the window, and a hidden window's timers are slowed down by the system.\n\n## What happens\n\n- The chat goes quiet and a 10 minute timer starts in the page.\n- The window is hidden, so the system only lets the timer fire about once a minute, sometimes less.\n- The shutdown lands late, or not until the window is shown again.\n\nThe fix is to keep the clock in the core, which is never slowed down, and have it tell the window when time is up.",
  ),
  ended,
  {
    kind: "user_message",
    text: "ok, move it to the core. last time a change like this broke the approvals flow and it took most of an evening to work out why, so please go slowly and check each step before moving on. a few things to keep in mind:\n\n- the timer should restart on any activity, not just my messages\n- an approval that is waiting on me should never be shut down\n- keep the 10 minutes as a setting we can change later\n\nalso here is the log from last night in case it helps:\n\n22:14:03 idle timer armed (600s)\n22:31:47 idle timer fired, 1064s after arming\n22:31:47 closing session agent-working",
  },
  tool("k3", "Edit", { file_path: `${home}/code/harness/src-tauri/src/sessions.rs`, old_string: "a", new_string: "b" }),
  result("k3", "ok"),
  tool("k4", "Edit", { file_path: `${home}/code/harness/src/lib/state/app.svelte.ts`, old_string: "a", new_string: "b" }),
  result("k4", "ok"),
  text(
    "k-m2",
    "The clock now lives in the core.\n\n- Any event from the agent or from you restarts it.\n- A chat waiting on an approval is skipped.\n- The length is one constant, `IDLE_AFTER`, ready to become a setting.\n\nYour log matches the cause: 1064 seconds instead of 600.",
  ),
  ended,
  { kind: "user_message", text: "nice. run the tests" },
  tool("k5", "Bash", { command: "cargo test", description: "Run the core's tests" }),
  result("k5", "test result: ok. 31 passed; 0 failed"),
  text("k-m3", "All 31 tests pass, including two new ones for the idle clock."),
  ended,
  { kind: "user_message", text: "and the window side?" },
  text("k-m4", "Nothing to run there yet: the window has no tests of its own. I checked by hand that a hidden window now shuts down on time."),
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

/** Chats with a scripted answer under way, and the messages sent to each meanwhile. */
const playing = new Map<string, string[]>();

/** A scripted answer to any message typed in the preview. */
async function play(id: string, messageId: string) {
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  playing.set(id, []);
  emit(id, { kind: "session_started", agent_session_id: "agent-" + id, model: "claude-fable-5-2", mode: "default" });
  emit(id, { kind: "status", state: "working" });
  await wait(300);
  emit(id, { kind: "message_seen", id: messageId });
  await wait(600);
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
  // Messages sent while the answer was under way are taken in after the step.
  for (const messageId of playing.get(id) ?? []) emit(id, { kind: "message_seen", id: messageId });
  playing.delete(id);
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
      emit("planning", { kind: "status", state: "waiting_approval" });
      emit("unseen", ended);
    }, 50);
  },
  check_setup: () => ({ binary: "claude", problem: null }),
  sidebar: () => projects,
  list_trash: () => trash,
  restore_chat: (args) => {
    const chat = trash.find((s) => s.id === args.sessionId);
    trash = trash.filter((s) => s.id !== args.sessionId);
    if (chat) projects[0].sessions.unshift({ ...chat, trashed_at: null });
  },
  empty_trash: () => {
    trash = [];
  },
  choose_worktree: (args) => {
    emit(args.sessionId, { kind: "worktree_chosen", choice: args.choice });
    if (args.choice === "worktree") {
      emit(args.sessionId, { kind: "worktree_changed", status: "created", branch: "harness/bump-dependencies-5e6f7a" });
      for (const p of projects)
        for (const s of p.sessions)
          if (s.id === args.sessionId) Object.assign(s, row(s.id, s.project_path, s.title, 0, "harness/bump-dependencies-5e6f7a"));
    }
    finish(args.sessionId);
  },
  load_session: (args) => ({
    session: [...projects.flatMap((p) => p.sessions), ...trash].find((s) => s.id === args.sessionId),
    events: args.sessionId === "images" ? imagesChat : args.sessionId === "planning" ? planningChat : args.sessionId === "saved" ? savedChat : args.sessionId === "long" ? longChat : args.sessionId === "looks" ? looksChat : args.sessionId === "waiting" ? askingChat : args.sessionId === "working" ? movedChat : [{ kind: "user_message", text: "(sample chat)" }, text("m", "Nothing saved for this one in the preview."), ended],
    live: args.sessionId === "planning" || args.sessionId === "waiting",
  }),
  rename_session: (args) => {
    for (const p of projects) for (const s of p.sessions) if (s.id === args.sessionId) s.title = args.title.trim();
  },
  delete_session: (args) => {
    const chat = projects.flatMap((p) => p.sessions).find((s) => s.id === args.sessionId);
    if (chat) trash = [{ ...chat, trashed_at: Date.now() }, ...trash];
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
    const waiting = playing.get(args.sessionId);
    if (waiting) waiting.push(args.messageId);
    else void play(args.sessionId, args.messageId);
  },
  answer_approval: (args) =>
    String(args.requestId).startsWith("plan-") ? void afterPlan(args.sessionId, args.decision) : finish(args.sessionId),
  interrupt: (args) => {
    playing.delete(args.sessionId);
    emit(args.sessionId, { kind: "interrupted" });
    emit(args.sessionId, { kind: "turn_ended", ok: false, duration_ms: null });
    emit(args.sessionId, { kind: "status", state: "idle" });
  },
  set_mode: (args) => emit(args.sessionId, { kind: "mode_changed", mode: args.mode }),
  set_model: () => undefined,
  set_effort: () => undefined,
  list_models: () => {
    const all = ["low", "medium", "high", "xhigh", "max"];
    return [
      { value: "default", name: "Default (Fable 5.2)", detail: null, efforts: all, default_effort: "medium" },
      { value: "opus", name: "Opus 5.5", detail: null, efforts: all, default_effort: "high" },
      { value: "haiku", name: "Haiku 5.5", detail: null, efforts: all, default_effort: "medium" },
      { value: "claude-haiku-4-5", name: "Haiku 4.5", detail: null, efforts: [], default_effort: null },
    ];
  },
  close_session: () => undefined,
  autotest: () => null,
  "plugin:path|resolve_directory": () => home,
  "plugin:dialog|open": () => null,
  "plugin:opener|open_url": () => undefined,
  "plugin:opener|reveal_item_in_dir": () => undefined,
  save_image: (args: Args) => {
    const id = String(args.sessionId);
    const list = pasted.get(id) ?? [];
    pasted.set(id, list);
    // Like the core: the full-size original is what's looked at.
    const shown = args.original?.data ?? args.data;
    const bytes = Uint8Array.from(atob(String(shown)), (c) => c.charCodeAt(0));
    list.push(bytes.buffer);
    // The sample chat's own two images come first.
    const n = list.length + (id === "images" ? 2 : 0);
    return { n, media_type: args.mediaType };
  },
  read_image: (args: Args) => {
    const id = String(args.sessionId);
    const n = Number(args.n);
    const first = id === "images" ? 2 : 0;
    if (n <= first) return sampleImage(n);
    const bytes = pasted.get(id)?.[n - first - 1];
    if (!bytes) throw "Image gone";
    return bytes;
  },
  // The browser: no page is drawn here (the real one is a separate web view
  // the core places over the panel); the tab and address line still work.
  browser_navigate: (args: Args) => {
    const url = String(args.url).trim();
    if (!/^(https?:\/\/)?(localhost|127\.0\.0\.1|\[::1\])?(:\d+)?(\/|$)/.test(url)) throw `Only localhost pages can be opened here, not ${url}.`;
    const full = url.includes("://") ? url : `http://${url.startsWith(":") ? "localhost" + url : url}`;
    emit(String(args.chatId), { kind: "browser_state", url: full, title: "", loading: true });
    setTimeout(() => emit(String(args.chatId), { kind: "browser_state", url: full, title: "Sample page", loading: false }), 400);
  },
  browser_reload: () => {},
  browser_place: () => {},
  browser_close: (args: Args) => emit(String(args.chatId), { kind: "browser_closed" }),
  browser_current: (args: Args) =>
    args.chatId === "planning" ? { url: "http://localhost:5173/", title: "My app", loading: false } : null,
  read_doc: (args: Args) => {
    const path = String(args.path);
    if (path.endsWith("gone.md")) throw "That file doesn't exist.";
    return `# ${path.split("/").pop()}\n\nRead from \`${path}\` on disk.\n\nA [link back](../PLAN.md) opens the plan file.`;
  },
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

// For screenshots: `#open=<chat id>` opens that chat, `&trash` opens the
// sidebar's trash list, and `&browser` the panel's browser view.
const wanted = new URLSearchParams(location.hash.slice(1));
setTimeout(() => {
  const title = [...projects.flatMap((p) => p.sessions), ...trash].find((s) => s.id === wanted.get("open"))?.title;
  if (wanted.has("trash")) document.querySelector<HTMLButtonElement>("aside .trash button")?.click();
  setTimeout(() => {
    if (!title) return;
    const buttons = [...document.querySelectorAll<HTMLButtonElement>("aside button.open")];
    buttons.find((b) => b.textContent?.includes(title))?.click();
    if (wanted.has("browser")) {
      setTimeout(() => {
        const bar = [...document.querySelectorAll<HTMLButtonElement>("header.bar button")];
        bar.find((b) => b.textContent?.includes("browser") || b.textContent?.includes("◂"))?.click();
      }, 200);
    }
  }, 100);
}, 400);
