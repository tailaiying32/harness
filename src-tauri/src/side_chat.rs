//! Side chats: small read-only conversations beside a chat, opened with
//! `/btw` or the side chat view's +. A chat may have several, one per tab;
//! each starts as a copy of the chat as it is then and goes its own way.
//! Nothing is saved by the app: a side chat's events go to the window under
//! its own id, `<chat id>#side-<tab>`, and are gone when it ends. Its process
//! closes after the same idle time as a chat's and resumes on the next
//! question, so a forgotten side chat costs no memory. Ending it deletes the
//! agent's saved copy too, and copies a crash left behind are deleted at the
//! next start. Each running Harness keeps its own list of copies, so a second
//! one (a test build on the same data folder) never deletes the first's. If a
//! side chat's copy is gone anyway, asking it again fails with [`GONE`] and
//! the window moves the question to a new side chat.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::agent::{AgentAdapter, EventSink, RunningAgent, SideOptions, SpawnOptions};
#[cfg(test)]
use crate::agent::ImagePayload;
use crate::events::{AgentEvent, Envelope, SessionState};
use crate::home::Home;
use crate::notes::Notes;
use crate::sessions::{Out, configured_idle, notes_for, project_docs_for};
use crate::store::Store;

/// Longest tool result kept in memory, as for chats.
const TOOL_RESULT_MAX_CHARS: usize = 20_000;
/// After a stop, how long to wait before deleting the saved copy, so the
/// process has ended and won't write it again.
const FORGET_AFTER: Duration = Duration::from_secs(7);
/// What separates a chat's id from its side chat's tab in a side chat's id.
const MARK: &str = "#side-";
/// The error when a side chat's saved copy was deleted while it sat idle; the
/// side chat is forgotten. The window knows this text (chat.svelte.ts).
pub const GONE: &str = "side chat gone";
/// The start of each running Harness's list of copies: `side_copies-<pid>.json`.
/// An older build kept one shared `side_copies.json`.
const COPIES: &str = "side_copies";

/// The chat a side chat belongs to, from the side chat's id.
pub fn chat_of(side_id: &str) -> Result<&str, String> {
    match side_id.split_once(MARK) {
        Some((chat, tab)) if !chat.is_empty() && !tab.is_empty() && !tab.contains('#') => Ok(chat),
        _ => Err(format!("Not a side chat: {side_id}")),
    }
}

#[derive(Default)]
struct Side {
    agent: Option<Arc<dyn RunningAgent>>,
    /// The side chat's own session, once the agent has named it.
    agent_session_id: Option<String>,
    /// The latest process start; events from an older process are ignored.
    /// Unique across all side chats, so a reused id never matches an old process.
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
    /// The chats' notes: a side chat may read its chat's.
    notes: Notes,
    /// By side chat id.
    sides: Mutex<HashMap<String, Side>>,
    /// Side chats ended before their copy was named: the copy is deleted
    /// when the name arrives.
    ended: Mutex<HashSet<String>>,
    next_run: AtomicU64,
    /// The agent's copies not yet deleted, kept on disk so a crash can't
    /// leave them behind for good.
    copies: Mutex<Vec<String>>,
    /// This process's list (`side_copies-<pid>.json`).
    copies_file: PathBuf,
    /// Fixed idle time for tests. None = the chats' setting.
    idle: Option<Option<Duration>>,
    forget_after: Duration,
}

#[derive(Clone)]
pub struct SideChats(Arc<Inner>);

impl SideChats {
    /// `dir` holds each running Harness's list of the agent's copies still to
    /// delete. Lists left by a Harness that is no longer running are deleted
    /// now, with their copies; a running one's are left alone.
    pub fn new(adapter: Arc<dyn AgentAdapter>, store: Arc<Store>, out: Out, home: Home, dir: &Path) -> Self {
        let pid = std::process::id();
        let chats = Self(Arc::new(Inner {
            adapter,
            notes: Notes::for_store(home, store.clone()),
            store,
            out,
            sides: Mutex::default(),
            ended: Mutex::default(),
            next_run: AtomicU64::new(1),
            copies: Mutex::default(),
            copies_file: dir.join(format!("{COPIES}-{pid}.json")),
            idle: None,
            forget_after: FORGET_AFTER,
        }));
        chats.sweep(dir, pid);
        chats
    }

    fn idle_time(&self) -> Option<Duration> {
        self.0.idle.unwrap_or_else(|| configured_idle(&self.0.store))
    }

