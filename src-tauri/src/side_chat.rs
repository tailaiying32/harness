//! Side chats: one small read-only conversation per chat, opened with `/btw`.
//! It starts as a copy of the chat and goes its own way from there. Nothing is
//! saved by the app: its events go to the window under `<chat id>#side` and
//! are gone when it ends. Its process closes after the same idle time as a
//! chat's and resumes on the next question, so a forgotten side chat costs
//! no memory; ending it deletes the agent's saved copy too.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::agent::{AgentAdapter, EventSink, RunningAgent, SideOptions, SpawnOptions};
#[cfg(test)]
use crate::agent::ImagePayload;
use crate::events::{AgentEvent, Envelope, SessionState};
use crate::sessions::{Out, configured_idle};
use crate::store::Store;

/// Longest tool result kept in memory, as for chats.
const TOOL_RESULT_MAX_CHARS: usize = 20_000;
/// After a stop, how long to wait before deleting the saved copy, so the
/// process has ended and won't write it again.
const FORGET_AFTER: Duration = Duration::from_secs(7);

/// The window's name for a chat's side chat.
pub fn side_id(chat_id: &str) -> String {
    format!("{chat_id}#side")
}

#[derive(Default)]
struct Side {
    agent: Option<Arc<dyn RunningAgent>>,
    /// The side chat's own session, once the agent has named it.
    agent_session_id: Option<String>,
    /// Counts process starts; events from an older process are ignored.
    run: u64,
    /// Counts turns started; an idle timer only acts if none started since.
    round: u64,
    /// A title has been asked for (from the first question).
    titled: bool,
}

struct Inner {
    adapter: Arc<dyn AgentAdapter>,
    store: Arc<Store>,
    out: Out,
    sides: Mutex<HashMap<String, Side>>,
    /// Fixed idle time for tests. None = the chats' setting.
    idle: Option<Option<Duration>>,
    forget_after: Duration,
}

#[derive(Clone)]
pub struct SideChats(Arc<Inner>);

impl SideChats {
    pub fn new(adapter: Arc<dyn AgentAdapter>, store: Arc<Store>, out: Out) -> Self {
        Self(Arc::new(Inner {
            adapter,
            store,
            out,
            sides: Mutex::default(),
            idle: None,
            forget_after: FORGET_AFTER,
        }))
    }

    fn idle_time(&self) -> Option<Duration> {
        self.0.idle.unwrap_or_else(|| configured_idle(&self.0.store))
    }

    /// Ask the side chat something, starting it (or bringing it back after an
    /// idle close) when it has no process.
    #[allow(clippy::too_many_arguments)]
    pub fn send(
        &self,
        chat_id: &str,
        binary: PathBuf,
        project_path: &Path,
        model: Option<String>,
        effort: Option<String>,
        text: &str,
        message_id: &str,
    ) -> Result<(), String> {
        let (live, run, own_id, first) = {
            let mut sides = self.0.sides.lock().unwrap();
            let side = sides.entry(chat_id.to_string()).or_default();
            side.round += 1;
            if side.agent.is_none() {
                side.run += 1;
            }
            let first = !side.titled;
            side.titled = true;
            (side.agent.clone(), side.run, side.agent_session_id.clone(), first)
        };
        if first {
            self.write_title(chat_id, binary.clone(), text);
        }

        let agent = match live {
            Some(agent) => agent,
            None => {
                let fork_from = self
                    .0
                    .store
                    .session(chat_id)
                    .ok()
                    .flatten()
                    .and_then(|s| s.agent_session_id);
                let resume_id = own_id.or_else(|| fork_from.clone());
                // Not under the lock: starting reports through the sink, which takes it.
                let agent: Arc<dyn RunningAgent> = Arc::from(self.0.adapter.spawn(
                    SpawnOptions {
                        binary,
                        project_path: self.folder(chat_id, project_path),
                        resume_id,
                        mode: None,
                        model,
                        effort,
                        side: Some(SideOptions { fork_from }),
                        tools: None,
                    },
                    self.sink(chat_id, run),
                )?);
                let mut sides = self.0.sides.lock().unwrap();
                match sides.get_mut(chat_id) {
                    Some(side) if side.run == run => side.agent = Some(agent.clone()),
                    // Ended while starting.
                    _ => {
                        agent.stop();
                        return Err("The side chat was ended".to_string());
                    }
                }
                agent
            }
        };
        // Pictures are for the main chat; a side question is words only.
        agent.send_user_message(text, &[], message_id)
    }

