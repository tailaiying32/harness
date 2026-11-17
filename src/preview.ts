// A stand-in for the core, so the window can be looked at in a plain browser
// with sample data: `npm run dev`, then open /preview.html. For working on the
// look only. It is not part of the built app (the build starts from index.html).

import { mount } from "svelte";
import { dropFiles, type AgentEvent, type PrInfo, type ProjectRow, type SessionRow } from "./lib/ipc";
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

// Each chat's browser tabs, as the core keeps them.
type PreviewTabs = { tabs: { id: string; url: string; title: string; loading: boolean }[]; shown: string | null; next: number };
const browsers = new Map<string, PreviewTabs>([
  ["planning", { tabs: [{ id: "1", url: "http://localhost:5173/", title: "My app", loading: false }], shown: "1", next: 1 }],
]);
function browserOf(chat: string): PreviewTabs {
  let pages = browsers.get(chat);
  if (!pages) browsers.set(chat, (pages = { tabs: [], shown: null, next: 0 }));
  return pages;
}
function newBrowserTab(chat: string): string {
  const pages = browserOf(chat);
  const id = String(++pages.next);
  pages.tabs.push({ id, url: "", title: "", loading: false });
  pages.shown = id;
  return id;
}
function emitBrowser(chat: string) {
  const pages = browserOf(chat);
  emit(chat, { kind: "browser_tabs", tabs: pages.tabs.map((t) => ({ ...t })), shown: pages.shown });
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
  agent: "claude-code",
});

/** A task split into pull requests: one landed, two open, the last built on the second. */
const split = [
  part(61, "merged", "harness/split-task-part-1"),
  part(62, "open", "harness/split-task-part-2"),
  part(63, "open", "harness/split-task-part-3"),
];

