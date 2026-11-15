# Harness v1 plan

Status, 2026-11-13: steps 1 to 5 of "Order of work" are built. The app has a
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
live once (see "Side panel, as built" and step 6 of "Order of work"). Not built yet: showing subagent work. The settings page
holds only theme and size so far.

Added 2026-11-13 (quick wins): rename a chat (double-click its title), close a single side panel tab, Cmd+N for a new chat, a
"queued" mark on a message sent while Claude works, plans always opening in
the side panel, and a chat that stays at its end when opened.

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
"unfinished work" list in settings; revisit if work actually gets lost. New chats get a title
written by Haiku. **Tested** live with the self-test (Haiku, scratch repo):
Claude asking by itself for real work (a module with tests) and not for a
one-line fix, the move with resume, put away and restore with an uncommitted
file, a hand merge cleaned up at startup, the trash and its purge. Checked in the preview page: the card, the sidebar marks, the trash
list, the bottom bar. Not tested live: a real pull request going through its
states (the lookup command was run read-only against pookie). See "Worktrees".

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

- A skills editor, a screen for managing app connections, and
  browser use. Skills, instructions and connections you already have still
  work; there is just no UI for editing them yet.
- Codex, Cursor or other agents. The design leaves room; nothing is built.
- Chats made earlier in the desktop app or the terminal. Only chats started in
  this app appear in the sidebar.
- Images and file attachments in messages.
- Sharing the app with other people. It is for personal use only.
- On Windows, stopping a chat (delete, quit, a hung close) kills its process
  outright, so its background commands keep running. Fix in the Windows pass
  (for example with a job object).
- Showing which chats have background work, or how much memory each uses.

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
    store.rs           SQLite schema, reads, writes
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
    panel/SidePanel.svelte   the tabbed side panel: plan and documents
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
- **Subagent traffic** has a non-null `parent_tool_use_id`. In v1, nest it
  under the tool call with that ID, collapsed.
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
UsageUpdate    { window, utilization, resets_at }             not saved
Error          { message }
```

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
send_message(session_id, project_path, mode, model, text, message_id)
rename_session(session_id, title)
answer_approval(session_id, request_id, decision, message?, answers?)
interrupt(session_id)
set_mode(session_id, mode) / set_model(session_id, model)   saved; a live process is told
list_models()
close_session(session_id) ask the chat's process to exit (used by the self-test)
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
  - Light: background `#f7f5ee`, text `#3a3832`, bright `#191813`, dim
    `#8a877b`, faint `#b9b6aa`, rule `#e0ddd0`, accent `#a8691c`, ok `#52733f`,
    bad `#b0463a`.
- **Sidebar:** bare chat titles under each project's path, one status dot per
  chat (accent = waiting on the user, ok = working, hollow = idle). No preview
  line, no time.
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
The built-in terminal is accepted as a later feature; its position (bottom or
side panel) is still open. How to build the selectors so the choice costs
nothing at runtime:

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
- **Built-in terminal** (secondary, after v1). A real shell in the project
  folder, either along the bottom or as a tab in the side panel. Needs a
  pseudo-terminal in the core and a terminal renderer in the window (likely
  the `portable-pty` crate and xterm.js; check both before building). Load the
  renderer only when a terminal is first opened so it costs nothing otherwise.
  Open: bottom or side.

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
  opens it over the chat and side panel (never the sidebar); "close", Escape
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
  ("High") next to the model; Harness has no effort setting yet.
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
  of step. `/btw` answers "isn't available in this environment" headless;
  it is deferred and will need Harness to build it (a short-lived second
  process with a copy of the chat and no tools, shown in a floating view).
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
  - **A terminal later** (the user's note, 2026-11-13: decide where its
    selector lives). The tab strip holds what the chat produced; the right
    end of the bar, before `×`, is kept free for things that aren't the
    chat's, such as a terminal. Not decided.
- **Rendered markdown must be safe.** As built: markdown-it runs with raw HTML
  turned off and its default link checks, and highlight.js escapes code, so
  the output holds only tags the renderer itself produces. dompurify was
  dropped as unneeded on top of that; add it back if raw HTML is ever turned
  on. Links open in the default browser, never inside the app window.

### Preview page

`npm run dev`, then open `http://localhost:1420/preview.html`: the same window
in a plain browser, with a stand-in for the core (`src/preview.ts`) that has a
sidebar of sample chats, one saved chat, a 300-turn chat, a chat with a plan
waiting and two documents (for the side panel; Keep planning brings a revised
plan, approving brings an edit to a document), and a scripted answer to
anything typed. For working on the look without building the app.
It is not part of the built app. `.claude/launch.json` starts it as
`ui-preview`.

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
   (wait for it to exit) and `@wait <ms>`. Set `HARNESS_DATA_DIR` so a test
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
