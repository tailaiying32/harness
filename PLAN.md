# Harness v1 plan

Added 2026-11-18 (branch `harness/antigravity-agent-3ca873`): chats can run
on Google's Antigravity CLI (`agy acp`) through the same ACP layer as Cursor.
Agents are listed Claude Code, Codex, Antigravity, Cursor everywhere. See
"Antigravity over ACP".

Added 2026-11-15 (branch `harness/cursor-acp-217ae2`): chats can run on
Cursor's agent as well as Claude Code, picked from the model menu before the
first message. See "Cursor over ACP".

Added 2026-11-16: Harness's own skills, imported in Settings and offered to
every chat whatever its agent. See "Skills".

Added 2026-11-16 (branch `harness/chat-info-view-83b2a2`): a fifth side
panel view, chat info: context use, the task list, the chat's subagents
(each opened as a tab with its steps) and background commands, for Claude
Code and Cursor. See "Chat info view".

Added 2026-11-17 (branch `harness/earlier-chats-2def0a`): any chat's agent
can search, list and read the project's earlier chats, whatever agent they
ran on, and a chat can be picked with @ in the message box. See "Earlier
chats".

Added 2026-11-17 (branch `harness/keep-awake-382ab7`): keep awake, a
cup in the top strip (and an agent tool) that holds the computer awake while the
user steps away, ending when they're back, when the work is done, or after
two hours; overnight is a deliberate step. See "Keep awake".

Added 2026-11-17 (branch `harness/harness-home-2cc4f2`): what chats write
for the user to read lives in one folder in the home directory, `~/.harness`,
split by project: each project's docs, shared by all its chats and
worktrees, and each chat's own notes and docs. Nothing is saved into a repo
unless the user asks for exactly that. The same folder holds each project's
memory and the user's preferences, for every agent, and all of it follows a
project whose folder is moved or renamed. See "The home folder".

Status, 2026-11-13: steps 1 to 5 of "Order of work" are built. Images
(paste, drop, tool images, a full-space image view) were added 2026-11-14;
see "Images". The app has a
sidebar of projects and chats, saved history that comes back after a quit,
resume after a chat's process has exited, idle shutdown, and one chat view in
the Quiet Terminal look: formatted text, code blocks, folded tool runs,
approval cards (tool, plan, question), the working indicator, mode switch,
stop, a folder picker, a model picker, twenty themes and three sizes.

Tested live through the real window: everything in step 4 (see there), two
chats side by side with a mark for the one working in the background, closing
a process and resuming it, idle shutdown then resume, quit and reopen, a
reloaded chat matching the live one exactly, deleting a chat, and reopening a
400-turn chat. Not tested: clicking a link (it opens the default browser),
picking a different folder inside the native picker, and the sidebar in the
real window by eye (the screen was locked; it was checked in the browser
preview instead, see "Preview page").

Step 6, the side panel, is built, checked in the browser preview and tested
live once (see "Side panel, as built" and step 6 of "Order of work"). Subagent work shows in chat info (see "Chat info view"). The settings page
holds only theme and size so far.

Added 2026-11-13 (quick wins): rename a chat (double-click its title), close a single side panel tab, Cmd+N for a new chat, a
"queued" mark on a message sent while Claude works, plans always opening in
the side panel, and a chat that stays at its end when opened.

Added 2026-11-15 (branch `harness/sidebar-chat-cap-86b1f0`): the sidebar cap,
"show more" and folding, and hiding the sidebar (see "Sidebar" under Look).
The side panel gets a full screen button on every view (before its ×): the
panel covers the chat but never the sidebar, using the same layout as a narrow
window; the button again, or Escape outside a text box, brings the chat back,
and hiding the panel ends it. Dragging the panel's edge 48px past the chat's
360px limit makes it full screen; 48px past the panel's own 280px limit hides
the panel. Both are live while the button is held (drag back over the line to
undo; letting go settles it), and the panel keeps its width from before the
drag for when it is back. The drag listens on the window, since the edge
itself disappears in full screen. Full screen drops the panel's left
line (and the top bar's) so it never doubles the sidebar's. The browser gets
an "open in your browser" button at the right, beside full screen and ×. The
address is text: one click copies it (it flashes the accent colour),
double-click turns it into the box for typing an address; with no page open
it is the box. In a narrow chat the model label under the message box shortens with an ellipsis
(like the folder), so the row never runs past the panel's line.

The model picker lists only Claude Code's short names (opus, fable, sonnet,
haiku), each of which always points at the newest model of its family; pinned
older versions (full ids with a dash) and "Default" are left out. The model
"default" stood for (same resolved model) is listed first and is what new
chats start on when the saved pick was "default". Older chats still on
"default" keep running on it and show as that first model. Trimmed in the
core (`parse_models`) and again for a list cached before the change.

Added 2026-11-14 (effort, branch `harness/effort-picker-15a45f`): the model
label is now an in-app menu (not a native one) listing the models, with one
row of effort levels under them. A level picked is for that chat only and is
saved with it (`effort` column, `--effort` on launch, `apply_flag_settings`
live); new chats use Claude Code's own default for their model, which
`list_models` reads per model with `set_model` + `get_settings` on its idle
process (about 7 s, no settings file touched). The label shows "model level".
Checked in the browser preview; not tried in the real app yet.

Added 2026-11-13 (links, branch `panel-links`, merged 2026-11-14): a link to
a markdown file in the chat or a document opens it as a side panel tab, read
fresh from disk on each click (`read_doc` in `docs.rs`); relative links start
from the folder Claude works in (its worktree, if any), or in a document from
that document's folder. Web links open in the browser; other local files are
shown in Finder, so a link never runs anything. Not yet clicked through in
the real app.

Added 2026-11-13 (worktrees, branch `worktrees`): when Claude judges a piece
of work deserves its own branch, it asks for a worktree, and Harness shows a
card ("new worktree" or "work here"); worktrees share
build files with the main folder as copy-on-write copies; idle ones are put
away (edits kept in a hidden git ref) and come back with the next message;
merged ones (by git or a merged pull request) are removed with their branch;
deleting a chat moves it to a trash for 30 days. Pull request status shows as
a branch mark in the sidebar and in the bottom bar. Since 2026-11-14 the
sidebar mark shows only once a chat's branch has a pull request (a worktree
alone is how a chat keeps its work apart, not news); the bottom bar still
shows the open chat's branch. Merges go through pull requests on GitHub
(repo `tailaiying32/harness`, private) so the marks track real events. Deferred (2026-11-14): a cue for idle
chats with work not on GitHub (uncommitted or unmerged), maybe as an
"unfinished work" list in settings; revisit if work actually gets lost.
Since 2026-11-14 a chat keeps a list of pull requests, not one (a task split
into parts): one mark stands for the set, the bottom bar shows the mark and
"#12" or "2/3 merged" (no branch name, nothing before a pull request exists)
with a menu listing them, and a worktree is removed only once no part is open
and the branch its folder is on has landed. See "Worktrees", "Several pull
requests". New chats get a title
written by Haiku. **Tested** live with the self-test (Haiku, scratch repo):
Claude asking by itself for real work (a module with tests) and not for a
one-line fix, the move with resume, put away and restore with an uncommitted
file, a hand merge cleaned up at startup, the trash and its purge. Checked in the preview page: the card, the sidebar marks, the trash
list, the bottom bar. Not tested live: a real pull request going through its
states (the lookup command was run read-only against pookie). See "Worktrees".

Added 2026-11-15 (terminal, branch `harness/terminal-7d6f44`): the side
panel's fourth view, real shells whose tabs belong to the project (every chat
of the project shows the same ones). New tabs open in the main folder; + also
offers the chat's worktree. Claude can read a tab (not type into it; since
2026-11-17 an agent can open a tab of its own, see "Agents open terminal
tabs"). A tab
unused for a day of awake time closes, keeping its output and a line saying
why; quitting saves tabs and they come back closed. See "Terminal
(`terminal.rs`)". **Tested** live with the self-test: a login shell in the
project folder, a running command asked about before closing, unique tab
names, the one-day close (at 5 s) with its line and a fresh shell after,
Claude (Haiku) reading a tab with no approval card. Not checked by eye in the
real window: Cmd+T, Cmd+W (new menu item), Cmd+K, copy and paste, vim, the
look against the mock-up. Restore after quit and worktree closing are tested
in Rust only. Windows untested.

Added 2026-11-14 (browser, branch `harness/harness-browser-pane-0ac457`, not
merged): the side panel's third view, a localhost page that Claude can drive
(open, read, click, type, console, run, screenshot). See "Browser (`browser.rs`)".
**Tested** live with the self-test (Haiku): all seven tools on a test page,
with no approval cards; a page Claude opened while the panel showed docs
(parked, screenshot still right); a non-localhost address
refused; Harness's own preview page loading cleanly in it. Not checked by eye
in the real window (the screen couldn't be captured): the page lining up over
the panel, following the panel's drag, hiding under Settings and the image
view. Not tried: clicking a link to another site (it should open the default
browser), Windows.

Added 2026-11-13 (background work): a chat running a background command or
agent is never closed for being idle, a message to a closing chat no longer
fails, and delete and quit stop background commands. **Tested** live: stays
open (also a real `caffeinate` idle for 14 minutes), closes after, delete,
quit. Also checked by the user: the new sidebar dot colours and the rename
field. See "Session lifecycle". **Tested** live
with the self-test: the queued mark (also with stop), and a plan asked for in
`default` and in `auto` mode. Not checked by eye: tab closing, Cmd+N and the
scroll fix (type check only; the scroll bug was not reproduced first).

## What it does

A light desktop app for chatting with Claude Code. It gives you a sidebar of
chats grouped by project, a chat view with normal text editing, and a side
panel for plans and documents. It runs on your Mac and your Windows laptop
from one codebase.

The two reasons to build it are memory and a good UI. The app itself is small,
and chats you aren't using hold no memory.

## Where it sits

The app does not replace Claude Code. It starts the official Claude Code
program in the background, once per active chat, and talks to it through
structured messages instead of a terminal. Your login, skills, global
instructions and app connections keep working because it is the same program
reading the same settings.

There are three parts:

- **The window** is the UI. It is a web page drawn by the browser engine
  already built into macOS and Windows, so no copy of Chrome ships with the app.
- **The core** is a small native program behind the window. It starts and stops
  Claude Code processes, passes messages both ways, and saves chat history.
- **Claude Code** does the actual work, one process per active chat.

Everything that knows how Claude Code talks lives in one piece of the core.
The rest of the app only sees the app's own events (text arrived, approval
needed, turn ended). Adding Codex or Cursor later means writing a second such
piece, not reworking the app.

## Flows it covers

- **Start a chat.** Pick a project folder, type a message. The core starts
  Claude Code in that folder and the answer streams in as it is written.
- **Keep chatting.** Follow-up messages go to the same running process, so
  they start at once.
- **Approve or deny.** When Claude wants to edit a file or run a command, a
  card appears in the chat with Allow, Always allow and Deny. Questions Claude
  asks you show up the same way, as a choice card.
- **Stop.** A stop button cuts off the current answer. The chat stays usable.
- **Plan.** Switch a chat to plan mode. The plan opens in the side panel with
  Approve and Keep planning, and stays out of the chat, which keeps one line
  that opens it.
- **Read what Claude wrote.** Each markdown file a chat writes gets a tab in
  the side panel, and the tab follows Claude's later edits to that file.
- **Share an image.** Paste or drop one into the message box. It shows as
  `[image 1]` in the text and goes to Claude with the message. Images a tool
  hands Claude (a Read of a .png, a screenshot) show as `[image 2]` on the
  folded work line. Clicking any of these words opens the image over the chat
  and side panel, the way settings are, sized to fit; clicking the picture
  shows its actual pixels. The chat itself never shows a picture.
- **Walk away.** Five minutes after a chat's last answer, its process exits
  and frees its memory. The chat stays in the sidebar. Your next message
  restarts it where it left off. A chat with background work still running
  is left alone until that work ends.
- **Quit and reopen.** The sidebar and every chat's history come back from the
  app's own saved copy.

## Why it works

Each of these was tested on this Mac on 2026-11-13 with your Max login and no
API key:

- One long-running Claude Code process took several messages in a row.
- Text streamed piece by piece.
- Approvals reached the test script, and both allow and deny worked.
- Stop worked mid-answer. Switching to plan mode and back worked.
- A plan arrived as its own approval request, with the full plan text.
- A second run picked a chat back up by its ID and remembered it.
- A waiting chat held about 260 MB, rising to about 310 MB after a few turns.
  That is the cost the idle rule removes.

The official docs back the login side: signing in to the unmodified Claude
Code program with your own subscription is allowed, and the app never touches
the login itself.

## Risks

- **The approval channel is not officially documented for outside apps.** It is
  what Anthropic's own toolkit uses to talk to Claude Code, and it worked in
  tests, but an update could change it. It is kept in the one Claude-specific
  piece, and there is a documented fallback (see Implementation details).
- **Windows is untested.** Claude Code supports Windows, and the design avoids
  Mac-only parts, but nothing has been run there yet.
- **Memory numbers are from tiny test chats.** Real chats with app connections
  will hold more per process. The plan includes measuring this.

## What it doesn't cover

- Editing skills in the app, and a screen for managing app connections.
  Skills can be imported and removed (see "Skills"); instructions and
  connections you already have still work, with no UI for editing them.
- Agents other than Claude Code, Codex, Antigravity and Cursor (Gemini CLI,
  Copilot, OpenCode). Most ACP agents would reuse `agent/acp.rs` with a
  `Dialect` of their own.
- In Antigravity chats: worktrees moving an existing chat, a
  subagent's own steps (agy doesn't send them), and sign-in other than
  Google's (API key, Enterprise).
- In Codex chats: chats made in Codex's own CLI or app.
- In Cursor chats: worktrees (loading a session in another folder
  works, so the same move Claude chats make is possible), the usage meter,
  and moving a chat between agents.
- Chats made earlier in the desktop app or the terminal. Only chats started in
  this app appear in the sidebar.
- Attachments other than images (PDFs and other files), pictures inside
  markdown (`![](shot.png)`) in the chat or the panel, and opening a link to
  a local image in the image view (it reveals the file in Finder). Copy or
  save from the image view.
- Sharing the app with other people. It is for personal use only.
- On Windows, stopping a chat (delete, quit, a hung close) kills its process
  outright, so its background commands keep running. Fix in the Windows pass
  (for example with a job object).