/** The app's own skills, for Settings. */
let skills = [
  {
    name: "update-kb",
    description:
      "Assess and draft updates to the public knowledge base for customer-visible product changes. Use while implementing, reviewing, or finishing a ticket, pull request, or code change.",
  },
  { name: "local-qa", description: "Click through the app locally before a review." },
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
      row("agents", `${home}/code/harness`, "Find where idle chats get closed", 1.2),
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
      // Past the sidebar's cap of eight: hidden behind "more", except the
      // one with an open pull request.
      row("old1", `${home}/code/harness`, "Pick a monospace font", 2000),
      row("old2", `${home}/code/harness`, "Check memory against the desktop app", 3000),
      row("oldpr", `${home}/code/harness`, "Windows build script", 3500, "harness/windows-build-5e6f7a", "open"),
      row("old3", `${home}/code/harness`, "First window", 5000),
      row("old4", `${home}/code/harness`, "Font size setting", 5200),
      row("old5", `${home}/code/harness`, "Theme list: twenty palettes", 5400),
      row("old6", `${home}/code/harness`, "Keep a chat process alive briefly", 5600),
      row("old7", `${home}/code/harness`, "Resume by session ID", 5800),
      row("old8", `${home}/code/harness`, "Sign-in card when login fails", 6000),
      row("old9", `${home}/code/harness`, "Trash and purge after 30 days", 6200),
      row("old10", `${home}/code/harness`, "Code block tint per theme", 6400),
      row("old11", `${home}/code/harness`, "Tool calls folded into one line", 6600),
      row("old12", `${home}/code/harness`, "Window buttons in the sidebar", 6800),
      row("old13", `${home}/code/harness`, "Native picker for folders", 7000),
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

/** A subagent's step: a tool call or text under the main chat's tool call `parent`. */
const sub = (parent: string, tool_id: string, name: string, input: unknown): AgentEvent => ({
  kind: "assistant_block",
  message_id: "m-" + tool_id,
  block: { type: "tool_call", tool_id, name, input },
  parent_tool_id: parent,
});
const subSaid = (parent: string, body: string): AgentEvent => ({
  kind: "assistant_block",
  message_id: "t-" + parent + body.length,
  block: { type: "text", text: body },
  parent_tool_id: parent,
});
const subResult = (parent: string, tool_id: string, content: string): AgentEvent => ({
  kind: "tool_result",
  tool_id,
  content,
  is_error: false,
  parent_tool_id: parent,
});

/** A long turn with subagents, for chat info: one finished, one failed, one
    still running (played live after the chat opens, see load_session). */
const agentsChat: AgentEvent[] = [
  // An earlier turn's batch, finished, so the page has more than one to fold.
  { kind: "user_message", text: "Why does the app use so much memory with a lot of chats open?" },
  { kind: "session_started", agent_session_id: "agent-agents", model: "claude-fable-5-2", mode: "auto" },
  tool("e1", "Agent", { description: "Measure memory per chat", prompt: "Open five chats and measure each process's memory." }),
  { kind: "batch_titled", tool_id: "e1", title: "Looking into memory use" },
  sub("e1", "e1a", "Bash", { command: "ps -o pid,rss,command | grep claude" }),
  subResult("e1", "e1a", "4412  268144  claude --output-format stream-json"),
  result("e1", "About 260 MB per chat, idle or not."),
  tool("e2", "Agent", { description: "Check for leaks in the panel", prompt: "Look for listeners the side panel never removes." }),
  sub("e2", "e2a", "Grep", { pattern: "addEventListener", path: "src/lib/panel" }),
  subResult("e2", "e2a", "PanelBar.svelte:41  window.addEventListener(\"keydown\", onKey)"),
  result("e2", "No leaks: every listener is removed when its view closes."),
  ended,
  { kind: "user_message", text: "Chats I've walked away from should stop using memory. Find every place a chat's process is closed, and measure an idle chat." },
  tool("k1", "TaskCreate", { subject: "Find where chats close", description: "" }),
  result("k1", "Task #1 created successfully: Find where chats close"),
  tool("k2", "TaskCreate", { subject: "Measure an idle chat", description: "" }),
  result("k2", "Task #2 created successfully: Measure an idle chat"),
  tool("k3", "TaskCreate", { subject: "Write the plan", description: "" }),
  result("k3", "Task #3 created successfully: Write the plan"),
  tool("k4", "TaskUpdate", { taskId: "1", status: "completed" }),
  result("k4", "Updated task #1 status"),
  tool("k5", "TaskUpdate", { taskId: "2", status: "in_progress" }),
  result("k5", "Updated task #2 status"),
  text("a1", "Two things can run at once here, so I'll hand them to subagents."),
  tool("ag1", "Agent", {
    description: "find where chats close",
    prompt: "Find every place a chat's Claude Code process is closed or killed in src-tauri, and what triggers each. Report file and line for each.",
    subagent_type: "Explore",
  }),
  { kind: "task_started", task_id: "t1", tool_id: "ag1", description: "find where chats close", agent_type: "Explore", background: true },
  result("ag1", "Async agent launched successfully."),
  { kind: "batch_titled", tool_id: "ag1", title: "Tracing how chats close" },
  sub("ag1", "s1", "Grep", { pattern: "kill|close_idle|shutdown", path: "src-tauri" }),
  subResult("ag1", "s1", "sessions.rs:512   fn close_idle(&self, id: &SessionId)\nsessions.rs:688   child.kill().await\nterminal.rs:97    fn shutdown_all(&mut self)"),
  sub("ag1", "s2", "Read", { file_path: `${home}/code/harness/src-tauri/src/sessions.rs` }),
  subResult("ag1", "s2", ""),
  subSaid("ag1", "`close_idle` is the only timer path. The kill at 688 is for quit."),
  sub("ag1", "s3", "Read", { file_path: `${home}/code/harness/src-tauri/src/terminal.rs` }),
  subResult("ag1", "s3", ""),
  subSaid("ag1", "Chats close in two places: `close_idle` (sessions.rs:512) after the idle timer, and `child.kill()` (sessions.rs:688) on quit. Terminals close their own shells separately."),
  { kind: "task_ended", task_id: "t1", tool_id: "ag1", status: "completed", summary: "Chats close in two places: `close_idle` (sessions.rs:512) after the idle timer, and `child.kill()` (sessions.rs:688) on quit. Terminals close their own shells separately.", duration_ms: 108000 },
  tool("ag3", "Agent", { description: "run session tests", prompt: "Run the session black-box tests and report failures." }),
  sub("ag3", "s9", "Bash", { command: "cargo test sessions::tests" }),
  subResult("ag3", "s9", "test black_box::idle_closes ... FAILED\nerror: test failed"),
  result("ag3", "cargo test exited 101", true),
  { kind: "context_usage", used: 92_400, max: 200_000, parts: [
    { name: "Messages", tokens: 61_200 },
    { name: "System tools", tokens: 14_800 },
    { name: "Skills", tokens: 7_300 },
    { name: "System prompt", tokens: 3_500 },
    { name: "Memory files", tokens: 900 },
  ] },
];

/** The running subagent and a background command, sent live once the chat is open. */
function playAgents() {
  const say = (event: AgentEvent, after: number) => setTimeout(() => emit("agents", event), after);
  say({ kind: "status", state: "working" }, 50);
  say(tool("ag2", "Agent", {
    description: "measure an idle chat",
    prompt: "Build in release mode, open three chats, leave them idle, and measure each process's memory every minute for five minutes.",
  }), 100);
  say({ kind: "task_started", task_id: "t2", tool_id: "ag2", description: "measure an idle chat", agent_type: "general-purpose", background: true }, 120);
  say(result("ag2", "Async agent launched successfully."), 140);
  say({ kind: "background_work", running: 2, agents: 1, tasks: [
    { id: "b1", kind: "local_bash", description: "npm run dev" },
    { id: "t2", kind: "local_agent", description: "measure an idle chat" },
  ] }, 160);
  say(sub("ag2", "s5", "Bash", { command: "cargo build --release" }), 300);
  say(subResult("ag2", "s5", "   Compiling harness v0.4.0\n    Finished `release` profile [optimized] target(s) in 1m 48s"), 600);
  say(sub("ag2", "s6", "Bash", { command: "ps -o pid,rss,command | grep claude" }), 800);
  say(subResult("ag2", "s6", "4412  268144  claude --output-format stream-json\n4419  259872  claude --output-format stream-json"), 1000);
  say(subSaid("ag2", "About 260 MB each at rest. Sampling once a minute to see whether it grows."), 1200);
  say(sub("ag2", "s7", "Bash", { command: "sleep 60 && ps -o rss -p 4412" }), 1400);
  say({ kind: "task_progress", task_id: "t2", tool_id: "ag2", description: "Running sleep 60 && ps -o rss -p 4412", tool_uses: 3, last_tool: "Bash" }, 1450);
}

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
    text: "The theme list on the settings page gets cut off at the bottom (reported in https://github.com/you/harness/issues/12). [image 1]",
    images: [{ n: 1, media_type: "image/png" }],
  },
  { kind: "session_started", agent_session_id: "agent-images", model: "claude-opus-5-5", mode: "default" },
  text("i1", "I see it: the list stops at Tokyo Night and there's no way to scroll. Checking the layout."),
  // A look at earlier chats: a search (opened, it lists the chats found) and a read.
  tool("ic1", "mcp__harness_chats__search", { query: "settings list height" }),
  result(
    "ic1",
    "looks  3 days ago  Move the idle timer to the core  (Claude Code, 4 turns, PR #31 draft)\n" +
      "    turn 2: …the settings list keeps a fixed height so the footer stays put…\n" +
      "saved  2 hours ago  Add a sidebar with saved chats  (Cursor, 9 turns, PR #28 open)\n" +
      "    turn 5: …height of the list follows the window…\n" +
      "(2 more matches not shown)\n",
  ),
  tool("ic2", "mcp__harness_chats__read", { id: "looks" }),
  result("ic2", "# Move the idle timer to the core · Claude Code · last used 3 days ago · 4 turns · PR #31 draft\n\n## Turn 1 of 4\n\n### User\n\n…"),
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
  // A long address, to show the page line cut short with "…".
  tool("it4", "mcp__harness_browser__open", {
    url: "localhost:5173/settings/themes/6ad870ba-ad9b-4475-9f7c-2abca1a5dec3/preview/13facc18-24ed-44d9-a7b8-2019fc4c5bae",
  }),
  result("it4", "Opened."),
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
// In the chat's own folder, so its tab has the save-to-project button.
// Written when chat folders were still under the app's data folder: the chat's
// folder has moved since (see `chatFolder`), and its tab shows it from there.
const testPlanPath = `${home}/Library/Application Support/com.harness.app/notes/planning/test-plan.md`;
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
  tool("k6", "mcp__harness_browser__open", { url: "localhost:1420" }),
  result("k6", "Opened http://localhost:1420/ (Harness)"),
  tool("k7", "mcp__harness_browser__screenshot", {}),
  result("k7", ""),
  text("k-m4", "I opened the app in the browser and checked it: the chat's idle mark now clears on time."),
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

