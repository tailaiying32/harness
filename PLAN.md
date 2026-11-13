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

Not built yet: showing subagent work, the side panel (step 6), a settings
page. The theme and size selectors sit in the line under the message box
until there is one.

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
  Approve and Keep planning, and stays out of the chat.
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

- Worktrees, a skills editor, a screen for managing app connections, and
  browser use. Skills, instructions and connections you already have still
  work; there is just no UI for editing them yet.
- Codex, Cursor or other agents. The design leaves room; nothing is built.
- Chats made earlier in the desktop app or the terminal. Only chats started in
  this app appear in the sidebar.
- Images and file attachments in messages.
- Sharing the app with other people. It is for personal use only.

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
    locate.rs          find the claude binary, check login
src/
  App.svelte           the window: sidebar + the open chat; the self-test
  preview.ts           a stand-in for the core, for the browser preview (dev only)
  lib/
    ipc.ts             typed wrappers over Tauri commands + the event channel
    state/app.svelte.ts   sidebar data, which chat is open, each chat's status
    state/chat.svelte.ts  one chat: items on screen, live events and replay
    Sidebar.svelte     projects and chats
    chat/              ChatView, WorkRun, ApprovalCard, Composer, Clipped, Markdown
    tools.ts           how tool calls are worded
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
  --permission-prompt-tool stdio
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

On startup run `claude auth status` (prints JSON with `loggedIn`,
`authMethod`, `subscriptionType`; **Tested**). If the binary is missing or
`loggedIn` is false, show a setup screen telling the user to install Claude
Code or run `claude` once in a terminal to sign in. The app never handles
tokens or credentials.

### Messages the app sends (stdin)

One JSON object per line.

User message (**Tested**):

```json
{"type":"user","message":{"role":"user","content":"text here"}}
```

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
| `user` | Tool result, or the interrupt notice | `message.content[]` with `tool_result` + `tool_use_id`, `is_error` |
| `control_request`, subtype `can_use_tool` | Approval needed | `request_id`, `request.tool_name`, `request.input`, `request.description`, `request.permission_suggestions`, `request.tool_use_id` |
| `control_response` | Reply to a request the app sent | `response.request_id`, `response.response` |
| `result` | Turn ended | `subtype`, `is_error`, `result`, `session_id`, `usage`, `duration_ms`, `stop_reason` |
| `system`, subtype `status` | Activity hint, or a mode change | `status` (e.g. `requesting`); `permissionMode` when the mode changed |
| `control_cancel_request` | An approval request was withdrawn | `request_id` |
| `system`, subtype `post_turn_summary` | One-line summary of the turn | `status_detail` |
| `rate_limit_event` | Subscription usage | `rate_limit_info.unifiedWindows.*.utilization`, `resetsAt` |

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
UserMessage    { text }
TurnEnded      { ok, summary?, duration_ms, usage? }
UsageUpdate    { window, utilization, resets_at }             not saved
Error          { message }
```

`AgentAdapter` trait (one implementation in v1, `ClaudeCodeAdapter`):

```
spawn(project_path, resume_id: Option<String>, mode) -> RunningAgent
RunningAgent:
  events()                      stream of AgentEvent
  send_user_message(text)
  answer_approval(request_id, decision, original_input, suggestions, answers)
  interrupt()
  set_mode(mode)
  close()                       close stdin; process exits on its own
  kill()                        force; only on app quit timeout
