//! Keeps track of chats and their live agent processes: start on the first
//! message, resume after an exit, shut a process down once its chat has been
//! idle for a while, and save what happens. See PLAN.md, "Session lifecycle".
//! Worktrees and the trash are in `sessions/worktrees.rs`.

mod worktrees;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::agent::{AgentAdapter, EventSink, RunningAgent, SpawnOptions};
use crate::events::{AgentEvent, Decision, Envelope, SessionState};
use crate::store::Store;
use crate::worktree;

pub use worktrees::{PrLookup, SWEEP_EVERY};

/// Where every chat's events go once saved: the window.
pub type Out = Arc<dyn Fn(Envelope) + Send + Sync>;

/// How long a chat sits idle before its process is asked to exit.
const DEFAULT_IDLE_SECONDS: u64 = 300;
/// Longest tool result kept, in characters. See `AgentEvent::cap_tool_result`.
const TOOL_RESULT_MAX_CHARS: usize = 20_000;
/// How long a process asked to exit may take before it is stopped.
const CLOSE_GRACE: Duration = Duration::from_secs(30);
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
}

/// The agent asked for a worktree, and waits for the user's answer.
struct Ask {
    repo_root: PathBuf,
    /// Requests waiting for the answer, as (request ID, the request).
    held: Vec<(String, AgentEvent)>,
    /// The request ID naming the question on screen.
    card: String,
    /// The name the agent suggested, used for the branch.
    name: Option<String>,
}

struct Inner {
    adapter: Arc<dyn AgentAdapter>,
    store: Arc<Store>,
    out: Out,
    sessions: Mutex<HashMap<String, Session>>,
    /// Starting a process is not instant; two sends must not both start one.
    start_lock: tokio::sync::Mutex<()>,
    timings: Timings,
    /// The app's data folder; worktrees are kept in it.
    data_dir: PathBuf,
    /// Finds a branch's pull request (GitHub in the app, a stand-in in tests).
    pr_lookup: PrLookup,
    /// One cleanup sweep at a time.
    sweep_lock: tokio::sync::Mutex<()>,
    /// When each chat's pull request was last looked up.
    pr_checked: Mutex<HashMap<String, Instant>>,
}

#[derive(Clone)]
pub struct SessionManager(Arc<Inner>);

impl SessionManager {
    pub fn new(adapter: Arc<dyn AgentAdapter>, store: Arc<Store>, out: Out, data_dir: PathBuf) -> Self {
        let timings = Timings {
            idle: None,
            close_grace: CLOSE_GRACE,
            worktree_idle: None,
            keep: None,
        };
        let pr_lookup: PrLookup = Arc::new(crate::github::pr_for);
        Self::build(adapter, store, out, timings, data_dir, pr_lookup)
    }

    fn build(
        adapter: Arc<dyn AgentAdapter>,
        store: Arc<Store>,
        out: Out,
        timings: Timings,
        data_dir: PathBuf,
        pr_lookup: PrLookup,
    ) -> Self {
        Self(Arc::new(Inner {
            adapter,
            store,
            out,
            sessions: Mutex::default(),
            start_lock: tokio::sync::Mutex::default(),
            timings,
            data_dir,
            pr_lookup,
            sweep_lock: tokio::sync::Mutex::default(),
            pr_checked: Mutex::default(),
        }))
    }

    /// The agent adapter, for calls that don't belong to one chat.
    pub fn adapter(&self) -> Arc<dyn AgentAdapter> {
        self.0.adapter.clone()
    }

    /// Idle time before a chat's process is closed. None = never.
    /// Order: test override, `HARNESS_IDLE_SECONDS`, the `idle_seconds` setting,
    /// then the default. Zero means never.
    fn idle_time(&self) -> Option<Duration> {
        if let Some(fixed) = self.0.timings.idle {
            return Some(fixed);
        }
        let seconds = std::env::var("HARNESS_IDLE_SECONDS")
            .ok()
            .or_else(|| self.0.store.setting("idle_seconds"))
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_IDLE_SECONDS);
        (seconds > 0).then(|| Duration::from_secs(seconds))
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
    /// switched with `set_mode`, `set_model` and `set_effort`).
    #[allow(clippy::too_many_arguments)]
    pub async fn send_message(
        &self,
        session_id: &str,
        binary: PathBuf,
        project_path: PathBuf,
        mode: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        text: &str,
        message_id: &str,
    ) -> Result<(), String> {
        self.send_inner(session_id, binary, project_path, mode, model, effort, text, message_id, true)
            .await
    }

