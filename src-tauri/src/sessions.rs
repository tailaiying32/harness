//! Keeps track of chats and their live agent processes: start on the first
//! message, resume after an exit, shut a process down once its chat has been
//! idle for a while, and save what happens. See PLAN.md, "Session lifecycle".

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::agent::{AgentAdapter, EventSink, RunningAgent, SpawnOptions};
use crate::events::{AgentEvent, Decision, Envelope, SessionState};
use crate::store::Store;

/// Where every chat's events go once saved: the window.
pub type Out = Arc<dyn Fn(Envelope) + Send + Sync>;

/// How long a chat sits idle before its process is asked to exit.
const DEFAULT_IDLE_SECONDS: u64 = 300;
/// Longest tool result kept, in characters. See `AgentEvent::cap_tool_result`.
const TOOL_RESULT_MAX_CHARS: usize = 20_000;
/// How long a new message waits for a process that is shutting down.
const CLOSING_WAIT: Duration = Duration::from_secs(30);

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
}

struct Inner {
    adapter: Arc<dyn AgentAdapter>,
    store: Arc<Store>,
    out: Out,
    sessions: Mutex<HashMap<String, Session>>,
    /// Starting a process is not instant; two sends must not both start one.
    start_lock: tokio::sync::Mutex<()>,
    /// Fixed idle time, for tests. Otherwise the environment or the settings decide.
    idle_override: Option<Duration>,
}

#[derive(Clone)]
pub struct SessionManager(Arc<Inner>);

impl SessionManager {
    pub fn new(adapter: Arc<dyn AgentAdapter>, store: Arc<Store>, out: Out) -> Self {
        Self::build(adapter, store, out, None)
    }