```

The adapter keeps each pending approval's original `input` and
`permission_suggestions` keyed by `request_id`, because the answer must echo
them back.

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
- **Idle timer fires:** if nothing happened since, mark `closing` and call
  `close()` (close stdin). Never kill; the process finishes background work,
  then exits. **Tested** live at 5 and 45 seconds, with the window hidden.
- **Send while `closing`:** wait up to 30 seconds for the exit, then spawn
  with `--resume`. After 30 seconds return an error asking to try again.
- **Process exits:** `Exited` clears the process. An unexpected exit also
  emits `Error` with the tail of stderr. The next send resumes.
- **App quit (`RunEvent::Exit`):** `close()` every live process and wait up to
  3 seconds. Anything still running loses its stdin when the app goes, and
  exits then. **Tested**: no chat processes are left after the app is killed.
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
check_setup()             find Claude Code, check login
sidebar()                 every project with its chats, newest first
load_session(session_id)  -> { session, events, live }
delete_session(session_id)
send_message(session_id, project_path, mode, model, text)
answer_approval(session_id, request_id, decision, message?, answers?)
interrupt(session_id)
set_mode(session_id, mode) / set_model(session_id, model)   saved; a live process is told
list_models()
close_session(session_id) ask the chat's process to exit (used by the self-test)
autotest() / autotest_wait(ms) / autotest_report(text)
```

Not built yet: `read_document(session_id, path)` for the side panel (only
paths inside the project folder or `~/.claude/plans`), and settings commands
(`Store::setting` and `set_setting` exist).

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
- **Chat:** a centred column, about 800px wide. The user's message starts with
  an accent `>` in bright text; Claude's text is indented under it, no bubble.
  Code blocks use style D (see "Appearance settings").
- **Claude's work:** a run of tool calls folds into one dim line ("Read
  sessions.rs, searched for SessionState::Idle ▸"), with +/- counts when it
  includes edits. Clicking opens it to one line per tool call, with the diff
  under each edit. While a run is still going, the line shows the current step.
- **Approval:** the only boxed element: a one-pixel accent border, tool and
  reason on one line, the command below, then Allow (filled accent), Always
  allow and Deny. Mouse only: no keyboard shortcut approves or denies, so a
  stray keypress can never approve a command.
- **Message box:** a rule above, an accent `>` prompt, and one faint line
  below with mode, model, folder and usage.
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
- **Settings page: later, not v1.** It will hold theme, size, font, idle
  timeout, the "Always allow" default and the path to Claude Code. Until then
  these are constants or a config file.
- **Slash commands: deferred.** The user uses `/btw` and `/compact`. v1 has no
  slash menu; whether typed slash commands work through the headless input is
  untested. `/btw` (a side question) will want its own floating view later.
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
- **Sidebar, as built:** width 204 / 228 / 256 px by size. "harness" at the
  top, then each project's folder (faint, home as `~`, the end kept when
  long), its chats under it, "+ new chat" at the bottom. A mark before each
  title: `●` green = working, `●` accent = needs your answer, `●` plain =
  finished since you last looked, `○` = nothing going on. The open chat is
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
  line. A mode switch (Ask before edits / Auto-accept edits / Plan) and a stop
  button that replaces send while working. Enter never answers an approval.
- **Streaming performance:** collect `TextDelta`s and apply them once per
  animation frame. Re-render markdown only for the block that is streaming;
  finished blocks render once and are never touched again. Use
  `content-visibility: auto` on message blocks so long chats stay cheap to
  scroll. Add a virtual list only if measurement shows it is needed.
- **Tool calls:** folded as described under "Look". An opened line shows each
  call's input and result; long results are cut with a show-more.
- **Side panel:** a tab per plan (from `ExitPlanMode`) and per markdown file
  the chat wrote (a `Write` tool call whose path ends in `.md`). Plans carry
  Approve and Keep planning (with a feedback box) while their request is
  pending.
- **Rendered markdown must be safe.** As built: markdown-it runs with raw HTML
  turned off and its default link checks, and highlight.js escapes code, so
  the output holds only tags the renderer itself produces. dompurify was
  dropped as unneeded on top of that; add it back if raw HTML is ever turned
  on. Links open in the default browser, never inside the app window.

### Preview page

`npm run dev`, then open `http://localhost:1420/preview.html`: the same window
in a plain browser, with a stand-in for the core (`src/preview.ts`) that has a
sidebar of sample chats, one saved chat, a 300-turn chat, and a scripted
answer to anything typed. For working on the look without building the app.
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
6. **Side panel.** Plans and markdown documents.
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
   lookup, no console window, paths, and that Git for Windows is present (the
   Bash tool needs it).

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