    /// Ask a side chat something, starting it (or bringing it back after an
    /// idle close) when it has no process. A new side chat copies its chat.
    #[allow(clippy::too_many_arguments)]
    pub fn send(
        &self,
        side_id: &str,
        binary: PathBuf,
        project_path: &Path,
        model: Option<String>,
        effort: Option<String>,
        text: &str,
        message_id: &str,
    ) -> Result<(), String> {
        let chat_id = chat_of(side_id)?;
        let (live, run, own_id, first) = {
            let mut sides = self.0.sides.lock().unwrap();
            let side = sides.entry(side_id.to_string()).or_default();
            side.round += 1;
            if side.agent.is_none() {
                side.run = self.0.next_run.fetch_add(1, Ordering::Relaxed);
            }
            let first = !side.titled;
            side.titled = true;
            (side.agent.clone(), side.run, side.agent_session_id.clone(), first)
        };
        if first {
            self.write_title(side_id, binary.clone(), text);
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
                // Its copy was deleted while it sat idle (by hand, or by
                // something outside Harness): it can't go on. Forgotten here;
                // the window starts a new side chat with the question.
                if let Some(own) = &own_id {
                    if !self.0.adapter.has_session(own) {
                        let mut sides = self.0.sides.lock().unwrap();
                        if sides.get(side_id).is_some_and(|s| s.run == run) {
                            sides.remove(side_id);
                        }
                        drop(sides);
                        self.forget(own);
                        return Err(GONE.to_string());
                    }
                }
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
                        tools: Vec::new(),
                        notes: notes_for(&self.0.notes, chat_id),
                        project_docs: project_docs_for(&self.0.notes, chat_id),
                        memory: self.0.notes.project_memory(chat_id),
                        preferences: self.0.notes.preferences(),
                        env: Vec::new(),
                    },
                    self.sink(side_id, run),
                )?);
                let mut sides = self.0.sides.lock().unwrap();
                match sides.get_mut(side_id) {
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
    fn write_title(&self, side_id: &str, binary: PathBuf, question: &str) {
        let chats = self.clone();
        let id = side_id.to_string();
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
                session_id: id,
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

    /// Take back a message the side chat hasn't read yet; see `Sessions::withdraw_message`.
    pub fn withdraw_message(&self, side_id: &str, message_id: &str) -> Result<(), String> {
        let agent = self.0.sides.lock().unwrap().get(side_id).and_then(|s| s.agent.clone());
        agent.ok_or("This side chat isn't running")?.withdraw_message(message_id)
    }

    pub fn interrupt(&self, side_id: &str) -> Result<(), String> {
        let agent = self.0.sides.lock().unwrap().get(side_id).and_then(|s| s.agent.clone());
        match agent {
            Some(agent) => agent.interrupt(),
            None => Ok(()),
        }
    }

    /// End one side chat: stop its process and, once it has ended, delete
    /// the agent's saved copy.
    pub fn end(&self, side_id: &str) {
        let side = self.0.sides.lock().unwrap().remove(side_id);
        if let Some(side) = side {
            self.finish(side_id, side);
        }
    }

    /// End all of a chat's side chats, when the chat goes to the trash.
    pub fn end_chat(&self, chat_id: &str) {
        let ended: Vec<(String, Side)> = {
            let mut sides = self.0.sides.lock().unwrap();
            let ids: Vec<String> = sides.keys().filter(|id| chat_of(id) == Ok(chat_id)).cloned().collect();
            ids.into_iter().filter_map(|id| sides.remove(&id).map(|s| (id, s))).collect()
        };
        for (id, side) in ended {
            self.finish(&id, side);
        }
    }

    /// End every side chat, when a fresh window starts: the old window's
    /// side chats went with it.
    pub fn end_all(&self) {
        let ended: Vec<(String, Side)> = self.0.sides.lock().unwrap().drain().collect();
        for (id, side) in ended {
            self.finish(&id, side);
        }
    }

    fn finish(&self, side_id: &str, side: Side) {
        if let Some(agent) = &side.agent {
            agent.stop();
        }
        match side.agent_session_id {
            Some(id) => {
                let wait = if side.agent.is_some() { self.0.forget_after } else { Duration::ZERO };
                self.forget_later(id, wait);
            }
            // Its copy isn't named yet; delete it when the name arrives.
            None if side.agent.is_some() => {
                self.0.ended.lock().unwrap().insert(side_id.to_string());
            }
            None => {}
        }
    }

    /// On quit: stop every side chat and delete their saved copies. There is
    /// no time to wait for the processes, so a copy written while one ends
    /// may be left behind; the next start deletes it.
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
                self.forget(&id);
            }
        }
    }

    /// Deletes the copies listed by a Harness that is no longer running (a
    /// list under this process's own id is an earlier one's, since this one
    /// has listed nothing yet), and those lists. A running Harness's copies
    /// may still be in use, so they stay.
    fn sweep(&self, dir: &Path, pid: u32) {
        let Ok(files) = std::fs::read_dir(dir) else {
            return;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            let Some(rest) = name.strip_prefix(COPIES).and_then(|r| r.strip_suffix(".json")) else {
                continue;
            };
            let owner = match rest {
                // The older shared list.
                "" => None,
                _ => match rest.strip_prefix('-').and_then(|p| p.parse::<u32>().ok()) {
                    Some(owner) => Some(owner),
                    None => continue,
                },
            };
            if owner.is_some_and(|owner| owner != pid && running(owner)) {
                continue;
            }
            let left: Vec<String> = std::fs::read_to_string(file.path())
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            for id in left {
                self.0.adapter.forget_session(&id);
            }
            let _ = std::fs::remove_file(file.path());
        }
    }

    fn remember_copy(&self, id: &str) {
        let mut copies = self.0.copies.lock().unwrap();
        if !copies.iter().any(|c| c == id) {
            copies.push(id.to_string());
            self.save_copies(&copies);
        }
    }

    fn forget(&self, id: &str) {
        self.0.adapter.forget_session(id);
        let mut copies = self.0.copies.lock().unwrap();
        copies.retain(|c| c != id);
        self.save_copies(&copies);
    }

    fn forget_later(&self, id: String, wait: Duration) {
        let chats = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            chats.forget(&id);
        });
    }

    /// Written to a side file and moved into place, so a crash never leaves half a list.
    fn save_copies(&self, copies: &[String]) {
        let file = &self.0.copies_file;
        let tmp = file.with_extension("tmp");
        let Ok(text) = serde_json::to_string(copies) else {
            return;
        };
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, file);
        }
    }

    /// What a side process's events pass through on their way to the window.
    fn sink(&self, side_id: &str, run: u64) -> EventSink {
        let chats = self.clone();
        let id = side_id.to_string();
        Arc::new(move |mut event: AgentEvent| {
            {
                let mut sides = chats.0.sides.lock().unwrap();
                // Ended, or an older process: drop it.
                let Some(side) = sides.get_mut(&id).filter(|s| s.run == run) else {
                    drop(sides);
                    // Ended before its copy was named: now it can go.
                    if let AgentEvent::SessionStarted { agent_session_id, .. } = &event {
                        if chats.0.ended.lock().unwrap().remove(&id) {
                            chats.remember_copy(agent_session_id);
                            chats.forget_later(agent_session_id.clone(), chats.0.forget_after);
                        }
                    }
                    return;
                };
                match &event {
                    AgentEvent::SessionStarted { agent_session_id, .. } => {
                        side.agent_session_id = Some(agent_session_id.clone());
                        let copy = agent_session_id.clone();
                        drop(sides);
                        chats.remember_copy(&copy);
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
                session_id: id.clone(),
                event,
            });
        })
    }

    /// After the idle time, close the process if no question came since. The
    /// side chat keeps its session, and the next question resumes it.
    fn close_when_idle(&self, side_id: &str, run: u64, round: u64) {
        let Some(idle) = self.idle_time() else {
            return;
        };
        let chats = self.clone();
        let id = side_id.to_string();
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

/// Whether a process with this id is running.
#[cfg(unix)]
fn running(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // Signal 0 sends nothing; it only checks. EPERM: running, but not ours.
    let sent = unsafe { libc::kill(pid, 0) } == 0;
    sent || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Whether a process with this id is running: it opens, and hasn't exited.
#[cfg(windows)]
fn running(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: plain calls; the handle is checked and closed.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok && code == STILL_ACTIVE as u32
    }
}

/// Not checked elsewhere: an earlier list is taken to be a dead one's.
#[cfg(not(any(unix, windows)))]
fn running(_: u32) -> bool {
    false
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
        /// Sessions deleted behind Harness's back.
        missing: Mutex<Vec<String>>,
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
            _: Option<String>,
        ) -> Result<(), String> {
            Ok(())
        }
        fn open_requests(&self) -> Vec<String> {
            Vec::new()
        }
        fn withdraw_message(&self, _: &str) -> Result<(), String> {
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
        fn list_models(&self, _: &Path, _: Option<&[String]>) -> Result<Vec<ModelInfo>, String> {
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
        fn has_session(&self, id: &str) -> bool {
            !self.missing.lock().unwrap().iter().any(|m| m == id) && !self.forgotten.lock().unwrap().iter().any(|f| f == id)
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
        let mut chats = SideChats::new(
            fake.clone(),
            store,
            out,
            Home::new(dir.join("home")),
            &dir,
        );
        let inner = Arc::get_mut(&mut chats.0).unwrap();
        inner.idle = Some(idle);
        inner.forget_after = Duration::from_millis(10);
        Rig { fake, chats, seen, dir }
    }

    impl Rig {
        fn ask(&self, text: &str) {
            self.ask_in("c1#side-a", text);
        }
        fn ask_in(&self, side: &str, text: &str) {
            self.chats
                .send(side, "claude".into(), Path::new("/work"), None, None, text, "m")
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
        assert!(rig.seen.lock().unwrap().iter().all(|e| e.session_id == "c1#side-a"));
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
        rig.chats.end("c1#side-a");
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
            .send("new#side-a", "claude".into(), Path::new("/work"), None, None, "hi", "m")
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

    fn forgotten(rig: &Rig) -> Vec<String> {
        rig.fake.forgotten.lock().unwrap().clone()
    }

    #[test]
    fn only_side_chat_ids_are_taken() {
        assert_eq!(chat_of("c1#side-a"), Ok("c1"));
        for bad in ["c1", "c1#side", "c1#side-", "#side-a", "c1#side-a#side-b"] {
            assert!(chat_of(bad).is_err(), "{bad}");
        }
        let rig = rig(None);
        assert!(rig.chats.send("c1", "claude".into(), Path::new("/work"), None, None, "hi", "m").is_err());
    }

    #[tokio::test]
    async fn each_side_chat_copies_the_chat_and_runs_on_its_own() {
        let rig = rig(None);
        rig.ask_in("c1#side-a", "why?");
        rig.answers("side-1");
        rig.ask_in("c1#side-b", "how?");
        rig.answers("side-2");
        // Both copy the chat, not each other.
        assert_eq!(
            *rig.fake.spawns.lock().unwrap(),
            vec![(Some("main-1".into()), Some("main-1".into())), (Some("main-1".into()), Some("main-1".into()))]
        );
        let ids: Vec<String> = rig.seen.lock().unwrap().iter().map(|e| e.session_id.clone()).collect();
        assert!(ids.contains(&"c1#side-a".to_string()) && ids.contains(&"c1#side-b".to_string()));

        // Ending one leaves the other running.
        rig.chats.end("c1#side-a");
        rig.ask_in("c1#side-b", "and then?");
        assert_eq!(rig.fake.spawns.lock().unwrap().len(), 2);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(forgotten(&rig), vec!["side-1".to_string()]);
    }

    #[tokio::test]
    async fn trashing_a_chat_ends_only_its_side_chats() {
        let rig = rig(None);
        rig.ask_in("c1#side-a", "why?");
        rig.answers("side-1");
        rig.ask_in("c1#side-b", "how?");
        rig.answers("side-2");
        rig.ask_in("c2#side-a", "what?");
        rig.answers("side-3");
        rig.chats.end_chat("c1");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut gone = forgotten(&rig);
        gone.sort();
        assert_eq!(gone, vec!["side-1".to_string(), "side-2".to_string()]);
        assert_eq!(rig.chats.0.sides.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_fresh_window_ends_every_side_chat() {
        let rig = rig(None);
        rig.ask_in("c1#side-a", "why?");
        rig.answers("side-1");
        rig.ask_in("c2#side-a", "what?");
        rig.answers("side-2");
        rig.chats.end_all();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(forgotten(&rig).len(), 2);
        assert!(rig.chats.0.sides.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_copy_named_after_its_side_chat_ended_is_still_deleted() {
        let rig = rig(None);
        rig.ask("why?");
        // Ended before the process said which session it copied into.
        rig.chats.end("c1#side-a");
        rig.answers("side-1");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(forgotten(&rig), vec!["side-1".to_string()]);
    }

    #[tokio::test]
    async fn a_new_side_chat_with_an_old_id_ignores_the_old_process() {
        let rig = rig(None);
        rig.ask("why?");
        let old = rig.fake.sinks.lock().unwrap()[0].clone();
        rig.chats.end("c1#side-a");
        rig.ask("again?");
        let before = rig.seen.lock().unwrap().len();
        old(AgentEvent::TurnEnded {
            ok: true,
            duration_ms: None,
        });
        assert_eq!(rig.seen.lock().unwrap().len(), before);
    }

    #[tokio::test]
    async fn copies_left_by_a_crash_are_deleted_at_the_next_start() {
        let rig = rig(None);
        rig.ask("why?");
        rig.answers("side-1");
        let file = rig.dir.join(format!("side_copies-{}.json", std::process::id()));
        let listed: Vec<String> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(listed, vec!["side-1".to_string()]);

        // The app dies here; the next start reads the list.
        let fake = Arc::new(Fake::default());
        let store = Arc::new(Store::open(&rig.dir).unwrap());
        let _again = SideChats::new(fake.clone(), store, Arc::new(|_| {}), Home::new(rig.dir.join("home")), &rig.dir);
        assert_eq!(*fake.forgotten.lock().unwrap(), vec!["side-1".to_string()]);
        assert!(!file.exists());
    }

    #[tokio::test]
    async fn a_start_leaves_a_running_harnesss_copies_alone() {
        let rig = rig(None);
        // A Harness that has quit (an older build's shared list, and one that
        // ended), and one still running (pid 1 always is on Unix; Windows has
        // none, so a process that waits).
        let quick = if cfg!(windows) { ["cmd", "/c", "exit"] } else { ["sh", "-c", "true"] };
        let ended = {
            let mut child = std::process::Command::new(quick[0]).args(&quick[1..]).spawn().unwrap();
            child.wait().unwrap();
            child.id()
        };
        #[cfg(windows)]
        let mut waiting = std::process::Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        #[cfg(windows)]
        let alive = waiting.id();
        #[cfg(not(windows))]
        let alive = 1;
        let list = |name: String, ids: &[&str]| {
            let file = rig.dir.join(name);
            std::fs::write(&file, serde_json::to_string(ids).unwrap()).unwrap();
            file
        };
        let old = list("side_copies.json".into(), &["old-1"]);
        let dead = list(format!("side_copies-{ended}.json"), &["dead-1"]);
        let live = list(format!("side_copies-{alive}.json"), &["live-1"]);

        let fake = Arc::new(Fake::default());
        let store = Arc::new(Store::open(&rig.dir).unwrap());
        let _again = SideChats::new(fake.clone(), store, Arc::new(|_| {}), Home::new(rig.dir.join("home")), &rig.dir);
        let mut gone = fake.forgotten.lock().unwrap().clone();
        gone.sort();
        assert_eq!(gone, vec!["dead-1".to_string(), "old-1".to_string()]);
        assert!(!old.exists() && !dead.exists());
        assert!(live.exists());
        #[cfg(windows)]
        let _ = waiting.kill();
    }

    #[tokio::test]
    async fn a_side_chat_whose_copy_was_deleted_is_forgotten() {
        let rig = rig(Some(Duration::from_millis(20)));
        rig.ask("why?");
        rig.answers("side-1");
        tokio::time::sleep(Duration::from_millis(60)).await;
        // Deleted while it sat idle.
        rig.fake.missing.lock().unwrap().push("side-1".into());
        let asked = rig.chats.send("c1#side-a", "claude".into(), Path::new("/work"), None, None, "and then?", "m");
        assert_eq!(asked, Err(GONE.to_string()));
        // Nothing resumed; the side chat and its listing are gone.
        assert_eq!(rig.fake.spawns.lock().unwrap().len(), 1);
        assert!(rig.chats.0.sides.lock().unwrap().is_empty());
        assert!(rig.chats.0.copies.lock().unwrap().is_empty());
        // A new side chat with the same question copies the chat again.
        rig.ask_in("c1#side-b", "and then?");
        assert_eq!(rig.fake.spawns.lock().unwrap()[1], (Some("main-1".into()), Some("main-1".into())));
    }

    #[tokio::test]
    async fn a_deleted_copy_leaves_the_list() {
        let rig = rig(None);
        rig.ask("why?");
        rig.answers("side-1");
        rig.chats.end("c1#side-a");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let listed: Vec<String> =
            serde_json::from_str(&std::fs::read_to_string(rig.chats.0.copies_file.clone()).unwrap()).unwrap();
        assert!(listed.is_empty());
    }
}