    /// Names the side chat from its first question, the way chats are named.
    /// The name only goes to the window; nothing is saved.
    fn write_title(&self, chat_id: &str, binary: PathBuf, question: &str) {
        let chats = self.clone();
        let id = chat_id.to_string();
        let question = question.to_string();
        tokio::spawn(async move {
            let adapter = chats.0.adapter.clone();
            let title = tokio::task::spawn_blocking(move || adapter.make_title(&binary, &question)).await;
            let Ok(Ok(title)) = title else {
                return;
            };
            // Ended meanwhile: nothing to name.
            if !chats.0.sides.lock().unwrap().contains_key(&id) {
                return;
            }
            (chats.0.out)(Envelope {
                session_id: side_id(&id),
                event: AgentEvent::TitleChanged { title },
            });
        });
    }

    /// Where the side chat reads: the chat's worktree while it is there, else
    /// its folder. A put-away worktree is not brought back for a side question.
    fn folder(&self, chat_id: &str, project_path: &Path) -> PathBuf {
        let git = self.0.store.chat_git(chat_id).ok().flatten();
        match git.and_then(|g| g.worktree.filter(|_| g.status.as_deref() == Some("live"))) {
            Some(wt) if wt.work_path().is_dir() => wt.work_path(),
            _ => project_path.to_path_buf(),
        }
    }

    pub fn interrupt(&self, chat_id: &str) -> Result<(), String> {
        let agent = self.0.sides.lock().unwrap().get(chat_id).and_then(|s| s.agent.clone());
        match agent {
            Some(agent) => agent.interrupt(),
            None => Ok(()),
        }
    }

    /// End the side chat: stop its process and, once it has ended, delete the
    /// agent's saved copy.
    pub fn end(&self, chat_id: &str) {
        let Some(side) = self.0.sides.lock().unwrap().remove(chat_id) else {
            return;
        };
        if let Some(agent) = &side.agent {
            agent.stop();
        }
        if let Some(id) = side.agent_session_id {
            let adapter = self.0.adapter.clone();
            let wait = if side.agent.is_some() { self.0.forget_after } else { Duration::ZERO };
            tokio::spawn(async move {
                tokio::time::sleep(wait).await;
                adapter.forget_session(&id);
            });
        }
    }

    /// On quit: stop every side chat and delete their saved copies. There is
    /// no time to wait for the processes, so a copy written while one ends
    /// may be left behind.
    pub fn shutdown(&self) {
        let sides: Vec<Side> = self.0.sides.lock().unwrap().drain().map(|(_, s)| s).collect();
        for side in &sides {
            if let Some(agent) = &side.agent {
                agent.stop();
            }
        }
        std::thread::sleep(Duration::from_millis(300));
        for side in sides {
            if let Some(id) = side.agent_session_id {
                self.0.adapter.forget_session(&id);
            }
        }
    }

    /// What a side process's events pass through on their way to the window.
    fn sink(&self, chat_id: &str, run: u64) -> EventSink {
        let chats = self.clone();
        let id = chat_id.to_string();
        Arc::new(move |mut event: AgentEvent| {
            {
                let mut sides = chats.0.sides.lock().unwrap();
                // Ended, or an older process: drop it.
                let Some(side) = sides.get_mut(&id).filter(|s| s.run == run) else {
                    return;
                };
                match &event {
                    AgentEvent::SessionStarted { agent_session_id, .. } => {
                        side.agent_session_id = Some(agent_session_id.clone());
                    }
                    AgentEvent::Status {
                        state: SessionState::Exited,
                    } => side.agent = None,
                    AgentEvent::TurnEnded { .. } => {
                        let round = side.round;
                        drop(sides);
                        chats.close_when_idle(&id, run, round);
                    }
                    _ => {}
                }
            }
            event.cap_tool_result(TOOL_RESULT_MAX_CHARS);
            (chats.0.out)(Envelope {
                session_id: side_id(&id),
                event,
            });
        })
    }

