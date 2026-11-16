# Harness v1 plan

Added 2026-11-15 (branch `harness/cursor-acp-217ae2`): chats can run on
Cursor's agent as well as Claude Code, picked from the model menu before the
first message. See "Cursor over ACP".

Added 2026-11-16: Harness's own skills, imported in Settings and offered to
every chat whatever its agent. See "Skills".

Added 2026-11-16 (branch `harness/chat-info-view-83b2a2`): a fifth side
panel view, chat info: context use, the task list, the chat's subagents
(each opened as a tab with its steps) and background commands, for Claude
Code and Cursor. See "Chat info view".

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
offers the chat's worktree. Claude can read a tab (not type into it). A tab
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
- Agents other than Claude Code and Cursor (Codex, Gemini). Most ACP agents
  would reuse `agent/acp.rs`.
- In Cursor chats: side chat, worktrees (loading a session in another folder
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
    side_chat.rs       SideChats: read-only side chats, several per chat as tabs (/btw), never saved
    store.rs           SQLite schema, reads, writes
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
devices since the reset isn't known). Cursor's usage (Auto and picked models are
counted apart) is on Cursor's servers but not sent over ACP or printed by
any command; reaching it would mean using the user's Cursor login, so it is
left out.

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

- **The request.** Every launch adds a line to Claude's instructions (call
  Claude Code's `EnterWorktree` before starting real changes in a repo; not
  for tiny edits, questions, reviews or plans, nor once in a worktree) and
  `--settings {"permissions":{"ask":["EnterWorktree"]}}`. Without that rule
  the tool runs without asking and makes its own worktree under
  `.claude/worktrees/` (tested on CLI 2.1.322); with it, the call reaches the
  approval channel in every mode. The session manager catches it
  (`screen_worktree_call`) and shows `WorktreeAsk`, with the name Claude gave
  the work. The call waits in Claude Code, which has no timeout. A call from
  a chat already in a worktree, or outside a git repo, is turned down at once
  with the reason. An earlier version asked before the first file edit (a
  PreToolUse hook); it was dropped 2026-11-14 because edits made with shell
  commands slipped past it, and the user wanted Claude to judge which work
  deserves a worktree.
- **"Work here"**: the call is turned down with "carry on here"; Claude may
  ask again later for other work. **"New worktree"**: the call is turned
  down (Harness makes its own worktree, not Claude Code's), the turn
  interrupted, the process stopped; `worktree::create` makes
  `harness/<Claude's name for the work>-<6 hex>` at
  `<data>/worktrees/<repo>-<6 hex>` from the latest commit of the branch the
  main folder was on when the chat began (`start_branch`, saved on the first
  message); then the chat resumes there (same session ID: `--resume` works
  from any folder) with a hidden note telling Claude where it is.
- **A message instead of a pick** (2026-11-14): the question closes as
  "skipped" with nothing saved, and the call is turned down with "read the
  message; ask again before any file changes if the work still needs a
  worktree". Before, the message queued behind the open call, and a later
  "New worktree" stopped the process and lost it.
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
- **Pull requests** via `gh pr list --head <branch> --state all --json
  number,state,isDraft,url,headRefOid` (`github.rs`), after a turn (at most
  once a minute per chat) and in the sweep (at most every 10 minutes). A repo
  `gh` can't answer for isn't asked again until restart.
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
side_send(side_id, project_path, model, effort, text, message_id)   ask a side chat (`<chat id>#side-<tab>`); starts it as a copy
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
  read as a fourth one, 2026-11-15) or Cmd+B / Ctrl+B; saved
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
  zoomed the same (`Place::frame`). wry and WebKit drop fractions of a point
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
  checked by eye: the shrunk page in the panel (the display was off).
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
lists stay in order. Tested with key presses in the editor. Saved 400 ms after typing stops and when editing
ends (`write_notes`), read lazily the first time the tab shows (`read_notes`).
- Stored in the app's data folder, `notes/<chat id>/notes.md` (`notes.rs`),
  never in the repo; written to a side file then moved into place; 1 MB cap.
  Purged with the chat when the trash is emptied; swept at start for chats
  that don't exist (a new chat never sent), like images.
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
on now), each in launch order under a faint line with the first line of the
user's message it answers. A batch Claude Code started on its own (a turn
after a background agent ended) gets the latest message plus "· follow-up".
History: a tab per agent with names (cluttered); the agent shown in the
page's place; boxes unfolding in place on the page (option F), dropped
because a 16-line box was too small to read; then a mark per agent in the
row (one click to switch), dropped because naming a mark took a hover label
that broke the terminal feel and the marks piled up in long chats.

- **The page** (`InfoView.svelte`). Sections, each
  left out when empty: context (bar, "92k of 200k · 46%", then what takes
  the room), tasks (✓ done, ● doing, ○ to do), subagents (every one, running
  and finished: name and run time, then its last step and how long ago), and
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
