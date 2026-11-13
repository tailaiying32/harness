# Harness v1 plan

Status, 2026-11-13: steps 1 and 2 of "Order of work" are built (setup and the
core spike: one chat in a bare window, plain text, with approvals and stop).
The early checkpoint passed; see the numbers under step 7. Next is step 3.

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
    main.rs            app setup, command registration
    commands.rs        Tauri commands (the only surface the UI calls)
    events.rs          AgentEvent and related types (agent-neutral)
    agent/
      mod.rs           AgentAdapter trait
      claude_code.rs   the only file that knows Claude Code's wire format
    sessions.rs        SessionManager: live processes, idle timers
    store.rs           SQLite schema, reads, writes
    locate.rs          find the claude binary, check login
src/
  lib/
    ipc.ts             typed wrappers over Tauri commands + the event channel
    state/             Svelte stores: projects, sessions, open chat
    chat/              ChatView, MessageBlock, ToolCall, ApprovalCard, Composer
    sidebar/           Sidebar, ProjectGroup, SessionRow
    panel/             SidePanel, PlanDoc, FileDoc
    markdown.ts        markdown-it + highlight.js + dompurify setup
  App.svelte
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

- Always allow (**Verify**): same as allow, plus
  `"updatedPermissions": <the request's permission_suggestions array>`.
  A test with `echo hello` was inconclusive because that command needs no
  approval. Test with a command that does, and confirm the second run of it
  asks nothing.
- Answering a question from Claude (`tool_name` is `AskUserQuestion`)
  (**Verify**, from the Agent SDK docs): allow with
  `"updatedInput": {"questions": <original>, "answers": {"<question text>": "<chosen label>"}}`.

Stop the current turn (**Tested**):

```json
{"type":"control_request","request_id":"<new uuid>","request":{"subtype":"interrupt"}}
```

Claude Code replies with a `control_response`, then a user message
`[Request interrupted by user]`, then a `result`.

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
| `system`, subtype `status` | Activity hint | `status` (e.g. `requesting`) |
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
    Deny with a message = keep planning with that feedback. **Verify** which
    mode the session is in after approval.
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
ApprovalRequest{ request_id, kind: Tool | Plan | Question, tool_name, title, input, suggestions, tool_id }
ApprovalAnswer { request_id, decision: Allow | AlwaysAllow | Deny, message?, answers? }
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

States per chat: `NotRunning`, `Starting`, `Working`, `WaitingApproval`,
`Idle`, `Closing`.

- **Send while `NotRunning`:** spawn (with `--resume` if the chat has an
  `agent_session_id`), then write the user message.
- **Send while `Idle`:** cancel the idle timer, write the message.
- **Send while `Working`:** write it anyway; Claude Code queues it.
- **`result` received:** state becomes `Idle`; start the idle timer (default
  300 seconds, a setting).
- **Idle timer fires:** state `Closing`; call `close()`. Do not kill. The
  process stays until its background work is done, then exits. If the user
  sends a message while `Closing`, wait for exit, then spawn with `--resume`.
- **Process exits unexpectedly:** emit `Error` with the tail of stderr, state
  `NotRunning`. The next send resumes.
- **App quit:** `close()` every live process, wait up to 3 seconds, then
  `kill()` what remains.
- **`agent_session_id`** comes from the first `init`. Save it at once so a
  crash before the first `result` can still resume.

### Store (`store.rs`)

One SQLite file in the Tauri app data directory.

```
projects(id TEXT PK, path TEXT UNIQUE, name TEXT, created_at, last_used_at)
sessions(id TEXT PK, project_id TEXT, agent TEXT DEFAULT 'claude-code',
         agent_session_id TEXT NULL, title TEXT, mode TEXT,
         created_at, last_used_at)
events(id INTEGER PK AUTOINCREMENT, session_id TEXT, kind TEXT,
       payload TEXT /* AgentEvent as JSON */, created_at)
       INDEX (session_id, id)
settings(key TEXT PK, value TEXT)
```

- Save every `AgentEvent` except `TextDelta`, `UsageUpdate` and `Status`.
- The app's history comes only from this table. Do not read Claude Code's
  transcript files under `~/.claude/projects`; their format is internal and
  can change.
- Title: first user message, trimmed to 60 characters.
- An approval left unanswered when a process exits is shown as expired on
  reload.

### Commands (`commands.rs`)

```
list_projects() / add_project(path) / remove_project(id)
list_sessions(project_id)
create_session(project_id, mode) -> session
load_session(session_id) -> saved events
send_message(session_id, text)
answer_approval(session_id, request_id, decision, message?, answers?)
interrupt(session_id)
set_mode(session_id, mode)
read_document(session_id, path) -> text     only paths inside the project
                                            folder or ~/.claude/plans
get_settings() / set_setting(key, value)
subscribe(channel)                          one app-wide tauri::ipc::Channel
                                            carrying { session_id, event }
```

One app-wide channel, not one per chat, so the sidebar can show status dots
for chats that are not open.

### Look: "Quiet Terminal" (chosen 2026-11-13)

Mock-up, with light and dark and the folded lines opened:
https://www.magicpatterns.com/os/tWmrW2q43Q7sWofaHnFJVj?file=2oZgfpBgi1cedNPY3NJS45
(frame "Quiet Terminal"; its source is the reference for spacing and colour).

- **Feel:** spacious, plain, text-first. Hairline rules instead of boxes. No
  blur, translucency, shadows, gradients or motion.
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
  allow, Deny, and a faint key hint (Enter allows, Esc denies).
- **Message box:** a rule above, an accent `>` prompt, and one faint line
  below with mode, model, folder and usage.
- **Side panel:** the plan as a plain document: `#` heading, `[x]` / `[ ]`
  steps, notes as dashes.

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

### UI rules

- **Layout:** sidebar left (projects, each with its chats, newest first, a
  status dot per chat), chat in the middle, side panel right (collapsible).
- **Composer:** a plain multi-line text box. Enter sends, Shift+Enter adds a
  line. A mode switch (Ask before edits / Auto-accept edits / Plan) and a stop
  button that replaces send while working.
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
- **Sanitize** all rendered markdown with dompurify. Links open in the
  default browser, never inside the app window.

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
   everything and a chat resumes after its process has exited.
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

   How to repeat: the app has a self-test, turned on by environment
   variables (see `autotest` in `src-tauri/src/lib.rs`). It sends scripted
   messages through the real window, allows every approval, and writes what
   the window showed to a file. Start the built binary with
   `HARNESS_AUTOTEST_FOLDER`, `HARNESS_AUTOTEST_MESSAGES` (JSON array) and
   `HARNESS_AUTOTEST_REPORT` set, and read memory with `ps` and `top`. WebKit's
   helper processes (`com.apple.WebKit.WebContent`, `.GPU`, `.Networking`) are
   not children of the app; count the ones that appear after launch.
8. **Windows pass.** Build and run on the Windows laptop. Check binary
   lookup, no console window, paths, and that Git for Windows is present (the
   Bash tool needs it).

### Verify list

- `--resume` combined with the streaming flags.
- Always allow via `updatedPermissions`.
- Answer format for `AskUserQuestion`.
- Mode after an approved plan.
- That closing stdin waits for background work before exit.
- Setting the model mid-chat (SDK has `setModel`; the request subtype is
  probably `set_model`).
- Everything on Windows.