    /// After the idle time, close the process if no question came since. The
    /// side chat keeps its session, and the next question resumes it.
    fn close_when_idle(&self, chat_id: &str, run: u64, round: u64) {
        let Some(idle) = self.idle_time() else {
            return;
        };
        let chats = self.clone();
        let id = chat_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(idle).await;
            let agent = {
                let sides = chats.0.sides.lock().unwrap();
                sides
                    .get(&id)
                    .filter(|s| s.run == run && s.round == round)
                    .and_then(|s| s.agent.clone())
            };
            if let Some(agent) = agent {
                agent.close();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Decision, ModelInfo};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fake {
        /// Each start: (resume id, fork_from).
        spawns: Mutex<Vec<(Option<String>, Option<String>)>>,
        sinks: Mutex<Vec<EventSink>>,
        told: Arc<Mutex<Vec<String>>>,
        forgotten: Mutex<Vec<String>>,
    }

    struct FakeAgent {
        sink: EventSink,
        told: Arc<Mutex<Vec<String>>>,
    }

    impl RunningAgent for FakeAgent {
        fn send_user_message(&self, text: &str, _: &[ImagePayload], _: &str) -> Result<(), String> {
            self.told.lock().unwrap().push(format!("say {text}"));
            Ok(())
        }
        fn answer_approval(
            &self,
            _: &str,
            _: Decision,
            _: Option<String>,
            _: Option<BTreeMap<String, String>>,
        ) -> Result<(), String> {
            Ok(())
        }
        fn interrupt(&self) -> Result<(), String> {
            self.told.lock().unwrap().push("interrupt".into());
            Ok(())
        }
        fn set_mode(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn set_model(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn set_effort(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn close(&self) {
            self.told.lock().unwrap().push("close".into());
            (self.sink)(AgentEvent::Status {
                state: SessionState::Exited,
            });
        }
        fn stop(&self) {
            self.told.lock().unwrap().push("stop".into());
        }
    }

    impl AgentAdapter for Fake {
        fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
            let side = opts.side.expect("a side chat");
            self.spawns.lock().unwrap().push((opts.resume_id, side.fork_from));
            self.sinks.lock().unwrap().push(sink.clone());
            Ok(Box::new(FakeAgent {
                sink,
                told: self.told.clone(),
            }))
        }
        fn list_models(&self, _: &Path) -> Result<Vec<ModelInfo>, String> {
            Ok(Vec::new())
        }
        fn make_title(&self, _: &Path, first: &str) -> Result<String, String> {
            Ok(format!("About {first}"))
        }
        fn sign_in(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn forget_session(&self, id: &str) {
            self.forgotten.lock().unwrap().push(id.to_string());
        }
    }

    struct Rig {
        fake: Arc<Fake>,
        chats: SideChats,
        seen: Arc<Mutex<Vec<Envelope>>>,
        dir: PathBuf,
    }

    fn rig(idle: Option<Duration>) -> Rig {
        let fake = Arc::new(Fake::default());
        let dir = std::env::temp_dir().join(format!("harness-side-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(&dir).unwrap());
        store.touch_session("c1", "/work", "hi", "default", "default").unwrap();
        store.set_agent_session_id("c1", "main-1").unwrap();
        let seen: Arc<Mutex<Vec<Envelope>>> = Arc::default();
        let out: Out = {
            let seen = seen.clone();
            Arc::new(move |e: Envelope| seen.lock().unwrap().push(e))
        };
        let mut chats = SideChats::new(fake.clone(), store, out);
        let inner = Arc::get_mut(&mut chats.0).unwrap();
        inner.idle = Some(idle);
        inner.forget_after = Duration::from_millis(10);
        Rig { fake, chats, seen, dir }
    }

    impl Rig {
        fn ask(&self, text: &str) {
            self.chats
                .send("c1", "claude".into(), Path::new("/work"), None, None, text, "m")
                .unwrap();
        }
        /// Plays the latest process: it names its session and ends a turn.
        fn answers(&self, side_session: &str) {
            let sink = self.fake.sinks.lock().unwrap().last().unwrap().clone();
            sink(AgentEvent::SessionStarted {
                agent_session_id: side_session.into(),
                model: None,
                mode: None,
            });
            sink(AgentEvent::TurnEnded {
                ok: true,
                duration_ms: None,
            });
        }
        fn told(&self) -> Vec<String> {
            self.fake.told.lock().unwrap().clone()
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[tokio::test]
    async fn the_first_question_copies_the_chat_and_follow_ups_reuse_the_process() {
        let rig = rig(None);
        rig.ask("why?");
        rig.answers("side-1");
        rig.ask("and then?");
        assert_eq!(
            *rig.fake.spawns.lock().unwrap(),
            vec![(Some("main-1".into()), Some("main-1".into()))]
        );
        assert_eq!(rig.told(), vec!["say why?", "say and then?"]);
        // Everything reaches the window under the side chat's name.
        assert!(rig.seen.lock().unwrap().iter().all(|e| e.session_id == "c1#side"));
    }

    #[tokio::test]
    async fn after_an_idle_close_it_resumes_its_own_copy() {
        let rig = rig(Some(Duration::from_millis(20)));
        rig.ask("why?");
        rig.answers("side-1");
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(rig.told(), vec!["say why?", "close"]);
        rig.ask("and then?");
        let spawns = rig.fake.spawns.lock().unwrap().clone();
        assert_eq!(spawns.len(), 2);
        // Resumes the side chat's own session; copying again would lose it.
        assert_eq!(spawns[1].0.as_deref(), Some("side-1"));
    }

    #[tokio::test]
    async fn a_question_in_time_keeps_it_open() {
        let rig = rig(Some(Duration::from_millis(40)));
        rig.ask("why?");
        rig.answers("side-1");
        tokio::time::sleep(Duration::from_millis(10)).await;
        rig.ask("and then?");
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(!rig.told().contains(&"close".to_string()));
    }

    #[tokio::test]
    async fn ending_stops_it_and_forgets_the_copy() {
        let rig = rig(None);
        rig.ask("why?");
        rig.answers("side-1");
        rig.chats.end("c1");
        assert!(rig.told().contains(&"stop".to_string()));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(*rig.fake.forgotten.lock().unwrap(), vec!["side-1".to_string()]);
        // Late events from the stopped process are dropped.
        let before = rig.seen.lock().unwrap().len();
        let sink = rig.fake.sinks.lock().unwrap()[0].clone();
        sink(AgentEvent::TurnEnded {
            ok: true,
            duration_ms: None,
        });
        assert_eq!(rig.seen.lock().unwrap().len(), before);
    }

    #[tokio::test]
    async fn a_chat_with_nothing_to_copy_starts_fresh() {
        let rig = rig(None);
        rig.chats
            .send("new", "claude".into(), Path::new("/work"), None, None, "hi", "m")
            .unwrap();
        assert_eq!(*rig.fake.spawns.lock().unwrap(), vec![(None, None)]);
    }

    #[tokio::test]
    async fn the_first_question_names_it_once() {
        let rig = rig(None);
        rig.ask("why?");
        rig.answers("side-1");
        rig.ask("and then?");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let titles: Vec<String> = rig
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match &e.event {
                AgentEvent::TitleChanged { title } => Some(title.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, vec!["About why?".to_string()]);
    }
}