// Keep awake, as the core holds it.
let keepAwake: { mode: "away" | "overnight"; ends_at: number } | null = null;

/** Documents edited in a tab, kept for the page's life. */
const docsByPath = new Map<string, string>();

/** A project's folder in the app's home folder, named from its path as the core names it. */
const projectHome = (project: string) => `${home}/.harness/projects/${project.replace(/[^A-Za-z0-9]/g, "-")}`;
/** A chat's own folder: its notes and docs, under its project. */
const chatFolder = (project: string, chatId: string) => `${projectHome(project)}/chats/${chatId}`;
const projectOf = (chatId: string) =>
  [...projects.flatMap((p) => p.sessions), ...trash].find((s) => s.id === chatId)?.project_path ?? `${home}/code/harness`;

const notesByChat = new Map<string, string>([
  [
    "planning",
    "## Idle timeout\n\n- Five minutes feels right, but **measure** first.\n- Background work must never be cut off.\n\nSee [the plan file](../PLAN.md).",
  ],
]);

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
  check_setup: () => ({ binary: "claude", cursor: "agent", agents: ["claude-code", "cursor"], problem: null }),
  keep_awake_state: () => keepAwake,
  keep_awake_start: (args) => {
    const ends_at = args.mode === "overnight" ? args.endsAt : Date.now() + 2 * 60 * 60 * 1000;
    keepAwake = { mode: args.mode, ends_at };
    emit("", { kind: "keep_awake", ...keepAwake });
    return keepAwake;
  },
  keep_awake_stop: () => {
    keepAwake = null;
    emit("", { kind: "keep_awake", mode: null, ends_at: null });
  },
  on_battery: () => false,
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
  list_skills: () => skills,
  import_skill: (args) => {
    const name = String(args.path).split("/").pop()!.replace(/\.(zip|md)$/i, "").toLowerCase();
    if (name.startsWith("bad")) throw "SKILL.md has no description";
    const skill = { name, description: "Turn a pull request into a short demo video." };
    skills = [...skills.filter((s) => s.name !== name), skill].sort((a, b) => a.name.localeCompare(b.name));
    return skill;
  },
  remove_skill: (args) => {
    skills = skills.filter((s) => s.name !== args.name);
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
    events: args.sessionId === "agents" ? (setTimeout(playAgents, 300), agentsChat) : args.sessionId === "images" ? imagesChat : args.sessionId === "planning" ? planningChat : args.sessionId === "saved" ? savedChat : args.sessionId === "long" ? longChat : args.sessionId === "looks" ? looksChat : args.sessionId === "waiting" ? askingChat : args.sessionId === "working" ? movedChat : [{ kind: "user_message", text: "(sample chat)" }, text("m", "Nothing saved for this one in the preview."), ended],
    live: args.sessionId === "planning" || args.sessionId === "waiting" || args.sessionId === "agents",
    chat_folder: chatFolder(projectOf(args.sessionId), args.sessionId),
  }),
  title_batch: (args) => {
    setTimeout(() => emit(args.sessionId, { kind: "batch_titled", tool_id: args.toolId, title: "Measuring idle memory" }), 600);
  },
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
  // Side chats answer at once and are named from their first question.
  side_send: (args) => {
    const id = String(args.sideId);
    const text = String(args.text);
    setTimeout(() => {
      emit(id, { kind: "title_changed", title: text.split(/\s+/).slice(0, 3).join(" ") });
      emit(id, {
        kind: "assistant_block",
        message_id: `side-${args.messageId}`,
        block: { type: "text", text: `A side answer to: ${text}` },
        parent_tool_id: null,
      });
      emit(id, { kind: "turn_ended", ok: true, duration_ms: 300 });
    }, 300);
  },
  side_interrupt: () => undefined,
  side_withdraw_message: (args) => emit(args.sideId, { kind: "message_withdrawn", id: args.messageId, ok: false }),
  // A message still waiting for the scripted answer can be taken back.
  withdraw_message: (args) => {
    const waiting = playing.get(args.sessionId) ?? [];
    const at = waiting.indexOf(args.messageId);
    if (at >= 0) waiting.splice(at, 1);
    emit(args.sessionId, { kind: "message_withdrawn", id: args.messageId, ok: at >= 0 });
  },
  side_end: () => undefined,
  side_end_all: () => undefined,
  // Notes live in memory here; the "planning" chat starts with some.
  read_notes: (args) => notesByChat.get(String(args.chatId)) ?? "",
  write_notes: (args) => void notesByChat.set(String(args.chatId), String(args.text)),
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
  list_models: (args) => {
    const all = ["low", "medium", "high", "xhigh", "max"];
    // Cursor's, as the core sends them: Auto first, newest of each family.
    if (args.agent === "cursor")
      return [
        { value: "default", name: "Auto", detail: null, efforts: [], default_effort: null },
        { value: "claude-opus-5-5", name: "Claude Opus 5.5", detail: null, efforts: all, default_effort: "medium" },
        { value: "gpt-5.6-sol", name: "GPT-5.6 Sol", detail: null, efforts: ["none", "low", "medium", "high", "xhigh", "max"], default_effort: "medium" },
        { value: "gemini-4-hydrogen", name: "Gemini 4 Hydrogen", detail: null, efforts: ["low", "medium", "high", "xhigh"], default_effort: "high" },
        { value: "grok-4.7", name: "Grok 4.7", detail: null, efforts: ["low", "medium", "high", "xhigh"], default_effort: "high" },
        { value: "composer-2.5", name: "Composer 2.5", detail: null, efforts: [], default_effort: null },
      ];
    // As the core sends it: newest of each family, the recommended one first.
    return [
      { value: "opus", name: "Opus 5.5", detail: null, efforts: all, default_effort: "high" },
      { value: "fable", name: "Fable 5.2", detail: null, efforts: all, default_effort: "medium" },
      { value: "sonnet", name: "Sonnet 5.5", detail: null, efforts: all, default_effort: "medium" },
      { value: "haiku", name: "Haiku 5.5", detail: null, efforts: all, default_effort: "medium" },
    ];
  },
  close_session: () => undefined,
  autotest: () => null,
  "plugin:path|resolve_directory": () => home,
  // Only the skill file picker answers, so the folder picker stays a cancel.
  "plugin:dialog|open": (args) => (args.options?.filters ? "/Users/you/Downloads/pr-to-video.zip" : null),
  "plugin:opener|open_url": () => undefined,
  "plugin:opener|reveal_item_in_dir": () => undefined,
  // Dropped files. `__drop([...paths])` in the console stands in for a drop.
  file_kind: (args: Args) => (String(args.path).endsWith("/") ? "folder" : "file"),
  open_file: () => undefined,
  clipboard_files: () => [],
  read_image_file: () => {
    throw "The preview can't read files.";
  },
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
  // The browser: no page is drawn here (the real ones are separate web views
  // the core places over the panel); the tabs and address line still work.
  browser_navigate: (args: Args) => {
    const chat = String(args.chatId);
    const url = String(args.url).trim();
    if (!/^(https?:\/\/)?(localhost|127\.0\.0\.1|\[::1\])?(:\d+)?(\/|$)/.test(url)) throw `Only localhost pages can be opened here, not ${url}.`;
    const full = url.includes("://") ? url : `http://${url.startsWith(":") ? "localhost" + url : url}`;
    const pages = browserOf(chat);
    const id = (args.tab as string | null) ?? pages.shown ?? newBrowserTab(chat);
    const tab = pages.tabs.find((t) => t.id === id);
    if (!tab) throw `There is no tab ${id}`;
    Object.assign(tab, { url: full, title: "", loading: true });
    emitBrowser(chat);
    setTimeout(() => {
      Object.assign(tab, { title: "Sample page", loading: false });
      emitBrowser(chat);
    }, 400);
  },
  browser_new_tab: (args: Args) => {
    const id = newBrowserTab(String(args.chatId));
    emitBrowser(String(args.chatId));
    return id;
  },
  browser_pick: (args: Args) => {
    browserOf(String(args.chatId)).shown = String(args.tab);
    emitBrowser(String(args.chatId));
  },
  browser_close_tab: (args: Args) => {
    const pages = browserOf(String(args.chatId));
    const at = pages.tabs.findIndex((t) => t.id === args.tab);
    if (at < 0) return;
    pages.tabs.splice(at, 1);
    if (pages.shown === args.tab) pages.shown = pages.tabs[Math.min(at, pages.tabs.length - 1)]?.id ?? null;
    emitBrowser(String(args.chatId));
  },
  browser_move: (args: Args) => {
    const pages = browserOf(String(args.chatId));
    const at = pages.tabs.findIndex((t) => t.id === args.tab);
    if (at < 0) return;
    const [tab] = pages.tabs.splice(at, 1);
    pages.tabs.splice(Number(args.to), 0, tab);
    emitBrowser(String(args.chatId));
  },
  browser_reload: () => {},
  browser_place: () => {},
  browser_close: (args: Args) => {
    browsers.delete(String(args.chatId));
    emit(String(args.chatId), { kind: "browser_tabs", tabs: [], shown: null });
  },
  browser_current: (args: Args) => {
    const pages = browserOf(String(args.chatId));
    return { tabs: pages.tabs.map((t) => ({ ...t })), shown: pages.shown };
  },
  read_doc: (args: Args) => {
    const path = String(args.path);
    if (path.endsWith("gone.md")) throw "That file doesn't exist.";
    return (
      docsByPath.get(path) ??
      `# ${path.split("/").pop()}\n\nRead from \`${path}\` on disk.\n\nA [link back](../PLAN.md) opens the plan file.`
    );
  },
  write_doc: (args: Args) => void docsByPath.set(String(args.path), String(args.text)),
  save_doc_to_project: (args: Args) => {
    const from = String(args.path);
    const to = `${projectHome(String(args.project))}/docs/${from.split("/").pop()}`;
    docsByPath.set(to, docsByPath.get(from) ?? "");
    docsByPath.delete(from);
    emit(String(args.chatId), { kind: "doc_saved", from, to });
    return to;
  },
  save_notes_to_project: (args: Args) => {
    const chatId = String(args.chatId);
    const from = `${chatFolder(String(args.project), chatId)}/notes.md`;
    const to = `${projectHome(String(args.project))}/docs/notes.md`;
    docsByPath.set(to, notesByChat.get(chatId) ?? "");
    notesByChat.delete(chatId);
    emit(chatId, { kind: "doc_saved", from, to });
    return to;
  },
};