    /// `show` false sends a note from the app to the agent that the chat
    /// doesn't show or save as a user message.
    #[allow(clippy::too_many_arguments)]
    async fn send_inner(
        &self,
        session_id: &str,
        binary: PathBuf,
        project_path: PathBuf,
        mode: Option<String>,
        model: Option<String>,
        // None keeps the saved one.
        effort: Option<String>,
        text: &str,
        message_id: &str,
        show: bool,
    ) -> Result<(), String> {
        let inner = &self.0;

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
                },
            );
        }
        if created {
            let folder = project_path.clone();
            if let Some((branch, commit)) = blocking(move || worktree::head_of(&folder)).await {
                let _ = inner.store.set_start(session_id, branch.as_deref(), &commit);
            }
            self.write_title(session_id, binary.clone(), text);
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
            let agent = inner.adapter.spawn(
                SpawnOptions {
                    binary,
                    project_path: folder,
                    resume_id,
                    mode,
                    model,
                    effort: saved.map(|s| s.effort),
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

        self.agent(session_id)?.send_user_message(text, message_id)
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
    /// if it fails, the chat keeps the title cut from its first message.
    fn write_title(&self, session_id: &str, binary: PathBuf, first_message: &str) {
        let manager = self.clone();
        let id = session_id.to_string();
        let message = first_message.to_string();
        tokio::spawn(async move {
            let adapter = manager.0.adapter.clone();
            let title = tokio::task::spawn_blocking(move || adapter.make_title(&binary, &message)).await;
            let Ok(Ok(title)) = title else {
                return;
            };
            if manager.0.store.set_generated_title(&id, &title).unwrap_or(false) {
                manager.record(&id, AgentEvent::TitleChanged { title });
            }
        });
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
            // The agent's request for a worktree waits for the user's answer.
            let Some(event) = manager.screen_worktree_call(&id, event) else {
                return;
            };
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
                AgentEvent::Status { state } => manager.on_state(&id, run, state),
                AgentEvent::BackgroundWork { running } => manager.on_background(&id, run, *running > 0),
                AgentEvent::TurnEnded { .. } => manager.after_turn(&id),
                _ => {}
            }
            event.cap_tool_result(TOOL_RESULT_MAX_CHARS);
            manager.record(&id, event);
        })
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
    ) -> Result<(), String> {
        self.agent(session_id)?
            .answer_approval(request_id, decision.clone(), message, answers.clone())?;
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
    use crate::events::ModelInfo;
    use crate::worktree::PrInfo;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};

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
    }

    struct FakeAgent {
        sink: EventSink,
        told: Arc<Mutex<Vec<String>>>,
        hang_on_close: Arc<AtomicBool>,
    }

    impl RunningAgent for FakeAgent {
        fn send_user_message(&self, text: &str, _id: &str) -> Result<(), String> {
            self.told.lock().unwrap().push(format!("say {text}"));
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
        ) -> Result<(), String> {
            let decision = format!("{decision:?}").to_lowercase();
            self.told.lock().unwrap().push(format!("answer {request_id} {decision}"));
            (self.sink)(AgentEvent::Status {
                state: SessionState::Working,
            });
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
            self.sinks.lock().unwrap().push(sink.clone());
            sink(AgentEvent::Status {
                state: SessionState::Starting,
            });
            Ok(Box::new(FakeAgent {
                sink,
                told: self.told.clone(),
                hang_on_close: self.hang_on_close.clone(),
            }))
        }
        fn list_models(&self, _: &Path) -> Result<Vec<ModelInfo>, String> {
            Ok(Vec::new())
        }
        fn make_title(&self, _: &Path, first_message: &str) -> Result<String, String> {
            Ok(format!("Written: {first_message}"))
        }
        fn sign_in(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
    }

    struct Rig {
        fake: Arc<Fake>,
        store: Arc<Store>,
        seen: Arc<Mutex<Vec<AgentEvent>>>,
        manager: SessionManager,
    }

    fn rig(idle: Duration) -> Rig {
        rig_with(idle, LONG, LONG, Arc::new(|_: &Path, _: &str| Ok(None)))
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
        let manager = SessionManager::build(fake.clone(), store.clone(), out, timings, dir, lookup);
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
                .send_message("s1", "claude".into(), "/work".into(), Some("plan".into()), None, None, text, "m1")
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
            self.agent_says(AgentEvent::BackgroundWork { running });
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
            }]
        );
        // A second message goes to the same process.
        rig.send("And again").await;
        assert_eq!(rig.fake.spawns.lock().unwrap().len(), 1);
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
            .answer_approval("s1", "r1", Decision::Allow, None, None)
            .unwrap();
        rig.manager.interrupt("s1").unwrap();
        let saved = rig.store.events("s1").unwrap();
        assert!(matches!(&saved[1], AgentEvent::ApprovalAnswered { request_id, .. } if request_id == "r1"));
        assert_eq!(saved[2], AgentEvent::Interrupted);
        // With no process there is nothing to answer.
        assert!(rig.manager.interrupt("other").is_err());
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

    use crate::events::ApprovalKind;
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

    /// The agent asking for a worktree (Claude Code's own tool, which asks).
    fn worktree_call(id: &str, name: &str) -> AgentEvent {
        AgentEvent::ApprovalRequest {
            request_id: id.into(),
            approval: ApprovalKind::Tool,
            tool_name: "EnterWorktree".into(),
            title: "EnterWorktree".into(),
            input: serde_json::json!({ "name": name }),
            tool_id: Some(format!("t-{id}")),
            always_label: None,
        }
    }

    fn edit(id: &str, path: &Path) -> AgentEvent {
        AgentEvent::ApprovalRequest {
            request_id: id.into(),
            approval: ApprovalKind::Tool,
            tool_name: "Edit".into(),
            title: "Edit".into(),
            input: serde_json::json!({ "file_path": path }),
            tool_id: None,
            always_label: None,
        }
    }

    impl Rig {
        async fn send_at(&self, folder: &Path, mode: &str, text: &str) {
            self.manager
                .send_message("s1", "claude".into(), folder.into(), Some(mode.into()), None, None, text, "m")
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
        /// Starts a chat in `repo`, asks for an edit, and picks a new worktree.
        async fn into_worktree(&self, repo: &Path) -> worktree::Worktree {
            self.send_at(repo, "auto", "fix it").await;
            self.agent_says(AgentEvent::SessionStarted {
                agent_session_id: "agent-9".into(),
                model: None,
                mode: Some("auto".into()),
            });
            self.agent_says(worktree_call("r1", "fix it"));
            self.manager
                .choose_worktree("s1", "r1", "worktree", "claude".into())
                .await
                .unwrap();
            self.worktree().expect("a worktree")
        }
    }

    #[tokio::test]
    async fn a_worktree_request_waits_and_work_here_turns_it_down() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        rig.send_at(&repo, "default", "fix it").await;
        let shown = |r: &Rig| r.saw(|e| matches!(e, AgentEvent::ApprovalRequest { .. }));

        // Ordinary edits are not held.
        rig.agent_says(edit("e1", &repo.join("app.py")));
        assert_eq!(shown(&rig), 1);

        rig.agent_says(worktree_call("r1", "fix it"));
        rig.agent_says(worktree_call("r2", "fix it"));
        let asks = |r: &Rig| r.saw(|e| matches!(e, AgentEvent::WorktreeAsk { .. }));
        assert_eq!(asks(&rig), 1, "one question for both requests");
        assert_eq!(shown(&rig), 1, "requests aren't shown as approvals");
        assert!(rig.saw(|e| matches!(e, AgentEvent::WorktreeAsk { name, .. } if name.as_deref() == Some("fix it"))) == 1);

        // A wrong ID is refused.
        assert!(rig.manager.choose_worktree("s1", "r2", "here", "claude".into()).await.is_err());
        rig.manager.choose_worktree("s1", "r1", "here", "claude".into()).await.unwrap();
        assert!(rig.told().contains(&"answer r1 deny".to_string()));
        assert!(rig.told().contains(&"answer r2 deny".to_string()));
        assert!(rig.worktree().is_none());
        assert_eq!(rig.store.chat_git("s1").unwrap().unwrap().choice.as_deref(), Some("here"));

        // The agent may ask again later, for other work.
        rig.agent_says(worktree_call("r3", "other"));
        assert_eq!(asks(&rig), 2);
    }

    #[tokio::test]
    async fn a_request_outside_a_repo_is_turned_down_at_once() {
        let rig = rig(LONG);
        let folder = std::env::temp_dir().join(format!("harness-plain-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&folder).unwrap();
        rig.send_at(&folder, "auto", "hi").await;
        rig.agent_says(worktree_call("r1", "x"));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(rig.told().contains(&"answer r1 deny".to_string()));
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::WorktreeAsk { .. })), 0);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[tokio::test]
    async fn stopping_while_asked_closes_the_question() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        rig.send_at(&repo, "auto", "fix it").await;
        rig.agent_says(worktree_call("r1", "x"));
        rig.agent_says(AgentEvent::ApprovalCancelled { request_id: "r1".into() });
        assert_eq!(
            rig.saw(|e| matches!(e, AgentEvent::ApprovalCancelled { request_id } if request_id == "r1")),
            1
        );
        // A later request asks again.
        rig.agent_says(worktree_call("r2", "x"));
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::WorktreeAsk { .. })), 2);
    }

    #[tokio::test]
    async fn a_new_worktree_moves_the_chat_and_resumes_it_there() {
        let rig = rig(LONG);
        let repo = scratch_repo();
        let wt = rig.into_worktree(&repo).await;

        let told = rig.told();
        let at = |what: &str| told.iter().position(|t| t.starts_with(what)).unwrap_or_else(|| panic!("{what} in {told:?}"));
        assert!(at("answer r1 deny") < at("stop") && at("stop") < at("kill") && at("kill") < at("say You now work"));
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
        rig.agent_says(worktree_call("r9", "again"));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(rig.told().contains(&"answer r9 deny".to_string()));
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::WorktreeAsk { .. })), 1);

        // Its next start is in the worktree too.
        rig.manager.close("s1");
        rig.send_at(&repo, "auto", "more").await;
        assert_eq!(rig.fake.folders.lock().unwrap()[2], wt.work_path());
    }

    #[tokio::test]
    async fn an_idle_worktree_is_put_away_and_comes_back_with_its_edits() {
        let rig = rig_with(LONG, Duration::ZERO, LONG, Arc::new(|_: &Path, _: &str| Ok(None)));
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
    async fn a_merged_pull_request_removes_the_worktree_and_branch() {
        let found: Arc<Mutex<Option<PrInfo>>> = Arc::default();
        let lookup: PrLookup = {
            let found = found.clone();
            Arc::new(move |_: &Path, _: &str| Ok(found.lock().unwrap().clone()))
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
        *found.lock().unwrap() = Some(PrInfo { number: 3, state: "open".into(), url: "u".into(), head: tip.clone() });
        rig.turn_ends();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(rig.store.session("s1").unwrap().unwrap().pr.unwrap().state, "open");
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::PrChanged { .. })), 1);

        // Squash-merged on GitHub: gone after the next sweep once the process is closed.
        *found.lock().unwrap() = Some(PrInfo { number: 3, state: "merged".into(), url: "u".into(), head: tip });
        rig.manager.close("s1");
        rig.manager.0.pr_checked.lock().unwrap().clear();
        rig.sweep().await;
        assert!(!wt.path.exists());
        assert!(git(&repo, &["branch", "--list", &wt.branch]).is_empty(), "branch deleted");
        let row = rig.store.chat_git("s1").unwrap().unwrap();
        assert_eq!((row.status, row.choice), (None, None));
        // The merged pull request stays, so the sidebar keeps its purple mark.
        assert_eq!(row.pr.map(|pr| pr.state).as_deref(), Some("merged"));
        assert_eq!(rig.saw(|e| matches!(e, AgentEvent::WorktreeChanged { status, .. } if status == "merged")), 1);
    }

    #[tokio::test]
    async fn a_trashed_worktree_chat_is_restorable_then_purged_with_its_branch() {
        let rig = rig_with(LONG, LONG, Duration::ZERO, Arc::new(|_: &Path, _: &str| Ok(None)));
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
    async fn quitting_stops_chats_with_background_work_and_closes_the_rest() {
        let rig = rig(LONG);
        rig.send("caffeinate").await;
        rig.background(1);
        rig.turn_ends();
        rig.manager
            .send_message("s2", "claude".into(), "/work".into(), None, None, None, "hi", "m2")
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