    fn build(
        adapter: Arc<dyn AgentAdapter>,
        store: Arc<Store>,
        out: Out,
        idle_override: Option<Duration>,
    ) -> Self {
        Self(Arc::new(Inner {
            adapter,
            store,
            out,
            sessions: Mutex::default(),
            start_lock: tokio::sync::Mutex::default(),
            idle_override,
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
        if let Some(fixed) = self.0.idle_override {
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
    /// `mode` and `model` are saved with the chat; they are passed to the agent
    /// only when a process has to be started (a running one is switched with
    /// `set_mode` and `set_model`).
    pub async fn send_message(
        &self,
        session_id: &str,
        binary: PathBuf,
        project_path: PathBuf,
        mode: Option<String>,
        model: Option<String>,
        text: &str,
        message_id: &str,
    ) -> Result<(), String> {
        let inner = &self.0;

        // A process that was asked to exit can't take messages. Wait for it to
        // go, then start a fresh one that resumes the chat.
        let deadline = Instant::now() + CLOSING_WAIT;
        loop {
            let closing = inner
                .sessions
                .lock()
                .unwrap()
                .get(session_id)
                .is_some_and(|s| s.closing && s.agent.is_some());
            if !closing {
                break;
            }
            if Instant::now() > deadline {
                return Err(
                    "This chat is still finishing background work. Try again in a moment.".to_string(),
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let _starting = inner.start_lock.lock().await;

        // Save the chat first, so its events have somewhere to go.
        inner.store.touch_session(
            session_id,
            &project_path.to_string_lossy(),
            text,
            mode.as_deref().unwrap_or("default"),
            model.as_deref().unwrap_or("default"),
        )?;

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
            let resume_id = inner
                .store
                .session(session_id)?
                .and_then(|s| s.agent_session_id);
            // Not under the sessions lock: starting reports "starting" through
            // the sink, which takes that lock.
            let agent = inner.adapter.spawn(
                SpawnOptions {
                    binary,
                    project_path,
                    resume_id,
                    mode,
                    model,
                },
                self.sink(session_id, run),
            )?;
            let mut sessions = inner.sessions.lock().unwrap();
            if let Some(session) = sessions.get_mut(session_id) {
                // If it already died, its exit was reported; don't keep a dead handle.
                if session.exited_run != run {
                    session.agent = Some(Arc::from(agent));
                }
            }
        }

        self.record(
            session_id,
            AgentEvent::UserMessage {
                text: text.to_string(),
                id: Some(message_id.to_string()),
            },
        );
        self.agent(session_id)?.send_user_message(text, message_id)
    }

    /// What a process's events pass through: bookkeeping, saving, then the window.
    fn sink(&self, session_id: &str, run: u64) -> EventSink {
        let manager = self.clone();
        let id = session_id.to_string();
        Arc::new(move |mut event: AgentEvent| {
            let store = &manager.0.store;
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

    /// After the idle time, ask the process to exit unless the chat was used
    /// again. It is asked, never killed: it finishes background work first.
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
                let still_idle =
                    session.run == run && session.idle_round == round && !session.busy && !session.closing;
                if !still_idle {
                    return;
                }
                session.closing = true;
                session.agent.clone()
            };
            if let Some(agent) = agent {
                agent.close();
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

    /// Removes a chat for good: its process is let go and its history deleted.
    pub fn delete(&self, session_id: &str) -> Result<(), String> {
        // Dropping the handle closes the process's input, so it exits on its own.
        self.0.sessions.lock().unwrap().remove(session_id);
        self.0.store.delete_session(session_id)
    }

    pub fn rename(&self, session_id: &str, title: &str) -> Result<(), String> {
        self.0.store.rename_session(session_id, title)
    }

    /// On app quit: ask every process to exit and give them a moment. Any still
    /// running after that lose their input when the app goes, and exit then.
    pub fn shutdown(&self, wait: Duration) {
        let live: Vec<String> = {
            let sessions = self.0.sessions.lock().unwrap();
            sessions
                .iter()
                .filter(|(_, s)| s.agent.is_some())
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in &live {
            self.close(id);
        }
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline && live.iter().any(|id| self.is_live(id)) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ModelInfo;
    use std::path::Path;

    /// A stand-in agent: remembers how it was started and what it was told, and
    /// lets the test play the agent's side through `sink`.
    #[derive(Default)]
    struct Fake {
        spawns: Mutex<Vec<(Option<String>, Option<String>)>>, // (resume id, mode)
        sinks: Mutex<Vec<EventSink>>,
        told: Arc<Mutex<Vec<String>>>,
    }

    struct FakeAgent {
        sink: EventSink,
        told: Arc<Mutex<Vec<String>>>,
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
            _: Decision,
            _: Option<String>,
            _: Option<BTreeMap<String, String>>,
        ) -> Result<(), String> {
            self.told.lock().unwrap().push(format!("answer {request_id}"));
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
        fn close(&self) {
            self.told.lock().unwrap().push("close".into());
            // A real process exits once its input closes.
            (self.sink)(AgentEvent::Status {
                state: SessionState::Exited,
            });
        }
    }

    impl AgentAdapter for Fake {
        fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
            self.spawns.lock().unwrap().push((opts.resume_id, opts.mode));
            self.sinks.lock().unwrap().push(sink.clone());
            sink(AgentEvent::Status {
                state: SessionState::Starting,
            });
            Ok(Box::new(FakeAgent {
                sink,
                told: self.told.clone(),
            }))
        }
        fn list_models(&self, _: &Path) -> Result<Vec<ModelInfo>, String> {
            Ok(Vec::new())
        }
    }

    struct Rig {
        fake: Arc<Fake>,
        store: Arc<Store>,
        seen: Arc<Mutex<Vec<AgentEvent>>>,
        manager: SessionManager,
    }

    fn rig(idle: Duration) -> Rig {
        let fake = Arc::new(Fake::default());
        let dir = std::env::temp_dir().join(format!("harness-test-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(&dir).unwrap());
        let seen: Arc<Mutex<Vec<AgentEvent>>> = Arc::default();
        let out: Out = {
            let seen = seen.clone();
            Arc::new(move |envelope: Envelope| seen.lock().unwrap().push(envelope.event))
        };
        let manager = SessionManager::build(fake.clone(), store.clone(), out, Some(idle));
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
                .send_message("s1", "claude".into(), "/work".into(), Some("plan".into()), None, text, "m1")
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
    }

    const LONG: Duration = Duration::from_secs(3600);

    #[tokio::test]
    async fn first_message_starts_the_agent_and_saves_the_chat() {
        let rig = rig(LONG);
        rig.send("Hello there").await;
        assert_eq!(*rig.fake.spawns.lock().unwrap(), vec![(None, Some("plan".to_string()))]);
        assert_eq!(rig.told(), vec!["say Hello there"]);
        let chat = rig.store.session("s1").unwrap().unwrap();
        assert_eq!((chat.title.as_str(), chat.mode.as_str()), ("Hello there", "plan"));
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
        rig.manager.delete("s1").unwrap();
        assert!(!rig.manager.is_live("s1"));
        assert!(rig.store.sidebar().unwrap().is_empty());
        // Late events from its process are dropped, not saved.
        rig.agent_says(AgentEvent::Interrupted);
        assert!(rig.store.events("s1").unwrap().is_empty());
    }
}