// ---- The terminal: a pretend shell that echoes and answers a few commands ----

type PreviewTab = { id: string; project: string; label: string; running: string | null; closed: boolean; worktree: string | null };
const termTabs: PreviewTab[] = [];
const termOutput = new Map<string, string>();
const termSinks = new Map<string, { callback: number; index: number }>();
const termLine = new Map<string, string>();
let nextTab = 1;
const esc = "\x1b";
const promptFor = (tab: PreviewTab) =>
  `${esc}[36m~/${tab.project.split("/").pop()}${esc}[0m ${esc}[90mmain${esc}[0m % `;

function termInfo(project: string) {
  return termTabs
    .filter((t) => t.project === project)
    .map((t) => ({ id: t.id, title: t.running ? t.running.split(" ")[0] : t.label, running: t.running, closed: t.closed, worktree: t.worktree }));
}
function termSend(id: string, text: string) {
  termOutput.set(id, (termOutput.get(id) ?? "") + text);
  const sink = termSinks.get(id);
  if (sink) callbacks.get(sink.callback)?.({ message: text, index: sink.index++ });
}
function termTabsChanged(project: string) {
  emit("", { kind: "terminal_tabs", project, tabs: termInfo(project) });
}
function addTab(project: string, label: string, worktree: string | null, output = ""): PreviewTab {
  const tab: PreviewTab = { id: `t${nextTab++}`, project, label, running: null, closed: false, worktree };
  termTabs.push(tab);
  termOutput.set(tab.id, output || promptFor(tab));
  return tab;
}
{
  const project = `${home}/code/harness`;
  const shell = addTab(project, "harness", null);
  termOutput.set(
    shell.id,
    [
      `${promptFor(shell)}ls src/lib/panel`,
      "BrowserView.svelte  PanelBar.svelte  SidePanel.svelte  TerminalView.svelte",
      `${promptFor(shell)}git status -s`,
      ` ${esc}[31mM${esc}[0m src/lib/panel/SidePanel.svelte`,
      `${esc}[31m??${esc}[0m scratch/`,
      promptFor(shell),
    ].join("\r\n"),
  );
  const server = addTab(project, "harness", null);
  server.running = "python3 -m http.server 8000";
  termOutput.set(
    server.id,
    [
      `${promptFor(server)}python3 -m http.server 8000`,
      "Serving HTTP on :: port 8000 (http://[::]:8000/) ...",
      `::1 - - [15/Nov/2026 14:02:11] "GET / HTTP/1.1" ${esc}[32m200${esc}[0m -`,
      `::1 - - [15/Nov/2026 14:02:14] "GET /api/items HTTP/1.1" ${esc}[31m404${esc}[0m -`,
      "",
    ].join("\r\n"),
  );
}

