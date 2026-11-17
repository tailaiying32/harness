//! Keeps track of chats and their live agent processes: start on the first
//! message, resume after an exit, shut a process down once its chat has been
//! idle for a while, and save what happens. See PLAN.md, "Session lifecycle".
//! Worktrees and the trash are in `sessions/worktrees.rs`.

mod worktrees;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::agent::{AgentAdapter, Agents, AppTools, EventSink, ImagePayload, RunningAgent, SpawnOptions};
use crate::events::{AgentEvent, Block, Decision, Envelope, SessionState};
use crate::images::{self, ImageRef, Images};
use crate::notes::Notes;
use crate::skills::Skills;
use crate::store::Store;
use crate::worktree;

pub use worktrees::{PrLookup, SWEEP_EVERY};

/// Where every chat's events go once saved: the window.
pub type Out = Arc<dyn Fn(Envelope) + Send + Sync>;

/// Makes the app's own tools for one chat, given its ID and its project's
/// main folder (as the window names it).
pub type ToolsFor = Arc<dyn Fn(&str, &str) -> Vec<AppTools> + Send + Sync>;

/// How the sessions ask the side panel's terminal about worktree folders.
#[derive(Clone)]
pub struct TerminalHooks {
    /// Whether a running shell sits in this worktree folder.
    pub in_use: Arc<dyn Fn(&Path) -> bool + Send + Sync>,
    /// A worktree folder is gone, and why ("branch merged", "chat deleted",
    /// "cleaned up").
    pub gone: Arc<dyn Fn(&Path, &str) + Send + Sync>,
}

/// How long a chat sits idle before its process is asked to exit.
const DEFAULT_IDLE_SECONDS: u64 = 300;
/// Longest tool result kept, in characters. See `AgentEvent::cap_tool_result`.
const TOOL_RESULT_MAX_CHARS: usize = 20_000;
/// The command that opens a pull request. Only its output is read for links:
/// other `gh pr` commands print links to pull requests the chat merely looked at.
const PR_CREATE: &str = "gh pr create";
/// How long a process asked to exit may take before it is stopped.
const CLOSE_GRACE: Duration = Duration::from_secs(30);
/// What a request is told when the user writes instead of answering it.
const REPLIED: &str = "The user sent a message instead of answering this. Read it and go by it.";
/// How long a new message waits for a stopped process to exit.
const STOP_WAIT: Duration = Duration::from_secs(10);
/// On quit, how long processes that had to be stopped get to exit.
const QUIT_STOP_WAIT: Duration = Duration::from_secs(2);

/// Timer lengths. Fixed in tests; the real ones otherwise.
#[derive(Clone, Copy)]
struct Timings {
    /// Fixed idle time. None = the environment or the settings decide.
    idle: Option<Duration>,
    close_grace: Duration,
    /// Fixed worktree idle limit and keep time. None = environment or settings.
    worktree_idle: Option<Duration>,
    keep: Option<Duration>,
}

/// The idle time from `HARNESS_IDLE_SECONDS`, the `idle_seconds` setting, or
/// the default. None = never. Side chats use it too.
pub fn configured_idle(store: &Store) -> Option<Duration> {
    let seconds = std::env::var("HARNESS_IDLE_SECONDS")
        .ok()
        .or_else(|| store.setting("idle_seconds"))
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_IDLE_SECONDS);
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

/// The text the agent gets: the window's notes first, marked as the app's,
/// then the user's message.
fn with_app_notes(notes: &[String], text: &str) -> String {
    let notes: Vec<&str> = notes.iter().map(|n| n.trim()).filter(|n| !n.is_empty()).collect();
    if notes.is_empty() {
        return text.to_string();
    }
    format!("(From the app, not the user: {})\n\n{text}", notes.join(" "))
}

/// The chat's notes file, its folder made so the agent can be pointed at it.
/// None if the folder can't be made: the chat runs without notes.
pub fn notes_for(notes: &Notes, chat: &str) -> Option<PathBuf> {
    notes.ensure_folder(chat).ok()?;
    notes.path(chat).ok()
}

#[derive(Default)]
struct Session {
    /// The live process, if any.
    agent: Option<Arc<dyn RunningAgent>>,
    /// Counts process starts. Events from an earlier process must not touch the
    /// state of a later one.
    run: u64,
    /// The last run whose process has exited.
    exited_run: u64,
    /// A turn is in progress (or about to be).
    busy: bool,
    /// Counts the times the chat went idle. An idle timer only acts if no new
    /// turn has started and ended since it was set.
    idle_round: u64,
    /// Asked to exit; waiting for the process to finish up.
    closing: bool,
    /// The process has background commands or agents running. An idle chat
    /// with any is kept open: closing it would end them.
    background: bool,
    /// The repo the chat's process runs in; None outside one.
    repo_root: Option<PathBuf>,
    /// The agent's request for a worktree, while it waits for the user.
    ask: Option<Ask>,
    /// The saved history was checked for a worktree question left open
    /// before the app restarted (see `load_open_ask`).
    ask_checked: bool,
    /// The worktree tool's latest call, until its question goes on screen,
    /// so the chat can fold the call under the question.
    worktree_tool_id: Option<String>,
    /// Tool calls that run `gh pr create`, until their results arrive: what
    /// they print names the chat's new pull request.
    pr_calls: HashSet<String>,
}

/// The agent asked for a worktree; the question waits for the user.
struct Ask {
    repo_root: PathBuf,
    /// The ID naming the question on screen.
    card: String,
    /// The name the agent suggested, used for the branch.
    name: Option<String>,
}

struct Inner {
    /// Every agent a chat can run on, and where its program is.
    agents: Agents,
    store: Arc<Store>,
    out: Out,
    sessions: Mutex<HashMap<String, Session>>,
    /// Starting a process is not instant; two sends must not both start one.
    start_lock: tokio::sync::Mutex<()>,
    timings: Timings,
    /// The app's data folder; worktrees are kept in it.
    data_dir: PathBuf,
    /// Pasted images and images from tool results.
    images: Images,
    /// Each chat's notes.
    notes: Notes,
    /// The app's own skills, offered to every chat's agent.
    skills: Skills,
    /// Finds a branch's pull request (GitHub in the app, a stand-in in tests).
    pr_lookup: PrLookup,
    /// One cleanup sweep at a time.
    sweep_lock: tokio::sync::Mutex<()>,
    /// When each chat's pull request was last looked up.
    pr_checked: Mutex<HashMap<String, Instant>>,
    /// Links to new pull requests waiting to be looked up, as (chat, link),
    /// oldest first; and the lock that has them looked up one at a time.
    pr_links: Mutex<VecDeque<(String, String)>>,
    pr_learn_lock: tokio::sync::Mutex<()>,
    /// The app's own tools for each chat's agent; set once at start.
    app_tools: OnceLock<ToolsFor>,
    /// The side panel's terminal; set once at start.
    terminal: OnceLock<TerminalHooks>,
}

#[derive(Clone)]
pub struct SessionManager(Arc<Inner>);

impl SessionManager {
    pub fn new(agents: Agents, store: Arc<Store>, out: Out, data_dir: PathBuf) -> Self {
        let timings = Timings {
            idle: None,
            close_grace: CLOSE_GRACE,
            worktree_idle: None,
            keep: None,
        };
        Self::build(agents, store, out, timings, data_dir, PrLookup::github())
    }

    fn build(
        agents: Agents,
        store: Arc<Store>,
        out: Out,
        timings: Timings,
        data_dir: PathBuf,
        pr_lookup: PrLookup,
    ) -> Self {
        Self(Arc::new(Inner {
            agents,
            store,
            out,
            sessions: Mutex::default(),
            start_lock: tokio::sync::Mutex::default(),
            timings,
            images: Images::new(data_dir.join("images")),
            notes: Notes::new(data_dir.join("notes")),
            skills: Skills::new(data_dir.join("skills")),
            data_dir,
            pr_lookup,
            sweep_lock: tokio::sync::Mutex::default(),
            pr_checked: Mutex::default(),
            pr_links: Mutex::default(),
            pr_learn_lock: tokio::sync::Mutex::default(),
            app_tools: OnceLock::new(),
            terminal: OnceLock::new(),
        }))
    }

    /// Gives every chat's agent started from now on the app's own tools.
    pub fn set_app_tools(&self, tools: ToolsFor) {
        let _ = self.0.app_tools.set(tools);
    }

    /// Lets worktree cleanup ask about and tell the terminal.
    pub fn set_terminal_hooks(&self, hooks: TerminalHooks) {
        let _ = self.0.terminal.set(hooks);
    }

    /// Whether a terminal shell sits in this worktree folder.
    fn terminal_in(&self, root: &Path) -> bool {
        self.0.terminal.get().is_some_and(|t| (t.in_use)(root))
    }

    /// Tells the terminal a worktree folder is gone.
    fn terminal_gone(&self, root: &Path, why: &str) {
        if let Some(t) = self.0.terminal.get() {
            (t.gone)(root, why);
        }
    }