- Showing which chats have background work, or how much memory each uses.
- In the terminal: Claude typing into it, split panes, search, clickable
  links, terminal settings, and the terminal beside the browser at once.
  Commands running at quit don't come back, only their output. On Windows
  nothing counts as running (× never asks; tabs show the folder's name).

---

## Implementation details

For whoever builds this. Everything marked **Tested** was observed with Claude
Code CLI 2.1.322 on macOS arm64. Everything marked **Verify** is from docs or
inference and must be checked before relying on it.

### Stack

- **Shell:** Tauri 2 (Rust core, system webview: WKWebView on macOS, WebView2
  on Windows). Check the current Tauri 2 docs for exact APIs before writing
  code; versions move.
- **UI:** Svelte 5 + TypeScript + Vite. Chosen for a small runtime. No UI
  component library in v1.
- **Store:** SQLite through `rusqlite` with the `bundled` feature (no system
  SQLite needed on Windows), WAL mode.
- **Rust crates:** `tokio` (process, io, time), `serde`, `serde_json`, `uuid`,
  `thiserror`, `rusqlite`. Tauri plugins: `dialog` (folder picker), `opener`
  (open links and files in the default app).
- **UI libraries:** `markdown-it` (markdown), `highlight.js` with languages
  loaded on demand (code colours), `dompurify` (sanitize rendered HTML).

### Machine setup

- Present: Node 24, npm 11, git, Xcode command line tools.
- Missing: Rust. Install with rustup before anything else. This downloads and
  runs an installer, so ask the user first.
- Done on 2026-11-13: Rust installed with rustup; `src-tauri` taken from
  `npm create tauri-app@latest` (Svelte + TypeScript template); the UI side
  set up by hand as plain Svelte + Vite, because that template uses SvelteKit,
  which this app doesn't need.
- Commands: `npm run tauri dev` (run with live reload),
  `npx tauri build --bundles app` (release build; the app lands in
  `src-tauri/target/release/bundle/macos/`), `cargo test` in `src-tauri`
  (unit tests for the Claude Code adapter), `npm run check` (UI type check).
  Cargo needs `source "$HOME/.cargo/env"` in shells opened before the install.

### Repo layout

```
src-tauri/
  src/
    main.rs            starts lib.rs
    lib.rs             app setup and the Tauri commands (the only surface the UI calls)
    events.rs          AgentEvent and related types (agent-neutral)
    agent/
      mod.rs           AgentAdapter and RunningAgent traits
      claude_code.rs   the only file that knows Claude Code's wire format
    sessions.rs        SessionManager: live processes, saving, resume, idle shutdown
    side_chat.rs       SideChats: read-only side chats on the chat's agent, several per chat as tabs (/btw), never saved
    store.rs           SQLite schema, reads, writes, and the search index over what chats said
    chats.rs           the earlier-chats tools: search, list and read a project's chats
    home.rs            where the home folder (~/.harness) is, and each project's and chat's place in it
    notes.rs           each chat's own folder there: its notes and docs
    images.rs          pasted images and images from tools, as files per chat
    locate.rs          find the claude binary
src/
  App.svelte           the window: sidebar + the open chat; the self-test
  preview.ts           a stand-in for the core, for the browser preview (dev only)
  lib/
    ipc.ts             typed wrappers over Tauri commands + the event channel
    state/app.svelte.ts   sidebar data, which chat is open, each chat's status
    state/chat.svelte.ts  one chat: items on screen, live events and replay
    Sidebar.svelte     projects and chats
    chat/              ChatView, WorkRun, ApprovalCard, Composer, Clipped, Markdown
    chat/images.ts     shrinking and saving a pasted or dropped image
    ImageViewer.svelte an image drawn over the chat and panel
    panel/SidePanel.svelte   the side panel: docs (plan and documents) or the side chat
    TopBar.svelte            the bar in the Mac title bar: chat title, panel views
    clicks.ts          copy and link clicks in formatted text (chat and panel)
    tools.ts           how tool calls are worded; which calls write documents
    markdown.ts        markdown-it + highlight.js setup
    themes.ts, models.ts, paths.ts
preview.html           entry for the browser preview (not in the built app)
```

Rule: nothing outside `agent/claude_code.rs` may mention Claude Code message
types, flags or field names. The UI and store only see `AgentEvent`.

### Starting Claude Code

Command line for a new chat (**Tested**):

```
claude -p
  --input-format stream-json
  --output-format stream-json
  --verbose
  --include-partial-messages
  --replay-user-messages          echo each user message when it is taken in
  --permission-prompt-tool stdio
  --append-system-prompt <text>   the plan instruction; see "Behaviour decided"
  [--permission-mode plan]        only when the chat starts in plan mode
```

To resume a chat whose process has exited, add `--resume <agent_session_id>`
(**Tested** with plain `-p`; **Verify** together with the streaming flags).

- Working directory: the project folder. Resume must run from the same folder,
  because Claude Code stores sessions per folder.
- Environment: copy the parent environment, then remove `ANTHROPIC_API_KEY`
  and `ANTHROPIC_AUTH_TOKEN`. An API key in the environment overrides the
  subscription in headless mode and would bill per use.
- Never pass `--bare`; it ignores the subscription login.
- stdin, stdout piped. Read stdout line by line; each line is one JSON object.
  Capture stderr to a per-session ring buffer for error display.
- Windows: spawn with the `CREATE_NO_WINDOW` flag so no console flashes.

Finding the binary (`locate.rs`), in order:

1. A path saved in settings.
2. macOS: run the user's login shell, `$SHELL -lc 'command -v claude'`. Apps
   started from Finder do not get the terminal's PATH, so a plain PATH lookup
   fails.
3. `~/.local/bin/claude` (macOS), `%USERPROFILE%\.local\bin\claude.exe`
   (Windows), then `where claude` on Windows.

Startup only finds the binary; if it is missing, the chat says so. Login is
**not** checked at startup (changed 2026-11-14): `claude auth status` once
reported a working login as signed out after about 12 hours, locking the
message box while the desktop app worked fine. Instead a chat finds out from
Claude Code itself: a signed-out turn replies with an `assistant` line
carrying `error: "authentication_failed"` (text "Not logged in · Please run
/login"; **Tested** with `CLAUDE_CONFIG_DIR` pointed at an empty folder). The
chat shows a sign-in card instead of that text. Its button runs
`claude auth login` (opens the browser) and then sends the failed message
again; that turn never reached the model. Before resending, the chat's
process is closed, because a running Claude Code read the login when it
started and won't see the new one (seen in testing). **Tested** 2026-11-14 in
a signed-out copy: `claude auth login` works with no terminal (it opens the
browser and waits for the redirect), and the resent message got a reply. If
it fails, the card shows the error and says to run it in a terminal. A
process whose last turn failed exits with code 1 and no output when closed;
that exit is not reported as a crash. The app never handles tokens or
credentials.

### Messages the app sends (stdin)

One JSON object per line.

User message (**Tested**):

```json
{"type":"user","uuid":"<id the app made>","message":{"role":"user","content":"text here"}}
```

With images (**Tested** 2026-11-14), `content` is a list: the images first,
in the order their words appear, then the text. Claude Code 2.1.322 takes
PNG, JPEG, GIF and WebP, at most 2000 px a side and 5 MB once encoded:

```json
{"type":"user","uuid":"…","message":{"role":"user","content":[
  {"type":"image","source":{"type":"base64","media_type":"image/png","data":"…"}},
  {"type":"text","text":"[image 1] what is this?"}]}}
```

Claude Code echoes it back on stdout, with the same `uuid` and
`"isReplay": true`, when it takes the message in (**Tested**): about a second
after sending for the first message of a turn; right after the running tool's
result for one sent mid-turn, which is then answered in the same turn. A
message waiting when the user presses stop is taken in after the stopped
turn's `result` and starts a new turn.

Answer to an approval (**Tested** for allow and deny):

```json
{"type":"control_response","response":{"subtype":"success","request_id":"<id from the request>",
  "response":{"behavior":"allow","updatedInput":{ ...the request's input, unchanged... }}}}

{"type":"control_response","response":{"subtype":"success","request_id":"<id>",
  "response":{"behavior":"deny","message":"reason shown to Claude"}}}
```

- Always allow (**Tested**): same as allow, plus
  `"updatedPermissions": <the request's permission_suggestions array>`, each
  with `destination` forced to `"session"`. What the suggestions hold decides
  what "always" means, and it is not always a rule:
  - Edits (`Write`, `Edit`) and commands that create or move files (`mkdir`,
    `touch`) suggest `{"type":"setMode","mode":"acceptEdits"}`. Sending it back
    switches the whole chat to auto-accept edits. Claude Code then emits
    `system`/`status` with `permissionMode: "acceptEdits"`.
  - Other commands suggest `{"type":"addRules","rules":[{"toolName":"Bash",
    "ruleContent":"npm --version"}],"behavior":"allow"}`: a rule for that
    command.
  - Some requests also carry `{"type":"addDirectories", ...}`.
  The button text says which it is (`always_label` in the adapter): "Allow all
  edits this chat", "Always allow <rule>", or no button when there are no
  suggestions.
- Approving a plan (**Tested**): plain allow leaves plan mode for `default`
  (asks before each edit). Allow plus
  `"updatedPermissions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}]`
  leaves it for auto-accept edits. The plan card offers both.
- Answering a question from Claude (`tool_name` is `AskUserQuestion`)
  (**Tested**): allow with
  `"updatedInput": {"questions": <original>, "answers": {"<question text>": "<chosen label>"}}`.
  For a `multiSelect` question the labels are joined with `", "`. Free text
  works in place of a label. The tool result then reads
  `Your questions have been answered: "<question>"="<answer>", ...`.

Stop the current turn (**Tested**):

```json
{"type":"control_request","request_id":"<new uuid>","request":{"subtype":"interrupt"}}
```

Claude Code replies with a `control_response`, then a `result` with
`subtype: "error_during_execution"` and `is_error: true`. The process stays
alive and takes the next message. If an approval was waiting, Claude Code
first sends `{"type":"control_cancel_request","request_id":"<that request>"}`
and a rejected tool result (**Tested**).

Change mode (**Tested** for `plan` and `default`; `acceptEdits` is offered in
suggestions):

```json
{"type":"control_request","request_id":"<new uuid>","request":{"subtype":"set_permission_mode","mode":"plan"}}
```

Reply: `{"type":"control_response","response":{"subtype":"success","request_id":"…","response":{"mode":"plan"}}}`.

Closing stdin ends the process gracefully: it finishes the current turn, waits
for background work, then exits (from the docs; **Verify** the background-work
part).

### Messages the app receives (stdout)

Dispatch on `type`. Ignore any type or subtype not listed; new ones appear
between CLI versions and must never crash the reader.

| `type` | Meaning | Key fields |
|---|---|---|
| `system`, subtype `init` | Start of every turn (not just the first) | `session_id`, `model`, `permissionMode`, `cwd`, `tools`, `mcp_servers`, `slash_commands` |
| `stream_event` | Live piece of an answer | `event` (raw API event), `api_message_id`, `parent_tool_use_id` |
| `assistant` | One finished block of an answer | `message.id`, `message.content` (one block: `text`, `thinking` or `tool_use`), `parent_tool_use_id` |
| `command_lifecycle` | A sent message's progress: `queued`, `started`, `completed`, `cancelled`. `started` counts as taken in (see below) | `command_uuid` (the ID the app sent), `state` |
| `user` with `isReplay: true` | Echo of a message the app sent. For a slash command it comes after the command's output, so `started` is used instead and the echo is dropped | `uuid` |
| `user` | Tool result, or the interrupt notice | `message.content[]` with `tool_result` + `tool_use_id`, `is_error` |
| `control_request`, subtype `can_use_tool` | Approval needed | `request_id`, `request.tool_name`, `request.input`, `request.description`, `request.permission_suggestions`, `request.tool_use_id` |
| `control_response` | Reply to a request the app sent | `response.request_id`, `response.response` |
| `result` | Turn ended | `subtype`, `is_error`, `result`, `session_id`, `usage`, `duration_ms`, `stop_reason` |
| `system`, subtype `status` | Activity hint, a mode change, or compacting | `status` (e.g. `requesting`, `compacting`); `permissionMode` when the mode changed; `compact_result` (`success`) when compacting ended |
| `system`, subtype `compact_boundary` | Compacting cut the conversation here. Then come a summary `user` message (`isSynthetic`, ignored) and copies of the last few `assistant` messages, with the same line `uuid` but a new `message.id`; the app skips any assistant `uuid` it has already passed on | `compact_metadata.trigger` (`manual` or `auto`), `pre_tokens`, `post_tokens` |
| `control_cancel_request` | An approval request was withdrawn | `request_id` |
| `system`, subtype `post_turn_summary` | One-line summary of the turn | `status_detail` |
| `rate_limit_event` | Subscription usage | `rate_limit_info.unifiedWindows.*.utilization`, `resetsAt` |
| `system`, subtype `background_tasks_changed` | The full list of running background commands and agents, sent whenever it changes; `[]` when all are done | `tasks[]` (`task_id`, `task_type` such as `local_bash`, `description`) |
| `system`, subtype `task_started` | A subagent (task) started | `task_id`, `tool_use_id` (the `Agent` call), `description`, `subagent_type`, `is_backgrounded`, `prompt` |
| `system`, subtype `task_progress` | After each of its steps | `task_id`, `tool_use_id`, `description` (live line, e.g. "Reading src/one.txt"), `usage.tool_uses`, `last_tool_name` |
| `system`, subtype `task_updated` | A patch to a task; a final status comes just before `task_notification` | `task_id`, `patch.status` (`completed`, `failed`, `killed`…) |
| `system`, subtype `task_notification` | A task ended | `task_id`, `tool_use_id`, `status` (`completed`, `failed`, `stopped`), `summary`, `usage.duration_ms` |
| `control_response` to `get_context_usage` | Asked by the app after each turn | `totalTokens`, `maxTokens`, `categories[]` (`name`, `tokens`, `kind`: `used`, `deferred`, `buffer`, `free`) |

When background work ends, Claude Code starts a turn on its own (`init`, a
short reply, `result`) to report it. Tested on CLI 2.1.322.

Details that matter:

- **One `assistant` event per block.** A reply with thinking and text arrives
  as two `assistant` events with the same `message.id`. Append blocks under
  that ID; do not replace.
- **`stream_event` wraps raw API streaming events:** `message_start`,
  `content_block_start`, `content_block_delta` (`text_delta` or
  `input_json_delta`, with `index`), `content_block_stop`, `message_delta`,
  `message_stop`. Accumulate text per (`api_message_id`, `index`). When the
  matching `assistant` block arrives, it is the final truth; drop the draft.
  Only the main session streams; subagents do not.
- **Subagent traffic** has a non-null `parent_tool_use_id`: the main chat's
  `Agent` (or older `Task`) tool call that started it. It is kept out of the
  chat and shown in chat info (see "Chat info view").
- **Any `result` means the turn is over,** including after an interrupt
  (where `stop_reason` is null).
- **Special approval requests** by `tool_name`:
  - `ExitPlanMode`: `input.plan` is the plan as markdown; `input.planFilePath`
    is where Claude saved it. Show in the side panel. Allow = approve the plan.
    Deny with a message = keep planning with that feedback. After approval
    the mode is `default`, or `acceptEdits` if the answer asked for it.
  - `AskUserQuestion`: `input.questions[]`, each with `question`, `header`,
    `options[]` (`label`, `description`), `multiSelect`. Show as a choice card.
  - Anything else: a normal tool approval card showing `display_name`,
    `description` and a readable view of `input` (command text for Bash, file
    path and content or diff for Write and Edit).

### The app's own event model (`events.rs`)

The adapter turns the above into these. Serialize with a `kind` tag.

```
SessionStarted { agent_session_id, model, mode }
Status         { state: Starting | Working | WaitingApproval | Idle | Exited }
TextDelta      { message_id, block_index, text }              not saved
AssistantBlock { message_id, block: Text{text} | Thinking{text} | ToolCall{tool_id, name, input}, parent_tool_id? }
ToolResult     { tool_id, content, is_error, parent_tool_id? }
ApprovalRequest{ request_id, kind: Tool | Plan | Question, tool_name, title, input, tool_id,
                 always_label? }      button text for "always allow"; none = not offered
ApprovalCancelled { request_id }      the agent withdrew the request (turn stopped)
ApprovalAnswered { request_id, decision: Allow | AlwaysAllow | Deny, answers? }
                                      recorded by the core when the user answers
Interrupted                           recorded by the core when the user presses stop
ModeChanged    { mode }               by the user, or as a side effect of an approval
UserMessage    { text, id? }          id: made by the window; absent in older history
MessageSeen    { id }                 the agent took that message in
TurnEnded      { ok, summary?, duration_ms, usage? }
UsageUpdate    { window, utilization, resets_at }             not saved; one per limit
Error          { message }
```

Usage limits (2026-11-16): Claude Code's `rate_limit_event` lists every
limit that applies to the model that answered in `unifiedWindows`, so one
`UsageUpdate` goes out per limit. Seen on CLI 2.1.322: `seven_day` (all
models) every time, and `seven_day_overage_included` after a Fable reply,
which matched the Fable line of `/usage`. The window keeps the newest reading
of each limit by agent (localStorage `harness.limits`), because a limit is
only reported after a reply from a model it applies to. Settings → Usage
shows a heading per agent ("Claude Code") with "all models" and "fable"
lines, each "64% · resets Mon 4:37 PM", "unknown" before any reading, and
"0%" once its reset time has passed (until the next reading; use on other
devices since the reset isn't known).

Usage audit (2026-11-19, CLI 2.1.326, codex 0.169.1, agy 1.5.11, agent
2026.11.19):
- **Claude Code**: a `get_usage` control request (`skip_behaviors: true`)
  answers in about 0.3 s with no model call, before any message:
  `rate_limits.{five_hour, seven_day, seven_day_opus, seven_day_sonnet}`
  (null when the plan has none; `utilization` a percent, `resets_at` ISO)
  and `model_scoped[]` (`display_name` "Fable"). The app asks at start and
  after each turn (main chats) and names readings as `rate_limit_event`
  does, a single `model_scoped` entry being `seven_day_overage_included`.
  `rate_limit_event` can carry only `five_hour`, `seven_day` and
  `seven_day_overage_included` in `unifiedWindows`. Settings shows "5h",
  "opus", "sonnet" when read, "all models" and "fable" always, and any other
  window by its name. Extra usage and spend are not shown.
- **Codex**: `account/rateLimits/read` also has `rateLimitsByLimitId`, one
  snapshot per metered limit (`limitId`, `limitName`). All are read; a
  bucket other than `codex` is named first ("Spark 5h"), so the sparse
  `account/rateLimits/updated` for one bucket can't overwrite another.
  Credits are not shown.
- **Antigravity**: `agy -p /usage`; see "Antigravity over ACP".
- **Cursor**: the interactive TUI has `/usage` (Included, Auto, API percent
  used, monthly reset), from Cursor's `DashboardService/GetCurrentPeriodUsage`,
  but print mode (`-p "/usage"` goes to the model) and ACP don't. Reaching it
  means driving the TUI in a pty (fragile, and starting the TUI updates the
  CLI) or calling Cursor's service with the user's login, so it is left out.

Commands in each agent's own syntax (2026-11-19). The menus follow how
each agent's own screen takes them; Harness's own commands sit beside them
where the names don't clash:
- **Harness's**: `/model` (model menu), `/clear` or `/new` (a new chat in
  the same folder, as Cmd+N), `/btw` (a side chat), and its skills, in every
  agent's `/` menu. Claude Code's own `/clear` stays hidden.
- **Codex**: its app-server takes `/` and `$` as plain text; its own screen
  turns them into calls, so the app does too (`Builtin` in `codex.rs`):
  `/compact` → `thread/compact/start`; `/review [text]` → `review/start`
  (uncommitted changes, or `custom` instructions; the review comes back as
  an ordinary `agentMessage`); `/goal <goal>` → `thread/goal/set` (Codex
  starts its own turn at once, so `turn/started` now also says "working"),
  `/goal` alone, `clear`, `pause`, `resume` → `thread/goal/get|clear|set`
  answered in the chat; `/init` → Codex's own `/init` prompt (copied from
  codex-cli 0.169.1); `/mcp` → `mcpServerStatus/list`, answered in the
  chat. Skills come from `skills/list` (enabled only) and are picked with
  `$` in the message box; a `$name` in a message goes with it as a
  `{type: "skill", name, path}` input, as Codex's own screen sends it.
- **Antigravity**: built-ins aren't run over ACP (they reach the model), but
  print mode runs them without a model call: `usage` (`quota`), `credits`,
  `skills`, `agents`, `plugin`, `hooks`, `permissions`, `config`
  (`settings`), `changelog` (`agy -p /help`, agy 1.5.12). These are in the
  menu, and a message that is one of them alone runs `agy -p "<text>"` in
  the project; its tab-split lines show as a list (`print_commands` in the
  ACP `Dialect`). `/compact` over ACP works but says nothing; a
  "compacted" line now follows it (`compacts`).
- **Cursor**: ACP runs none of its built-ins except `/copy-request-id`.
- **Screen-only commands** (`SCREEN_ONLY` in `commands.svelte.ts`): an
  agent's own commands that only work in its own terminal screen (Codex
  `/status`, `/diff`, ...; Antigravity `/context`, `/rewind`, ...; Cursor
  `/summarize`, `/compact`, `/usage`, ...) are caught when sent, and the chat
  says so ("/diff only works in Codex's own terminal screen"), with what to
  use instead where there is something. One the agent lists itself (a skill
  or custom command of that name) is sent as usual. Claude Code turns these
  down itself, for free, so it has none.
- **Tested live**: Antigravity `/usage` and `/credits` (about 1.5 s each, no
  model call); Codex `/mcp` and `/goal` alone. `review/start` and
  `thread/goal/set` were tried on Codex's app-server directly (a review of
  a one-line bug came back as its answer; a goal started its own turn and
  ended `complete`). **Not tested live**: the `$` menu, `/clear`, and the
  screen-only notes (type-checked only).

`AgentAdapter` trait (one implementation in v1, `ClaudeCodeAdapter`):

```
spawn(project_path, resume_id: Option<String>, mode) -> RunningAgent
RunningAgent:
  events()                      stream of AgentEvent
  send_user_message(text, id)
  answer_approval(request_id, decision, original_input, suggestions, answers)
  interrupt()
  set_mode(mode)
  close()                       close stdin; process exits on its own
  kill()                        force; only on app quit timeout
```

The adapter keeps each pending approval's original `input` and
`permission_suggestions` keyed by `request_id`, because the answer must echo
them back.

### Worktrees (`worktree.rs`, `sessions/worktrees.rs`, `github.rs`)

Built 2026-11-13. The design discussion and plan: `~/.claude/plans/purrfect-rolling-whale.md`.

- **The request** (2026-11-17: one app tool for every agent). Every main
  chat gets the app's worktree tool, `harness_worktree` / `ask` with a short
  name for the work, built in `sessions/worktrees.rs`. When to call it (first
  step of real work in a repo, before any file is made or changed; not for
  tiny edits, questions, reviews or plans, nor once in a worktree) is in the
  group's instruction and in the tool's own description: Cursor hears
  instructions only when a conversation starts. The wording matters: a soft
  "ask for a worktree with the harness_worktree tool" made Haiku skip it for
  a new module with tests (2 of 2 runs, while the old "call EnterWorktree"
  asked 2 of 2); "your first step... is to call the ask tool of the
  harness_worktree server" asked 2 of 2 and still skipped a one-line fix.
  The tool carries `_meta: {"anthropic/alwaysLoad": true}` so Claude Code
  lists it up front instead of behind its tool search; other agents ignore it. Claude Code's own `EnterWorktree`/`ExitWorktree` are turned off
  with deny rules, which take them off the model's list (tested on CLI
  2.1.325). Before 2026-11-17 Claude Code chats asked through `EnterWorktree`
  (an `ask` rule made it reach the app) and Cursor chats couldn't ask at all.
  An earlier version still asked before the first file edit (a PreToolUse
  hook); dropped 2026-11-14 because shell edits slipped past it.
- **The call returns at once.** It puts `WorktreeAsk` on screen and tells the
  agent to end its turn and make no changes; the answer comes as its next
  message. Holding the call open until the answer doesn't work for every
  agent: Cursor gives up on an app tool call after 60 seconds (MCP's default
  request timeout; CLI 2026.11.13 has no setting for it over ACP), while
  Claude Code waited 10+ minutes. The cost: nothing forces the agent to stop.
  The question outlives the turn: the window doesn't cancel it when the turn
  ends (`settle` skips it), and after an app restart the core finds it again
  in the saved history (`load_open_ask`, once per chat per run), so it can
  still be answered. Asked again while the question is open: no second
  question. Asked from a chat already in a worktree, or outside a git repo:
  the reason, no question.
  The window folds the call under the question: the sink notes the tool's
  latest call ID (whole from Claude Code, in a later `ToolInput` from Cursor).
- **"Work here"**: a hidden note tells the agent to carry on here; it may ask
  again later for other work. **"New worktree"**: the process is stopped if it
  still runs; `worktree::create` makes `harness/<the agent's name for the
  work>-<6 hex>` at `<data>/worktrees/<repo>-<6 hex>` from the latest commit
  of the branch the main folder was on when the chat began (`start_branch`,
  saved on the first message); then the chat resumes there with a hidden
  note telling the agent where it is. Same session ID for both agents:
  Claude Code's `--resume` and Cursor's `session/load` with the new `cwd`
  both work from another folder (Cursor tested 2026-11-17: history kept,
  edits and shell in the new folder).
- **A message instead of a pick**: the question closes as "skipped" with
  nothing saved, and the message goes with a note before it: it answers the
  question; ask again before any file changes if the work still needs a
  worktree.
- **Tested live** (2026-11-17, self-test with `HARNESS_DATA_DIR`): Claude Code
  (Haiku) and Cursor (GPT-5.3 Codex) each asked first thing for a new module
  with tests, made no changes, and after "New worktree" carried on and
  finished inside it (`pwd` there); the main folder kept only its README. A
  card left open past the end of the turn, then answered by a message
  ("never mind, do it here"), showed "skipped" and the work happened here.
- **Ignored files** (builds, packages, `.env`) are copied from the main folder
  with `clonefile` (copy-on-write, no space until changed); off APFS, files
  are copied plainly and folders skipped. Folders holding a `.git` (nested
  repos and worktrees) are never copied.
- **Put away** = uncommitted edits (not ignored files) saved as a commit under
  `refs/harness/saved/<chat>` (a temporary index; branch untouched), then
  `git worktree remove --force`. **Restore** = `worktree add` at the same
  path, `git restore --source=<ref> --worktree -- .`, ref deleted, ignored
  files copied again. Sending a message to a put-away (or trashed) chat
  restores it first.
- **Merged** = the branch has commits of its own, no uncommitted edits (in
  the folder or saved), and its tip is in the start branch, `origin/HEAD`,
  `main` or `master`; or its pull request is merged with the same head commit
  (covers squash merges).
- **Several pull requests** (2026-11-14). A chat's `prs` is a list, oldest
  first (`sessions.prs`, JSON; the old single `pr` column is copied in once
  and then unused). It grows two ways: the link `gh pr create` prints in a
  Bash result (also the "already exists" failure; other `gh pr` commands are
  ignored, as they print links the chat only looked at), and the usual
  lookup by the worktree's branch. Only open and draft ones are looked up
  again, each by its link, and every answer updates just its own entry
  (`Store::upsert_pr`, the only writer), so a refresh can't drop one added
  meanwhile. Chats without a worktree are swept too while one is open.
  - One state for the set (`worktree::summary`, `prSummary` in paths.ts):
    open if any is open, else draft, else merged, else closed. The bar's
    count leaves closed parts out ("1/1 merged"; "closed" if all were).
  - **Merged** also needs no part open or a draft, and a folder that is on a
    branch.
  - **The folder's branch is followed** (`worktree::follow`): after each
    turn, before putting away and in the sweep, a folder found on another
    branch updates the saved worktree (branch, start commit = where it
    forked from main, and the old branch added to `seen`). So a put-away
    folder comes back on the branch it was on. A detached folder is left
    alone: never put away for idleness, never merged.
  - On removal, branches in `seen` whose merged pull request's head is still
    their tip go too, unless the main folder is on one. Other branches are
    never deleted.
  - A new worktree starts a fresh list only if no part is open or a draft.
  - Not covered: pull requests opened outside the chat on a branch the
    folder was never on; a closed one reopened later (never looked up again).
- **Sweep** (`SessionManager::sweep`): at start, hourly, and after any
  process exits; never touches a chat whose process runs. Merged → folder and
  branch removed, chat asks again at its next edit. No message for 14 days
  (`worktree_idle_days`) → put away. Put away for 30 days (`trash_days`) →
  branch and saved edits deleted. Trashed for 30 days → chat purged.
  `HARNESS_WORKTREE_IDLE_SECONDS` and `HARNESS_TRASH_SECONDS` override both
  for tests.