Object.assign(commands, {
  terminal_tabs: (args: Args) => termInfo(String(args.project)),
  terminal_new: (args: Args) => {
    const project = String(args.project);
    const worktree = args.worktree ? String(args.worktree) : null;
    const tab = addTab(project, args.label ? String(args.label) : project.split("/").pop() ?? "shell", worktree);
    termTabsChanged(project);
    return termInfo(project).find((t) => t.id === tab.id);
  },
  terminal_attach: (args: Args) => {
    const id = String(args.tab);
    const callback = (args.onData as { id: number }).id;
    termSinks.set(id, { callback, index: 0 });
    const kept = termOutput.get(id) ?? "";
    termOutput.set(id, "");
    termSend(id, kept);
  },
  terminal_write: (args: Args) => {
    const id = String(args.tab);
    const tab = termTabs.find((t) => t.id === id);
    if (!tab) throw "That terminal tab is gone";
    const data = String(args.data);
    if (tab.running && data === "\x03") {
      tab.running = null;
      termSend(id, `^C\r\n${promptFor(tab)}`);
      termTabsChanged(tab.project);
      return;
    }
    if (tab.running) return;
    for (const ch of data) {
      const line = termLine.get(id) ?? "";
      if (ch === "\r") {
        termLine.set(id, "");
        const answer: Record<string, string> = {
          ls: "PLAN.md  package.json  src  src-tauri",
          pwd: tab.worktree ?? tab.project,
          "echo hello": "hello",
        };
        if (line.startsWith("sleep") || line.startsWith("python3")) {
          tab.running = line;
          termSend(id, "\r\n");
          termTabsChanged(tab.project);
        } else termSend(id, `\r\n${line && (answer[line] ?? `zsh: command not found: ${line.split(" ")[0]}`)}${line ? "\r\n" : ""}${promptFor(tab)}`);
      } else if (ch === "\x7f") {
        if (line) {
          termLine.set(id, line.slice(0, -1));
          termSend(id, "\b \b");
        }
      } else if (ch >= " ") {
        termLine.set(id, line + ch);
        termSend(id, ch);
      }
    }
  },
  terminal_resize: () => {},
  terminal_place: () => {},
  terminal_running: (args: Args) => termTabs.find((t) => t.id === args.tab)?.running ?? null,
  terminal_close: (args: Args) => {
    const at = termTabs.findIndex((t) => t.id === args.tab);
    if (at < 0) return;
    const [tab] = termTabs.splice(at, 1);
    termTabsChanged(tab.project);
  },
  terminal_move: (args: Args) => {
    const tab = termTabs.find((t) => t.id === args.tab);
    if (!tab) return;
    const others = termTabs.filter((t) => t !== tab && t.project === tab.project);
    termTabs.splice(termTabs.indexOf(tab), 1);
    const next = others[Number(args.to)];
    termTabs.splice(next ? termTabs.indexOf(next) : termTabs.length, 0, tab);
    termTabsChanged(tab.project);
  },
  terminal_text: (args: Args) => termOutput.get(String(args.tab)) ?? "",
  close_window: () => {},
});

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
(window as any).__drop = dropFiles;

// For screenshots: `#open=<chat id>` opens that chat, `&trash` opens the
// sidebar's trash list, `&browser` the panel's browser view, `&terminal`
// its terminal and `&info` chat info (`&agent` also opens the first subagent).
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
        document.querySelector<HTMLButtonElement>('header.bar button[title="browser"]')?.click();
      }, 200);
    }
    if (wanted.has("info")) {
      setTimeout(() => {
        document.querySelector<HTMLButtonElement>('header.bar button[title="chat info"]')?.click();
        if (wanted.has("agent")) setTimeout(() => document.querySelector<HTMLButtonElement>(".page .agent")?.click(), 200);
      }, 600);
    }
    if (wanted.has("terminal")) {
      setTimeout(() => {
        document.querySelector<HTMLButtonElement>('header.bar button[title="terminal"]')?.click();
      }, 200);
    }
  }, 100);
}, 400);