    /// Chats' images.
    pub fn images(&self) -> &Images {
        &self.0.images
    }

    /// Chats' notes.
    pub fn notes(&self) -> &Notes {
        &self.0.notes
    }

    /// The app's own skills.
    pub fn skills(&self) -> &Skills {
        &self.0.skills
    }

    /// Removes images and notes left by chats that don't exist. Run once at start.
    pub fn sweep_chat_files(&self) {
        match self.0.store.session_ids() {
            Ok(ids) => {
                self.0.images.sweep(&ids);
                self.0.notes.sweep(&ids);
            }
            Err(e) => eprintln!("harness: couldn't list chats to tidy images and notes: {e}"),
        }
    }

    /// Idle time before a chat's process is closed. None = never.
    /// Order: test override, `HARNESS_IDLE_SECONDS`, the `idle_seconds` setting,
    /// then the default. Zero means never.
    fn idle_time(&self) -> Option<Duration> {
        if let Some(fixed) = self.0.timings.idle {
            return Some(fixed);
        }
        configured_idle(&self.0.store)
    }

    /// Saves an event (if it is the lasting kind) and passes it to the window.
    fn record(&self, session_id: &str, event: AgentEvent) {
        if event.is_saved() {
            if let Err(e) = self.0.store.append_event(session_id, &event) {
                eprintln!("harness: could not save an event: {e}");
            }
        }
        (self.0.out)(Envelope {
            session_id: session_id.to_string(),
            event,
        });
    }