- **Pull requests** via one `gh api graphql` query per host (`github.rs`,
  `prs`): each chat's open and draft pull requests by number, and the newest
  one for its worktree's branch (`pullRequests(headRefName:)`). Names and
  branches go in as variables. A folder's repo comes from `gh repo view`,
  asked once per folder. Looked up after every turn, when the window comes
  to the front (every chat that may have one changing), when a chat is
  opened (both at most every 10 s per chat, so flicking between windows
  doesn't ask again and again), and in the sweep (at most every 10 minutes).
  No timer: merges by others while Harness is in front are rare, and nothing
  acts on a merge while the user is away (cleanup keeps its own clock). A
  repo `gh` can't answer for isn't asked again until restart.
- **Titles**: on a chat's first message, `claude -p --model haiku
  --no-session-persistence --tools "" --system-prompt <name the chat>` writes
  a title (about 1.5 s). Without its own system prompt Haiku answered "use
  your Write tool…" with a tool call. A renamed chat (`title_source = 'user'`)
  is never retitled.
- **If Claude doesn't ask**, its changes go into the main folder, as
  before worktrees existed.
- **Self-test steps**: `@worktree <worktree|here|none>` answers the worktree card,
  `@moved` waits for a turn to end in the worktree, `@turn` for any turn end.
  A message starting with `/btw ` goes to the side chat on show and the test
  waits for its answer; `@newside` starts another side chat tab, `@endside`
  ends the one on show. The report has `side` (the shown one's items),
  `sides` (every tab's title), `panelView` and `planPending`.

### Starting chats from a plan (`sessions/new_chats.rs`, built 2026-11-17)

When the user asks, an agent splits work into parts and asks to start a chat
for each. One card covers them all. Once started, each is an ordinary chat:
the user manages them, and the chat that asked doesn't follow them.

- **The request**: the app's `harness_new_chats` tool group, one tool
  `start` (`mcp__harness_new_chats__start` for both agents), offered to every
  main chat next to the worktree tool. Input: `parts` (1 to 8 of `title`,
  `summary`, `why`, `brief`, optional `agent`) and optional `from` (`main`, the
  default, or `this_chat`). The brief shape follows Claude Code's
  send-to-project handoff. The rule (only when the user explicitly asks; not
  for subagents; briefs stand alone; `from` only when asked, commit first) is
  in both the instructions and the tool's description, like the worktree
  rule. It has `anthropic/alwaysLoad` for the same reason.
- **Returns at once**, like the worktree tool (Cursor's 60 s limit on a tool
  call). Refused at once, with no card: no parts, a part without a title or
  brief, more than 8, an agent that isn't installed, a folder outside a git
  repo, or a card already open.
- **The card** (`NewChatsAsk`, saved) lists each part's title, summary and
  why. A click opens the full brief. "Start n chats" or "Don't start". It
  outlives the turn and an app restart, like the worktree card.
- **Start** (`NewChatsChosen`, then `NewChatsStarted` with one entry per
  part): in the background, one part at a time. Each gets:
  - a worktree from `worktree::create`;
  - a saved row (title, agent, start, worktree "live", choice "worktree");
  - then its brief as its first message, through `send_inner`. The row
    already exists, so no written title or project-chats note.
  A hidden note goes with the brief: the chat was started from another chat
  (linked) to do one part; read the brief and the code, reply with what you
  understood, how you'd do it and any questions; change no files until the
  user says go. A part that fails is cleaned up (folder, branch, row) and
  listed with its error, and the others carry on. The asking chat's agent
  hears which chats started (as `[@Title](<chat:ID>)` links) with the user's
  next message (a pending note), so starting costs it no turn.
- **Don't start**: a pending note says so. **A message instead**: the card
  closes as skipped, and the message goes with a note to change the briefs
  and ask again if still wanted.
- **Branches start** where a new chat in the project would (`head_of` on the
  main folder), or with `this_chat` from the asking chat's worktree branch
  (its commits only). With no worktree, it falls back to the main folder.
- **Agent**: the asking chat's agent, mode, model and effort; a part naming
  another agent gets that agent's defaults.
- **Not covered**: parts that clash or depend on each other (the user's
  call), coordinating the children, folders outside git.
- **Tested live** (2026-11-17, self-test with `HARNESS_DATA_DIR`):
  - Haiku split two parts. On start, both chats showed in the sidebar, each
    in its own worktree, and each replied with its understanding and plan.
    Neither changed any files.
  - A large task with no request for chats asked for a worktree, not chats.
  - A typed "make it three parts, branch them off this chat" closed the card
    as skipped, and Haiku asked again with three parts and `from:
    this_chat`.
  - Cursor (GPT-5.3 Codex) as the asking chat gave the README part to
    Claude Code as asked. Both children replied without edits.
- **Self-test step**: `@newchats <start|no|none>` answers the card.

### Session lifecycle (`sessions.rs`)

As built. Per chat the manager keeps: the live process (if any), `busy` (a
turn is running), `closing` (asked to exit), and two counters. `run` counts
process starts, so events from an old process can't change the state of a new
one. `idle_round` counts the times the chat went idle, so an old idle timer
can't close a chat that has been used since.

- **Send with no process:** save the chat (`touch_session`), spawn with
  `--resume` if the chat has an `agent_session_id`, record `UserMessage`,
  write the message. **Tested** live: the resumed chat remembers earlier turns.
- **Send with a process:** record and write. Claude Code queues it if a turn
  is running.
- **Turn ends (`Idle`):** start the idle timer. Its length is, in order:
  `HARNESS_IDLE_SECONDS`, the `idle_seconds` setting, 300. Zero means never.
- **Background work** (`BackgroundWork { running }`, from Claude Code's
  `background_tasks_changed`): while any runs, the chat is never closed for
  being idle, with no time limit. This keeps a dev server watcher or a
  `caffeinate` alive. When the last one ends between turns, the idle timer
  starts (Claude Code's own follow-up turn usually restarts it anyway).
  Added after a chat running a 2-hour `caffeinate` was asked to close: Claude
  Code waits for background work before exiting, so the chat was locked for
  hours. **Tested** live: a chat with a 45-second background `sleep` stayed
  open with a 5-second idle time, then closed about 5 seconds after its
  follow-up reply.
- **Idle timer fires:** if nothing happened since and nothing runs in the
  background, mark `closing` and call `close()` (close stdin). If it hasn't
  exited 30 seconds later, `stop()` it.
- **`stop()`:** SIGTERM on Mac and Linux, which Claude Code answers by ending
  its background commands; killed 5 seconds later if still running. On
  Windows it is a kill, which leaves background commands running (see
  "What it doesn't cover"). A stopped process's non-zero exit is not
  reported as an error.
- **Send while `closing`:** `stop()` the old process at once (safe: chats are
  only closed with nothing in the background), wait up to 10 seconds for it
  to exit, then spawn with `--resume`.
- **Delete:** the chat's process is `stop()`ped, background work included.
  **Tested** live: the process and its background `sleep` were gone within
  2 seconds.
- **Process exits:** `Exited` clears the process. An unexpected exit also
  emits `Error` with the tail of stderr. The next send resumes.
- **App quit (`RunEvent::Exit`):** `stop()` chats with background work,
  `close()` the rest, and wait up to 3 seconds; then `stop()` anything still
  live and wait up to 2 more. **Tested** live by the user: a chat running a
  background `caffeinate`, then Cmd+Q; no `caffeinate` and no chat process
  were left, and the reopened chat resumed.
- **`agent_session_id`** comes from every turn's `init` and is saved at once,
  so a crash before the first `result` can still resume.
- **Locks:** no lock is held while calling an agent method or `spawn`. Both
  report through the sink, which takes the sessions lock.
- The idle timer lives in the core on purpose. A hidden window's page is
  frozen by the system while nothing happens (seen with the screen locked):
  its timers don't fire, though events from the core still wake it.

### Images (`images.rs`, built 2026-11-14)

- **Where they live:** `<data dir>/images/<chat id>/<n>.<png|jpg|gif|webp>`,
  numbered from 1 per chat. One count is shared by pasted images and images
  from tools, so every `[image n]` in a chat is unique. Saved history holds
  only `{ n, media_type }`: on `UserMessage.images`, and in tool results as
  `{"type":"image_ref","n":…,"media_type":…}`. A paste that had to be shrunk
  for Claude also keeps its original as `<n>.full.<ext>`, for looking at
  only; `read_image` returns it when there is one.
- **Pasting** (`chat/images.ts`, `Composer.svelte`): a paste with any plain
  text in it pastes the text (copied spreadsheet cells carry a picture too).
  Otherwise each image is shrunk to fit Claude Code's limits if needed (PNG
  kept as PNG when it fits, else JPEG at falling quality), saved with
  `save_image`, and its word is inserted at the cursor as typing would, so
  undo works. A dropped image file comes by path (see "Files") and is read
  with `read_image_file`, then goes the same way. A pasted image whose word is deleted
  isn't sent, but keeps its number, so a later one may skip a number.
- **The word in the box:** a textarea can't colour part of its text, so
  while the draft holds an image word a copy of the text sits behind it with
  the words in the accent colour, and the box's own text is see-through.
  Clicking inside a word opens the image.
- **Sending:** `Chat.send` attaches each draft image whose word is in the
  text. The core reads the files, fails the send if one is missing, and
  writes them to Claude Code as base64 (see "Messages the app sends").
- **Tool results:** the session sink moves each base64 image in a tool
  result into the store and leaves an `image_ref` in its place, before the
  result is capped, saved or sent to the window. Before this, a Read of a
  .png put its whole base64 into the history.
- **Showing:** `ImageWord.svelte` draws `[image n]` in the accent colour, in
  sent messages and after a work run's summary (outside its cut-off, so it
  always shows). Clicking one sets `Chat.viewing`, and `ImageViewer.svelte`
  is drawn over the chat and panel like settings: the picture sized to fit
  (never blown up), a dim line with `image n · W×H`, `‹ k of N ›` and `×`.
  Clicking the picture switches to actual pixels (one image pixel per screen
  pixel), scrolled to the clicked spot, and back. ← → step through the
  chat's images by number; Escape, `×` or a click beside the picture closes
  it and gives the keys back to the message box. The bytes are read only
  while an image is shown, and freed after.
- **Cleanup:** a chat's images go when it is deleted for good (purge), not
  when it is trashed. At start, folders of chats that don't exist are removed:
  pastes into a new chat that was never sent leave one.

### Files (`chat/files.ts`, built 2026-11-14)

Any file can be dropped on the window, or copied in Finder and pasted. It
is never copied: the message carries its path and Claude opens it itself,
so Claude sees the file as it is now.

- **Dropping:** the window's native drag-drop is on (the page's own drop
  events never carry a path). `onFileDrop` in `ipc.ts` hands the paths to
  the message box, wherever on the window the drop landed; not while
  settings or an image is drawn over the chat. An image file (by extension)
  becomes `[image n]`; one that can't be read, and everything else, becomes
  a file word.
- **Pasting:** a paste that carries files asks the core for the clipboard's
  file paths (`clipboard_files`, the `arboard` crate). With none, it does
  what it did before: text pastes as text, a lone picture becomes an image.
- **The word:** `[report.pdf]` in the message box (a folder ends in `/`; a
  second file of the same name shows its folder, `[b/report.pdf]`), kept in
  `Chat.draftFiles` with its path and coloured like an image's word. On
  send, `toLinks` writes each word still in the text as
  `[report.pdf](</full/path>)`. That text is what is sent and saved, so
  nothing else is stored: `withWords` finds the form again and
  `FileWord.svelte` draws the word. A path with `>` or a newline can't be
  written that way and goes in as plain text. Chat titles cut from a first
  message leave the paths out (`without_file_paths` in `store.rs`).
- **Clicking a word** (`open_file`): a markdown file opens as a side panel
  tab; any other file in its own app. Folders, apps and files marked
  runnable are shown in their folder instead, so a click never runs anything.
- **Reading without a card:** `send_user_message` in `claude_code.rs`
  remembers the paths a message links (`file_links`). When Claude Code asks
  to use Read or Grep on exactly one of them, `quiet_read` allows it and no
  card shows. Not its folder, not other tools (a shell `head` still asks),
  and the list is per running process, so after a restart an old file asks
  once.
- **Not built:** a key or command to pick a file, `@` file search, folding
  long pasted text.

### Store (`store.rs`)

One SQLite file, `harness.db`, in the Tauri app data directory
(`~/Library/Application Support/com.harness.app` on macOS), or in
`HARNESS_DATA_DIR` if set (tests use this). WAL mode, `synchronous = NORMAL`.

```
projects(id TEXT PK, path TEXT UNIQUE, created_at, last_used_at)
sessions(id TEXT PK, project_id TEXT -> projects ON DELETE CASCADE,
         agent TEXT DEFAULT 'claude-code', agent_session_id TEXT NULL,
         title TEXT, mode TEXT, model TEXT, created_at, last_used_at)
events(id INTEGER PK AUTOINCREMENT, session_id TEXT -> sessions ON DELETE CASCADE,
       payload TEXT /* AgentEvent as JSON */, created_at)
       INDEX (session_id, id)
settings(key TEXT PK, value TEXT)
chat_text(body, session_id, event_id, role)  /* FTS5; see "Earlier chats" */
```

Times are milliseconds since 1970.

- Save every `AgentEvent` except `TextDelta`, `UsageUpdate` and `Status`
  (`AgentEvent::is_saved`).
- A chat is created by its first message (`touch_session`), not before. Its
  project is the folder it started in. Each later message updates `mode`,
  `model` and `last_used_at`.
- Title: the first non-empty line of the first message, cut to 60 characters.
- A tool result longer than 20,000 characters is cut before it is saved or
  sent to the window (`cap_tool_result`). The window shows a few lines at most.
- The app's history comes only from this table. Do not read Claude Code's
  transcript files under `~/.claude/projects`; their format is internal and
  can change.
- Rows this version can't parse are skipped on load, so data from another
  build never breaks a chat.
- Deleting a chat deletes its events; a project left with no chats goes too.
- **Reload is replay.** `load_session` returns the saved events in order, and
  the window feeds them through the same code that handles live events
  (`Chat.fromSaved`). **Tested**: a chat with a denial, an edit, a question and
  a stop came back with identical items and text. If the process is gone,
  anything left open is closed: an unanswered approval shows as cancelled.

### Commands (in `lib.rs`)

```
subscribe(channel)        one app-wide tauri::ipc::Channel carrying { session_id, event }
check_setup()             find Claude Code
sign_in()                 run `claude auth login`; resolves when it exits
sidebar()                 every project with its chats, newest first
load_session(session_id)  -> { session, events, live }
delete_session(session_id)
send_message(session_id, project_path, mode, model, text, images?, message_id)
                          images: numbers of the chat's saved images to send
save_image(session_id, media_type, data, original?)  base64 in; -> { n, media_type }
read_image(session_id, n) raw bytes for looking at (the original of a shrunk paste)
rename_session(session_id, title)
answer_approval(session_id, request_id, decision, message?, answers?)
interrupt(session_id)
set_mode(session_id, mode) / set_model(session_id, model)   saved; a live process is told
list_models()
close_session(session_id) ask the chat's process to exit (used by the self-test)
side_send(side_id, project_path, agent, model, effort, text, message_id)   ask a side chat (`<chat id>#side-<tab>`); starts it as a copy on the chat's agent (`agent`: only for a chat not saved yet)
side_interrupt(side_id) / side_end(side_id)   stop its turn / end it and forget it
side_end_all()            a fresh window ends the side chats a reloaded one left running
autotest() / autotest_wait(ms) / autotest_report(text)
```

Not built yet: settings commands (`Store::setting` and `set_setting` exist).
`read_document` was planned for the side panel and turned out not to be
needed: the panel is built from the chat's own history (see "Side panel, as
built"). Add it only if tabs should show files as they are on disk.

One app-wide channel, not one per chat, so the sidebar can show a mark for
chats that are not open.

### Look: "Quiet Terminal" (chosen 2026-11-13)

Mock-up, with light and dark and the folded lines opened:
https://www.magicpatterns.com/os/tWmrW2q43Q7sWofaHnFJVj?file=2oZgfpBgi1cedNPY3NJS45
(frame "Quiet Terminal"; its source is the reference for spacing and colour).

- **Feel:** spacious, plain, text-first. Hairline rules instead of boxes. No
  blur, translucency, shadows or gradients. The only motion is the text cursor
  and the working indicator.
- **Formatted text:** render markdown the way the Claude Code CLI does, in one
  type size (decided: headings are not scaled up; revisit only if they get
  lost in long side-panel documents). Headings hide their `#`. Top level:
  bold, accent colour, blank line above, hairline rule below. Second level:
  bold and bright, blank line above. Third level and below: bold in the
  normal text colour. Bold is
  bold and bright; italic is italic. Bullets keep `-`, numbered lists keep
  their numbers, checkboxes keep `[x]` / `[ ]`. Links are underlined in the
  accent colour. Quotes get a dim rule down the left. Tables are aligned
  columns with hairline rules.
- **Working indicator:** like the CLI's: one changing character, the current
  step, and elapsed seconds, on the folded line of the run in progress. Update
  it a few times a second at most; it must not cause the rest of the chat to
  redraw.
- **Font:** one monospace font for everything by default
  (`ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace`,
  size per the density setting below). The font is a setting; the layout must
  not depend on it being monospace. Lay things out in `ch` and `em` units so
  everything scales with the type size.
- **Appearance:** dark by default, light available as a setting.
  - Dark: background `#111210`, text `#cfccc2`, bright text `#f1eee4`, dim
    `#7d7a70`, faint `#4d4b43`, rule `#24251f`, accent `#d9a35b`, ok `#8fae7e`,
    bad `#d47766`.
  - Light: background `#fcfcfb`, text `#3a3832`, bright `#191813`, dim
    `#858480`, faint `#b7b6b2`, rule `#e6e6e3`, accent `#a8691c`, ok `#52733f`,
    bad `#b0463a`.
- **Sidebar:** bare chat titles under each project's path, one status dot per
  chat (accent = waiting on the user, ok = working, hollow = idle). No preview
  line, no time. Each project shows its 8 most recently used chats; past that,
  only the open chat, chats working, waiting or finished unseen, and chats
  with an open or draft pull request. Under them, a faint "show more" adds the
  next 8 hidden chats each click, until none are left. It is the only label:
  clicking the folder name folds one step, back to the first 8, then down to
  the name alone. A caret in the dots' column (folder name lined up with the
  titles) points down while open, right once folded to the name (still showing the open chat and any working, waiting or
  finished-unseen chat; open pull requests don't count there). Folded folders
  are saved across quits (prefs `foldedProjects`, by path); "show more" steps
  are not. Tried and dropped: a search box in place of "more", and "show
  less" as a second label (two labels never looked clean). Added 2026-11-15.
  The whole sidebar hides with the sidebar icon (in the top strip: at the
  sidebar's right edge while it shows; at the top bar's start once hidden,
  with a clear gap after a Mac's window buttons, since right beside them it
  read as a fourth one, 2026-11-15) or Cmd+S / Ctrl+S (was Cmd+B until
  2026-11-17, which became bold in the text boxes); saved
  across quits (prefs `sidebarHidden`). Projects keep a fixed order, set by
  dragging a folder name (a 4px move starts the drag, so a click still
  folds); a new project goes on top; saved as prefs `projectOrder`, by path.
- **Scrolling:** a chat opens at its end and follows new output. It lets go
  only when the view moves up, and takes hold again within 40px of the end.
  Growth of the page under a still view never counts as scrolling away.
- **Chat:** a centred column, about 800px wide. The user's message is a block
  on the right in the theme's `panel` colour: bright text, left aligned, at
  most 85% of the column, nearly square corners, with about twice the normal
  gap above and below. This is the one place the look follows a chat app
  rather than a terminal: with only a `>` marker, the user's messages were
  hard to find when scrolling back. Claude's text has no bubble. Code blocks
  use style D (see "Appearance settings").
- **Images (chosen 2026-11-14):** never pictured in the chat. Each is a word
  like `[image 1]` in the accent colour, in the message box, in sent messages
  and on work lines; clicking it opens the image over the chat and panel, the
  way settings are. Thumbnails were weighed and dropped: they cost space and
  still needed a click. A side panel tab was built first and dropped: too
  small to make out a screenshot's details. Options are on the "Images in
  Chat" canvas in the Magic Patterns OS above.
- **Claude's work:** a run of tool calls folds into one dim line ("Read
  sessions.rs, searched for SessionState::Idle ▸"), with +/- counts when it
  includes edits. Clicking opens it to one line per tool call, with the diff
  under each edit. While a run is still going, the line shows the current step.
- **Approval:** the only boxed element: a one-pixel accent border, tool and
  reason on one line, the command below, then Allow (filled accent), Always
  allow and Deny. Mouse only: no keyboard shortcut approves or denies, so a
  stray keypress can never approve a command.
- **Message box:** a rule above, an accent `>` prompt, and one faint line
  below with mode, model, folder and usage. The box is two rows tall when
  empty; the line below sits a fifth of a gap under it (tightened 2026-11-13).
- **Side panel:** the plan as a plain document, formatted like chat text: a
  bold bright heading, `[x]` / `[ ]` steps, notes as dashes.

#### Appearance settings (decided 2026-11-13)

Theme and size are settings: every theme and all three sizes below ship, each
behind a selector. Code block style is not a setting: style D ships, a tinted
panel with a line naming the language and a copy action, syntax colours
inside, and tinted inline code. The tint (each theme's `panel` colour) is
tuned per theme so the panel is always clearly visible against the
background; where a palette's own value is too close, override it (GitHub
Light and Nord are already adjusted in the mock-up). Working defaults, cheap
to change since both are settings: Harness Dark and the regular 12.5px size.
There are two theme picks, one dark and one light, and an appearance setting
that says which shows: light, dark, or auto (the default), which follows the
system and switches the moment it does (added 2026-11-14). Picking a theme for
the side not showing only saves it.
The built-in terminal is built as a side panel view (see "Terminal
(`terminal.rs`)"). How to build the selectors so the choice costs nothing at
runtime:

- All colours and sizes are CSS custom properties on the root element. A
  theme is one small object of values; switching sets the properties. Nothing
  re-renders and only the active theme is ever applied.
- Density is a class or attribute on the root, styled in CSS. No component
  branches on it.
- Syntax colouring must emit token classes (keyword, string and so on) that
  the theme's six syntax colours style. Do not load a separate highlighter
  theme per colour theme.
- Check every theme for readable contrast on dim and faint text before
  shipping; some palettes are low-contrast when mapped onto these roles.

Mocked up on the same board:

- **Colour themes.** A theme is data: eleven role colours (background, panel,
  text, bright, dim, faint, rule, accent, ok, bad) plus six syntax colours
  (keyword, function, string, type, number, comment). The UI reads only these
  roles, so adding a theme never touches a component. Mocked up: Harness Dark
  and Light, Solarized Dark and Light, Gruvbox Dark and Light, Dracula, Nord,
  Tokyo Night, Catppuccin Mocha and Latte, One Dark, Monokai, GitHub Dark and
  Light, Rosé Pine and Dawn, Everforest, Kanagawa, Ayu Dark. The values are in
  the board's `shared/harness/themes.ts`. Open: which ship, and the default.
- **Code blocks must stand out** from the monospace text around them. Six
  treatments are mocked up (syntax colours only, tinted panel, tinted panel
  with a language line, with an accent bar, hairline box). All but the first
  use syntax colours; inline code is tinted. Open: which one.
- **Density.** The app should fit more on screen than the desktop app. Three
  sizes are mocked up: compact 11.5px, regular 12.5px, roomy 13.5px, each with
  its own line height and gaps. Size is a setting. Open: the default.
- **Built-in terminal.** Built 2026-11-15 as the side panel's fourth view;
  see "Terminal (`terminal.rs`)". Mock-up: Magic Patterns canvas "Terminal".

#### Behaviour decided 2026-11-13

- **"Always allow" lasts for the current chat only.** When answering, set
  every permission suggestion's `destination` to `"session"` before sending
  it back, so nothing is written to Claude Code's settings files. A later
  settings page will let the user change this default (chat, project).
  As Claude Code's own prompt does, "always" on an edit means auto-accept
  edits for the rest of the chat; the button says so and the mode shown under
  the message box follows.
- **Auto mode** (added 2026-11-13 at the user's request). The mode switch
  cycles ask before edits, auto-accept edits, plan, auto. "auto" is Claude
  Code's own mode of that name (`--permission-mode auto`, or
  `set_permission_mode` with `"auto"`): it decides by itself which actions
  are safe to run without asking. **Tested** on CLI 2.1.322 both ways: `init`
  reports `permissionMode: "auto"` and a `mkdir` ran with no approval request.
- **New chats start in auto mode** (2026-11-13, the user's request), or in
  the mode last picked with the mode switch. Plan is never remembered. Before
  this every new chat started in ask-before-edits.
- **The mode shown follows Claude Code.** It is set from every turn's `init`
  and from `ModeChanged`, so approving a plan or an "allow all edits" answer
  updates it.
- **Question card.** Options are rows to click (`( )` pick one, `[ ]` pick
  any), each question has a free-text row, and "Send answers" is enabled once
  every question has an answer. Skip denies with a short message.
- **Stop.** The chat shows a faint "stopped" line. An approval that was
  waiting is marked cancelled. A tool call the user denied, or that was
  cancelled, reads "Skipped ..." in its folded run and is not counted as a
  failure.
- **Settings page: started 2026-11-13** (the user wanted theme and size out of
  the bottom bar). "settings" at the bottom of the sidebar, or Cmd+, / Ctrl+,
  opens it over the top bar, chat and side panel (never the sidebar; the top
  bar was left showing until 2026-11-15, which looked out of place); its
  empty space moves the window, as the top bar did; "close", Escape
  or opening a chat closes it. It holds spacing
  (compact, regular, roomy: line height, gaps and margins), font size (11 to
  16 px) and theme (dark and light lists). Spacing and font size were one
  "size" setting until 2026-11-13; they are separate now, and a saved size
  with no saved font size keeps the text size it used to give (11.5, 12.5,
  13.5); a click applies at once and is remembered. Since 2026-11-14 it also
  holds the trash (moved out of the sidebar: small lists that don't help the
  work at hand belong in settings): each chat with its days left and
  "restore"; clicking one opens it; "empty trash" asks before deleting for
  good. Deleting a chat (× in the sidebar) asks "delete? yes / no" first. Still to
  add: font, idle timeout, the "Always allow" default and the path to Claude
  Code, which are constants until then.
- **The line under the message box** follows the Claude desktop app (the
  user's choice, 2026-11-13, from a screenshot of it): on a new chat the
  folder picker sits at the left, and it disappears once the first message
  is sent (the sidebar shows the folder from then on). Model, mode and usage
  sit at the right, in that order. The desktop app also shows an effort level
  ("High") next to the model; Harness has no effort setting yet. A chat with
  pull requests shows their mark and count at the left (2026-11-14); the
  branch name and "no PR" are gone.
- **Queued mark** (2026-11-13). A message sent while Claude works shows a
  "send now" button under it, at the right, until `MessageSeen` arrives. The
  button presses stop: the running turn is cut off and the message starts a
  new one (**Tested**). Left alone, the message moves to the end
  of the chat's items at that moment, so it sits after the step Claude
  finished before reading it. A message sent when idle gets no mark. The
  adapter only reports echoes of IDs it sent, and follows each with
  `Status: Working`, which covers a message taken in after a turn has ended.
  Replay gives the same order. A chat reloaded with no live process clears
  every mark; a message lost because the process died gets no warning.
- **Edit a queued message** (2026-11-16, the user's request).
  Double-clicking a message that still has its queued mark takes it back
  from the agent (`withdraw_message`) and, once that worked, moves it into
  the message box, after anything typed there, with the cursor at the end.
  Its files become short words again and its images go back with the
  draft, so sending works as if it were just typed. Taken back first, so
  the agent can't read the old version mid-edit; if it read the message
  already (`MessageWithdrawn { ok: false }`, not saved), nothing happens.
  Only a taken-back message is saved, so replay drops it. Claude Code:
  control request `cancel_async_message` with `message_uuid`, answered
  `{ cancelled }` (**Tested** on CLI 2.1.325: `cancelled: true`, and the
  message was never answered). Cursor: the app's own queue drops the ID and
  the driver skips it. Side chats too (their own box). First built as an
  editor in the bubble itself; the user found moving it to the message box
  less jarring. Checked in the preview; not tried in the real app yet.
- **A message answers what Claude waits on** (2026-11-14). Sent while a plan,
  tool approval or question is open, it first turns each one down with "the
  user sent a message instead; read it and go by it" (a plan: keep
  planning), then goes in as usual, so Claude reads it next. Left open, the
  request held the turn and the message queued behind it, which looked
  stuck. The open requests come from the adapter (`open_requests`), so the
  ones it answers itself are never touched. **Tested** live by the user
  (2026-11-15): a plan and the worktree question, each answered by typing.
- **Plans always open in the side panel** (2026-11-13, the user's request).
  Every launch appends an instruction to Claude Code's system prompt
  (`PLAN_INSTRUCTION` in `claude_code.rs`): when asked for a plan in any mode,
  call `EnterPlanMode`, write the plan, present it with `ExitPlanMode`.
  **Tested**: `EnterPlanMode` raises no approval request, and after a plain
  approval the chat returns to the mode it was in (`default` and `auto`
  checked). It is an instruction, not a guarantee.
- **Cmd+N / Ctrl+N** starts a new chat in the open chat's folder and puts the
  cursor in the message box.
- **Slash commands: typed, no menu** (built 2026-11-14). The user types them
  into the message box as is; there is no slash menu, by choice. **Tested**
  on CLI 2.1.322: `/compact`, `/context` and `/usage` run headless, and
  `/context` and `/usage` come back as ordinary replies. While compacting
  the working line says "Compacting…", and a faint "compacted" line (or
  "compacted automatically") marks the cut; earlier messages stay on screen.
  A command stays above its own output, live and after reopening, because
  `command_lifecycle` `started` is the "taken in" signal. Commands that
  duplicate Harness controls (`/model`, `/clear`, `/rename`, `/effort`) are
  left alone: Claude Code runs them, and Harness's own controls may fall out
  of step. `/btw` answers "isn't available in this environment" headless,
  so Harness catches it and opens a side chat instead (next item).
- **Side chat (`/btw`): built** (2026-11-14; mock-up: "Top Bar" canvas,
  version A). `/btw <question>` in the message box, or **side chat** in the
  top bar, opens a small second conversation in the side panel. It starts as
  a copy of the chat at that moment (`--resume <chat's session>
  --fork-session`) and never refreshes from it; follow-ups go in its own box
  and the same process remembers them. Read-only: `--tools Read,Grep,Glob`
  and `--strict-mcp-config` with no config. Without the latter, the user's
  claude.ai connectors (Gmail, Drive…) loaded from the second message on
  (tested, CLI 2.1.322). No approval channel, so anything needing permission
  is refused without asking. Nothing is saved by Harness: its events go to
  the window under the side chat's own id (`side_chat.rs`). Its process closes after
  the chats' idle time and resumes its own session on the next question, so
  a forgotten side chat costs no memory; that is why Claude Code saves it.
  **× end** stops it and, a few seconds later, deletes Claude Code's saved
  copy (`forget_session`: the exact `<id>.jsonl` under
  `~/.claude/projects/*/`). Quitting or deleting the chat ends it too.
  Switching chats or hiding the panel doesn't. A plan that arrives while
  side chat is showing doesn't switch the panel; ● on `docs` in the top bar
  says it's waiting. Each tab is named from its first question by the
  same small model that names chats (`make_title`); it reads "side chat"
  until the name arrives, and the name is not saved.
- **Several side chats, as tabs (2026-11-16).** The side chat view has a tab
  row like the terminal's: + (or Cmd+T) starts an empty one, × (or Cmd+W)
  ends that one, and the tab beside it takes its place. Each forks the main
  chat as it is when its first question is sent; they never see each other.
  `/btw` asks the one on show, starting one if there are none. Ids are
  `<chat id>#side-<8 random hex>`: random, so a reloaded window never reuses
  an id the core still knows, and each process start gets a run number
  unique across all side chats, so an old process's late events can't land
  in a new side chat. Cleanup, each closing a leak found along the way:
  a fresh window calls `side_end_all` (a reload lost the side chats on
  screen, but the core kept them, and the next `/btw` quietly resumed the old
  one, untitled); switching away from a chat that never started ends its side
  chats; a side chat ended before its copy was named deletes the copy when the
  name arrives; and the core lists live copies in `side_copies.json` in the
  data folder, deleting any left there at the next start (after a crash or
  force quit). Trashing a chat ends all its side chats (`end_chat`). **Tested** live with the self-test (`/btw …` messages,
  `@endside`): answers from the copied chat, a file read, idle close then
  resume with its history, the saved copy deleted on end, the main chat
  untouched, and a plan arriving while in side chat.
- **Side chats on every agent (2026-11-19).** A side chat runs on its chat's
  agent (the saved chat's; the window's pick for a chat not saved yet), fixed
  at its first question: if the chat moves to another agent, the side chat
  stays where it started, and new ones use the new agent. How each gets its
  copy (`AgentAdapter::side_copy`):

  | Agent | Copy | Read-only |
  |---|---|---|
  | Claude Code | its fork (`--resume … --fork-session`) | `--tools Read,Grep,Glob`, no approval channel |
  | Codex | its fork (`thread/fork`, `excludeTurns`); later starts `thread/resume` the fork | a `side` mode: `read-only` sandbox, approvals `never`; `mcp_servers` replaced by the app's own only |
  | Cursor | the app copies `~/.cursor/acp-sessions/<id>/` (`CURSOR_CONFIG_DIR` replaces `~/.cursor`) under a new ID, then `session/load`s it | Ask mode |
  | Antigravity | the app copies `~/.gemini/antigravity-cli/conversations/<id>.db`, `annotations/<id>.pbtxt` and `brain/<id>/`, and sets `trajectory_meta.cascade_id` to the new ID | Default mode, every ask refused at once (`AcpConfig::side`) |

  Neither ACP agent offers `session/fork` (both answer "method not found",
  Cursor CLI 2026.11.19, agy 1.5.12), and neither documents its files, so
  the copy is the app's (`copy_session`). Databases are copied with SQLite's
  `VACUUM INTO`, which takes what is still in the log too, while the chat may
  be writing it. Cursor's session folder names nothing inside, so a plain
  copy loads as itself. Antigravity's copy without the `cascade_id` change
  loads, then fails its first prompt ("trajectory not found"); the old ID
  also appears inside stored steps, left as is. A copy is the side chat's own
  session from the start and is listed for deleting like a fork. Deleting:
  Cursor removes the folder, Antigravity the three, Codex calls
  `thread/delete` through a short `app-server` (it finds its own program,
  since startup cleanup runs before programs are found). Codex refuses to
  delete a thread a fork still points at ("forked history still references
  it"); the app only ever deletes side chats, the forks, so it never meets
  that. The list of copies records each copy's agent; an older list's bare
  IDs are Claude Code's.

  ACP side chats are told `SIDE_INSTRUCTION`, may read but not write the
  notes, and get no memory instruction. Cursor hears it with the first prompt
  of a side chat's first start, though it loaded a copy (`SideOptions::first`).
  Antigravity stops its turn without a word when a call is refused; a side
  chat then tells it once why (`REFUSED_NOTE`) and it answers in the same turn.

  **If there's nothing to copy** (the agent can't, its copy fails, or the
  chat's saved session is gone; `has_session` is checked first), the side
  chat starts fresh with the earlier-chats tool (`harness_chats`; Claude Code
  gets `--mcp-config` for it and `--allowedTools mcp__harness_chats`), its
  first question carries a note to read the chat first (as an agent switch
  does), and the side chat shows "Couldn't copy the chat into this side chat,
  so it reads the chat first." A chat moved to another agent and not yet run
  on it reads the same way. **Tested** live with the self-test on each agent
  (`HARNESS_AUTOTEST_MODEL=<agent>:<model>`, then a secret word, two `/btw`
  questions, a main-chat question): every side chat answered from the copy;
  none created the file it was asked to (Cursor and Antigravity said they
  couldn't; Codex's sandbox refused); the main chat's history held none of the
  side's messages; and ending the side chat deleted its copy (Cursor's folder,
  Codex's thread).
- **Drag tabs to reorder (2026-11-16).** In the docs, side chat, browser and
  terminal rows, a tab can be dragged along its own row (never to another
  view). Press and move 4 px to start; the other tabs make room as it passes
  their middles; letting go leaves it there, Escape puts it back, and the
  click that ends a drag doesn't pick the tab (`panel/tabDrag.svelte.ts`).
  Notes and plan stay first. Each store moves the tab at once, so it doesn't
  jump back while the core answers. Where order lives: browser tabs in the
  core (`browser_move`); terminal tabs in the core (`terminal_move`), shared by
  every chat of the project and saved at quit; side chats and documents in
  the window only, so a reloaded chat lists documents in the order first
  written again (as it already brings closed ones back). Same-named terminal
  and side chat tabs are numbered by when they opened, not by place, so a
  dragged "harness 2" keeps its name. **Tested** in the preview (browser and
  terminal rows: move, Escape, click after drag) and with core unit tests;
  not yet checked by eye in the built app.
- **Top bar: built** (2026-11-14). On a Mac the window draws into its own
  title bar (`titleBarStyle: Overlay`, `hiddenTitle`, `trafficLightPosition`
  in `tauri.conf.json`); the window buttons sit at the top of the sidebar and
  a bar runs over the chat and panel: the chat's title on the left, the
  panel's views (`docs`, shown when there are tabs, and `side chat`) on the
  right, laid out exactly like the panel below (its width and dividing line),
  so they sit over the panel when it is open. Clicking a view only switches
  the panel (opening it if hidden); ▸ at the bar's right end hides the
  panel. With the panel closed the names are hidden and only ◂ shows; it
  brings the panel back on the view it had. A plan opens the panel by itself;
  one left waiting after the panel was hidden shows in the chat's last line. Most
  openings happen on their own (`/btw`, a plan, a doc link), so picking a
  particular view from closed taking two clicks is accepted. The default window is 1400 × 900, since below
  1100 px the panel replaces the chat.
  The bar and the sidebar's top strip carry `data-tauri-drag-region`
  (needs `core:window:allow-start-dragging`). It replaces the old "panel ·
  N tabs ◂" line. Windows ignores the overlay settings: the bar sits under
  the normal title bar there. In Mac full screen the window buttons hide and
  the sidebar's top strip stays empty; accepted for now. Renaming the chat
  from the bar is not built.
  **Changed 2026-11-14 (branch `harness/harness-browser-pane-0ac457`):** the
  views became three drawn icons (`PanelIcon.svelte`, drawn like the branch
  mark: a document, a chat bubble, a globe) at the right of the chat's half of
  the bar, against the panel's line. Clicking one opens the panel on it;
  clicking the one on show hides the panel; ◂ and ▸ are gone. The docs icon is
  faded and does nothing until there is a plan or document. The icon on show
  is bright, like the open chat in the sidebar (orange was tried and dropped).
  Plans carry no dot or colour anywhere (not on the docs icon or plan tab, not
  in the chat line): "Plan · ready for your review · open plan", the link
  alone is the cue (the user's call). The bar
  over the open panel now holds that view's own row (`PanelBar.svelte`): docs'
  tabs, the side chat's name and ×, or the browser's address, ↻ and ×. The
  panel itself has no row, so every view starts right under the bar. The user
  liked it in the Magic Patterns canvas "Panel Picker Icons" (it trimmed the
  browser from three stacked rows to one).
- **Model picker: built** (the user asked for it on 2026-11-13). A selector
  in the line under the message box lists the models Claude Code itself
  reports, by their real names ("Opus 5.5", "Default (Fable 5.2)"), including
  older ones. The list comes from an `initialize` control request sent to a
  short-lived Claude Code process at startup (about a second, nothing spent,
  no chat message), read by `list_models` in the adapter; each entry has
  `value`, `displayName` and `description`. The last list is cached so the
  picker is right at first paint. A new chat starts on the choice (`--model`); a running
  chat is switched with a `set_model` request and the next turn uses it. An
  unknown name comes back as an error line and the chat stays on its model.
  The last choice is remembered for new chats.
- **No @file menu in v1.**
- **Thinking** folds into a dim line, like tool calls.
- **Long diffs and long command output** are cut, with a "show more".
- **A chat that needs the user** shows only its sidebar dot. No system
  notifications in v1.

### UI rules

- **Layout:** sidebar left (projects, each with its chats, newest first, a
  status dot per chat), chat in the middle, side panel right (collapsible).
- **Sidebar, as built:** width 204 / 228 / 256 px by size. Each project's
  folder (faint, home as `~`, the end kept when long) with its chats under
  it; "+ new chat" and "settings" at the bottom. No app name at the top (the
  user removed it for room). A mark before each
  title: `●` grey = working, `●` yellow = needs your answer, `●` blue =
  finished since you last looked, `○` = nothing going on. Every theme has
  its own `blue` and `yellow` from its palette (yellow darkened on light
  themes), so the meaning holds across themes (user's choice, 2026-11-13,
  matching the desktop app). The open chat is
  bright. On hover a folder shows `+` (new chat there) and a chat shows `×`;
  `×` asks "delete? yes no" in place before deleting. Hidden when the window
  is narrower than 720 px.
- **Chats in the window:** a new chat is empty and unsaved until its first
  message; the app opens on one, in the most recently used folder. Chats
  opened since the app started stay in memory, so switching back is instant
  and a chat working in the background keeps up. Unsent text stays with its
  chat.
- **Long chats:** only the last 60 groups are drawn; scrolling near the top
  draws 60 more, keeping the view in place, and staying at the bottom of a
  growing chat lets the top go again. Tool output that is never shown (a file
  that was read) is not kept in the window's memory.
- **A call waiting for approval** reads "Waiting to run npm test", not "Ran".
- **Composer:** a plain multi-line text box. Enter sends, Shift+Enter adds a
  line. It edits like a document (`chat/editing.ts`, the user's request,
  2026-11-13): Tab indents the cursor's line, or every selected line, by two
  spaces, also on a plain or empty line; Shift+Tab takes them off; Tab never
  moves to the next control. Shift+Enter on a list item (`-`, `*`, `+`, `1.`,
  `1)`, with or without a checkbox) starts the next item at the same indent,
  with the next number; on an item that is still empty it steps back one
  level, or removes the marker at the top level. Edits go through
  `insertText`, so undo works. Each of these keys also renumbers
  (`renumbered`): in the run of non-blank lines around the edit, every
  numbered item counts on from the one before it at the same indent; a list's
  first item keeps its number unless the key just moved it to that level,
  where it starts at 1. The renumbering and the edit are one replacement, so
  one undo step. Edits made by hand (Backspace, paste) do not renumber, and a
  blank line ends a list, so removing an empty item leaves the items below it
  as they were. A mode switch (Ask before edits / Auto-accept edits / Plan) and a stop
  button that replaces send while working. Enter never answers an approval.
- **Streaming performance:** collect `TextDelta`s and apply them once per
  animation frame. Re-render markdown only for the block that is streaming;
  finished blocks render once and are never touched again. Use
  `content-visibility: auto` on message blocks so long chats stay cheap to
  scroll. Add a virtual list only if measurement shows it is needed.
- **Tool calls:** folded as described under "Look". An opened line shows each
  call's input and result; long results are cut with a show-more.
- **Side panel, as built** (decided with the user 2026-11-13; mock-up: frame
  "Side Panel" on the Magic Patterns board, section 5). A tabbed space on the
  right, per chat, built for more kinds of tab later.
  - **Tabs:** one `plan` tab (the latest `ExitPlanMode` request; a revised
    plan replaces the old one in the same tab) and one tab per markdown file
    the chat wrote, in the order first written. A tab's name is the file name,
    with its folder when two share a name. Each tab has a `×` (shown on the
    tab in front, or under the pointer); a plan still waiting can't be
    closed. A closed document keeps following edits, and its name in the
    chat, or "open plan", brings the tab back. Closed tabs are not saved.
  - **Documents come from the chat's history, not from disk.** A `Write` whose
    path ends in `.md` or `.markdown` starts the tab with its content; later
    `Edit` / `MultiEdit` calls on that path are applied to the text
    (`textAfter` in `tools.ts`), so the tab follows Claude's edits. Only calls
    whose result is not an error count. Changes made any other way (by the
    user, by a shell command) are not seen. An edit to a markdown file the
    chat never wrote in full gets no tab. No new core command, and a reloaded
    chat rebuilds the same tabs by replay.
  - **The plan's own file is not a document.** Claude Code drafts the plan in
    a file (`planFilePath` on the request). When the request arrives, a tab
    for that path is dropped and later writes to it are ignored. **Tested**
    live: Claude Code writes the draft to `~/.claude/plans/<name>.md` before
    the request, so until the request arrives the draft is an ordinary
    document tab for a moment (added quietly, then dropped).
  - **Opening:** a plan request opens the panel on the plan tab (also when a
    chat is reopened with its plan still waiting). Documents add a tab
    quietly. In the chat, a document's name in a folded run, and in the
    opened run, is a link to its tab. With the panel closed and at least one
    tab, a faint "panel · 3 tabs ◂" sits at the top right of the chat, with an
    accent dot while a plan waits. `▸` at the right end of the tab bar closes the panel. Open
    or closed, and the chosen tab, are kept per chat for the run, not saved.
  - **The plan in the chat** is one line: "● Plan · ready for your review ·
    open plan" while waiting, then a faint "Plan · approved", "Plan · approved
    · auto-accepting edits", "Plan · kept planning" or "Plan · cancelled".
    Only the latest plan's line keeps the "open plan" link.
  - **Answering a plan** happens at the foot of its tab, mouse only: a
    feedback box, then Approve plan, Approve and auto-accept edits, Keep
    planning. Keep planning sends the feedback as the denial message
    ("Keep planning." when empty). Enter in the box adds a line; it never
    answers. Once answered the foot shows the outcome.
  - **Links in chat text and in tabs** (added 2026-11-13, so plans kept as
    project files can be reopened in any chat). `contentClick` in
    `clicks.ts` reads the link as written (`getAttribute("href")`, not
    `link.href`, which would turn a bare file name into a page of the app).
    `http(s)`, `mailto` and `tel` open in the browser. Anything else goes
    through `localPath` in `paths.ts`: plain paths and `file:` links,
    `~` for home, relative paths from the chat's project folder (in a
    document tab, from that document's folder; in the plan tab, the
    project folder); `#heading`, `?query` and a `:12` line number are
    dropped. A `.md` / `.markdown` path opens as a tab via `Chat.openFile`,
    which reads it fresh from disk with the core command `read_doc`
    (`docs.rs`: absolute markdown paths only, up to 2 MB) and refreshes an
    existing tab's text; a link to the plan's own file opens the plan tab;
    a file that can't be read adds an error line to the chat. Any other
    local file is shown in Finder or Explorer, never opened, so a link
    can't run a script. `markdown.ts` lets `file:` links through, which
    markdown-it refuses by default. Tabs opened this way aren't saved; a
    reloaded chat doesn't bring them back.
  - **Width:** 46% of the window by default. Its left edge drags to resize
    (the chat keeps at least 360 px, the panel 280 px); a double-click on the
    edge goes back to 46%. The width is one saved setting for every chat.
    Below 1100 px the open panel takes the chat's place.
  - **The terminal** is the panel's fourth view, picked by its icon like the
    others; its row holds the project's tabs and + (see "Terminal").
- **Rendered markdown must be safe.** As built: markdown-it runs with raw HTML
  turned off and its default link checks, and highlight.js escapes code, so
  the output holds only tags the renderer itself produces. dompurify was
  dropped as unneeded on top of that; add it back if raw HTML is ever turned
  on. Links open in the default browser, never inside the app window.

### Browser (`browser.rs`, built 2026-11-14)

A page beside the chat, for localhost frontend work: the user looks at it,
Claude drives it. Pages are tabs, several per chat (2026-11-16), each made
the first time either of them opens an address in it; closed by its tab's ×,
or all of them when the chat is deleted; put away when left unused (below). Not tied to the chat's Claude Code process: an idle close
leaves the page as it is. Only http(s) on
`localhost`, `127.0.0.1` or `[::1]` loads; a link anywhere else opens in the
default browser (`on_navigation`). The side chat gets no browser tools.

- **The page** is a child web view of the main window (tauri `unstable`
  feature, `Window::add_child`), so it has its own WebKit page process (19 MB
  for a small test page; the GPU and network processes are shared). It starts
  on `about:blank`; on Mac its start-up scripts are then replaced with the
  bridge alone (`mac::only_bridge`), because Tauri's own (`__TAURI_INTERNALS__`,
  `isTauri`, locked in place) made a Tauri app's dev page, Harness's included,
  believe it ran inside Harness. On Windows they stay for now; calls through
  them are refused, as no capability covers `browser-*` views.
- **Placing it:** `BrowserView.svelte` sends the panel body's rect
  (`.slot`, CSS px = logical px) with `browser_place` on every layout change,
  at most once a frame, and whether it should show: the browser view is on
  screen and nothing covers the panel (Settings, the image view: App passes
  `covered`). The rect and "shows" belong to the chat; only its tab on show
  is placed there (`Browsers::arrange`). Not shown = parked at x = -20000 at
  the same size, so Claude's reads and screenshots match what the user would see. Leaving the view
  (another view, chat or the panel hidden) unmounts the component, which parks it.
- **Claude's tools** are an in-process MCP server over the existing pipe:
  `--mcp-config {"mcpServers":{"harness_browser":{"type":"sdk",...}}}`, and
  Claude Code sends `control_request` `mcp_message` lines that
  `claude_code.rs` answers (`answer_tool_message`) with `mcp_response`. No
  port, no token; each process's calls belong to its chat. `--settings` allows
  `mcp__harness_browser`, so they never ask. Tested on CLI 2.1.322: no
  `initialize` from the host is needed first; the tools come as deferred tools
  (Claude loads them with ToolSearch). Agent-agnostic side: `AppTools` in
  `agent/mod.rs`, made per chat by `SessionManager::set_app_tools`.
- **Tabs for Claude (2026-11-16):** `tabs` lists id, title, address and the
  one on show; `close {tab}` closes one; every other tool takes an optional
  `tab` id and otherwise works in the tab on show. `open` loads into the tab
  on show (or the one given) unless `new_tab` is set; a new tab it opens
  becomes the one on show, but the panel isn't opened for it. Tab ids count
  up per chat ("1", "2", …). The tool descriptions explain tabs on their own,
  because Cursor hears `INSTRUCTION` only on a fresh conversation. Cursor gets
  the same tools over `mcp_http.rs`, so nothing was Cursor-specific.
- **Actions** run as scripts in the page (`BRIDGE_JS`): started with `eval`,
  answered through `eval_with_callback` as a JSON string, async ones (`run`)
  collected by polling `__hb.take(id)`. The page has no way to call the app.
  `read` lists visible buttons, links and fields as `[n]`, marking each with
  `data-harness-ref="n"` for `click`/`type`. `type` goes through
  `execCommand("insertText")` so frameworks see real input, with the value
  setter as fallback. Errors and warnings (console, uncaught, failed loads of
  scripts and images) are kept in `sessionStorage`, so they outlast a reload;
  `open` clears them, `console` reads and clears them.
- **Putting away:** a tab off screen and unused (no tool call, not on
  screen) for 10 minutes has its page closed, which frees its page process
  (gone 15 to 30 s later); the chat keeps the address and the window keeps
  its tab. The agent's next tool call, showing the view, or ↻ loads it again,
  so page state (form input, scroll) is lost, as a reload would. Checked every
  30 s; `HARNESS_BROWSER_IDLE_SECONDS` overrides the 10 minutes. The user
  asked for this (2026-11-14): they leave chats sitting with a page open and
  don't want to remember to close it. **Tested** with 5 s.
- **Screenshots:** Mac only, `WKWebView takeSnapshotWithConfiguration`, drawn
  at the page's layout size (view size / zoom, so a shrunk page comes out
  full size and sharp), shrunk so the long edge is at most 1568 px, sent as
  PNG. They work while parked. Other systems say they aren't supported yet.
- **Resize (changed 2026-11-16):** `resize {width, height}` gives the page a
  window size in CSS px, like resizing a real browser window: the page lays
  out at exactly that size (`innerWidth`/`innerHeight`) and the panel shows it
  shrunk to fit, at the top, centred across (never enlarged). The user wanted
  Claude to see a page as a desktop user would, not the narrow panel's shape;
  the old width-only zoom gave a tall page (1440 × 2160) and a blurry
  screenshot. How: the web view is the size times a zoom, and the page is
  zoomed the same (`Place::frame`). On Mac the zoom is WebKit's view scale
  (`_setViewScale:` with `_setLayoutMode:` 2, "dynamic size computed from view
  scale"; private, checked with `respondsToSelector:`), not page zoom: page
  zoom brings in WebKit's "smart minimum" text size (text of 9 px or more is
  kept at least 9 points on screen), so at 28% a 12.5 px font became 32 px
  and the page ballooned (the user caught it, 2026-11-16). View scale alone
  (mode 0) laid fixed elements and media queries out at the small view.
  Elsewhere it is still page zoom. wry and WebKit drop fractions of a point
  and WebKit rounds the layout down, so the view gets whole-point sizes picked
  to map back exactly (live, a 521.09 px panel first gave 1439 × 900). No
  height: as tall as the panel holds. No width: fits the panel again. Stays
  across loads. With a page open, resize waits for the new size to reach it
  and two frames (`settle` in the bridge), then says the size the page
  measures. **Tested** live (Haiku): 1440 × 900, 1366 × 768 and 390 × 844
  measured exactly, desktop and phone layouts, footer pinned to the bottom,
  1568 × 980 sharp screenshot with 11 px text readable. A page that isn't on
  screen (window hidden, display off; parked likely too) gets no frames, so
  its resize handlers wait until it shows; CSS layout is right at once. Not
  checked by eye: the shrunk page in the panel (the display was off). The
  tool text and instruction line name no sizes: with "e.g. 1440 x 900" there,
  every agent copied it, and the user found it too small. Claude picks the
  size itself (user, 2026-11-16).
- **Window side:** `Chat.browserTabs` and `browserShownTab`, sent whole by
  the core in a `browser_tabs` event on every change (an empty list = closed);
  `Chat.browser` is the tab on show; `browserOn` (view open, maybe with no tab
  yet: an empty address line). The bar shows the tabs (title, else host, else
  "new tab"), +, ↻, the address of the tab on show, then the shared corner
  buttons. + (or Cmd+T) makes an empty tab and puts the cursor in the address. Claude opening a page never switches
  the panel; instead a successful `open` adds a line to the chat, like a
  plan's: "Page · localhost:5173 · open browser" (a `page` item, rebuilt from
  history on replay; only the latest line has the button, which shows the tab
  on that address, or opens it in a new tab). The address copies on one
  click, like the panel's (accent for a moment, no underline). The line never
  wraps: a long address is cut short with "…" where it stops fitting, and
  "open browser" stays in view (2026-11-16). No marks in the bars: the
  user disliked a ● beside ◂ (2026-11-14). The globe icon in the top bar
  opens the view at any time; with no page yet it shows an empty address line
  in the bar, and typing there makes a tab. A chat loaded after its pages were opened asks `browser_current`.
- **Self-test:** `@browser <url>` types an address (waits for the load),
  `@settings` toggles settings; the report has `browser {on, page, tabs, pageLines, slot}`.

### Notes and plan edits (built 2026-11-14, branch `harness/chat-notes`)

**Notes.** Every chat has a `notes` tab, always first in the docs row and
never closed, for longer thinking than a message. Markdown edited in place
while shown formatted, like Obsidian's live preview (the user's choice over a
separate edit mode and over Notion-style editing): headings, bold, code and
links look as when read, and their marks show only on the line the cursor is
on, so the file is exactly what was typed (`liveMarkdown.ts`, CodeMirror 6
with the `@lezer/markdown` parser plus GFM; `@codemirror/lang-markdown` was
dropped because it pulls in HTML, CSS and JavaScript support: 500 KB to
307 KB, about 100 KB gzipped). It loads on demand, the first time notes or a
waiting plan are shown (`LiveMarkdown.svelte` shows the formatted text until
then). A shown link is followed on click, not edited. Escape leaves the
editor. Empty notes get the cursor. Lists edit like the message box, through the same
`editing.ts` functions: Enter in a list starts the next item (an empty item
steps back a level or ends the list), Tab and Shift+Tab indent, and numbered
lists stay in order. Tested with key presses in the editor. Format keys
(2026-11-17, `emphasis` and `link` in `editing.ts`): Cmd+B bold (`**`), Cmd+I
italic (`*`), Cmd+K link. They act on the selection (spaces at its ends left
out) or the word at the cursor; with no word, an empty pair goes in with the
cursor between. Pressed again they take the marks off; `***` counts as both.
Cmd+K puts the cursor where the address goes, puts a selected address in the
address instead (a cursor in an address takes all of it), and inside a link
turns it back to its text. Pasting a web address over selected text on one
line makes it a link to that address (`pasteLink`). The message box has the
same three keys and the paste (Cmd only on a Mac, where Ctrl+B/K move the
cursor). The sidebar key moved to Cmd+S to free Cmd+B. No underline
(markdown has none; the user's choice), no strike or headings. Saved 400 ms
after typing stops and when editing ends (`write_notes`), read lazily the
first time the tab shows (`read_notes`).
- **Pairs and stepping over** (editor only, `closers` in `liveMarkdown.ts`):
  typing `(`, `[`, `{` or a backtick puts in the closing one, but only before
  a space, the line's end or closing punctuation, and a backtick not after a
  word or a backtick (so ``` still makes a fence). Not quotes (apostrophes).
  A `*` waits for the next key, since it may be a list item (`starAt`): a
  word's first character right after a typed `*`, `**` or `***` puts the
  same stars in after it (`*h|*`); a space, a cursor move or any other edit
  leaves the stars alone. Only for stars at the line's start or after a
  space or opening punctuation (not `2*3`), and not in code (the user's
  ask, 2026-11-17). Typing a closing mark the editor put in itself (from a
  pair, or from Cmd+B/I/K: `**`, `](` and `)`) steps over it; Backspace
  between such a mark and its opener removes both. Marks the user typed are
  never stepped over; the editor forgets its marks once the cursor leaves
  their line. Written by hand rather than with `@codemirror/autocomplete`'s
  `closeBrackets`, which can't track the Cmd+B/I/K marks and adds a package.
  Tested with key presses (typing through CodeMirror's input hook).
- Stored in the chat's own folder in the home folder,
  `~/.harness/projects/<key>/chats/<chat id>/notes.md` (`notes.rs`; see "The
  home folder"), never in the repo; written to a side file then moved into
  place; 1 MB cap. Purged with the chat when the trash is emptied; swept at
  start for chats that don't exist (a new chat never sent), like images.
- The agent reads them only when the user mentions them (the user's choice:
  "I wrote some notes, take a look"). Each launch, main and side chat, gets
  `--add-dir <notes folder>` (no permission prompt, tested on CLI 2.1.322) and
  a line in its instructions with the file's path. Nothing is sent per turn,
  so prompt caching isn't disturbed.
- **The main chat writes them too** (2026-11-15), when the user asks ("put
  this in my notes"): it reads first and changes only what was asked. No
  approval card: `--settings` allows `Edit(//<notes file>)`, which covers
  Write and Edit (tested on CLI 2.1.322, default mode, a path with a space).
  Cursor is told the same; its own permission asks decide. The side chat
  stays read-only. The panel reads the notes back from disk when a Write or
  Edit to the file finishes, and at every turn end (catches shell writes).
  The editor takes the new text even while the user types, replacing only
  the changed middle so the cursor stays put; that change isn't saved back,
  and a pending save of older typing is dropped, so neither undoes Claude's
  write. A link to the notes file opens the notes tab, not a second tab.
  **Tested** live with the self-test (Haiku, `@answer none`): notes edited
  with no card, the panel showed the new text, no extra tab.
- The docs icon is never faded now. Ending the side chat or closing the
  browser still closes the panel when only the notes are left
  (`Chat.hasPanelWork`).

**Plan edits.** A waiting plan is editable the same way, in the same editor. Approve (and the
"always" variant) sends the edit: `answer_approval` takes `plan`, which
replaces `plan` in the approved input; Claude Code works from that text (tested
on CLI 2.1.322: no write to its plan file needed). A saved `PlanEdited` event,
recorded before the answer, keeps the edited plan after a restart. Keep
planning with an edit sends the whole edited plan in the message. An answered
plan is read-only.

**Tested** live with the self-test (Haiku): notes read by the main chat with
no approval card, and by the side chat; an edited plan (GOODBYE added)
followed exactly and still shown after a restart. Checked in the preview: the
tab order, formatted notes, editing. Self-test: `@notes <text>`,
`@planedit <text>`; report `notes` and `plans`.

**Document edits** (2026-11-15). Every markdown tab (files the chat wrote or
a link opened, in the project or not) is edited in place like the notes and
saved as the user types (`write_doc`, 400 ms after typing stops, when editing
ends, and before typing starts in another tab). The answered plan stays
read-only.
- Tabs follow the file, not Claude's calls: a tab is read from disk when it
  shows, after each Write/Edit to it, and at every turn end. A read that
  finishes after the user typed is dropped (the typing is newer).
- `docs.rs` only writes a file the panel has read (`Docs` remembers paths
  `read_doc` returned), that still exists (typing doesn't bring back a file
  Claude deleted), and whose text was valid UTF-8 (else the shown text
  differs from the bytes). Written to a side file in the same folder and
  moved into place, keeping the file's permissions; a link is followed and
  stays a link.
- Same clash rules as the notes: Claude's change replaces only the changed
  middle while the user types and isn't saved back; a pending save of older
  typing is dropped. If the user edits after Claude read a file, Claude's
  next Edit fails ("modified since read") and it reads again.
- **Tested**: Rust tests; in the preview, typing saves, also when switching
  tabs at once; live with the self-test (Haiku): Claude wrote a doc,
  `@docedit <text>` saved an edit through the core, Claude's next Edit
  landed on top, and tab, file and Claude agreed. The self-test's
  `HARNESS_AUTOTEST_MODEL` now also picks the agent (a plain name is Claude
  Code) and marks the model as picked: since new chats take the project's
  last pick, tests had been running on Cursor.

**Docs in the chat folder** (2026-11-16, branch `harness/docs-in-harness-98cbdf`).
Asking for notes, a write-up or "put it in the docs" used to make a Claude
Doc on claude.ai: its connector tells Claude to make docs there "not a local
file, even when coding", and Harness said nothing about its own panel.

- **Every agent is told** (`docs_rule` in `agent/mod.rs`, part of the
  writable `notes_instruction`, so Claude and Cursor get the same words):
  write-ups go as a new markdown file in the chat's folder
  (`…/chats/<chat id>/`, beside notes.md), named for what they hold, with a
  one-line reply linking the file as `[name](<full path>)` (angle brackets so
  the space in "Application Support" works; markdown-it encodes it and the
  link opens the tab); never into notes.md unless the user says "my notes";
  "docs" means these tabs, never an online service; edit with the edit tool;
  no worktree needed; "save it to the project" means write it into the
  project's docs folder in the home folder (named in full), delete the old
  file and keep editing the new one; into the repo itself only when the user
  asks for that in so many words. Answers and discussion stay in the chat.
- **Claude Docs is off in Harness only** (`launch_settings`):
  `deniedMcpServers: [{serverName: "claude.ai Claude Docs"}]` drops the
  server, its tools and its instructions, while other claude.ai connectors
  stay. A permission deny alone hid the tools but left the instructions
  (tested on CLI 2.1.325; connectors load after the first message, so
  probes need two turns). The docs skill can't be taken off the list;
  `Skill(anthropic-skills:docs)` in `deny` refuses it, and Claude is told
  "Claude Docs isn't available in this app."
- **No card for the folder**: the allow rule is now `Edit(//<folder>/**)`
  (was the notes file alone). Tested live (CLI 2.1.325, default mode,
  folder with a space): docs and notes written with no prompt.
- **A new chat-folder doc opens its tab at once** (`showNewChatDoc`), like
  a plan; later edits to it, and markdown files elsewhere, don't switch tabs.
- **Tabs from disk** (`Chat.isChatDoc`): an edit that doesn't carry the
  text (Cursor's tools) still opens a tab for a chat-folder doc, read from
  disk; turn end and reopening read them again, and one that is gone (saved
  to the project, deleted) loses its tab.
- **Save to project**: a chat-folder doc's tab has a "Save to project"
  button at its foot, styled like Approve plan (grey while the agent works; one shared `button.primary` rule in `app.css` for every accent button). `save_doc_to_project`
  (`docs::save_to_project`) copies it into the project's docs folder in the
  home folder (`~/.harness/projects/<key>/docs/`; the project's main folder
  names it, never a worktree, so every chat of the project shares it),
  adding `-2`, `-3`… rather than overwriting, and removes the original. The
  notes tab has the same button (`save_notes_to_project`): the notes become a
  doc there named after the chat, and start empty again. Either way a saved
  `doc_saved {from, to}` event lets the tab, and links to the old path,
  follow the file when the chat is reopened, and the agent is told with the
  next message.
- **App notes**: things the window tells the agent with the next message
  (`appNotes` → `send_message` → `with_app_notes` in `sessions.rs`), put in
  front of the agent's text as "(From the app, not the user: …)". Not shown
  or saved; lost if the app quits before the next message.
- **Tested**: Rust tests; a live Claude run with the real launch args:
  "write some short notes" made project-notes.md, "put X in my notes" went
  to notes.md, "make it a Claude Doc" was turned down. Not tested live: the
  button in the real window, Cursor's tab from disk.

### Comments on docs and plans (built 2026-11-17, branch `harness/comments-on-docs-that-agents-f-0774cd`)

The user selects text in a doc tab or a waiting plan, leaves a comment, and
sends all of a tab's comments together. The agent edits where a comment asks
for a change, answers questions in the chat, and asks in the chat when a
comment is unclear. Sent comments go away. Notes don't take comments yet
(easy: same editor). The chat's latest answer does: see "Comments on chat
answers".

Mock-ups: "Comments on Docs" canvas in the Magic Patterns OS (see "Look"),
option E "Floating, with a list" (chosen over inline, in-the-foot, and
windows pinned to the edge, which covered the text in a narrow panel).

- **Look.** Commented text gets an accent underline. Selecting text shows a
  small "+ comment" button under its last line. The button, the new
  comment's input and a written comment's window follow one rule (`under`),
  so the input opens exactly where the button was and the comment later
  shows where it was typed: under the text's last screen line; while
  narrower than the text on that line, its right edge sits at the text's
  end; wider, it starts where that line's text starts (past a list marker
  on a wrapped line) and grows right. So a box grows left, then right, as
  it is typed in. A window narrower than the button is placed as if it had
  the button's width, so the box's left edge opens exactly on the button's.
  The button has a popup's colours (panel, hairline border, accent words)
  and the padding of the app's strong buttons (Approve plan, Allow): none
  above or below, 1ch at the sides. Tried and dropped: the box's own
  padding (too wide), the filled accent button itself, and a pixel less
  padding above than below in the windows (the text looked too high). The
  windows (and the list) look like the app's other
  popups (model menu, / and @): panel colour, hairline border, no shadow,
  but 1ch side padding (see the × below), no marker (an accent rule down the left, then an
  accent `>`, were tried and dropped). A comment's
  window is as wide as its text needs, up to 40ch. Resting the pointer on a
  mark opens its window (after 150 ms; it closes 250 ms after the pointer
  leaves both the mark and the window, so the pointer can cross to it),
  focused or not, without moving the cursor. So does the cursor in a mark
  (a click, or arrows). Hover was first left out because docs are edited
  too; it went in (2026-11-18) since commented text is for the agent to
  change, so a window over it rarely gets in the way. Text selected: the
  button wins over hover. Spacing is 1ch all round: left padding, comment
  to ×, × to the right edge (2026-11-18; before, 2ch padding with a 1ch
  gap left the × closer to the text than to the edge; 2ch gaps were tried
  too). The written comment is itself the
  box: click in and type to edit (no edit button), × removes it; the window
  stays while its box has the cursor. Enter saves, Shift+Enter a new line,
  Escape gives up; both close the window and put the cursor back at the
  text, and no window opens there until the cursor moves (it used to reopen
  at once, a flash). Clicking away keeps what was typed; emptied, a comment
  is removed.
- **The list of all comments** is docked at the top of the tab's foot,
  above the plan's "what should change?" and the buttons: one line with the
  count (`2 comments ▸`) that unfolds every comment under it (its quote,
  then its text with × on that line; either jumps to its mark). It is part
  of the panel, not a popup: the doc gets shorter and nothing is covered
  (30vh at most, then it scrolls). It is set apart from the note and the
  buttons under it by space (1.5 gaps), not a second rule. The line is there
  only while the tab has comments. Folded or open, it stays that way across tabs, chats and
  restarts (prefs `commentListOpen`), folded to begin with. A doc's button
  says "Send comments" (the line above has the count). Sketches: "Comment
  List Placement" canvas in the Magic Patterns OS, option C. Tried and
  dropped: a popup floating over the doc's end (covered text once the list
  stayed open), opened from a count in the plan's foot, then from a comment
  icon in the top bar; a popup dropping from that icon was sketched only.
- **The comment box** is a `contenteditable="plaintext-only"` element, not
  a textarea: it is as big as its text by itself. A textarea had to be
  sized by measuring a hidden copy of the text, and wrapped the last letter
  on some keys however it was padded. Shift+Enter is done by hand
  (`insertLineBreak`); the placeholder is `:empty::before`. The window is
  `width: max-content` (capped at 40ch): shrink-to-fit made it narrow
  whenever it opened near the panel's right edge, which is the window's
  edge; the editor moves it left to fit instead. After each change to the
  text the windows are placed again (`repositionTooltips`). Text that wraps
  leaves a box at its full width, with a ragged gap at the right that one
  line never has, so a box of several lines is cut to its widest line
  (`fit`: laid out at its natural width, then measured word by word, on
  every change, so it can't flicker between two widths).
- **Where they live** (`comments.ts`, `DocComment`): on the chat
  (`Chat.comments`, by tab: a doc's path or the plan tab), never in the file,
  in memory only (lost on quit, like the plan's "what should change?" text).
  The editor (`commentMarks.ts`) holds each comment's place and maps it
  through every change, the user's typing or the agent's write alike; text
  typed just before or after a mark stays outside it. If the commented text
  is deleted, the comment stays (list only, quote struck through) and is
  sent with its old quote. The editor is rebuilt each time a tab shows, so
  it reports every change to the list back (`oncomments`), and takes a list
  from outside (`replaceComments`) only when it differs. A doc saved to the
  project takes its comments along; a doc that is gone drops them; a new
  plan starts with none.
- **What the agent gets** is plain text, the same for every agent: "Comments
  on <where>:" and a numbered list, each "Line N, on "<quote>": <comment>"
  (quotes cut at 200 characters).
  - **Doc:** "Send N comments" flushes unsaved typing, then sends that as a
    message from the user, with the doc as a link (it shows in the chat).
    The guidance line goes as an app note: change the text where a comment
    asks for a change, answer questions in the chat, ask instead of
    guessing. The marks turn faint, and go when the turn that took the
    message ends (a queued message waits for its own turn). It waits its turn
    like any message if the agent is busy.
  - **Keep planning:** what was typed, then the list and the guidance line,
    in the answer's message (`Chat.keepPlanningMessage`).
  - **Approve** (and "always"): the plan goes as an edit with "## Comments
    from the user", the list and "Take these into account while building. If
    one is unclear, ask in the chat before acting on it." at its end
    (`Chat.approvedPlan`). Claude Code builds from the `plan` in
    `updatedInput`; Cursor gets its usual "approved after editing it" note
    with that text. No Rust changes. The answered plan shows the section, a
    record of what went with it.
- **Windows** are CodeMirror tooltips, put in the page's body so the panel's
  scrolling box doesn't cut them off; `getCoords` places them under the
  mark. Side to side they are kept within the text's column
  (`tooltipSpace`), so one near the right edge stops at the text's edge, as
  one at the left does, not at the window's edge. The comment input takes the cursor a frame after it opens: the
  editor places a new window after making it, which took the focus from the
  box and closed it. A blur only finishes the input after a press elsewhere
  or a move to another field; otherwise the box takes the cursor back.
- **Self-test:** `@doccomment <quote>|<text>` comments on the first doc,
  `@sendcomments` sends them, `@plancomment <quote>|<text>` comments on the
  next plan before its auto answer (Approve, or Keep planning with
  `@answer deny`), through the same code as the buttons.
- **Tested**: in the preview (select, button, input, marks following typing
  above them, the comment window, the list and jumping, Keep planning's and
  Approve's text, a doc's send and its marks clearing at turn end). Live
  with the self-test on Claude Code (Haiku) and Cursor (Composer 2.5): a
  doc's "banana → mango" comment was made; a "??" comment got a question in
  the chat, not a guess; an approved plan with "name the file greeting.txt
  instead" built greeting.txt. The live runs were before the look was
  reworked (the box, its placing, the docked list); what is sent didn't
  change. The user tried the look in the preview by hand. Not tested: the
  built window (real selection and typing there), Keep planning live, and,
  in the preview, the last two changes (windows kept inside the text's
  column, the space under the docked list): its page was hidden.

### Comments on chat answers (built 2026-11-18, branch `harness/comments-on-chat-answers-c6d80e`)

The user selects text in the agent's latest answer, leaves a comment, and
the comments go with the next message. For replying to a long answer with
several points without losing one's place. Nobody stores them: sent, they
go. Prior art: ChatGPT and claude.ai quote one selection into the next
message; batching pinned comments is only in add-ons and side products.

- **Decisions.** Latest answer only (what the agent wrote since the user's
  last message, or, before it has written anything since, the answer that
  message was about); older answers, thinking, tools and the user's own
  messages take none. Comments and typed text go together on the normal
  send (no button of their own); comments alone can be sent. A message
  starting with `/` goes as typed and leaves them waiting. The sent message
  shows the quote and comment pairs as text (no folding: collapsible things
  in the chat felt odd). Waiting comments are listed above the message box.
  Comments can be made while the answer streams in, on its finished parts.
  Side chats take none.
- **Look and keys:** the docs' (see above): the same button, box, windows,
  hover and list (`CommentDock.svelte`, shared with the side panel, as is
  the folded/open pref). The list sits in the message box's frame, under
  its rule and above the `>` line.
- **What the agent gets:** the typed text, then "Comments on your last
  answer:" and a numbered list, `N. On "<quote>": <comment>` (quotes cut at
  200 characters), in answer order (`withChatComments`). An app note goes
  with it: each comment is about the quoted part of the last answer;
  respond to each, act on any that ask for a change, ask if one is unclear.
  It is a normal message: sent while the agent works, it waits its turn
  like any other. The list clears once it is sent.
- **Drawing answers by block** (`renderBlocks` in markdown.ts,
  Markdown.svelte): the text is parsed once and each top-level block
  rendered on its own, a direct child of `.md` as before (no look change).
  While text streams in only the last block's string changes, so Svelte
  redraws only it; the others are never touched again. That makes a
  finished block's text stable, and also saves redrawing a whole long
  answer on every word.
- **Finished:** every block of a text item but the last while the item is
  streaming. A selection touching the last one, or spanning two items,
  gets no button.
- **Where they live** (`ChatComment` in comments.ts, `Chat.chatComments`):
  in memory, with the text item's id and offsets into its `.md` element's
  text. Pushing a new text item drops comments not on the latest answer.
- **Marks and windows** (`chatComments.ts`, `ChatComments`, made by
  ChatView): the marks are `span.cm-comment` wrapped around the commented
  text nodes (never the text between blocks, which belongs to Svelte's
  drawing). After every change to the comments or the latest answer they
  are checked and, if missing or stale, unwrapped and put back from the
  offsets. Windows are plain elements with the docs' classes in a layer in
  the body, styled by the same rules (`commentLook` in commentBox.ts, put on
  the page by `mountLook` under the chat's scope), placed by the docs'
  `under` rule from the text's screen rects, kept inside the answer's
  column, hidden when scrolled out of view. One window at a time: the input,
  then a comment being edited, then the button, then the hovered (or
  clicked, or jumped-to) comment.
- **Self-test:** `@chatcomment <quote>|<text>` comments on the first match
  in the latest answer; `@sendchat <text>` sends with the comments, through
  the same code as the message box.
- **Tested** in the preview (its scripted answer now streams four blocks):
  finished blocks take comments mid-stream and the streaming one doesn't;
  button, box, marks, hover open/close, the list, jump, ×, send with text
  and alone (the list and marks clear, the message reads as above). Docs
  hover on a plan comment: opens, survives the move onto its window, closes.
  Not tested: live on Claude Code or Cursor, and by hand in the built app.

### The home folder (`home.rs`, built 2026-11-17)

What chats write for the user to read is kept in one folder in the user's
home directory, split by project, and never in a repo unless the user asks
for exactly that. Before this, "Save to project" put a doc into the repo's
`docs/`: it showed up in git, someone had to decide whether to commit it, an
untracked copy could block a pull, and a doc saved from a worktree chat
existed only on that worktree's branch.

```
~/.harness/
  projects/<key>/docs/               the project's docs, shared by all its chats
  projects/<key>/chats/<chat id>/    that chat's notes.md and its own docs
  unsent/<chat id>/                  notes typed before a chat's first message
```

- **Why outside the repo, and not an ignored folder in it:** a worktree is a
  second copy of the repo's files, so anything inside the repo exists once
  per worktree. Only a folder outside is the same for every chat of a
  project.
- **`<key>`** (`Home::key`) is the project's main folder path with every
  character that isn't an ASCII letter or digit turned into `-`
  (`/Users/me/tailai/harness` → `-Users-me-tailai-harness`), the way Claude
  Code names its own project folders. Two paths can give one key
  (`/a/b-c`, `/a/b/c`); they would share a folder.
- **Which folder** (`Home::resolve`): `HARNESS_HOME` when set; else, when
  `HARNESS_DATA_DIR` is set, `<data folder>/home`, so a test or a test copy
  of the app never touches the real one; else `~/.harness`.
- **Chat folders** (`notes.rs`): `Notes` finds a chat's project in the store.
  A chat has none until its first message makes it, so notes typed before
  then are in `unsent/<chat id>/` and move in at that moment (`adopt`,
  called in `send_inner` before the agent starts, so the agent only ever
  sees the final path). The sweep at start removes folders of chats that
  don't exist, in both places, and never a project's `docs`.
- **A project's docs** are where "Save to project" puts a doc or the notes
  (`save_doc_to_project`, `save_notes_to_project`: the window sends the
  project's main folder, never the worktree). Agents are told the folder in
  `docs_rule` and to look there for a doc that isn't in the chat. Harness
  never deletes anything in it.
- **Access.** Claude Code: `--add-dir` and `Edit(//<folder>/**)` for the
  chat's folder and for the project's docs folder (a side chat: `--add-dir`
  only). Cursor has no such switch; its own global settings
  (`~/.cursor/cli-config.json`, `~/.cursor/permissions.json`) allow reading
  and writing `~/.harness/**`.
- **The one-time move.** Chat folders used to be `<data folder>/notes/<chat
  id>/`. At start (`SessionManager::move_chat_folders`, before the sweep)
  each existing chat's folder is renamed into its project; one that can't be
  moved stays and is tried again next start; with nothing left the old
  folder is removed. On a copy of the real data: 79 folders moved, nothing
  to do on the second start.
- **Old paths keep working.** A moved chat's history still names the old
  folder. The window is given the chat's folder as it is now
  (`load_session` → `chat_folder`) and `Chat.currentPath` maps a path
  containing `/notes/<chat id>/` to the same file there, so doc tabs rebuilt
  from history and links in old messages open; `Notes::current` does the
  same for the earlier-chats `read` tool.
- **Telling the agent once.** `sessions.pending_note` holds text for a
  chat's agent that is sent with the user's next message there and then
  cleared (`add_pending_note`, `take_pending_note`; put before the message
  like other app notes). The move leaves each moved chat "This chat's own
  folder moved from … to …".
- **Not here:** the database, images, skills and worktrees stay in the app's
  data folder; they aren't for reading. The to-do list
  (`~/.harness/projects/-Users-tailaiying-tailai-harness/docs/TODO.md`) and
  the research doc moved out of this repo on 2026-11-17; this plan doc
  stays in it.
- **Tested:** Rust tests (layout, early notes, sweep, the move, old paths,
  the pending note, access); the preview page (a doc written under the old
  layout shows at the new place, and Save to project moves it to the
  project's docs); a test copy on a copy of the real chats: the move, an old
  chat's doc tab at its new path, and new chats on Claude Code and on Cursor
  (GPT) that wrote a doc and saved it to the project when asked, with nothing
  new in the repo; another chat then opened that doc by name.
- **Known gap:** in a Cursor chat, a doc the agent itself saves to the
  project has no tab until its link is clicked (Cursor's edits carry no
  text, and only chat-folder docs are read from disk for a tab). The Save to
  project button moves the tab for both agents.
- **Project memory, for every agent** (2026-11-17, branch
  `harness/home-memory-2cc4f2`): `projects/<key>/memory/`, one memory per
  project (its main folder names it, so worktrees share it). Before this,
  what Claude Code remembered was in its own folder and Cursor never saw it.
  - **Claude Code keeps its own memory, moved.** `launch_settings` adds
    `"autoMemoryDirectory": "<folder>"` to the JSON passed with `--settings`,
    and an `Edit(//<folder>/**)` allow rule so a save shows no card. Nothing
    is turned off and nothing is said in its instructions: it saves quietly
    and loads `MEMORY.md` as before. A side chat gets the same setting
    (`memory_settings`). Tested on CLI 2.1.326: a new chat read the index
    there, saved a note there, and a resumed chat followed the setting.
  - **Cursor has no memory of its own**, so a fresh conversation's intro
    carries `memory_instruction` (`agent/mod.rs`): where the folder is, that
    `MEMORY.md` is the index (one line per note), that a note is one
    markdown file with frontmatter (`name`, `description`,
    `metadata.type`), when to write one, and the index as it is now (its
    first 200 lines, as Claude Code cuts its own). Any other agent without
    a memory can be told the same way; one that lets its memory be moved
    should be pointed at the folder instead.
  - **What Claude Code already remembered is copied in once**
    (`Home::ensure_memory`, when a chat's process starts): from
    `~/.claude/projects/<key>/memory/` (the same key) when the folder here
    has no `MEMORY.md` yet. Copied, never moved, so Claude Code run outside
    the app keeps its own, and from then on the two differ.
  - **Tested** in a test copy: Claude Code saved a fact and a new Claude
    chat recalled it; a Cursor (GPT) chat recalled it, then saved another as
    a note with frontmatter and an index line; a Claude chat started after
    that recalled both.
- **The user's preferences, for every agent** (2026-11-17, branch
  `harness/home-preferences-2cc4f2`): `~/.harness/preferences.md`, standing
  instructions whatever the project. Before this they were in Claude Code's
  own `~/.claude/CLAUDE.md`, which no other agent reads.
  - **In each agent's instructions** (`preferences_instruction`,
    `agent/mod.rs`; the file is read when a chat's process starts, cut to
    20,000 characters; missing or blank means none): Claude Code gets them
    at the end of `--append-system-prompt`, main and side chat; Cursor in
    the intro of a fresh conversation.
  - **A reminder with every message** (`SessionManager::reminder`, added in
    `send_inner`): "(From the app, not the user: Reminder: follow the user's
    preferences in <file>.)", after the user's words, never before (Cursor
    titles a chat from the start of its first prompt). Agents drift from
    the rules in a long chat, and Cursor hears its instructions only once.
    For an agent with no memory of its own the reminder also names the
    project's memory folder. Not saved in the history, not shown. A
    `/command` handed on to the agent goes exactly as typed, with no
    reminder, recent-chats note or pending note: text after it would be
    read as the command's arguments. A pending note waits for the next
    message.
  - **Helper agents don't inherit them.** Claude Code hands its own
    `CLAUDE.md` to the subagents it starts, but not text added at launch
    (tested on CLI 2.1.326: the main agent followed a rule given that way
    and its helper did not). So the instruction tells the agent to pass on
    the ones that matter, and in a test it put them in the helper's prompt.
  - **The switch-over is by hand, last**, once a build with this is running:
    copy the text of `~/.claude/CLAUDE.md` to `~/.harness/preferences.md`,
    empty that file (or Claude Code gets the text twice) and remove the
    `UserPromptSubmit` hook in `~/.claude/settings.json` that reminds Claude
    Code of it, each with a backup. Claude Code run outside the app then has
    no preferences; the user accepted that.
  - **Tested** in a test copy with a made-up rule ("end every reply with
    the word pineapple"): Claude Code and Cursor (GPT) both followed it on
    every message, and Cursor's chat title was still about the user's
    message.
- **A project folder that moved or was renamed** (2026-11-17, branch
  `harness/home-moved-project-2cc4f2`). Before this, everything a project
  kept was tied to its folder's path as text, nothing could change that
  path, and a message to a chat of a missing folder failed with "Not a
  folder".
  - **What is tied to the path:** `projects.path` (chats point at the
    project by id, so they follow); `terminals.project` and `.folder` and the
    live tabs; each chat's saved worktree (`repo_root`, `subpath`) and git's
    own links between the repo and its worktrees; `~/.harness/projects/<key>`;
    the window's `foldedProjects`, `projectOrder` and `projectModels`; and
    paths inside saved events, which are history and are not rewritten.
  - **Noticing.** The `sidebar` command marks a project `missing` when its
    folder is gone (`sessions::folder_gone`: not a folder, and not on a
    `/Volumes/<name>` that isn't connected, which shows as it always did).
    The sidebar draws a missing project faint with a `locate` text button
    where `+` was; its chats still open.
  - **The move** (`SessionManager::relocate_project(from, to)`, command
    `relocate_project`). Refused when the two are the same, `to` isn't a
    folder, `from` is still there, or `from` isn't a project. Then, in
    order: the store, in one transaction (`Store::relocate_project`: rename
    the project row, or when `to` is already a project move the chats to it
    and delete the old row; add `from` to `projects.former_paths`; re-point
    the terminal tabs, after the target's own when merging; rewrite each
    chat's saved worktree for the new repo root; leave every chat the
    pending note "This project's folder moved from … to …"); close each
    chat's process and wait for it (it resumes on the next message); forget
    cached repo roots; `git worktree repair` in the new root, which relinks
    every worktree (a failure is logged, and such a chat falls back to the
    main folder as before); drop the folder's GitHub cache
    (`github::forget_under`); `Home::move_project`; re-key live terminal
    tabs (`Terminals::project_moved`; running shells are left alone). The
    command also ends the moved chats' side chats.
  - **The home folder follows** (`Home::move_project`): a rename when the
    new key has no folder. Otherwise a merge: chat folders move across, docs
    move with `-2`… on a name clash, and memory moves in whole when the
    target has none, else as `memory/from<old key>/` with one line added to
    the target's `MEMORY.md`.
  - **By hand:** `locate` opens the folder picker at the old folder's parent
    and calls the command; an error shows in the sidebar's problem line.
  - **By itself, when certain** (`find_moved`, in `send_inner` for a first
    message in a folder the store has never seen): a missing project matches
    when the newest of its chats with a `start_commit` names a commit that
    exists in the new folder (`worktree::has_commit`, `git cat-file -e`).
    Exactly one match is followed, and the new chat records `ProjectMoved
    { from, to }`, drawn as a faint line. None or several: nothing happens.
    Projects on an unconnected drive are never candidates. It costs nothing
    unless some project is missing.
  - **The window.** `moveProjectPrefs` swaps the old path for the new in the
    three saved settings before the sidebar prunes paths it no longer sees.
    An open chat takes its row's new `project_path` on the next refresh.
    `SessionRow.former_paths` lets `Chat.currentPath` map a path under an
    old project folder to the same file under the new one, so doc tabs
    rebuilt from history and links in old messages open. Worktree chats'
    paths are under the worktree folder, which doesn't move.
  - **Tested:** Rust tests on real git repos (a renamed repo with a chat in
    a worktree: chats, terminals, home folder and worktree all follow and
    git works in the worktree; following by itself; two missing clones of
    one repo and a folder that isn't a repo are not followed; a merge; an
    unconnected drive), and the preview page (the missing row, `locate`,
    the saved order). In a test copy, on a scratch repo with three chats
    (Claude Code in the main folder, Cursor (GPT), Claude Code in a
    worktree) that was then renamed: a new chat in the renamed folder
    brought the project along by itself, and each old chat, sent a message,
    carried on its own conversation (it recalled a word from before the
    rename) from the new folder or its worktree; the terminal tab, the docs,
    the memory and the chat folders were under the new path. Claude Code
    finds a conversation by its id whatever folder it was started in (CLI
    2.1.326). Not tried in the real window: the `locate` button itself.
  - **The cleanup pass leaves a missing repo alone** (`sweep`): a chat whose
    worktree's repo isn't a folder is skipped. Found in that test: with the
    repo gone every git question fails, and for a put-away worktree a
    failure was read as "its branch was deleted outside the app", so the
    chat forgot its branch and could not bring its folder back once the
    project was pointed at. The same held for a repo on an unplugged drive.
- **Not covered yet:** following a moved folder that isn't a git repo
  without a click (there is nothing to recognise it by); noticing a move
  the moment it happens (it is seen the next time projects are listed); a
  list of a project's docs, a view of its memory or a preferences editor in
  the app; memory shared across projects; syncing between machines.

### Terminal (`terminal.rs`, built 2026-11-15)

Real shells in the side panel, for scratch work (finding things, a quick
command, a scratch server); real servers stay Claude's job. Mock-up: Magic
Patterns canvas "Terminal" (file `tHBjJsfaSXZFYgQhxfUVbS`).

- **Tabs belong to the project**, keyed by the project's main folder exactly
  as chats name it (`Chat.projectPath`, `projects.path`), never the project's
  database id (it doesn't exist before a first message and changes when the
  last chat is purged). Every chat of the project shows the same tabs; which
  view the panel shows is still per chat (`Chat.terminalOn`).
- **Shells:** `portable-pty`. The user's `$SHELL -l` (so their profile sets
  PATH; `/bin/zsh` if unset), in the main folder or a chat's worktree folder,
  with `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Harness`,
  `LANG=en_US.UTF-8` if unset, and no `ANTHROPIC_API_KEY`/`_AUTH_TOKEN` (so
  `claude` run there uses the subscription). Windows: `new_default_prog`.
  A reader thread per shell keeps the last 512 KB of raw output (`tail`) and
  sends each chunk to the window over the tab's own `Channel` as raw bytes.
  A shell that exits by itself takes its tab.
- **What runs:** once a second the core reads the pty's foreground process
  group; when it isn't the shell's, `ps -o args=` gives the command line.
  Tab name: argv[0]'s base name when something runs ("python3"), else the
  label (the project folder's name, or the worktree branch's short name);
  repeats get " 2", " 3". Changes go to the window as `terminal_tabs`
  events (empty session ID, not saved).
- **Closing:** × or Cmd+W asks "<name> is running. Close it?" when
  something runs, else closes at once. Closing hangs up the shell's process
  group and the job in front, waits 2 s, then kills what is left.
- **Unused for a day** (`HARNESS_TERMINAL_IDLE_SECONDS` for tests): checked
  every 30 s with `Instant` (doesn't count Mac sleep). Use = typing, the tab
  on screen (`terminal_place`), Claude's `read`; output never counts. The
  shell closes; the tab keeps its output plus a dimmed `── closed after a day
  unused: <command line> ──` line; typing starts a fresh shell under it.
- **Quit and restore:** `Terminals::shutdown` (before the chats' shutdown)
  saves each tab's last 256 KB to the `terminals` table, then ends every
  shell (1 s grace). Also saved every 5 minutes when there was output. On
  start, tabs of projects still in `projects` come back closed with a
  `── restored Nov 15 14:02 ──` line.
- **Worktrees:** + offers "new tab in <short branch>" while the open chat's
  worktree is live. A running shell in a worktree keeps the 14-day sweep
  from putting it away (`TerminalHooks::in_use`). When a worktree goes
  (chat deleted, branch merged, cleaned up), its tabs close with
  `closed: worktree removed (<why>)` and start again in the main folder
  (`TerminalHooks::gone`, called from `trash`, the sweep's merged and gone
  arms, and `purge`).
- **Claude:** a second in-process tool server, `harness_terminal`, with
  `tabs` and `read {tab?, lines?}` (default the tab on show, else the newest;
  200 lines, at most 2,000). `read` returns plain text: escape sequences
  dropped, `\r` overwrites, backspace, erase-line and clear-screen applied; a
  note when a full-screen program has the screen. `SpawnOptions.tools` is now
  a list, so a chat gets both the browser's and the terminal's servers.
  Shown as "read terminal <tab>".
- **Window:** xterm.js 6 with the fit addon, loaded on first use
  (`src/lib/terminal/xterm.ts`, a 334 KB chunk, 84 KB gzipped). Each tab's
  xterm lives in `src/lib/state/terminals.svelte.ts`, outside the panel
  (which is made again per chat), and its element is moved into
  `TerminalView`. Theme roles map to ANSI colours (`xtermTheme`); theme and
  text size changes come through the `harness-look` event.
- **Keys** (only while the terminal has the keys): Cmd+T new tab in the main
  folder, Cmd+Shift+[ / ] previous / next tab, Cmd+K clear, Cmd+W close tab.
  Cmd+W elsewhere closes the window as before. Ctrl keys always go to the shell.
- **Tab keys everywhere with tabs (2026-11-16):** the same Cmd+T, Cmd+W and
  Cmd+Shift+[ / ] work in the side chat and browser views too, only while
  that view has the keys (a side chat's box; the browser's address or a page
  clicked into). On the Mac they are menu items (`TAB_KEYS` in `lib.rs`:
  New Tab, Close Tab or Window, Show Previous / Next Tab) that send `tab_key
  {action}`; menu keys arrive even while a browser page holds the keys. The
  window's `tabKey` (`state/tabKeys.ts`) picks the view that has the keys;
  with none, Cmd+W closes the window and the rest do nothing. Off a Mac
  there is no menu, so the window's key handler calls the same router.
- **Memory** (release build, 2026-11-15): one terminal open adds about 6 MB
  to the window's web page (38 → 44 MB at start) and 1 MB to the core; each
  shell is about 4 MB.
- Self-test: `@terminal <text>`, `@termwait <text>`, `@termtab`,
  `@termclose`, `@docs`; report `terminal: {on, tabs, asked, text}`.

### Preview page

`npm run dev`, then open `http://localhost:1420/preview.html`: the same window
in a plain browser, with a stand-in for the core (`src/preview.ts`) that has a
sidebar of sample chats, one saved chat, a 300-turn chat, a chat with
images ("Settings list cut off"; pasting works too), a chat with a plan
waiting and two documents (for the side panel; Keep planning brings a revised
plan, approving brings an edit to a document), and a scripted answer to
anything typed. For working on the look without building the app. Add
`#open=planning&browser` for the browser view (no page is drawn in the
preview; the tab and address line work), or `&terminal` for the terminal (a
pretend shell with two tabs: one at its prompt, one running a server; it
answers `ls`, `pwd` and `echo hello`, and `sleep`/`python3` "run" until Ctrl+C).
It is not part of the built app. `.claude/launch.json` starts it as
`ui-preview`.

### Cursor over ACP (probed 2026-11-15, Cursor CLI 2026.11.13-eacade1)

Cursor's command-line agent (`agent`, in `~/.local/bin`; `cursor-agent` is the
same program) is driven with `agent acp`: newline-delimited JSON-RPC 2.0 on
stdin/stdout. What a live test showed:

- **`initialize`** result: `loadSession: true`, `mcpCapabilities {http, sse}`
  true, `promptCapabilities.image` true (a PNG was described correctly),
  `authMethods: [cursor_login]`. Sending `clientCapabilities._meta.parameterizedModelPicker: true`
  changes the model list (below). `_meta.subagents` also exists; not used.
- **`session/new`** returns `sessionId`, `modes` (`agent`, `plan`, `ask`) and
  `configOptions`. A new session starts on the **last model used in any
  Cursor session**, so always set the model explicitly.
- **Models.** Without the `_meta` flag, model ids look like
  `claude-opus-5-5[context=300k,effort=medium,fast=false]` and only those
  exact values are accepted. With it, the `model` option lists 48 plain ids
  with display names (`claude-opus-5-5` / "Claude Opus 5.5", `default` /
  "Auto"), and each model brings its own extra options: a `thought_level`
  option (id `effort`, `reasoning_effort` or `reasoning`; levels differ per
  model, e.g. `none … extra-high`, `low … max`), sometimes `thinking`
  (`true`/`false`), and `model_config` options `context` and `fast` (fast is
  2.5x the price; leave it). Change with `session/set_config_option
  {sessionId, configId, value}`; the reply is the full option list for the
  new model. Reading every model's levels takes ~40 s (one set per model), so
  do it in the background and cache it.
- **Modes**: `session/set_mode {modeId}` replies `{}` and sends
  `current_mode_update`. Plan mode has a `SwitchMode` tool the agent may use.
  **Tested 2026-11-19 (CLI 2026.11.19)**: agent mode has it too; asked to
  "make a plan first", the agent switches itself to `plan` and calls
  `createPlan`. The switch is a tool call (`rawInput.targetModeId`) with no
  `current_mode_update`, so the app takes the mode from the finished call.
  Given a build task with no word about plans, it builds straight away.
- **Turn text**: `agent_thought_chunk` then `agent_message_chunk` deltas;
  no message ids. `session_info_update {title}` arrives at the start of the
  first turn with a short chat title.
- **Tool calls**: `tool_call` arrives with `rawInput: {}` and a generic
  title ("Terminal", "Read File"); a `tool_call_update` then fills `title`
  and `rawInput`. Ids contain a newline. Several can run at once. Kinds and
  inputs seen: `execute {command}` → result `rawOutput {exitCode, stdout,
  stderr}`; `read {path}` → `{content}`; `edit {path}` → `content: [{type:
  diff, path, oldText, newText}]`; `search {pattern, path?}` (grep and file
  find) → `{totalMatches|totalFiles, truncated}`; MCP tools: kind `other`,
  `rawInput {providerIdentifier, toolName, args}` → `{success}` only.
- **Approvals**: only shell commands and MCP tools asked; edits and reads
  didn't. `session/request_permission` carries `toolCall {toolCallId, title,
  kind, content: "Not in allowlist: echo"}` and options with `optionId`
  `allow-once` / `allow-always` / `reject-once` (`kind` `allow_once` etc.).
  The answer is `{outcome: {outcome: "selected", optionId}}`.
- **Plans**: in plan mode the agent calls `createPlan` and Cursor sends the
  request `cursor/create_plan {toolCallId, name, overview, plan (markdown),
  todos, isProject, phases}`. **Answers to Cursor's own requests are wrapped
  like permission answers**: `{outcome: {outcome: "accepted"}}`,
  `{outcome: {outcome: "rejected", reason}}` (Cursor's code reads
  `outcome.outcome`; its docs show them unwrapped, and an unwrapped answer
  reaches the model as "User cancelled"). A rejection's `reason` reaches the
  model. After acceptance the agent stops and the mode stays `plan`, so the
  app switches to `agent` once the turn ends (switching while the plan call
  finishes made it read as cancelled) and sends "approved, build it" (with
  the edited plan, if edited) as a note, in the same turn as far as the
  window can tell.
- **Questions**: `cursor/ask_question` is handled in Cursor's code, but no
  model was given an ask-question tool (listed tools in plan mode: Shell,
  Glob, rg, ReadFile, Delete, TodoWrite, WebSearch, WebFetch, CreatePlan,
  SwitchMode, ApplyPatch, Subagent, MCP resource tools…). The agent asks in
  plain text instead. Nothing in the docs or the client code enables it.
  Answers are `{outcome: {outcome: "answered", answers: [{questionId,
  selectedOptionIds}]}}` or `skipped` with a `reason`; the app handles it in
  case it starts arriving.

**As built (2026-11-15, branch `harness/cursor-acp-217ae2`).** `agent/acp.rs`
speaks ACP (driver task: `initialize`, `session/new` or `session/load`, then
one `session/prompt` at a time; reader task routes replies, requests and
updates); `agent/cursor.rs` starts `agent acp` with the `_meta` flag, lists
every model (Auto first; those named in `efforts_for` are picked once each
to read their effort levels, then the user's own current model is put back)
and runs `agent login`. **Which Cursor models the menu shows is the user's
pick** (Settings, "Cursor models"; changed from a guessed "newest of each
family" on 2026-11-15, since Cursor's ids don't say what is current): the
picked ones listed with "remove", plus one "add a model" box that finds the
rest as you type (every word must be in the name or id; Enter adds the first
match). The first pick is Auto alone; a model Cursor lists later that wasn't
seen before joins the menu by itself. Kept in the window's storage
(`harness.models.cursor.picked`: `shown`, `known`); effort levels are read
only for shown models, and for one when it is added. `agent/mcp_http.rs` serves the browser and terminal tools over HTTP
on 127.0.0.1 (one random token per chat process); calls to them are allowed
without a card. A chat's agent is saved in `sessions.agent` when it is made
and never changes; `Agents` in `agent/mod.rs` maps agent IDs to adapters and
programs (`locate::find_cursor` prefers `cursor-agent`). Messages sent while
a turn runs wait and go together as the next prompt; app notes (a denial's
reason, an approved plan) are added to it. Titles come from Cursor's
`session_info_update`. In the window the model menu lists both agents under
their names until the first message, the label shows just the model (no agent name; the message box says "Message Cursor"),
the mode button steps through agent / plan / ask only, and side chat is
hidden. **Tested live** with the self-test (Auto model): a command approved,
an image read, the browser opened through the app's tool, a plan approved and
built in one turn, a stop mid-command, a close and resume that kept the
conversation, and the model list loaded. Not tested live: the sign-in card
(the error shape was probed), a question card (Cursor never sent one), and
"Keep planning" from the panel.
- **Stop**: `session/cancel` (notification) makes the open `session/prompt`
  return `stopReason: "cancelled"`. A permission request still waiting is not
  withdrawn by Cursor; the client answers it `cancelled` itself. The next
  prompt works normally.
- **Resume**: `session/load {sessionId, cwd, mcpServers}` replays history as
  `session/update`s (tool calls with ids `replay-N-M`) before its reply, which
  has the same `modes`/`models`/`configOptions` as `session/new`, with the
  session's own model. The chat remembers earlier turns. **Loading with a
  different `cwd` works**: history kept, commands run in the new folder (what
  a later worktree move for Cursor chats needs). `agent --worktree` and a
  `/worktree` slash command also exist.
- **MCP over HTTP**: an `{type: "http", name, url, headers: []}` server in
  `session/new.mcpServers` is used as is (Streamable HTTP: `initialize`,
  `notifications/initialized`, a `GET` that may be refused with 405,
  `tools/list`, `tools/call`). Its calls go through `session/request_permission`
  like shell commands.
- **Signed out**: `session/new` fails with error `-32000` "Authentication
  required" (`data.message` says to run `agent login`).
- **Errors in a turn** show up as message text ("Error: RetriableError: …")
  with `stopReason: "end_turn"`.

### Antigravity over ACP (probed 2026-11-18, agy 1.5.11)

Antigravity's CLI (`agy`, in `~/.local/bin`) has a built-in ACP server,
`agy acp`, Google's own way for editors to drive it. It runs through the same
`agent/acp.rs` as Cursor; what differs is `antigravity::DIALECT`. What live
probes showed:

- **`initialize`**: `loadSession: true`; `sessionCapabilities` resume, close,
  load, list, additionalDirectories; `_meta.antigravity.systemPrompt.append:
  true`; four `authMethods` (Google, Gemini Enterprise, API key, Agent
  Platform).
- **`session/new`** returns `models`, `modes` (`default`, `accept-edits`,
  `auto`, `plan`) and `configOptions`: `model` (plain names such as
  `gemini-4-neon`, `claude-opus-5-5`), `thought_level` and `mode`. Thinking
  levels differ by model (Gemini 4 to `xhigh`, Claude Opus and Flash to
  `high`, GPT-OSS none) and show once a model is picked. A new session does
  not start on the last model used elsewhere. A `usage_update {used, size}`
  follows, then `available_commands_update` (`/goal`, `/compact`, `/plan`,
  `/grill-me`, `/learn`, ...).
- **System prompt**: `_meta: {antigravity: {systemPrompt: {append}}}` on
  `session/new` and `session/load` is followed (tested: "end every reply with
  BANANA"; after a load the agent still knew the notes path, which only the
  appended prompt gives).
- **Tool calls** carry a top-level `name` (`run_command`, `read_file`,
  `write_file`, `mcp_<server>_<tool>`), `kind`, `title`, `locations[].path`,
  and `rawInput` with capitalized keys (`CommandLine`, `Path`, `Content`,
  `toolSummary`). A command's `rawOutput` is `{output, exit_code}`, its output
  after a "Created At / Completed At / Command exited ... Command output:"
  header, lines ending `\r\n`. Writes carry a `diff` content block. MCP calls
  carry their arguments in `rawInput` but no output.
- **Approvals**: standard `session/request_permission`, options
  `{optionId: allow, kind: allow_once}` and `{deny, reject_once}`. Commands,
  writes and MCP calls ask; reads don't.
- **Plans**: in `plan` mode the plan is a `write_file` into
  `~/.gemini/antigravity-cli/brain/<session>/` with
  `rawInput.ArtifactMetadata.RequestFeedback: true`; the turn then ends with a
  text ask. There is no plan request to answer. **Tested 2026-11-19 (agy
  1.5.12)**: the agent has no tool to enter plan mode (it said only the
  user's `/plan` can), but asked for a plan in `default` mode it writes the
  same marked file and stops. A walkthrough written after building is
  `userFacing` with no `requestFeedback`.
- **MCP over HTTP** works as Cursor's does.
- **Questions**: an `ask_question` tool call (`rawInput.questions[]` with
  `question`, `options`, `is_multi_select`), then an `elicitation/create`
  request (`mode: "form"`, offered only when the client declares
  `clientCapabilities.elicitation.form`). Each question is a `requestedSchema`
  field with `oneOf [{const, title}]`, plus `<field>_other` for a free-form
  answer. The answer is `{action: "accept", content: {q1: "1"}}`; `decline`
  and `cancel` also exist.
- **Subagents**: one `invoke_subagent` call (`Role`, `Prompt`, `TypeName`)
  that returns at once; the agent may wait with `manage_subagents`. The
  subagent's steps aren't sent. Its result arrives **after the turn's
  `session/prompt` reply**, as more `agent_message_chunk`s and a closing
  `usage_update`.
- **Signed out**: `session/new` fails with `-32000` "Authentication
  required". `agy acp --login` prints a Google link and waits for a pasted
  code (redirect to `antigravity.google/oauth-callback`), so it can't run
  without a terminal. ACP `authenticate {methodId: "oauth-personal"}` opens
  the browser itself.
- **Usage limits** (probed 2026-11-19): not sent over ACP, and `/usage`
  sent as an ACP prompt goes to the model. `agy -p /usage` (alias `/quota`)
  prints them in about 2 s, with no model call and no saved conversation:
  one tab-split line per limit, group, name, percent left, reset (UTC), e.g.
  `Gemini Models\tFive Hour Limit Remaining\t96%\t2026-11-19T20:03:27Z`.
  Seen: Gemini 5h and week, Claude and GPT 5h and week, Gemini 4 Argon week.
  The app runs it when an Antigravity chat starts and after each turn (one
  read at a time) and sends a `UsageUpdate` per line, named "gemini 5h",
  "claude and gpt week", "gemini 4 argon week". Settings → Usage lists them
  under "Antigravity".

**As built (2026-11-18, branch `harness/antigravity-agent-3ca873`):**
`acp.rs` takes a `Dialect` per agent: tool naming, own-tool detection,
plan-as-file detection, output tidying, the mode a plan is built in
(Cursor `agent`, Antigravity `default`), where the app's instructions go
(Cursor: end of the first prompt; Antigravity: the system prompt, on every
start), and whether a chat with no model picks one. A plan written as a file
is offered for approval once the turn ends (`Waiting::Plan` with no RPC ID);
the answer becomes the next prompt, switching to `default` first if
approved. **Changed 2026-11-19** (branch `harness/plan-card-for-every-agent-30d44b`,
the user's call: agents plan natively, and any plan they present gets the
card): the marked file is a plan in any mode, dropped if the agent starts
another tool call after it outside plan mode (it went on to build); an
approved plan leaves plan mode only if the chat is in it, so a plan made in
Auto is built in Auto. Each dialect's `plan_instruction` (left out of side
chats) tells the agent to present a plan through its own path: Cursor
`CreatePlan`, switching with `SwitchMode` first; Antigravity its plan artifact
with `RequestFeedback`. Without it, Antigravity (Gemini 4 Hydrogen, in Auto,
with the user's preferences) once wrote the plan only in the chat. **Tested
live by the user** (2026-11-19, test copy): the plan card for every agent.
`usage_update` feeds the context meter for any ACP agent;
question forms are shown as question cards for any ACP agent whose dialect
offers them (`forms`), and answered as a form; updates that come with no
prompt open make a follow-up turn, ended by the closing `usage_update` (or 8
seconds of quiet). `ask_question` and `invoke_subagent` show as Claude Code's
question and subagent cards. Sign-in uses ACP `authenticate`. The notes folder, the project's docs and
its memory are passed with `--add-dir`. Short jobs (models, commands,
listing commands) share `acp_sync.rs` with Cursor. Agents are
one ordered list, `agent::ALL` and `src/lib/agents.ts`: Claude Code, Codex,
Antigravity, Cursor. **Tested live** with the self-test on Gemini 3.8 Flash: a
command, a read and a write approved; a plan shown in the panel, approved and
built in Default mode; the context meter; titles; resume after a restart
(history and instructions kept); the worktree card through
`mcp__harness_worktree__ask`; a write to the chat's notes; a question card
answered; a subagent whose result came after the turn and was kept. A Cursor
run (command, plan approved and built) still passed. ACP `authenticate` on a
signed-out data folder returned in seconds once the browser finished, with no
code to paste, and that folder then opened sessions. **Not tested live**: the
sign-in card's button itself (it runs that same call), and "Keep planning"
(unit-tested).

### Codex over app-server (built 2026-11-18, codex-cli 0.169.1)

Codex is the third agent (`agent/codex.rs`, id `codex`). It runs `codex
app-server`, the protocol OpenAI's own Codex apps use, rather than an ACP
adapter (Zed's moved onto the app-server anyway). The JSON-RPC client is
shared with ACP (`agent/rpc.rs`); Codex's messages leave out `"jsonrpc"` (it
also accepts them with it). One app-server per chat, ~140 MB idle.

**Probed live** (2026-11-18, scratch folder, `gpt-6-luna` and
`gpt-5.6-terra` at low effort, on a free ChatGPT plan):
- **Handshake**: `initialize` with `capabilities.experimentalApi: true` (plan
  mode and questions need it), then the `initialized` notification.
- **A turn**: `turn/start` returns at once; then `turn/started`,
  `item/started` / deltas / `item/completed` per item, `turn/completed`
  with `status` (`completed`, `interrupted`, `failed`). `agentMessage.phase`
  is `commentary` (between steps) or `final_answer`. Reasoning summaries
  were empty at low effort.
- **Commands** come as `/bin/zsh -lc '<command>'`; `commandActions[].command`
  is the plain one (a single `read` / `search` action has `path` / `query`).
  `aggregatedOutput` is null for no output. `/tmp` is writable in
  `workspace-write`.
- **Approvals**: a command outside the project sends
  `item/commandExecution/requestApproval` *before* its `item/started`, with
  `availableDecisions` = `accept`, `{acceptWithExecpolicyAmendment}` (a rule
  Codex keeps, like Cursor's allowlist), `cancel`; no `decline` or
  `acceptForSession` there. Always = the amendment ("Always allow
  `<command>`"); deny = `decline` if offered, else `cancel`, with the reason
  sent as a note.
- **MCP**: HTTP servers given per thread as config overrides
  (`mcp_servers.<name>.url`) work with `mcp_http`. Without more, every call
  asked first through `mcpServer/elicitation/request` (`_meta.codex_approval_kind:
  "mcp_tool_call"`); `mcp_servers.<name>.default_tools_approval_mode =
  "approve"` stops that. `mcpToolCall.result.content` carries the answer,
  so no served log is needed (unlike Cursor). The user's own Codex servers
  start too.
- **Writable folders**: `sandbox_workspace_write.writable_roots` (config) and
  `sandboxPolicy.writableRoots` (per turn) let the notes, docs and memory
  folders be written with no request.
- **Plan mode**: `turn/start.collaborationMode = {mode: "plan", settings:
  {model, reasoning_effort, developer_instructions: null}}`. The turn ends
  with a `plan` item (id `<turnId>-plan`) and no final message; there is no
  approval request, so the app shows the plan card itself at `turn/completed`
  and, on approve, starts a default-mode turn ("Build it now") that looks
  like the same turn. Questions come as `item/tool/requestUserInput` and are
  answered by question `id` (`gpt-6-luna` once sent a malformed question
  call; `gpt-5.6-terra` asked properly).
- **Plans outside plan mode** (tested 2026-11-19, codex-cli 0.169.1): only
  the app sets the collaboration mode; Codex's plan-mode prompt says the mode
  never changes by user intent, and there is no request or notice for the
  agent to switch. Plan mode tells it to wrap its plan in `<proposed_plan>`,
  which the app server makes the `plan` item. Asked to "make a plan first" in
  default mode, it once planned in the chat and waited, and once planned and
  built in the same turn. Told by the app's instructions to use the block, it
  wrote one and ended the turn; outside plan mode the block arrives as plain
  `agentMessage` text. The app now tells it so (`PLAN_INSTRUCTION` in
  `codex.rs`), cuts the block out of the message and shows the plan card at
  turn end, dropped if a tool call follows it. Approving builds in the mode
  the chat is in.
- **Mid-turn messages**: `turn/steer` takes them; the model read one sent
  during a `sleep 8`. If steering fails the message waits for the next turn.
- **Resume**: `thread/resume {threadId, cwd}` keeps history and works in
  another folder (worktrees). The sandbox must be sent again: left out, it
  came back read-only.
- **Usage**: `account/rateLimits/read` / `…/updated` give `primary` and
  `secondary` windows (`usedPercent`, `windowDurationMins`, `resetsAt`); the
  free plan has one 30-day window. `thread/tokenUsage/updated.last.totalTokens`
  over `modelContextWindow` is the context ring (Codex's base prompt alone is
  ~16K tokens).

**How it maps** (`codex.rs`):
- Modes are Codex's own: `default` (on-request, workspace-write), `plan`,
  `read-only`, `full-access` (never ask, no sandbox); sent with every
  `turn/start` as `approvalPolicy`, `sandboxPolicy` and `collaborationMode`.
- Instructions (tools, notes and docs, memory, preferences) go in
  `developerInstructions` on every start and resume, like Claude's system
  prompt; nothing is added to the first message.
- Items become Claude-shaped tool calls: `Bash`, `Read`, `Grep`, `Write` /
  `Edit` per changed file (`<itemId>#n` after the first), `WebSearch`,
  `mcp__<server>__<tool>`, `Agent` for `spawnAgent`, `TodoWrite` for
  `turn/plan/updated`.
- Subagents: a `spawnAgent` call's `receiverThreadIds` become tasks; a child
  thread's items carry the call as `parent_tool_id`; its `turn/completed` or
  `agentsStates` end the task. Not seen live yet.
- `/compact` alone runs `thread/compact/start`.
- Titles and batch labels: a throwaway `ephemeral` thread on the first model
  Codex calls fast or affordable, at its lowest effort, with the title
  prompt as `baseInstructions` and the user's MCP servers off.
- Sign-in: `account/read` with no account → `SignInNeeded`; the card runs
  `codex login`. Settings → Usage lists Codex's windows by length.
- Found at `HARNESS_CODEX_PATH`, Homebrew's `/opt/homebrew/bin` or
  `/usr/local/bin`, `~/.local/bin`, then the login shell.

**Tested live** in the app (self-test, `HARNESS_DATA_DIR` + `HARNESS_HOME`,
`gpt-6-luna`): shell and file edits, an approval card for a write outside
the project, the earlier-chats tool with no card, plan → approve → built in
one turn (it then asked for a worktree, as told), close and resume with
history, a Codex-written title, the context ring and usage reading. Then:
stop mid-command, two queued messages answered together, a worktree chosen
mid-turn (resumed in the worktree, file written there), a note written to
the shared memory folder with no card, switching Codex → Haiku → Codex
(each reread the chat), `/compact`, and the panel tool opening a doc.
Unit tests in `codex.rs` cover the mappings above.

### Chat info view (built 2026-11-16, branch `harness/chat-info-view-83b2a2`)

A fifth side panel view, for checking a long task is still going. The
top bar's icons now run chat info, docs, side chat, terminal, browser: this
chat's own things first, then the project's tools (agreed 2026-11-16).
Chat info's icon is a small tree with rounded elbows, a dot with two well-spaced dots under it ("Tree ·
nodes only" on the Magic Patterns canvas "Chat Info"; picked over a bullet
list, which looked too much like docs beside it, and the earlier
"hand-off"). It is close in shape to the branch mark; watch for mix-ups.
Its row in the top bar reads "chat info   run session tests" (changed
2026-11-16): the overview, then the name of the subagent picked last
(`ChatInfo.picked`, kept while the overview shows), no mark and no ▾ (the
user found both noisy). Like a tab, clicking the name from the overview goes
straight to that agent; clicking it while on the agent opens an in-app menu
of every subagent, grouped by batch as on the page, each with its mark;
picking one shows it full size in the panel.
"chat info" comes back to the overview. Before any is picked the row reads
"subagents", and clicking it opens the menu. The menu is laid
against the whole row so it stays inside a narrow panel. The page groups
subagents by batch, newest batch first (what chat info is for: what's going
on now), each in launch order under a faint line saying what the batch is
doing. That line is written by Haiku (2026-11-16): 1.5 s after a live batch's
last subagent starts, the window asks once (`title_batch`) with the user's
message and each subagent's short task; the label comes back as a saved
`batch_titled` event, so a reopened chat keeps it. Until it comes, and for
Cursor or older chats, the line is the first line of the user's message it
answers (also its hover text). A batch Claude Code started on its own (a turn
after a background agent ended) gets "· follow-up" after it.
History: a tab per agent with names (cluttered); the agent shown in the
page's place; boxes unfolding in place on the page (option F), dropped
because a 16-line box was too small to read; then a mark per agent in the
row (one click to switch), dropped because naming a mark took a hover label
that broke the terminal feel and the marks piled up in long chats.

- **The page** (`InfoView.svelte`). Sections, each
  left out when empty: context (bar, "92k of 200k · 46%", then what takes
  the room), tasks (✓ done, ● doing, ○ to do), subagents (every one: a
  running one shows its name and run time, then its latest step and how long
  ago; a failed one its error; a finished one only its name and how many
  steps it took, like the CLI's "Done (12 tool uses …)", since a frozen,
  unlabelled run time read like a stuck timer and its last step was just what
  it did before answering; its run time is in its full view, "done after …"), and
  in the background (commands, not agents). A running agent with no step for
  3 minutes shows "… ago" in the accent colour.
- **A subagent, full size** (`AgentView.svelte`). Clicking its row on the
  page (marked ● running, ✓ done, ✗ failed or – stopped), or picking it in
  the top bar's ▾ menu, shows it in the panel: its mark and name, how it is doing, the
  prompt (three lines, all on a click), its steps folded like the chat's
  (`WorkRun`), the text it wrote, and its answer (`←`), unless that repeats
  its last text. It stays on the newest step unless scrolled up. The agent on
  show (`ChatInfo.shown`) lives on the `Chat`, so it stays when leaving the
  chat but not after a restart. Cmd+Shift+[ / ] steps through the overview
  and each agent while chat info has the keys.
- A subagent's line in an opened work run in the chat shows chat info on
  that agent, full size.

State lives in `state/chatInfo.svelte.ts` (`ChatInfo`, one per chat, fed by
`Chat.handle` before its own switch; events with a `parent_tool_id` stop
there and never reach `items`). Clock times are taken only for live events,
so a reopened chat shows run times from `TaskEnded.duration_ms` and no "ago".

**What Claude Code sends** (probed live on CLI 2.1.325 with `--forward-subagent-text`):

- Most subagents now run in the background, even ones not asked to: the
  `Agent` call's result comes at once ("Async agent launched successfully"),
  and only `task_notification` says when it's done. Their steps still carry
  `parent_tool_use_id`, also after the main turn's `result`. When one ends,
  Claude Code starts a turn on its own.
- A subagent hands its report back by calling `SubagentHandback`
  (`input.message`). The task's `summary` and the `Agent` result then only
  point at it, so the handback's message is the answer and the call is not
  shown as a step. A stopped task's summary is just its description again.
- After the user presses stop, a cut-short `Agent` call returns an error
  ("The user doesn't want to proceed…"); with an `interrupted` before it,
  the agent is "stopped", not "failed".
- `--forward-subagent-text` adds subagents' text and thinking. Their thinking
  is dropped in `translate`.
- The task list now uses `TaskCreate` (`subject`; the result says "Task #3
  created") and `TaskUpdate` (`taskId`, `status`, `deleted` removes). Older
  `TodoWrite` (whole list each time) is still read.
- `get_context_usage` with `detail: "summary"` answers in about 20 ms. The
  app asks after each `result` (main chats only) with request ID
  `harness-context-usage`; an error answer is ignored. Only categories of
  kind `used` with tokens are kept, largest first.

**What Cursor sends** (probed live, CLI 2026.11.13, with
`clientCapabilities._meta.subagents: true`, now always sent):

- On the chat's session: `subagent_spawned` (`subagentSessionId`, `name`,
  `task`, `_meta.cursor.toolCallId`), sent just before the tool call itself
  (`rawInput._toolName: "task"`, `description`, `prompt`; shown as `Agent`),
  then `subagent_state_update` (`state`: `completed`, `failed`, `cancelled`,
  `disconnected`).
- The subagent's own updates arrive on `subagentSessionId`: thinking, tool
  calls and text, like a chat's. `acp.rs` keeps a `Child` per running
  subagent and tags its tool calls and text with the parent tool call; text
  is collected and sent as one block; thinking is dropped.
- A `cursor/task` request follows the finished tool call; it is answered
  with `{}`.
- Cursor's todo updates (`updateTodos`) carry only the changed items, with
  statuses like `TODO_STATUS_COMPLETED`; they are merged by `id`.
- Cursor reports no context use, so its chats have no context section.

Self-test: `@info` (the overview), `@agent <n>` (the nth subagent full size), `@agents`
(wait until none runs), `@model claude:<name>`; the report's `info` part.
Preview: `#open=agents&info` (a chat with a finished, a failed and a live
subagent; add `&agent` to open the first one).

Not covered: subagents started by subagents (one step inside their parent),
agents inside multi-agent workflows, stopping one subagent from the view,
Gemini and Codex (Codex shows subagents only over its app-server protocol;
Gemini reports only start and end).

### Skills (`skills.rs`, built 2026-11-16)

Harness's own skills, which every chat can use whatever its agent. Claude Code's
and Cursor's own skills keep working beside them; the user deletes doubles
from those by hand.

- **Storage**: `<data dir>/skills/<name>/SKILL.md` plus the skill's other
  files. Imports are copies; nothing in an agent's own folders is touched.
- **Import** (Settings → Skills, "import folder" / "import file"): a folder
  with SKILL.md at its top, a `.zip` (the shallowest SKILL.md's folder is the
  skill; two at that depth is an error), or a lone `.md` (becomes SKILL.md).
  Skipped: symlinks, `.DS_Store`, `.git`, `__MACOSX`, zip paths leaving the
  folder. 50 MB cap, counted as written. Unpacked into
  `skills/.importing-<id>` and moved into place, so a failed import leaves
  nothing. Name: the header's `name`, else the folder, zip or file name;
  made lowercase `[a-z0-9-]`, up to 64 chars. A description is required. Same
  name replaces the old copy (how a skill is updated). Remove asks yes/no.
- **Header**: read by a small parser (`parse_header`), not a YAML library:
  plain, quoted and block (`>`, `>-`, `|`) values for `name` and `description`.
- **The tool**: an app tool group `harness_skills` with one tool, `skill`
  (`name`, optional `file`). Without `file` it returns the skill's folder, its
  other files and SKILL.md; with `file`, that file as text (256 KB cap, must
  stay inside the skill). So no agent needs access to the folder. The group is
  left out when there are no skills.
- **The list**: each skill's name and description (cut at 300 chars) goes in
  the tool's description and in the instructions. The instructions matter for
  Claude Code, whose app tools are deferred (it only sees descriptions after
  ToolSearch); Cursor gets instructions only in a fresh session, but sees the
  tool description. Both are read when a chat's process starts, so a running
  chat sees new skills after its next start.
- **Typed `/name`**: `send_inner` swaps a shown message `/name request` for the
  skill's text plus the request before it goes to the agent, when `name` is an
  imported skill. The chat and history keep what was typed. Anything else
  (`/compact`, Claude's own skills) goes through as before.
- Not covered: editing skills in the app, turning them on or off per project
  or agent, side chats (they get no tools).

### Earlier chats (`chats.rs`, built 2026-11-17)

A new chat can pick up a piece of earlier work without the user explaining
it again: the agent finds the earlier chat in this project and reads it. It
is the same for every agent, and a Cursor chat can read a Claude chat. The
research and the tests behind each choice are in "ADE chat search and
memory.md", in this project's docs folder in the home folder
(`~/.harness/projects/-Users-tailaiying-tailai-harness/docs/`), not in the
repo.

- **The tools** are a fourth app tool group, `harness_chats`, in every main
  chat (side chats get none), made in `lib.rs` beside the browser's:
  - `search {query, chat?}`: the project's chats that match, best first, 8
    at most and a count of the rest. With `chat`, the matching turns of that
    one chat.
  - `recent {}`: the 15 latest chats.
  - `read {id, upto_turn?, doc?}`: one chat (below), or one of its docs.
  Same project only (`projects.path` = the main folder the window names);
  the trash and the asking chat are left out. An ID is given by its first 8
  characters; `find_chat` takes any start that names one chat.
- **What a read gives** (`chats::render`): each turn's user message and the
  agent's text, each tool call as one line (`tool_line`: the tool and the
  one thing that says what it was for, ` [FAILED]` when its result was an
  error), plans (the edited text when there is one) with how they were
  answered, answered questions, and marks for a stop, an error and a merged
  branch. The header names the chat's docs (markdown files it wrote, not
  its notes or an agent's plan file), and `doc` reads one. Left out: tool
  output, thinking, a subagent's own steps, messages taken back.
  **Why this shape** (tested on real chats, with Claude and GPT reading):
  tool output is about 90% of a saved chat and little of its meaning, so
  the rest is small (most chats under 10,000 tokens). The one-line tool
  calls stay because a tried-and-dropped attempt shows there and often
  nowhere else: without them GPT agreed with a wrong premise 4 times of 4.
  An outline or only the last turns missed decisions.
- **A long chat** comes in parts: the newest turns that fit 60,000
  characters, and the header says how to get earlier ones (`upto_turn`). A
  single turn over that has its middle cut.
- **The search index** is `chat_text`, an FTS5 table (porter stemming; the
  bundled SQLite has FTS5): one row per user message, per piece of the
  agent's own text and per title. `append_event` and the three title writes
  keep it current; a message taken back and a purged chat leave it. It is
  built once from saved history when a build with it first opens the store
  (`build_chat_index`, marked by the `chat_index` setting): 0.1 s and 3 MB
  on 98 chats. **Ranking**: by how many of the query's words a chat holds,
  a word in the title counting three times; then by its three best
  messages; then by last use. Summing every matching message was tried
  first and put long chats that mention common words often above the chat
  named for the thing.
- **Telling the agent.** When to look is said in three places, because each
  agent hears a different one late in a chat:
  - `chats::INSTRUCTION`, with the other app tools' lines. Claude Code gets
    it in its system prompt on every launch; Cursor only when a conversation
    starts. It is the same on every launch.
  - The tool descriptions, which Cursor sees on every turn. (Claude Code
    loads app tools late, with ToolSearch.) In a test with ten unrelated
    turns first, Cursor's Composer missed a reference with the guidance
    only in the first message and found it with it in the descriptions.
  - A new chat's first message ends with the project's 15 latest chats
    (`chats::titles_note`, added in `send_inner` when the chat is created),
    so a passing mention is easy to spot. Not in the instructions: those
    are sent again on every start, and a list that changes would make a
    resumed chat pay for its history anew. After the user's words, not
    before, for Cursor's titles (next item).
- **Cursor's first prompt** now has the user's message first and the app's
  intro last (`acp.rs`). Cursor titles a chat from the start of its first
  prompt, and with the intro first chats were being titled "Harness Agent"
  and "Panel Tools Guide".
- **Cursor's tool calls are saved fuller** (`acp.rs`):
  - A call is shown when it starts, often before Cursor has sent its input,
    and nothing later was saved (`Grep {"pattern": null}`). Now a new saved
    event, `tool_input {tool_id, name, input, parent_tool_id}`, follows when
    a started call's name or input changed (`refresh_tool`); the window and
    `render` put it in place of the first.
  - Cursor reports an app tool's call as only `{"success": true}`. The app
    keeps its own answers (`recording`, `take_served`: matched by tool and
    input) and saves the answer as the call's result.
- **In the chat** (`tools.ts`, `WorkRun.svelte`): the calls sit in the
  folded run like any tool, worded "searched chats for “…”", "read chat
  “Title”" (the title is kept from the read's first line). Opened, a search
  lists the chats it found (`chatLines` reads the tool's own lines, so
  `chat_line` in `chats.rs` and it must agree): a title opens that chat,
  and "use this" sends the agent "Use the chat “Title” (id …) instead.", an
  ordinary message, so mid-work it is taken in at the next step.
- **@ in the message box** (`Composer.svelte`): typed at the start of a
  word, it lists the project's other chats, narrowed by what follows; up,
  down, Enter or Tab, Escape. A picked chat is kept like a dropped file
  (`Chat.addDraftChat`): the word `[@Title]` with the path `chat:<id>`, so
  it is one thing in the box, comes back with a message taken back, is sent
  as `[@Title](<chat:ID>)` (which the agent is told to read by that ID),
  and a click opens the chat (`Chat.onOpenChat`, set by `AppState`).
- **Tested**: Rust tests (render, tool lines, index, ranking, IDs, the
  first-message note, the Cursor fixes); the preview page (the lines, the
  opened search, @: list, keys, pick, send, click); and a test copy of the
  app on a copy of the real chats, with Claude Code, GPT and Composer: a
  passing mention ("same deal as the panel tabs") was looked up, "my last
  chat" read the newest, a question that stood alone looked nothing up, and
  a Claude chat read a Cursor chat.
- **To keep an eye on: an agent reading many chats for one message.**
  Nothing limits how many chats an agent reads in a row; one read is at most
  about 15,000 tokens, a search lists 8 chats, and what is read stays in the
  chat for its later turns. In testing agents sometimes read four or five
  where one would have done (20,000 to 40,000 tokens). It would matter most
  on a model with a small context window, in a project with many chats on
  one topic, and near a usage limit. No limit was added (2026-11-17, the
  user's call): some requests need many chats ("what's still open across
  this week's chats"), and the chat's line shows every chat read. If it
  becomes a problem, first tell the agent in the `read` description to
  start with the best match and read more only when that one doesn't
  answer; a hard limit only after that.
- **Not covered:** search by meaning (the agent retries wordings instead),
  shared memory across agents, tool output from an old chat, chats of other
  projects. Not tested yet: a chat long enough to come in parts in real
  use, and a chat whose context was squeezed.

### Keep awake (`keep_awake.rs`, built 2026-11-17)

For stepping away (lunch, a shower) with the lid open while chats keep
working. One for the whole app: sleep is machine-wide.

- **Starts** from a cup icon in the top strip, in one row with the sidebar
  toggle, just before it (with the sidebar put away, the row flips: toggle
  first; the top bar leaves room for both). The cup has three lines of steam,
  the middle one longest; it is drawn half a pixel high because its weight
  sits low under the thin steam. Steam above, no steam, a sun eye and a plain
  eye were tried; the user kept the cup (it means "caffeinate"). A text line in the sidebar's
  foot looked messy (user, 2026-11-17); an icon in the foot row was also
  tried. It can also start from any chat's
  agent through the `harness_keep_awake` tools (`keep_awake` with optional
  `until` "HH:MM", `stop_keep_awake`). Their descriptions point agents there
  instead of running `caffeinate`; running it directly isn't blocked.
- **Holds** idle system sleep only, so the screen still sleeps and locks.
  Mac: `caffeinate -i -w <app pid>`, killed when it ends and gone even if the
  app crashes. Windows: `SetThreadExecutionState` on its own thread
  (untested). Lid close and the power button still sleep the Mac.
- **Ends** (checked every 15 s in the core, since a hidden window's timers
  run late) at the first of:
  - **back**: once there has been no keyboard or mouse input for 2 min (the
    user left; their display sleeps at 2 min), the next input ends it. Input
    before that, like the click that started it, doesn't.
  - **done**: no chat working for 1 min, once one has worked since it
    started. Working = starting/working, or any background work running
    (commands like a test run, and subagents). Waiting for approval doesn't
    count (staying awake can't move it). A command that never ends (a dev
    server) holds it until the limit: the limit is the safety net, and wins.
    (Changed 2026-11-17: background commands used to be left out, so a chat
    whose turn ended while its tests ran in the background let the Mac sleep.)
    Side chats don't count. With nothing ever working, only back or the limit
    end it ("start something, then leave").
  - **limit**: 2 h after starting; overnight: its clock time (Settings,
    6:00 to 10:00, default 8:00), and unplugging.
- **While on** the cup is lit with the time beside it, `1h 52m` / `till 8:00`
  (with the sidebar put away, the cup alone; the time is on hover). Clicking
  it opens, downward, `stop` and `overnight until 8:00` (`overnight · plug in first` on
  battery). Overnight first shows a check: `stays awake until 8:00 at most`
  and, in yellow, the chats waiting on the user. Switching to overnight keeps
  the hold and what has been seen so far. No message when it ends.
- **Tested** live with a test copy (2026-11-17): Haiku called the tool for
  "caffeinate the session"; the hold appeared (`PreventUserIdleSystemSleep`
  only) and ended 10 s (test setting) after the turn; with a 5 s away time
  the user's own input ended it; `kill -9` of the app left no `caffeinate`.
  Not tested: overnight end and unplugging live, Windows.
- `HARNESS_KEEP_AWAKE_{LIMIT,AWAY,QUIET,TICK}_SECONDS` override the times.
- Not covered: coming back after a restart.

### Showing things in the panel (`panel.rs`, built 2026-11-17)

"Pull up the to-do list" used to need a reply link and a click: a file only
opened by itself when the agent wrote or edited it.

- **The tool**: an app tool group, `harness_panel`, in every chat, with one
  tool, `show`. It takes `path` (a markdown file, full path) or `view`
  (`notes`, `plan`, `browser`, `terminal`, `info`). It opens the panel if
  hidden and picks that tab or view. Its instruction line tells the agent to
  use it when the user asks to see, show, open or pull up something.
- **Checks in the core** first, so the agent hears why: the same rule as a
  link (absolute `.md`/`.markdown`, exists, not too big). Other files are
  refused; the agent links them instead.
- **The event** `show_panel` goes to the chat; a side chat's opens in its
  chat's panel. Not saved, so a reopened chat doesn't reopen anything.
- **Why a tool, not a guess**: opening every file the agent reads would
  fill the panel with files read only to understand the code.
- Tried by the user in a test copy (2026-11-17): works.

### Agents open terminal tabs (`harness-tab`, built 2026-11-17)

To work together through the terminal: the agent runs a command in a new
tab that the user can watch and type into (a dev server, a sign-in prompt,
"run it in the terminal"). The agent still can't type into the user's tabs.

- **A shell command, not an app tool.** Harness writes
  `<home>/bin/harness-tab` at start (`Home::write_tab_command`; `.cmd` on
  Windows, untested). The agent runs it with its own shell tool, so the
  agent's own checks decide whether to ask. Harness adds no risk rules (the
  user's call: that belongs to the model and mode). App tools are allowed
  without a card, so a tool here would have skipped those checks.
- **Approvals, tested 2026-11-17** with a stand-in in a test copy: Claude
  Code's auto mode treated wrapped and plain commands alike. Cursor (agent
  mode, auto-review) asked about risky ones both ways and asks once for any
  `harness-tab` call, since it isn't on its allowed list. Deny means no tab.
- **The link**: each main chat's process gets `HARNESS_TAB_URL` (a token on
  the app's local server, `mcp_http::open_tab`, dropped with the process).
  The script posts its arguments as one command line with curl; the reply
  is plain text. Side chats get none.
- **The tab** opens in the chat's folder (its worktree, if any) through
  `Terminals::run_for_agent`. The command is typed in once the shell has
  printed something (up to 3 s), so it shows once. One line only. The chat's
  panel switches to that tab (`show_panel` with `tab`) without taking the
  keys. The agent is told the tab's ID, since the name changes to what runs.
- **Tested** live (2026-11-17) with Claude Code and Cursor: tab opened, the
  panel showed it, the agent read the server's line, and a denied Cursor
  call opened nothing.
- Not covered: typing into existing tabs; Windows untried.

### Switching agents in one chat (built 2026-11-17)

Any installed agent's model can be picked in the model picker at any point
in a chat. The next message goes to that agent, which starts fresh and
rereads the chat. Uses: plan with a big model and build with a small one,
or carry on when one agent's usage runs out.

- **One-way handoff.** Only the new agent's context matters. The old
  agent's session is left behind, never synced or resumed; switching back is
  a fresh handoff like any other.
- **The handoff is a reread.** No note from the old agent, so it works when
  that agent is out of usage. The new agent's first message carries a
  pending note (`handoff_note` in `sessions.rs`) telling it to read this
  whole chat with the `harness_chats` read tool by the chat's full ID, every
  part. The cost of the reread is the user's to accept by switching.
- **Between turns only.** While a turn runs, other agents' models are greyed
  out (`locked` in `Composer.svelte`); `switch_agent` also refuses a busy
  chat. Mid-turn switching may come later.
- **The new agent's own defaults.** Mode is the agent's default
  (`defaultMode` in `models.ts`: Cursor "agent", Claude Code the user's usual
  mode, never plan); effort is "default". Nothing is carried over.
- **The flow.** Picking another agent in a started chat only changes the
  window's `chat.agent`; `chat.ranOn` keeps the saved agent, and
  `chat.switching` is true while they differ. Mode, model and effort picks
  then aren't sent to the old process. Picking the original agent back
  restores what the chat had (`held`). On send, `send_inner` sees the saved
  agent differ from the one sent and calls `switch_agent`: stop the old
  process and wait for it, `set_agent`, `clear_agent_session_id` (so the new
  one doesn't resume), effort to default, add the handoff note, record
  `AgentSwitched { from, to, model }`. The send then starts the new agent as
  usual.
- **The line.** `AgentSwitched` is saved and shows as a quiet note,
  "→ Cursor · Composer 2.5", above the message that went to the new agent.
  The window draws it as it sends (and takes it back if the send fails); a
  replay draws it from the event. The chat lookup read shows it as "(the
  chat moved from X to Y here)" at the top of that turn.
- **Reading this chat.** `find_chat` now returns the asking chat for its
  whole ID, or for a prefix that matches nothing else; `search` and `recent`
  still leave it out.
- Not covered: switching mid-turn; side chats are unchanged; the old agent's
  background commands end at the switch (as when a chat closes). Codex
  joined with its adapter (see "Codex over app-server").

### If the approval channel breaks

`--permission-prompt-tool stdio` and the `control_request` /
`control_response` messages are what the Agent SDK uses, but they are not
documented for other hosts. If an update breaks them, the documented fallback
is a `PreToolUse` hook of type `http` pointing at a small server inside the
core. The hook returns `permissionDecision` of `allow`, `deny` or `ask`.
Whether localhost URLs are allowed there is not stated in the docs; check
before building it. Only `claude_code.rs` would change.

### Order of work

1. **Setup.** Install Rust, scaffold Tauri + Svelte, app window opens.
2. **Core spike.** `locate.rs`, `claude_code.rs`, `events.rs`. A bare page
   with a text box that sends one message and prints streamed text. Done when
   a two-message chat works in the window.
3. **Chat view.** Markdown, code colours, thinking, tool call rows, streaming
   rules above.
4. **Approvals, stop, modes.** Approval cards, question cards, stop button,
   mode switch. Settle the three **Verify** items here: always allow, question
   answers, mode after plan approval.
5. **Store and sidebar.** SQLite, projects, chat list, reload history, resume
   after exit, idle timer. Done when quitting and reopening restores
   everything and a chat resumes after its process has exited. **Done.**

   Measured after this step (release build, window hidden, same Mac; "memory"
   is what Activity Monitor shows, `ps` resident memory in brackets):
   - App alone with a short chat: 105 to 160 MB (160 to 190 MB).
   - App alone after reopening a 400-turn chat (3,604 saved events, 2,002
     items): 167 MB (518 MB), opened in 0.3 seconds. Before only the latest
     part was drawn it was 243 MB (605 MB).
   - That is over the 150 MB target for a very long chat. The next step, if
     it matters in real use: load only the last turns from the store and
     fetch earlier ones on scroll, instead of replaying the whole history.
6. **Side panel.** Plans and markdown documents. **Built**; checked in the
   browser preview at 1280 and 1000 px wide, and **tested** live with the
   self-test (a debug build, 2026-11-13): a plan-mode chat was approved with
   auto-accept, wrote `NOTES.md`, then edited it; the report's `panel` showed
   the plan answered, one document tab, and its text equal to the file on
   disk. Not tested live: Keep planning with feedback, and the look in the
   real window by eye.
7. **Measure.** Memory of the app alone with a long chat open; memory per live
   chat with the user's real app connections; confirm idle chats hold none.
   Target for the app alone: under 150 MB. Fix what misses.

   Baseline to beat, measured on this Mac on 2026-11-13 with the Claude
   desktop app mid-answer in one long chat (`ps` resident memory; `top`
   reports more):
   - Desktop app's own processes: about 1.7 GB in total. The window drawing
     the chat was 850 MB to 1 GB of that.
   - Its four Claude Code session processes: about 700 MB in total (87 to
     264 MB each). They are the same native `claude` program this app runs.
   - CPU while working: window process 20 to 45%, its helper 12 to 24%, the
     Claude Code processes about 5% or less.

   **Early checkpoint:** take the same three readings at the end of step 2
   (core spike) and again after step 3 with a long chat. If the app alone is
   not clearly lighter than the baseline by then, stop and rethink before
   building the rest.

   **Checkpoint after step 2 (2026-11-13, release build, same Mac):**
   - App alone (core + web view processes): 136 to 157 MB resident, 94 to
     112 MB as `top` counts it, after chats of 3 short turns and of 3 long
     streamed answers (about 14,000 characters shown).
   - CPU for the app during 88 seconds of streaming: 5% average, 33% peak
     (the peak includes app launch).
   - Claude Code process: 209 to 274 MB, as expected.
   - App bundle size: 4.1 MB.
   - Not yet comparable on three counts: text is plain (no markdown or code
     colours), the chat is short next to a day-long one, and the window may
     have been in the background, where the system draws less. Repeat after
     step 3.

   **Checkpoint after step 3 (2026-11-13, formatted text and code colours):**
   - App alone: 103 to 144 MB resident, 88 to 114 MB as `top` counts it,
     including after three long formatted answers streamed over 96 seconds.
   - CPU for the app during that stream: 3.8% average, 26% peak.
   - Formatting did not move the numbers in any way that shows. The window
     may again have been in the background; repeat once with it in front.
   - The UI bundle is 260 kB (97 kB compressed), mostly markdown-it and
     highlight.js with a fixed set of seventeen languages.

   How to repeat: the app has a self-test, turned on by environment
   variables (see `autotest` in `src-tauri/src/lib.rs`). It sends scripted
   messages through the real window, allows every approval, and writes what
   the window showed to a file. Messages starting with `@` steer the test
   instead of being sent: `@model <name>`, `@mode <mode>`,
   `@answer <allow|always_allow|deny|none>` (how approvals are answered from
   then on; `none` leaves them waiting), `@stop <ms>` (press stop that long
   into the next message), `@background` (don't wait for the next message's
   answer), `@new`, `@open <n>` and `@delete <n>` (the nth chat in the
   sidebar, from 0), `@close` (shut the open chat's process down), `@exited`
   (wait for it to exit), `@wait <ms>`, `@image <word>` (attach a picture
   of that word to the next message, through the same code a paste uses) and
   `@file <path>` (add that file to the next message, as a drop would). Set `HARNESS_DATA_DIR` so a test
   never touches the real saved chats, and `HARNESS_IDLE_SECONDS` to test idle
   shutdown quickly. With the window hidden or the screen locked the page is
   frozen while nothing happens, so `@wait` never ends there: wait on an event
   instead. The report's `shown` leaves out blocks that aren't laid out;
   `text` has them all. Start the built binary with
   `HARNESS_AUTOTEST_FOLDER`, `HARNESS_AUTOTEST_MESSAGES` (JSON array) and
   `HARNESS_AUTOTEST_REPORT` set, and read memory with `ps` and `top`. WebKit's
   helper processes (`com.apple.WebKit.WebContent`, `.GPU`, `.Networking`) are
   not children of the app; count the ones that appear after launch.
8. **Windows pass.** Build and run on the Windows laptop. Check binary
   lookup, no console window, paths, that Git for Windows is present (the
   Bash tool needs it), and that Ctrl+N doesn't open a browser window in
   WebView2.

### Verify list

- Done: `--resume` combined with the streaming flags. The agent's session ID
  stays the same across a resume.
- Done: always allow via `updatedPermissions`, the answer format for
  `AskUserQuestion`, and the mode after an approved plan. See "Messages the
  app sends".
- That closing stdin waits for background work before exit.
- Done: setting the model mid-chat. The request is
  `{"type":"control_request","request_id":…,"request":{"subtype":"set_model","model":"sonnet"}}`.
  Aliases, full names and `"default"` work; the reply carries the resolved
  model, or `subtype: "error"` with `error` text for an unknown name.
- Everything on Windows.