    /// Send a message, starting or resuming the agent if it isn't running.
    /// `mode`, `model` and `effort` are saved with the chat; they are passed to
    /// the agent only when a process has to be started (a running one is
    /// switched with `set_mode`, `set_model` and `set_effort`). `agent` is the
    /// agent a new chat runs on; a saved chat keeps its own.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_message(
        &self,
        session_id: &str,
        agent: &str,
        project_path: PathBuf,
        mode: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        text: &str,
        images: &[u32],
        message_id: &str,
        // Things the window tells the agent with this message (a doc the user
        // moved, say); not shown or saved.
        app_notes: &[String],
    ) -> Result<(), String> {
        self.send_inner(session_id, agent, project_path, mode, model, effort, text, images, message_id, true, app_notes)
            .await
    }

    /// `show` false sends a note from the app to the agent that the chat
    /// doesn't show or save as a user message.
    #[allow(clippy::too_many_arguments)]
    async fn send_inner(
        &self,
        session_id: &str,
        agent: &str,
        project_path: PathBuf,
        mode: Option<String>,
        model: Option<String>,
        // None keeps the saved one.
        effort: Option<String>,
        text: &str,
        // Numbers of the chat's saved images to send with the text.
        images: &[u32],
        message_id: &str,
        show: bool,
        app_notes: &[String],
    ) -> Result<(), String> {
        let inner = &self.0;

        // A saved chat stays on its agent. Its program must be there before
        // anything is saved.
        let agent_id = match inner.store.session(session_id)? {
            Some(saved) => saved.agent,
            None => agent.to_string(),
        };
        let (adapter, binary) = inner.agents.get(&agent_id)?;

        // Read before anything is saved: a missing image fails the send.
        let mut refs = Vec::new();
        let mut payloads = Vec::new();
        for &n in images {
            let (bytes, media_type) = inner.images.read(session_id, n)?;
            refs.push(ImageRef {
                n,
                media_type: media_type.clone(),
            });
            payloads.push(ImagePayload {
                media_type,
                data_b64: images::encode_base64(&bytes),
            });
        }

        // A process that was asked to exit can't take messages. Stop it now
        // rather than wait: chats are only closed with nothing running in the
        // background, so nothing is lost. Then start a fresh one that resumes.
        let closing = inner
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .filter(|s| s.closing)
            .and_then(|s| s.agent.clone());
        if let Some(agent) = closing {
            agent.stop();
            self.wait_for_exit(session_id).await?;
        }

        let _starting = inner.start_lock.lock().await;

        // Save the chat first, so its events have somewhere to go.
        let created = inner.store.touch_session(
            session_id,
            &project_path.to_string_lossy(),
            text,
            mode.as_deref().unwrap_or("default"),
            model.as_deref().unwrap_or("default"),
        )?;
        if created {
            inner.store.set_agent(session_id, &agent_id)?;
        }
        if let Some(effort) = &effort {
            inner.store.set_effort(session_id, effort)?;
        }
        // Recorded before any worktree is brought back, so the history reads
        // in the order it happened on screen.
        if show {
            self.record(
                session_id,
                AgentEvent::UserMessage {
                    text: text.to_string(),
                    id: Some(message_id.to_string()),
                    images: refs,
                },
            );
        }
        if created {
            let folder = project_path.clone();
            if let Some((branch, commit)) = blocking(move || worktree::head_of(&folder)).await {
                let _ = inner.store.set_start(session_id, branch.as_deref(), &commit);
            }
            self.write_title(session_id, adapter.clone(), binary.clone(), text);
        }

        let (running, run) = {
            let mut sessions = inner.sessions.lock().unwrap();
            let session = sessions.entry(session_id.to_string()).or_default();
            session.busy = true;
            if session.agent.is_none() {
                session.run += 1;
            }
            (session.agent.is_some(), session.run)
        };

        if !running {
            let saved = inner.store.session(session_id)?;
            let resume_id = saved.as_ref().and_then(|s| s.agent_session_id.clone());
            // A saved chat runs where its row says: its worktree if it has one,
            // brought back first if it was put away.
            let folder = self.work_folder(session_id, &project_path).await;
            self.note_repo(session_id, &folder).await;
            // Not under the sessions lock: starting reports "starting" through
            // the sink, which takes that lock.
            let agent = adapter.spawn(
                SpawnOptions {
                    binary,
                    project_path: folder,
                    resume_id,
                    mode,
                    model,
                    effort: saved.map(|s| s.effort),
                    side: None,
                    tools: {
                        let mut tools = inner
                            .app_tools
                            .get()
                            .map(|tools| tools(session_id, &project_path.to_string_lossy()))
                            .unwrap_or_default();
                        // Every main chat's agent asks for a worktree this way.
                        tools.push(self.worktree_tools(session_id));
                        tools
                    },
                    notes: notes_for(&inner.notes, session_id),
                },
                self.sink(session_id, run),
            );
            let agent = match agent {
                Ok(agent) => agent,
                Err(e) => {
                    if let Some(session) = inner.sessions.lock().unwrap().get_mut(session_id) {
                        session.busy = false;
                    }
                    return Err(e);
                }
            };
            let mut sessions = inner.sessions.lock().unwrap();
            if let Some(session) = sessions.get_mut(session_id) {
                // If it already died, its exit was reported; don't keep a dead handle.
                if session.exited_run != run {
                    session.agent = Some(Arc::from(agent));
                }
            }
        }

        let agent = self.agent(session_id)?;
        let mut app_notes = app_notes.to_vec();
        if show {
            if let Some(note) = self.answer_by_message(session_id, agent.as_ref()) {
                app_notes.push(note.to_string());
            }
        }
        // A typed `/name` for one of the app's skills goes to the agent with
        // the skill attached; the chat keeps what the user typed.
        let expanded = if show { inner.skills.expand(text) } else { None };
        let mut text = with_app_notes(&app_notes, expanded.as_deref().unwrap_or(text));
        // A new chat's first message carries the project's latest chats, so
        // the agent can spot a passing mention of one. Said here once, not in
        // the instructions: those are sent again on every start, and a list
        // that changes would make a resumed chat pay for its history anew.
        // It goes after the user's words: Cursor titles a chat from the start
        // of its first message.
        if created && show {
            if let Some(note) = crate::chats::titles_note(&inner.store, &project_path.to_string_lossy(), session_id) {
                text.push_str(&format!("\n\n(From the app, not the user: {note})"));
            }
        }
        agent.send_user_message(&text, &payloads, message_id)
    }

    /// The user wrote while the agent waited on a question (a plan, a tool,
    /// the worktree question...). The message is the answer: each open
    /// request is turned down with a note pointing the agent at the message,
    /// which follows. Left open, they would hold the turn, and the message
    /// queued behind it. Returns a note to put before the message when the
    /// worktree question was open (that call has already returned).
    fn answer_by_message(&self, session_id: &str, agent: &dyn RunningAgent) -> Option<&'static str> {
        let note = self.reply_closes_ask(session_id);
        for request_id in agent.open_requests() {
            if agent
                .answer_approval(&request_id, Decision::Deny, Some(REPLIED.to_string()), None, None)
                .is_ok()
            {
                self.record(
                    session_id,
                    AgentEvent::ApprovalAnswered {
                        request_id,
                        decision: Decision::Deny,
                        answers: None,
                    },
                );
            }
        }
        note
    }

    /// Waits for the chat's process to be gone, after a stop.
    async fn wait_for_exit(&self, session_id: &str) -> Result<(), String> {
        let deadline = Instant::now() + STOP_WAIT;
        while self.is_live(session_id) {
            if Instant::now() > deadline {
                return Err("This chat's old process didn't stop. Try again in a moment.".to_string());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    /// Asks the agent for a short title in the background. Until it arrives, or
    /// if it fails, the chat keeps the title cut from its first message. (An
    /// agent that names the chat itself sends `TitleChanged` instead.)
    fn write_title(&self, session_id: &str, adapter: Arc<dyn AgentAdapter>, binary: PathBuf, first_message: &str) {
        let manager = self.clone();
        let id = session_id.to_string();
        let message = first_message.to_string();
        tokio::spawn(async move {
            let title = tokio::task::spawn_blocking(move || adapter.make_title(&binary, &message)).await;
            let Ok(Ok(title)) = title else {
                return;
            };
            if manager.0.store.set_generated_title(&id, &title).unwrap_or(false) {
                manager.record(&id, AgentEvent::TitleChanged { title });
            }
        });
    }

    /// Names a batch of subagents for chat info, by the chat's own agent. The
    /// window asks once per batch; the label is saved with the chat.
    pub async fn title_batch(&self, session_id: &str, tool_id: &str, ask: &str, agents: Vec<String>) -> Result<(), String> {
        let saved = self.0.store.session(session_id)?.ok_or("No such chat")?;
        let (adapter, binary) = self.0.agents.get(&saved.agent)?;
        let ask = ask.to_string();
        let title = tokio::task::spawn_blocking(move || adapter.name_batch(&binary, &ask, &agents))
            .await
            .map_err(|e| e.to_string())??;
        // Deleted meanwhile: nothing to label.
        if self.0.store.session(session_id)?.is_none() {
            return Ok(());
        }
        self.record(session_id, AgentEvent::BatchTitled { tool_id: tool_id.to_string(), title });
        Ok(())
    }

    /// Notes in the chat's history that a doc was saved to the project, so
    /// the chat finds it at its new place, now and after a reopen.
    pub fn doc_saved(&self, session_id: &str, from: &str, to: &str) {
        self.record(session_id, AgentEvent::DocSaved { from: from.to_string(), to: to.to_string() });
    }

    /// What a process's events pass through: bookkeeping, saving, then the window.
    fn sink(&self, session_id: &str, run: u64) -> EventSink {
        let manager = self.clone();
        let id = session_id.to_string();
        Arc::new(move |event: AgentEvent| {
            let store = &manager.0.store;
            // A deleted chat's process may still talk while it stops; drop that.
            if !manager.0.sessions.lock().unwrap().contains_key(&id) {
                return;
            }
            let mut event = event;
            match &event {
                // Saved at once, so a crash before the turn ends can still resume.
                AgentEvent::SessionStarted {
                    agent_session_id,
                    mode,
                    ..
                } => {
                    let _ = store.set_agent_session_id(&id, agent_session_id);
                    if let Some(mode) = mode {
                        let _ = store.set_mode(&id, mode);
                    }
                }
                AgentEvent::ModeChanged { mode } => {
                    let _ = store.set_mode(&id, mode);
                }
                // The agent named the chat. A chat the user renamed keeps its name.
                AgentEvent::TitleChanged { title } => {
                    if !store.set_generated_title(&id, title).unwrap_or(false) {
                        return;
                    }
                }
                AgentEvent::Status { state } => manager.on_state(&id, run, state),
                AgentEvent::BackgroundWork { running, .. } => manager.on_background(&id, run, *running > 0),
                AgentEvent::TurnEnded { .. } => manager.after_turn(&id),
                AgentEvent::AssistantBlock {
                    block: Block::ToolCall { tool_id, name, .. },
                    parent_tool_id: None,
                    ..
                }
                | AgentEvent::ToolInput {
                    tool_id,
                    name,
                    parent_tool_id: None,
                    ..
                } if name == worktrees::CALL_NAME => {
                    if let Some(session) = manager.0.sessions.lock().unwrap().get_mut(&id) {
                        session.worktree_tool_id = Some(tool_id.clone());
                    }
                }
                AgentEvent::AssistantBlock {
                    block: Block::ToolCall { tool_id, name, input },
                    ..
                } if name == "Bash" && input["command"].as_str().is_some_and(|c| c.contains(PR_CREATE)) => {
                    if let Some(session) = manager.0.sessions.lock().unwrap().get_mut(&id) {
                        session.pr_calls.insert(tool_id.clone());
                    }
                }
                // Read whole, before it is cut below. A failed call counts
                // too: "already exists" comes with the link.
                AgentEvent::ToolResult { tool_id, .. } => {
                    let ours = manager
                        .0
                        .sessions
                        .lock()
                        .unwrap()
                        .get_mut(&id)
                        .is_some_and(|s| s.pr_calls.remove(tool_id));
                    if ours {
                        let text = event.tool_result_text().unwrap_or_default();
                        manager.learn_prs(&id, crate::github::pr_links(&text));
                    }
                }
                _ => {}
            }
            manager.keep_tool_images(&id, &mut event);
            event.cap_tool_result(TOOL_RESULT_MAX_CHARS);
            manager.record(&id, event);
        })
    }

    /// Moves the pictures in a tool result into the image store, leaving
    /// `{"type":"image_ref","n":..,"media_type":..}` in their place, so neither
    /// the window nor the saved history carries the picture itself.
    fn keep_tool_images(&self, session_id: &str, event: &mut AgentEvent) {
        let AgentEvent::ToolResult { content, .. } = event else {
            return;
        };
        let Some(parts) = content.as_array_mut() else {
            return;
        };
        for part in parts.iter_mut().filter(|p| p["type"] == "image") {
            let source = &part["source"];
            let kept = match (source["type"].as_str(), source["media_type"].as_str(), source["data"].as_str()) {
                (Some("base64"), Some(media_type), Some(data)) => images::decode_base64(data)
                    .and_then(|bytes| self.0.images.save(session_id, media_type, &bytes)),
                _ => Err("not a base64 image".to_string()),
            };
            *part = match kept {
                Ok(image) => serde_json::json!({ "type": "image_ref", "n": image.n, "media_type": image.media_type }),
                Err(e) => {
                    eprintln!("harness: couldn't keep an image from a tool: {e}");
                    serde_json::json!({ "type": "text", "text": "[image could not be saved]" })
                }
            };
        }
    }

    fn on_state(&self, session_id: &str, run: u64, state: &SessionState) {
        let mut sessions = self.0.sessions.lock().unwrap();
        let Some(session) = sessions.get_mut(session_id) else {
            return;
        };
        if *state == SessionState::Exited {
            session.exited_run = session.exited_run.max(run);
        }
        if session.run != run {
            return; // from an earlier process
        }
        match state {
            SessionState::Exited => {
                session.agent = None;
                session.closing = false;
                session.busy = false;
                session.background = false;
                // A waiting request went with the process; close its question.
                let card = session.ask.take().map(|a| a.card);
                drop(sessions);
                if let Some(request_id) = card {
                    self.record(session_id, AgentEvent::ApprovalCancelled { request_id });
                }
                self.sweep_soon();
            }
            SessionState::Idle => {
                session.busy = false;
                session.idle_round += 1;
                let round = session.idle_round;
                drop(sessions);
                self.close_when_idle(session_id.to_string(), run, round);
            }
            _ => session.busy = true,
        }
    }

    /// Background work started or ended. When the last of it ends between
    /// turns, the idle timer starts: it was skipped while work was running.
    fn on_background(&self, session_id: &str, run: u64, running: bool) {
        let mut sessions = self.0.sessions.lock().unwrap();
        let Some(session) = sessions.get_mut(session_id) else {
            return;
        };
        if session.run != run {
            return; // from an earlier process
        }
        let ended = session.background && !running;
        session.background = running;
        if ended && !session.busy && !session.closing {
            session.idle_round += 1;
            let round = session.idle_round;
            drop(sessions);
            self.close_when_idle(session_id.to_string(), run, round);
        }
    }

    /// After the idle time, ask the process to exit unless the chat was used
    /// again or has background work running. If it hasn't exited after
    /// `close_grace`, it is stopped.
    fn close_when_idle(&self, session_id: String, run: u64, round: u64) {
        let Some(wait) = self.idle_time() else {
            return;
        };
        let manager = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            let agent = {
                let mut sessions = manager.0.sessions.lock().unwrap();
                let Some(session) = sessions.get_mut(&session_id) else {
                    return;
                };
                let still_idle = session.run == run
                    && session.idle_round == round
                    && !session.busy
                    && !session.closing
                    && !session.background;
                if !still_idle {
                    return;
                }
                session.closing = true;
                session.agent.clone()
            };
            let Some(agent) = agent else {
                return;
            };
            agent.close();
            tokio::time::sleep(manager.0.timings.close_grace).await;
            let stuck = manager
                .0
                .sessions
                .lock()
                .unwrap()
                .get(&session_id)
                .is_some_and(|s| s.run == run && s.closing && s.agent.is_some());
            if stuck {
                agent.stop();
            }
        });
    }

    /// The chat's live process. Returned as its own handle so no lock is held
    /// while it is used: its methods report through the sink, which locks.
    fn agent(&self, session_id: &str) -> Result<Arc<dyn RunningAgent>, String> {
        self.0
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .and_then(|s| s.agent.clone())
            .ok_or_else(|| "This chat has no running process".to_string())
    }

    pub fn answer_approval(
        &self,
        session_id: &str,
        request_id: &str,
        decision: Decision,
        message: Option<String>,
        answers: Option<BTreeMap<String, String>>,
        plan: Option<String>,
    ) -> Result<(), String> {
        self.agent(session_id)?
            .answer_approval(request_id, decision.clone(), message, answers.clone(), plan.clone())?;
        // Saved before the answer, so a reopened chat shows the plan as approved.
        if let Some(plan) = plan {
            self.record(
                session_id,
                AgentEvent::PlanEdited {
                    request_id: request_id.to_string(),
                    plan,
                },
            );
        }
        self.record(
            session_id,
            AgentEvent::ApprovalAnswered {
                request_id: request_id.to_string(),
                decision,
                answers,
            },
        );
        Ok(())
    }

    /// Take back a message the agent hasn't read yet, to edit it. The answer
    /// comes as `MessageWithdrawn`, which is saved only when it worked.
    pub fn withdraw_message(&self, session_id: &str, message_id: &str) -> Result<(), String> {
        self.agent(session_id)?.withdraw_message(message_id)
    }

    pub fn interrupt(&self, session_id: &str) -> Result<(), String> {
        self.agent(session_id)?.interrupt()?;
        self.record(session_id, AgentEvent::Interrupted);
        Ok(())
    }

    /// Saved either way; a running process is also told now.
    pub fn set_mode(&self, session_id: &str, mode: &str) -> Result<(), String> {
        let _ = self.0.store.set_mode(session_id, mode);
        self.agent(session_id)?.set_mode(mode)
    }

    /// Saved either way; a running process is also told now.
    pub fn set_model(&self, session_id: &str, model: &str) -> Result<(), String> {
        let _ = self.0.store.set_model(session_id, model);
        self.agent(session_id)?.set_model(model)
    }

    /// Saved either way; a running process is also told now.
    pub fn set_effort(&self, session_id: &str, effort: &str) -> Result<(), String> {
        let _ = self.0.store.set_effort(session_id, effort);
        self.agent(session_id)?.set_effort(effort)
    }

    /// Ask the chat's process to finish up and exit. The chat can be resumed later.
    pub fn close(&self, session_id: &str) {
        let agent = {
            let mut sessions = self.0.sessions.lock().unwrap();
            let Some(session) = sessions.get_mut(session_id) else {
                return;
            };
            if session.agent.is_some() {
                session.closing = true;
            }
            session.agent.clone()
        };
        if let Some(agent) = agent {
            agent.close();
        }
    }

    /// Whether the chat has a process right now.
    pub fn is_live(&self, session_id: &str) -> bool {
        self.0
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .is_some_and(|s| s.agent.is_some())
    }

    /// Moves a chat to the trash: its process is stopped (with any background
    /// work) and its worktree put away. Its history stays until the trash is
    /// emptied (see `worktrees.rs`).
    pub async fn delete(&self, session_id: &str) -> Result<(), String> {
        let agent = self
            .0
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .and_then(|s| s.agent.clone());
        if let Some(agent) = agent {
            agent.stop();
            // Its folder may be put away next; let the process finish first.
            let _ = self.wait_for_exit(session_id).await;
        }
        self.0.sessions.lock().unwrap().remove(session_id);
        self.trash(session_id).await
    }

    pub fn rename(&self, session_id: &str, title: &str) -> Result<(), String> {
        self.0.store.rename_session(session_id, title)
    }

    /// On app quit: ask every process to exit and give them a moment. Ones with
    /// background work, and any still running after the wait, are stopped, so
    /// their background commands end too instead of running on unseen.
    pub fn shutdown(&self, wait: Duration) {
        let live: Vec<(String, bool)> = {
            let sessions = self.0.sessions.lock().unwrap();
            sessions
                .iter()
                .filter(|(_, s)| s.agent.is_some())
                .map(|(id, s)| (id.clone(), s.background))
                .collect()
        };
        for (id, background) in &live {
            if *background {
                self.stop(id);
            } else {
                self.close(id);
            }
        }
        let any_live = || live.iter().any(|(id, _)| self.is_live(id));
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline && any_live() {
            std::thread::sleep(Duration::from_millis(50));
        }
        for (id, _) in &live {
            self.stop(id);
        }
        let deadline = Instant::now() + QUIT_STOP_WAIT;
        while Instant::now() < deadline && any_live() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// End the chat's process now, if it has one.
    fn stop(&self, session_id: &str) {
        let agent = self
            .0
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .and_then(|s| s.agent.clone());
        if let Some(agent) = agent {
            agent.stop();
        }
    }
}

/// Runs blocking work (git, files) off the async threads.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work).await.expect("background work failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests written from the outside: what a user does and should see.
    mod black_box;
    use crate::events::ModelInfo;
    use crate::worktree::PrInfo;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The stand-in, as the only agent: Claude Code's.
    fn fake_agents(fake: &Arc<Fake>) -> Agents {
        let agents = Agents::default();
        agents.add(crate::agent::CLAUDE, fake.clone(), Some("claude".into()));
        agents
    }

    /// A stand-in agent: remembers how it was started and what it was told, and
    /// lets the test play the agent's side through `sink`.
    #[derive(Default)]
    struct Fake {
        spawns: Mutex<Vec<(Option<String>, Option<String>)>>, // (resume id, mode)
        /// Each start's folder.
        folders: Mutex<Vec<PathBuf>>,
        sinks: Mutex<Vec<EventSink>>,
        told: Arc<Mutex<Vec<String>>>,
        /// Plays a process that ignores being asked to exit.
        hang_on_close: Arc<AtomicBool>,
        /// Requests waiting for an answer, as the real adapter keeps them.
        open: Arc<Mutex<Vec<String>>>,
    }

    struct FakeAgent {
        sink: EventSink,
        told: Arc<Mutex<Vec<String>>>,
        hang_on_close: Arc<AtomicBool>,
        open: Arc<Mutex<Vec<String>>>,
    }

    impl RunningAgent for FakeAgent {
        fn send_user_message(&self, text: &str, images: &[ImagePayload], _id: &str) -> Result<(), String> {
            let mut said = format!("say {text}");
            for image in images {
                said += &format!(" +{}:{}", image.media_type, image.data_b64);
            }
            self.told.lock().unwrap().push(said);
            (self.sink)(AgentEvent::Status {
                state: SessionState::Working,
            });
            Ok(())
        }
        fn answer_approval(
            &self,
            request_id: &str,
            decision: Decision,
            _: Option<String>,
            _: Option<BTreeMap<String, String>>,
            _: Option<String>,
        ) -> Result<(), String> {
            let decision = format!("{decision:?}").to_lowercase();
            self.open.lock().unwrap().retain(|id| id != request_id);
            self.told.lock().unwrap().push(format!("answer {request_id} {decision}"));
            (self.sink)(AgentEvent::Status {
                state: SessionState::Working,
            });
            Ok(())
        }
        fn open_requests(&self) -> Vec<String> {
            self.open.lock().unwrap().clone()
        }
        fn withdraw_message(&self, id: &str) -> Result<(), String> {
            self.told.lock().unwrap().push(format!("withdraw {id}"));
            Ok(())
        }
        fn interrupt(&self) -> Result<(), String> {
            self.told.lock().unwrap().push("stop".into());
            Ok(())
        }
        fn set_mode(&self, mode: &str) -> Result<(), String> {
            self.told.lock().unwrap().push(format!("mode {mode}"));
            Ok(())
        }
        fn set_model(&self, model: &str) -> Result<(), String> {
            self.told.lock().unwrap().push(format!("model {model}"));
            Ok(())
        }
        fn set_effort(&self, effort: &str) -> Result<(), String> {
            self.told.lock().unwrap().push(format!("effort {effort}"));
            Ok(())
        }
        fn close(&self) {
            self.told.lock().unwrap().push("close".into());
            // A real process exits once its input closes.
            if !self.hang_on_close.load(Ordering::SeqCst) {
                (self.sink)(AgentEvent::Status {
                    state: SessionState::Exited,
                });
            }
        }
        fn stop(&self) {
            self.told.lock().unwrap().push("kill".into());
            (self.sink)(AgentEvent::Status {
                state: SessionState::Exited,
            });
        }
    }

    impl AgentAdapter for Fake {
        fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
            self.spawns.lock().unwrap().push((opts.resume_id, opts.mode));
            self.folders.lock().unwrap().push(opts.project_path);
            // What the test plays as the agent passes the adapter first, which
            // keeps the requests that wait for an answer.
            let open = self.open.clone();
            let inner = sink.clone();
            self.sinks.lock().unwrap().push(Arc::new(move |event: AgentEvent| {
                match &event {
                    AgentEvent::ApprovalRequest { request_id, .. } => open.lock().unwrap().push(request_id.clone()),
                    AgentEvent::ApprovalCancelled { request_id } => open.lock().unwrap().retain(|id| id != request_id),
                    _ => {}
                }
                inner(event)
            }));
            sink(AgentEvent::Status {
                state: SessionState::Starting,
            });
            Ok(Box::new(FakeAgent {
                sink,
                told: self.told.clone(),
                hang_on_close: self.hang_on_close.clone(),
                open: self.open.clone(),
            }))
        }
        fn list_models(&self, _: &Path, _: Option<&[String]>) -> Result<Vec<ModelInfo>, String> {
            Ok(Vec::new())
        }
        fn make_title(&self, _: &Path, first_message: &str) -> Result<String, String> {
            Ok(format!("Written: {first_message}"))
        }
        fn name_batch(&self, _: &Path, ask: &str, agents: &[String]) -> Result<String, String> {
            Ok(format!("{ask}: {}", agents.join(", ")))
        }
        fn sign_in(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn forget_session(&self, _: &str) {}
    }

    struct Rig {
        fake: Arc<Fake>,
        store: Arc<Store>,
        seen: Arc<Mutex<Vec<AgentEvent>>>,
        manager: SessionManager,
    }

    fn rig(idle: Duration) -> Rig {
        rig_with(idle, LONG, LONG, PrLookup::none())
    }

    fn rig_with(idle: Duration, worktree_idle: Duration, keep: Duration, lookup: PrLookup) -> Rig {
        let fake = Arc::new(Fake::default());
        let dir = std::env::temp_dir().join(format!("harness-test-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(&dir).unwrap());
        let seen: Arc<Mutex<Vec<AgentEvent>>> = Arc::default();
        let out: Out = {
            let seen = seen.clone();
            Arc::new(move |envelope: Envelope| seen.lock().unwrap().push(envelope.event))
        };
        let timings = Timings {
            idle: Some(idle),
            close_grace: Duration::from_millis(100),
            worktree_idle: Some(worktree_idle),
            keep: Some(keep),
        };
        let manager = SessionManager::build(fake_agents(&fake), store.clone(), out, timings, dir, lookup);
        Rig {
            fake,
            store,
            seen,
            manager,
        }
    }

    impl Rig {
        async fn send(&self, text: &str) {
            self.manager
                .send_message("s1", crate::agent::CLAUDE, "/work".into(), Some("plan".into()), None, None, text, &[], "m1", &[])
                .await
                .unwrap();
        }
        /// Plays an event from the most recently started process.
        fn agent_says(&self, event: AgentEvent) {
            let sink = self.fake.sinks.lock().unwrap().last().unwrap().clone();
            sink(event);
        }
        fn turn_ends(&self) {
            self.agent_says(AgentEvent::TurnEnded {
                ok: true,
                duration_ms: None,
            });
            self.agent_says(AgentEvent::Status {
                state: SessionState::Idle,
            });
        }
        fn told(&self) -> Vec<String> {
            self.fake.told.lock().unwrap().clone()
        }
        fn background(&self, running: u32) {
            self.agent_says(AgentEvent::BackgroundWork { running, agents: 0, tasks: vec![] });
        }
        fn count(&self, what: &str) -> usize {
            self.told().iter().filter(|t| *t == what).count()
        }
    }

    const LONG: Duration = Duration::from_secs(3600);

    #[tokio::test]
    async fn first_message_starts_the_agent_and_saves_the_chat() {
        let rig = rig(LONG);
        rig.send("Hello there").await;
        assert_eq!(*rig.fake.spawns.lock().unwrap(), vec![(None, Some("plan".to_string()))]);
        assert_eq!(rig.told(), vec!["say Hello there"]);
        let chat = rig.store.session("s1").unwrap().unwrap();
        // The cut title, or the written one if it already arrived.
        assert!(chat.title.ends_with("Hello there"));
        assert_eq!(chat.mode, "plan");
        assert_eq!(
            rig.store.events("s1").unwrap(),
            vec![AgentEvent::UserMessage {
                text: "Hello there".into(),
                id: Some("m1".into()),
                images: vec![],
            }]
        );
        // A second message goes to the same process.
        rig.send("And again").await;
        assert_eq!(rig.fake.spawns.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn images_go_with_the_message_and_only_their_numbers_are_saved() {
        let rig = rig(LONG);
        let images = rig.manager.images();
        images.save("s1", "image/png", b"one").unwrap();
        images.save("s1", "image/jpeg", b"two").unwrap();
        rig.manager
            .send_message("s1", crate::agent::CLAUDE, "/work".into(), None, None, None, "[image 2] [image 1]", &[2, 1], "m1", &[])
            .await
            .unwrap();
        assert_eq!(rig.told(), vec!["say [image 2] [image 1] +image/jpeg:dHdv +image/png:b25l"]);
        let AgentEvent::UserMessage { images, .. } = &rig.store.events("s1").unwrap()[0] else {
            panic!("not a user message");
        };
        assert_eq!(
            images,
            &vec![
                ImageRef { n: 2, media_type: "image/jpeg".into() },
                ImageRef { n: 1, media_type: "image/png".into() },
            ]
        );
    }

    #[tokio::test]
    async fn app_notes_reach_the_agent_but_not_the_saved_message() {
        let rig = rig(LONG);
        let notes = vec!["The doc moved.".to_string(), " ".to_string()];
        rig.manager
            .send_message("s1", crate::agent::CLAUDE, "/work".into(), None, None, None, "change it", &[], "m1", &notes)
            .await
            .unwrap();
        assert_eq!(rig.told(), vec!["say (From the app, not the user: The doc moved.)\n\nchange it"]);
        let AgentEvent::UserMessage { text, .. } = &rig.store.events("s1").unwrap()[0] else {
            panic!("not a user message");
        };
        assert_eq!(text, "change it");
    }

    #[tokio::test]
    async fn a_new_chats_first_message_names_the_projects_other_chats() {
        let rig = rig(LONG);
        let send = |chat: &'static str, text: &'static str, id: &'static str| {
            let manager = rig.manager.clone();
            async move {
                manager
                    .send_message(chat, crate::agent::CLAUDE, "/work".into(), None, None, None, text, &[], id, &[])
                    .await
                    .unwrap();
            }
        };
        // The project's first chat has no others to name.
        send("s1", "Fix the login bug", "m1").await;
        assert_eq!(rig.told(), vec!["say Fix the login bug"]);
        // The next chat is told about it, once.
        send("s2", "same deal as the login fix", "m2").await;
        // (The first chat's title may have been written by now, or not yet.)
        let told = rig.told();
        assert!(
            told[1].starts_with(
                "say same deal as the login fix\n\n(From the app, not the user: Recent chats in this project \
(id, last used, title), readable with the harness_chats tools: s1 · just now · "
            ),
            "{}",
            told[1]
        );
        assert!(told[1].ends_with("Fix the login bug.)"), "{}", told[1]);
        send("s2", "and one more thing", "m3").await;
        assert_eq!(rig.told()[2], "say and one more thing");
        // What is saved is what the user typed.
        let AgentEvent::UserMessage { text, .. } = &rig.store.events("s2").unwrap()[0] else {
            panic!("not a user message");
        };
        assert_eq!(text, "same deal as the login fix");
    }

    #[tokio::test]
    async fn a_missing_image_fails_the_send_before_anything_is_saved() {
        let rig = rig(LONG);
        let sent = rig
            .manager
            .send_message("s1", crate::agent::CLAUDE, "/work".into(), None, None, None, "[image 4]", &[4], "m1", &[])
            .await;
        assert!(sent.is_err());
        assert!(rig.told().is_empty());
        assert!(rig.store.session("s1").unwrap().is_none());
    }

    #[tokio::test]
    async fn pictures_in_tool_results_are_kept_as_files_not_in_history() {
        let rig = rig(LONG);
        rig.send("Look at the logo").await;
        rig.agent_says(AgentEvent::ToolResult {
            tool_id: "t1".into(),
            content: serde_json::json!([
                { "type": "text", "text": "a picture" },
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "cGljdHVyZQ==" } },
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "not base64!" } },
            ]),
            is_error: false,
            parent_tool_id: None,
        });
        let saved = rig.store.events("s1").unwrap();
        let AgentEvent::ToolResult { content, .. } = saved.last().unwrap() else {
            panic!("not a tool result");
        };
        assert_eq!(content[1], serde_json::json!({ "type": "image_ref", "n": 1, "media_type": "image/png" }));
        assert_eq!(content[2]["type"], "text");
        assert!(!serde_json::to_string(content).unwrap().contains("cGljdHVyZQ"));
        assert_eq!(rig.manager.images().read("s1", 1).unwrap().0, b"picture".to_vec());
    }

    #[tokio::test]
    async fn a_new_chat_gets_a_written_title_once() {
        let rig = rig(LONG);
        rig.send("Hello there").await;
        rig.send("Second").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(rig.store.session("s1").unwrap().unwrap().title, "Written: Hello there");
        let told = rig.seen.lock().unwrap().iter().filter(|e| matches!(e, AgentEvent::TitleChanged { .. })).count();
        assert_eq!(told, 1);
        // Not saved in the history: the store holds the title.
        assert!(!rig.store.events("s1").unwrap().iter().any(|e| matches!(e, AgentEvent::TitleChanged { .. })));
    }

    #[tokio::test]
    async fn a_batch_label_is_saved_with_the_chat() {
        let rig = rig(LONG);
        rig.send("Fix login").await;
        let agents = vec!["review auth".to_string(), "run tests".to_string()];
        rig.manager.title_batch("s1", "ag1", "Fix login", agents).await.unwrap();
        let labelled = AgentEvent::BatchTitled { tool_id: "ag1".into(), title: "Fix login: review auth, run tests".into() };
        assert!(rig.store.events("s1").unwrap().contains(&labelled));
        assert!(rig.seen.lock().unwrap().contains(&labelled));
        // A chat that doesn't exist gets nothing.
        assert!(rig.manager.title_batch("nope", "ag1", "", vec![]).await.is_err());
    }

    #[tokio::test]
    async fn a_saved_doc_is_noted_in_the_chat_history() {
        let rig = rig(LONG);
        rig.send("Write it up").await;
        rig.manager.doc_saved("s1", "/notes/s1/report.md", "/proj/docs/report.md");
        let moved = AgentEvent::DocSaved { from: "/notes/s1/report.md".into(), to: "/proj/docs/report.md".into() };
        assert!(rig.store.events("s1").unwrap().contains(&moved));
        assert!(rig.seen.lock().unwrap().contains(&moved));
    }

    #[tokio::test]
    async fn only_lasting_events_are_saved_but_all_reach_the_window() {
        let rig = rig(LONG);
        rig.send("hi").await;
        rig.agent_says(AgentEvent::TextDelta {
            message_id: "m".into(),
            block_index: 0,
            text: "par".into(),
        });
        rig.agent_says(AgentEvent::ModeChanged {
            mode: "acceptEdits".into(),
        });
        let saved = rig.store.events("s1").unwrap();
        assert_eq!(saved.len(), 2); // the user message and the mode change
        assert!(rig.seen.lock().unwrap().iter().any(|e| matches!(e, AgentEvent::TextDelta { .. })));
        assert_eq!(rig.store.session("s1").unwrap().unwrap().mode, "acceptEdits");
    }

    #[tokio::test]
    async fn an_exited_chat_resumes_with_the_agents_own_id() {
        let rig = rig(LONG);
        rig.send("one").await;
        rig.agent_says(AgentEvent::SessionStarted {
            agent_session_id: "agent-7".into(),
            model: None,
            mode: None,
        });
        rig.turn_ends();
        rig.manager.close("s1");
        assert!(!rig.manager.is_live("s1"));
        rig.send("two").await;
        assert_eq!(rig.fake.spawns.lock().unwrap()[1].0.as_deref(), Some("agent-7"));
        assert!(rig.manager.is_live("s1"));
    }

    #[tokio::test]
    async fn an_idle_chat_is_closed_but_a_used_one_is_not() {
        let rig = rig(Duration::from_millis(60));
        rig.send("one").await;
        rig.turn_ends();
        // Used again before the idle time is up: the first timer must not close it.
        tokio::time::sleep(Duration::from_millis(30)).await;
        rig.send("two").await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(rig.manager.is_live("s1"), "closed while a turn was running");
        rig.turn_ends();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(!rig.manager.is_live("s1"), "not closed after going idle");
        assert_eq!(rig.told().iter().filter(|t| *t == "close").count(), 1);
    }

    #[tokio::test]
    async fn answers_and_stops_are_saved_for_replay() {
        let rig = rig(LONG);
        rig.send("hi").await;
        rig.manager
            .answer_approval("s1", "r1", Decision::Allow, None, None, None)
            .unwrap();
        rig.manager.interrupt("s1").unwrap();
        let saved = rig.store.events("s1").unwrap();
        assert!(matches!(&saved[1], AgentEvent::ApprovalAnswered { request_id, .. } if request_id == "r1"));
        assert_eq!(saved[2], AgentEvent::Interrupted);
        // An edited plan is saved too, before its answer.
        rig.manager
            .answer_approval("s1", "r2", Decision::Allow, None, None, Some("my plan".into()))
            .unwrap();
        let saved = rig.store.events("s1").unwrap();
        let n = saved.len();
        assert_eq!(
            saved[n - 2],
            AgentEvent::PlanEdited { request_id: "r2".into(), plan: "my plan".into() }
        );
        assert!(matches!(&saved[n - 1], AgentEvent::ApprovalAnswered { request_id, .. } if request_id == "r2"));
        // With no process there is nothing to answer.
        assert!(rig.manager.interrupt("other").is_err());
    }

    #[tokio::test]
    async fn a_message_sent_while_claude_waits_answers_what_it_waits_on() {
        let rig = rig(LONG);
        rig.send("make a plan").await;
        rig.agent_says(AgentEvent::ApprovalRequest {
            request_id: "p1".into(),
            approval: crate::events::ApprovalKind::Plan,
            tool_name: "ExitPlanMode".into(),
            title: "ExitPlanMode".into(),
            input: serde_json::json!({ "plan": "do it" }),
            tool_id: None,
            always_label: None,
        });
        rig.send("smaller, please").await;
        let told = rig.told();
        let at = |line: &str| told.iter().position(|t| t == line).unwrap_or_else(|| panic!("{line}: {told:?}"));
        assert!(at("answer p1 deny") < at("say smaller, please"));
        let saved = rig.store.events("s1").unwrap();
        assert!(saved.iter().any(|e| matches!(
            e,
            AgentEvent::ApprovalAnswered { request_id, decision: Decision::Deny, .. } if request_id == "p1"
        )));
        // Nothing open: a message is just a message.
        rig.send("thanks").await;
        assert_eq!(rig.told().iter().filter(|t| t.starts_with("answer")).count(), 1);
    }

    #[tokio::test]
    async fn deleting_a_chat_forgets_it() {
        let rig = rig(LONG);
        rig.send("hi").await;
        rig.manager.delete("s1").await.unwrap();
        assert!(!rig.manager.is_live("s1"));
        assert!(rig.store.sidebar().unwrap().is_empty());
        // In the trash, with its history, until the trash is emptied.
        assert_eq!(rig.manager.list_trash().unwrap()[0].id, "s1");
        // Stopped, so background work it had doesn't run on unseen.
        assert_eq!(rig.count("kill"), 1);
        // Late events from its process are dropped, not saved.
        let before = rig.store.events("s1").unwrap().len();
        rig.agent_says(AgentEvent::Interrupted);
        assert_eq!(rig.store.events("s1").unwrap().len(), before);
        rig.manager.empty_trash().await.unwrap();
        assert!(rig.manager.list_trash().unwrap().is_empty());
        assert!(rig.store.session("s1").unwrap().is_none());
    }

    #[tokio::test]
    async fn an_idle_chat_with_background_work_stays_open() {
        let rig = rig(Duration::from_millis(40));
        rig.send("start a dev server").await;
        rig.background(1);
        rig.turn_ends();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(rig.manager.is_live("s1"), "closed while background work ran");
        assert_eq!(rig.count("close"), 0);
        // Still takes messages as usual.
        rig.send("is it up?").await;
        assert_eq!(rig.fake.spawns.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn background_work_ending_starts_the_idle_timer() {
        let rig = rig(Duration::from_millis(40));
        rig.send("caffeinate").await;
        rig.background(1);
        rig.turn_ends();
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(rig.manager.is_live("s1"));
        // Ends with no follow-up turn: the timer must still start.
        rig.background(0);
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(!rig.manager.is_live("s1"), "not closed after background work ended");
        assert_eq!(rig.count("close"), 1);
    }

    #[tokio::test]
    async fn a_message_while_closing_stops_the_old_process_and_resumes() {
        let rig = rig(Duration::from_millis(20));
        rig.fake.hang_on_close.store(true, Ordering::SeqCst);
        rig.send("one").await;
        rig.agent_says(AgentEvent::SessionStarted {
            agent_session_id: "agent-7".into(),
            model: None,
            mode: None,
        });
        rig.turn_ends();
        // Idle close fires; the process ignores it.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(rig.count("close"), 1);
        assert!(rig.manager.is_live("s1"));
        // A message now doesn't wait out the grace or fail.
        rig.send("two").await;
        assert_eq!(rig.told(), vec!["say one", "close", "kill", "say two"]);
        let spawns = rig.fake.spawns.lock().unwrap();
        assert_eq!(spawns.len(), 2);
        assert_eq!(spawns[1].0.as_deref(), Some("agent-7"));
    }

    #[tokio::test]
    async fn a_closing_process_that_hangs_is_stopped_after_the_grace() {
        let rig = rig(Duration::from_millis(20));
        rig.fake.hang_on_close.store(true, Ordering::SeqCst);
        rig.send("one").await;
        rig.turn_ends();
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(rig.manager.is_live("s1"), "stopped before the grace was up");
        assert_eq!(rig.count("kill"), 0);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(rig.count("kill"), 1);
        assert!(!rig.manager.is_live("s1"));
    }


    // ---- Worktrees ----

    use std::process::Command;

    /// A scratch git repo with one commit on main.
    fn scratch_repo() -> PathBuf {
        let root = std::env::temp_dir().join(format!("harness-repo-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git").arg("-C").arg(&root).args(args).output().unwrap().status.success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(root.join("app.py"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "init"]);
        root
    }

    impl Rig {
        async fn send_at(&self, folder: &Path, mode: &str, text: &str) {
            self.manager
                .send_message("s1", crate::agent::CLAUDE, folder.into(), Some(mode.into()), None, None, text, &[], "m", &[])
                .await
                .unwrap();
        }
        fn saw(&self, pick: impl Fn(&AgentEvent) -> bool) -> usize {
            self.seen.lock().unwrap().iter().filter(|e| pick(e)).count()
        }
        fn worktree(&self) -> Option<worktree::Worktree> {
            let git = self.store.chat_git("s1").unwrap().unwrap();
            git.worktree.filter(|_| git.status.is_some())
        }
        /// Runs a full sweep, after any one already started.
        async fn sweep(&self) {
            drop(self.manager.0.sweep_lock.lock().await);
            self.manager.sweep().await;
        }
        /// The agent calls the worktree tool; returns what the call answers.
        async fn ask(&self, name: &str) -> String {
            let tools = self.manager.worktree_tools("s1");
            let content = (tools.call)("ask".into(), serde_json::json!({ "name": name })).await.unwrap();
            content[0]["text"].as_str().unwrap().to_string()
        }
        /// The questions shown so far.
        fn asks(&self) -> usize {
            self.saw(|e| matches!(e, AgentEvent::WorktreeAsk { .. }))
        }
        /// The latest question's ID.
        fn card(&self) -> String {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find_map(|e| match e {
                    AgentEvent::WorktreeAsk { request_id, .. } => Some(request_id.clone()),
                    _ => None,
                })
                .expect("a question")
        }
        /// Starts a chat in `repo`, asks for a worktree, and picks a new one.
        async fn into_worktree(&self, repo: &Path) -> worktree::Worktree {
            self.send_at(repo, "auto", "fix it").await;
            self.agent_says(AgentEvent::SessionStarted {
                agent_session_id: "agent-9".into(),
                model: None,
                mode: Some("auto".into()),
            });
            self.ask("fix it").await;
            self.manager
                .choose_worktree("s1", &self.card(), "worktree")
                .await
                .unwrap();
            self.worktree().expect("a worktree")
        }
    }

    #[tokio::test]
    async fn a_worktree_request_shows_a_question_and_work_here_says_so() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        rig.send_at(&repo, "default", "fix it").await;

        // The call returns at once, telling the agent to end its turn.
        assert!(rig.ask("fix it").await.contains("End your turn"));
        assert_eq!(rig.asks(), 1);
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::WorktreeAsk { name, .. } if name.as_deref() == Some("fix it"))), 1);
        // Asked again while it's open: no second question.
        assert!(rig.ask("fix it").await.contains("already being asked"));
        assert_eq!(rig.asks(), 1);

        // A wrong ID is refused.
        assert!(rig.manager.choose_worktree("s1", "nope", "here").await.is_err());
        rig.manager.choose_worktree("s1", &rig.card(), "here").await.unwrap();
        assert!(rig.told().iter().any(|t| t.starts_with("say The user answered the worktree question: do this work here")));
        assert!(rig.worktree().is_none());
        assert_eq!(rig.store.chat_git("s1").unwrap().unwrap().choice.as_deref(), Some("here"));
        // The note isn't saved as a message from the user.
        let saved = rig.store.events("s1").unwrap();
        assert!(!saved.iter().any(|e| matches!(e, AgentEvent::UserMessage { text, .. } if text.contains("worktree question"))));

        // The agent may ask again later, for other work.
        rig.ask("other").await;
        assert_eq!(rig.asks(), 2);
    }

    #[tokio::test]
    async fn a_message_answers_the_worktree_question_with_nothing_chosen() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        rig.send_at(&repo, "default", "fix it").await;
        rig.ask("fix it").await;
        let card = rig.card();
        rig.send_at(&repo, "default", "actually, rename it too").await;

        // The message goes with a note saying it answers the question.
        let said = rig.told().into_iter().find(|t| t.contains("actually, rename it too")).unwrap();
        assert!(said.starts_with("say (From the app, not the user: The user sent the message below instead of answering the worktree question."), "{said}");
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::ApprovalAnswered { request_id, .. } if *request_id == card)), 1);
        assert!(rig.manager.choose_worktree("s1", &card, "here").await.is_err());
        assert!(rig.worktree().is_none());
        assert_eq!(rig.store.chat_git("s1").unwrap().unwrap().choice, None);
        // The saved message is the user's own words.
        let saved = rig.store.events("s1").unwrap();
        assert!(saved.iter().any(|e| matches!(e, AgentEvent::UserMessage { text, .. } if text == "actually, rename it too")));

        // A later message carries no note.
        rig.send_at(&repo, "default", "and one more").await;
        assert!(rig.told().iter().any(|t| t == "say and one more"));

        // The agent may ask again after reading the message.
        rig.ask("fix it").await;
        assert_eq!(rig.asks(), 2);
    }

    #[tokio::test]
    async fn an_open_question_survives_a_restart() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        rig.send_at(&repo, "auto", "fix it").await;
        rig.ask("fix it").await;
        let card = rig.card();
        rig.agent_says(AgentEvent::TurnEnded { ok: true, duration_ms: Some(1) });
        // What a restart loses: the question in memory.
        let forget = |r: &Rig| {
            let mut sessions = r.manager.0.sessions.lock().unwrap();
            let session = sessions.get_mut("s1").unwrap();
            session.ask = None;
            session.ask_checked = false;
        };
        forget(&rig);
        // It comes back from the saved history, and can be answered.
        rig.manager.choose_worktree("s1", &card, "here").await.unwrap();
        assert_eq!(rig.store.chat_git("s1").unwrap().unwrap().choice.as_deref(), Some("here"));

        // An answered one doesn't come back.
        forget(&rig);
        assert!(rig.manager.choose_worktree("s1", &card, "worktree").await.is_err());

        // A new open one is closed by a message, with the note.
        rig.ask("other").await;
        forget(&rig);
        rig.send_at(&repo, "auto", "never mind").await;
        assert!(rig.told().iter().any(|t| t.contains("instead of answering the worktree question") && t.ends_with("never mind")));
    }

    #[tokio::test]
    async fn a_request_outside_a_repo_is_turned_down_at_once() {
        let rig = rig(LONG);
        let folder = std::env::temp_dir().join(format!("harness-plain-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&folder).unwrap();
        rig.send_at(&folder, "auto", "hi").await;
        assert!(rig.ask("x").await.contains("isn't in a git repo"));
        assert_eq!(rig.asks(), 0);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[tokio::test]
    async fn the_question_names_the_call_that_asked() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        rig.send_at(&repo, "auto", "fix it").await;
        let tool_id = |r: &Rig| {
            r.seen.lock().unwrap().iter().rev().find_map(|e| match e {
                AgentEvent::WorktreeAsk { tool_id, .. } => Some(tool_id.clone()),
                _ => None,
            })
        };
        // Claude Code: the call arrives whole.
        rig.agent_says(AgentEvent::AssistantBlock {
            message_id: "m1".into(),
            block: Block::ToolCall { tool_id: "t1".into(), name: worktrees::CALL_NAME.into(), input: serde_json::json!({}) },
            parent_tool_id: None,
        });
        rig.ask("x").await;
        assert_eq!(tool_id(&rig), Some(Some("t1".to_string())));
        rig.manager.choose_worktree("s1", &rig.card(), "here").await.unwrap();

        // Cursor: its name comes in a later update.
        rig.agent_says(AgentEvent::ToolInput {
            tool_id: "t2".into(),
            name: worktrees::CALL_NAME.into(),
            input: serde_json::json!({}),
            parent_tool_id: None,
        });
        rig.ask("y").await;
        assert_eq!(tool_id(&rig), Some(Some("t2".to_string())));
        rig.manager.choose_worktree("s1", &rig.card(), "here").await.unwrap();

        // Each call is used once.
        rig.ask("z").await;
        assert_eq!(tool_id(&rig), Some(None));
    }

    #[tokio::test]
    async fn a_new_worktree_moves_the_chat_and_resumes_it_there() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        let wt = rig.into_worktree(&repo).await;

        let told = rig.told();
        let at = |what: &str| told.iter().position(|t| t.starts_with(what)).unwrap_or_else(|| panic!("{what} in {told:?}"));
        assert!(at("stop") < at("kill") && at("kill") < at("say You now work"));
        // Named after the agent's suggestion.
        assert!(wt.branch.starts_with("harness/fix-it-"), "{}", wt.branch);
        let note = &told[at("say You now work")];
        assert!(note.contains(&wt.work_path().display().to_string()) && note.contains(&wt.branch));

        // Resumed, as the same chat, in the worktree, without the check.
        assert_eq!(rig.fake.spawns.lock().unwrap()[1].0.as_deref(), Some("agent-9"));
        assert_eq!(rig.fake.folders.lock().unwrap()[1], wt.work_path());
        assert!(wt.path.join("app.py").exists());

        assert_eq!(wt.base_branch.as_deref(), Some("main"));
        // The note isn't shown as a message from the user.
        let saved = rig.store.events("s1").unwrap();
        assert!(!saved.iter().any(|e| matches!(e, AgentEvent::UserMessage { text, .. } if text.starts_with("You now"))));
        assert!(saved.iter().any(|e| matches!(e, AgentEvent::WorktreeChanged { status, .. } if status == "created")));
        let row = rig.store.session("s1").unwrap().unwrap();
        assert_eq!(row.work_path, Some(wt.work_path().to_string_lossy().into_owned()));
        assert_eq!(row.project_path, repo.to_string_lossy());

        // Asking again inside the worktree is turned down without a question.
        assert!(rig.ask("again").await.contains("already works in its own git worktree"));
        assert_eq!(rig.asks(), 1);

        // Its next start is in the worktree too.
        rig.manager.close("s1");
        rig.send_at(&repo, "auto", "more").await;
        assert_eq!(rig.fake.folders.lock().unwrap()[2], wt.work_path());
    }

    #[tokio::test]
    async fn an_idle_worktree_is_put_away_and_comes_back_with_its_edits() {
        let rig = rig_with(LONG, Duration::ZERO, LONG, PrLookup::none());
        let repo = scratch_repo();
        let wt = rig.into_worktree(&repo).await;
        std::fs::write(wt.path.join("app.py"), "half done\n").unwrap();

        // Not while its process runs.
        rig.sweep().await;
        assert!(wt.path.exists());
        rig.manager.close("s1");
        tokio::time::sleep(Duration::from_millis(5)).await;
        rig.sweep().await;
        assert!(!wt.path.exists(), "folder put away");
        assert_eq!(rig.store.chat_git("s1").unwrap().unwrap().status.as_deref(), Some("put_away"));

        rig.send_at(&repo, "auto", "back to it").await;
        assert_eq!(std::fs::read_to_string(wt.path.join("app.py")).unwrap(), "half done\n");
        assert_eq!(rig.store.chat_git("s1").unwrap().unwrap().status.as_deref(), Some("live"));
        assert_eq!(*rig.fake.folders.lock().unwrap().last().unwrap(), wt.work_path());
        assert!(rig.saw(|e| matches!(e, AgentEvent::WorktreeChanged { status, .. } if status == "restored")) == 1);
    }

    #[tokio::test]
    async fn a_worktree_with_a_terminal_in_it_stays_and_a_deleted_chat_tells_the_terminal() {
        let rig = rig_with(LONG, Duration::ZERO, LONG, PrLookup::none());
        let repo = scratch_repo();
        let wt = rig.into_worktree(&repo).await;
        let shell_in: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
        let told: Arc<Mutex<Vec<(PathBuf, String)>>> = Arc::default();
        {
            let (shell_in, told) = (shell_in.clone(), told.clone());
            rig.manager.set_terminal_hooks(TerminalHooks {
                in_use: Arc::new(move |_: &Path| *shell_in.lock().unwrap()),
                gone: Arc::new(move |root: &Path, why: &str| told.lock().unwrap().push((root.to_path_buf(), why.to_string()))),
            });
        }
        rig.manager.close("s1");
        tokio::time::sleep(Duration::from_millis(5)).await;
        rig.sweep().await;
        assert!(wt.path.exists(), "kept while a shell sits in it");
        *shell_in.lock().unwrap() = false;

        rig.manager.delete("s1").await.unwrap();
        assert!(!wt.path.exists());
        assert_eq!(*told.lock().unwrap(), vec![(wt.path.clone(), "chat deleted".to_string())]);
    }

    #[tokio::test]
    async fn a_merged_pull_request_removes_the_worktree_and_branch() {
        let found: Arc<Mutex<Option<PrInfo>>> = Arc::default();
        let lookup = {
            let found = found.clone();
            PrLookup {
                by_branch: Arc::new(move |_: &Path, _: &str| Ok(found.lock().unwrap().clone())),
                ..PrLookup::none()
            }
        };
        let rig = rig_with(LONG, LONG, LONG, lookup);
        let repo = scratch_repo();
        let wt = rig.into_worktree(&repo).await;
        std::fs::write(wt.path.join("app.py"), "done\n").unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        git(&wt.path, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qam", "work"]);
        let tip = git(&wt.path, &["rev-parse", "HEAD"]);

        // An open PR is shown, and nothing is removed.
        let pr = |state: &str| PrInfo {
            number: 3,
            state: state.into(),
            url: "u".into(),
            head: tip.clone(),
            branch: wt.branch.clone(),
        };
        *found.lock().unwrap() = Some(pr("open"));
        rig.turn_ends();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(rig.store.session("s1").unwrap().unwrap().prs[0].state, "open");
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::PrsChanged { .. })), 1);

        // Squash-merged on GitHub: gone after the next sweep once the process is closed.
        *found.lock().unwrap() = Some(pr("merged"));
        rig.manager.close("s1");
        rig.manager.0.pr_checked.lock().unwrap().clear();
        rig.sweep().await;
        assert!(!wt.path.exists());
        assert!(git(&repo, &["branch", "--list", &wt.branch]).is_empty(), "branch deleted");
        let row = rig.store.chat_git("s1").unwrap().unwrap();
        assert_eq!((row.status, row.choice), (None, None));
        // The merged pull request stays, so the sidebar keeps its purple mark.
        assert_eq!(row.prs.iter().map(|pr| pr.state.as_str()).collect::<Vec<_>>(), ["merged"]);
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::WorktreeChanged { status, .. } if status == "merged")), 1);
    }

    #[tokio::test]
    async fn a_trashed_worktree_chat_is_restorable_then_purged_with_its_branch() {
        let rig = rig_with(LONG, LONG, Duration::ZERO, PrLookup::none());
        let repo = scratch_repo();
        let wt = rig.into_worktree(&repo).await;
        std::fs::write(wt.path.join("app.py"), "unsaved\n").unwrap();

        rig.manager.delete("s1").await.unwrap();
        assert!(!wt.path.exists(), "folder put away on delete");
        assert_eq!(rig.manager.list_trash().unwrap().len(), 1);
        // Restored, then brought back by a message.
        rig.manager.restore_chat("s1").unwrap();
        rig.send_at(&repo, "auto", "again").await;
        assert_eq!(std::fs::read_to_string(wt.path.join("app.py")).unwrap(), "unsaved\n");

        // Deleted again; with no keep time the sweep purges it.
        rig.manager.delete("s1").await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        rig.sweep().await;
        assert!(rig.store.session("s1").unwrap().is_none());
        let branches = Command::new("git").arg("-C").arg(&repo).args(["branch", "--list", &wt.branch]).output().unwrap();
        assert!(branches.stdout.is_empty());
    }

    #[tokio::test]
    async fn a_chat_keeps_the_agent_it_started_on() {
        let rig = rig(LONG);
        // Cursor isn't installed here: a new chat on it can't start, and nothing is saved.
        let err = rig
            .manager
            .send_message("c1", crate::agent::CURSOR, "/work".into(), None, None, None, "hi", &[], "m0", &[])
            .await
            .unwrap_err();
        assert!(err.contains("Unknown agent"), "{err}");
        assert!(rig.store.session("c1").unwrap().is_none());

        rig.send("hi").await;
        assert_eq!(rig.store.session("s1").unwrap().unwrap().agent, crate::agent::CLAUDE);
        rig.turn_ends();
        // A later send naming another agent still goes to the chat's own.
        rig.manager
            .send_message("s1", crate::agent::CURSOR, "/work".into(), None, None, None, "again", &[], "m2", &[])
            .await
            .unwrap();
        assert_eq!(rig.store.session("s1").unwrap().unwrap().agent, crate::agent::CLAUDE);
    }

    #[tokio::test]
    async fn quitting_stops_chats_with_background_work_and_closes_the_rest() {
        let rig = rig(LONG);
        rig.send("caffeinate").await;
        rig.background(1);
        rig.turn_ends();
        rig.manager
            .send_message("s2", crate::agent::CLAUDE, "/work".into(), None, None, None, "hi", &[], "m2", &[])
            .await
            .unwrap();
        rig.agent_says(AgentEvent::TurnEnded {
            ok: true,
            duration_ms: None,
        });
        rig.agent_says(AgentEvent::Status {
            state: SessionState::Idle,
        });
        rig.manager.shutdown(Duration::from_millis(50));
        assert!(!rig.manager.is_live("s1") && !rig.manager.is_live("s2"));
        assert_eq!(rig.count("kill"), 1, "the background chat is stopped");
        assert_eq!(rig.count("close"), 1, "the plain chat is closed");
    }
}
