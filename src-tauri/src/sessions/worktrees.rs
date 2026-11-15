//! Chats and their worktrees: the agent's request for one, moving a chat into
//! a new worktree, bringing a put-away one back, the trash, and the cleanup
//! sweep. The git work itself is in `crate::worktree`. See PLAN.md, "Worktrees".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Ask, SessionManager, blocking};
use crate::events::{AgentEvent, Decision};
use crate::store::{ChatGit, SessionRow, now_ms};
use crate::worktree::{self, Merged, PrInfo, Worktree};

/// How pull requests are looked up: GitHub in the app, stand-ins in tests.
#[derive(Clone)]
pub struct PrLookup {
    /// The newest pull request for (repo root, branch).
    pub by_branch: Arc<dyn Fn(&Path, &str) -> Result<Option<PrInfo>, String> + Send + Sync>,
    /// The pull request at a link.
    pub by_url: Arc<dyn Fn(&str) -> Result<Option<PrInfo>, String> + Send + Sync>,
}

impl PrLookup {
    pub fn github() -> Self {
        Self {
            by_branch: Arc::new(crate::github::pr_for),
            by_url: Arc::new(crate::github::pr_at),
        }
    }

    /// Finds nothing anywhere.
    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            by_branch: Arc::new(|_, _| Ok(None)),
            by_url: Arc::new(|_| Ok(None)),
        }
    }
}

/// Days without a message before a chat's worktree folder is put away.
const DEFAULT_IDLE_DAYS: u64 = 14;
/// Days a put-away worktree's branch, or a trashed chat, is kept.
const DEFAULT_TRASH_DAYS: u64 = 30;
/// How often the sweep runs while the app is open.
pub const SWEEP_EVERY: Duration = Duration::from_secs(3600);
/// Least time between pull request lookups for one chat: after a turn, and in a sweep.
const PR_AFTER_TURN: Duration = Duration::from_secs(60);
const PR_IN_SWEEP: Duration = Duration::from_secs(600);

/// Claude Code's own worktree tool: how the agent asks for one.
const WORKTREE_TOOL: &str = "EnterWorktree";

/// What the agent's request is told when the chat moves to a worktree.
const MOVING: &str = "Harness is making this chat's worktree itself. Stop here; you will be told \
where to continue.";
/// When the user says no.
const STAY_HERE: &str = "The user wants this work done here, in the current folder, not in a \
worktree. Carry on with the task here.";
/// What the agent is told if making the worktree failed.
const STAYING: &str = "The worktree couldn't be made. Carry on with the task in this folder.";

fn days_ms(days: u64) -> i64 {
    (days * 24 * 3600 * 1000) as i64
}

impl SessionManager {
    fn worktrees_dir(&self) -> PathBuf {
        self.0.data_dir.join("worktrees")
    }

    /// A length in ms: an environment override in seconds (for tests), else a
    /// setting in days, else the default.
    fn period_ms(&self, env: &str, setting: &str, default_days: u64) -> i64 {
        if let Some(seconds) = std::env::var(env).ok().and_then(|v| v.trim().parse::<i64>().ok()) {
            return seconds * 1000;
        }
        let days = self
            .0
            .store
            .setting(setting)
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(default_days);
        days_ms(days)
    }

    fn idle_limit_ms(&self) -> i64 {
        if let Some(fixed) = self.0.timings.worktree_idle {
            return fixed.as_millis() as i64;
        }
        self.period_ms("HARNESS_WORKTREE_IDLE_SECONDS", "worktree_idle_days", DEFAULT_IDLE_DAYS)
    }

    fn keep_ms(&self) -> i64 {
        if let Some(fixed) = self.0.timings.keep {
            return fixed.as_millis() as i64;
        }
        self.period_ms("HARNESS_TRASH_SECONDS", "trash_days", DEFAULT_TRASH_DAYS)
    }

    /// The folder a chat's agent starts in. A chat in the trash comes out of
    /// it; one whose worktree was put away gets it back first. If that fails,
    /// the chat goes back to the main folder and will ask again.
    pub(super) async fn work_folder(&self, session_id: &str, project_path: &Path) -> PathBuf {
        let store = &self.0.store;
        let Ok(Some(git)) = store.chat_git(session_id) else {
            return project_path.to_path_buf();
        };
        if git.trashed_at.is_some() {
            let _ = store.set_trashed(session_id, None);
        }
        let Some(wt) = git.worktree.clone().filter(|_| git.status.is_some()) else {
            return project_path.to_path_buf();
        };
        if git.status.as_deref() == Some("live") && wt.path.join(".git").exists() {
            return wt.work_path();
        }
        // Put away (or its folder went missing): bring it back.
        let chat = session_id.to_string();
        let back = wt.clone();
        match blocking(move || worktree::restore(&back, &chat)).await {
            Ok(()) => {
                let _ = store.set_worktree(session_id, &wt, "live");
                self.record(
                    session_id,
                    AgentEvent::WorktreeChanged {
                        status: "restored".into(),
                        branch: Some(wt.branch.clone()),
                    },
                );
                wt.work_path()
            }
            Err(e) => {
                let _ = store.clear_worktree(session_id);
                self.record(
                    session_id,
                    AgentEvent::Error {
                        message: format!(
                            "This chat's worktree couldn't be brought back ({e}). It works in the main \
                             folder now, and will ask again before its next edit."
                        ),
                    },
                );
                project_path.to_path_buf()
            }
        }
    }

    /// Notes which repo the chat's folder is in (None outside one), for when
    /// the agent asks for a worktree.
    pub(super) async fn note_repo(&self, session_id: &str, folder: &Path) {
        let at = folder.to_path_buf();
        let root = blocking(move || worktree::repo_of(&at).map(|(root, _)| root)).await;
        if let Some(session) = self.0.sessions.lock().unwrap().get_mut(session_id) {
            session.repo_root = root;
        }
    }

    /// Catches the agent's request for a worktree (Claude Code's own worktree
    /// tool, which always asks; see the adapter). It becomes the question on
    /// screen and waits in the agent, which has no timeout, until the answer.
    /// A request that can't be met is turned down at once with the reason.
    /// Returns the event to pass on.
    pub(super) fn screen_worktree_call(&self, session_id: &str, event: AgentEvent) -> Option<AgentEvent> {
        match &event {
            AgentEvent::ApprovalRequest {
                request_id,
                tool_name,
                input,
                tool_id,
                ..
            } if tool_name == WORKTREE_TOOL => {
                let has_worktree = self
                    .0
                    .store
                    .chat_git(session_id)
                    .ok()
                    .flatten()
                    .and_then(|g| g.worktree.filter(|_| g.status.is_some()));
                let mut sessions = self.0.sessions.lock().unwrap();
                let session = sessions.get_mut(session_id)?;
                // Asked twice at once: both wait on the same question.
                if let Some(ask) = session.ask.as_mut() {
                    ask.held.push((request_id.clone(), event));
                    return None;
                }
                let refusal = match (&has_worktree, &session.repo_root) {
                    (Some(wt), _) => Some(format!(
                        "This chat already works in its own git worktree at {}. Carry on there.",
                        wt.work_path().display()
                    )),
                    (None, None) => Some(
                        "This folder isn't in a git repo, so there is no worktree to make. Carry on here."
                            .to_string(),
                    ),
                    (None, Some(_)) => None,
                };
                if let Some(message) = refusal {
                    drop(sessions);
                    self.turn_down_later(session_id, request_id, message);
                    return None;
                }
                let repo_root = session.repo_root.clone()?;
                session.ask = Some(Ask {
                    repo_root: repo_root.clone(),
                    held: vec![(request_id.clone(), event.clone())],
                    card: request_id.clone(),
                    name: input["name"].as_str().map(str::to_owned),
                });
                Some(AgentEvent::WorktreeAsk {
                    request_id: request_id.clone(),
                    repo_root: repo_root.to_string_lossy().into_owned(),
                    tool_id: tool_id.clone(),
                    name: input["name"].as_str().map(str::to_owned),
                })
            }
            // The agent withdrew the request (the user pressed stop): the
            // question closes once nothing is waiting on it.
            AgentEvent::ApprovalCancelled { request_id } => {
                let mut sessions = self.0.sessions.lock().unwrap();
                let session = sessions.get_mut(session_id)?;
                let Some(ask) = session.ask.as_mut() else {
                    return Some(event);
                };
                let Some(at) = ask.held.iter().position(|(id, _)| id == request_id) else {
                    return Some(event);
                };
                ask.held.remove(at);
                if !ask.held.is_empty() {
                    return None;
                }
                let card = ask.card.clone();
                session.ask = None;
                Some(AgentEvent::ApprovalCancelled { request_id: card })
            }
            _ => Some(event),
        }
    }

    /// Answers a request with a refusal, outside the event sink (answering
    /// reports through the sink).
    fn turn_down_later(&self, session_id: &str, request_id: &str, message: String) {
        let manager = self.clone();
        let (id, request) = (session_id.to_string(), request_id.to_string());
        let answer = move || {
            if let Ok(agent) = manager.agent(&id) {
                let _ = agent.answer_approval(&request, Decision::Deny, Some(message), None);
            }
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move { answer() });
        } else {
            answer();
        }
    }

    /// The user's answer to the agent's request for a worktree: "worktree" or "here".
    ///
    /// "here": the request is turned down, and the agent told to carry on in
    /// the folder it is in.
    ///
    /// "worktree": the request is turned down (Harness makes its own worktree,
    /// not Claude Code's), the process stopped, a worktree made, and the same
    /// chat resumes inside it with a note telling the agent where it now is.
    pub async fn choose_worktree(
        &self,
        session_id: &str,
        request_id: &str,
        choice: &str,
        binary: PathBuf,
    ) -> Result<(), String> {
        if choice != "worktree" && choice != "here" {
            return Err(format!("Unknown choice: {choice}"));
        }
        let ask = {
            let mut sessions = self.0.sessions.lock().unwrap();
            let session = sessions
                .get_mut(session_id)
                .filter(|s| s.ask.as_ref().is_some_and(|a| a.card == request_id))
                .ok_or("This question is no longer waiting for an answer")?;
            session.ask.take().unwrap()
        };
        let store = &self.0.store;
        store.set_choice(session_id, Some(choice))?;
        self.record(session_id, AgentEvent::WorktreeChosen { choice: choice.into() });
        let agent = self.agent(session_id)?;

        if choice == "here" {
            for (id, _) in &ask.held {
                agent.answer_approval(id, Decision::Deny, Some(STAY_HERE.to_string()), None)?;
            }
            return Ok(());
        }

        for (id, _) in &ask.held {
            let _ = agent.answer_approval(id, Decision::Deny, Some(MOVING.to_string()), None);
        }
        let _ = agent.interrupt();
        agent.stop();
        self.wait_for_exit(session_id).await?;

        let git = store.chat_git(session_id)?.ok_or("That chat no longer exists")?;
        let root = ask.repo_root.clone();
        let dir = self.worktrees_dir();
        let made = {
            let git = git.clone();
            let name = ask.name.clone();
            blocking(move || {
                let project = PathBuf::from(&git.project_path);
                let sub = worktree::repo_of(&project).map(|(_, s)| s).unwrap_or_default();
                worktree::create(
                    &root,
                    &sub,
                    name.as_deref().unwrap_or(&git.title),
                    git.start_branch.as_deref(),
                    git.start_commit.as_deref(),
                    &dir,
                )
            })
            .await
        };
        let note = match made {
            Ok(wt) => {
                store.set_worktree(session_id, &wt, "live")?;
                self.record(
                    session_id,
                    AgentEvent::WorktreeChanged {
                        status: "created".into(),
                        branch: Some(wt.branch.clone()),
                    },
                );
                let from = match &wt.base_branch {
                    Some(b) => format!("the latest {b}"),
                    None => format!("commit {}", &wt.base_commit[..wt.base_commit.len().min(10)]),
                };
                format!(
                    "You now work in a separate git worktree at {} on branch {}, started from {from}. \
                     Files there may differ from what you read before, so read them again before \
                     editing. Carry on with the task there, using paths inside this folder, never \
                     the old one. Don't call EnterWorktree again in this chat.",
                    wt.work_path().display(),
                    wt.branch
                )
            }
            Err(e) => {
                store.set_choice(session_id, Some("here"))?;
                self.record(
                    session_id,
                    AgentEvent::Error {
                        message: format!("Couldn't make a worktree ({e}). This chat keeps working in the main folder."),
                    },
                );
                STAYING.to_string()
            }
        };
        let saved = store.session(session_id)?.ok_or("That chat no longer exists")?;
        let message_id = uuid::Uuid::new_v4().to_string();
        self.send_inner(
            session_id,
            binary,
            PathBuf::from(&saved.project_path),
            Some(saved.mode),
            Some(saved.model),
            None,
            &note,
            &[],
            &message_id,
            false,
        )
        .await
    }

    /// After a turn: the chat may have switched its folder to another
    /// branch, or opened a pull request.
    pub(super) fn after_turn(&self, session_id: &str) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let manager = self.clone();
        let id = session_id.to_string();
        tokio::spawn(async move {
            manager.follow_branch(&id).await;
            if manager.pr_due(&id, PR_AFTER_TURN) {
                manager.refresh_prs(&id).await;
            }
        });
    }

    /// Whether a chat's pull requests are due a lookup; marks them looked up if so.
    fn pr_due(&self, session_id: &str, every: Duration) -> bool {
        let mut checked = self.0.pr_checked.lock().unwrap();
        if checked.get(session_id).is_some_and(|at| at.elapsed() < every) {
            return false;
        }
        checked.insert(session_id.to_string(), Instant::now());
        true
    }

    /// The chat's live worktree, after noting any branch its folder was
    /// switched to (a split task's next part). Putting it away, bringing it
    /// back and cleanup all go by the branch saved here.
    async fn follow_branch(&self, session_id: &str) -> Option<Worktree> {
        let git = self.0.store.chat_git(session_id).ok()??;
        let wt = git.worktree.filter(|_| git.status.as_deref() == Some("live"))?;
        let now = {
            let wt = wt.clone();
            blocking(move || worktree::follow(&wt)).await
        };
        match now {
            Some(moved) => {
                let _ = self.0.store.set_wt_branch(session_id, &moved);
                Some(moved)
            }
            None => Some(wt),
        }
    }

    /// Saves one pull request to the chat's list and reports a change.
    fn save_pr(&self, session_id: &str, pr: &PrInfo) {
        if let Ok((prs, true)) = self.0.store.upsert_pr(session_id, pr) {
            self.record(session_id, AgentEvent::PrsChanged { prs });
        }
    }

    /// The agent ran `gh pr create` and these links were in what it printed
    /// (a new pull request, or the one that already exists): each is looked
    /// up and added to the chat's list.
    ///
    /// Links wait in a queue and are looked up one at a time, oldest first,
    /// so the list keeps the order the pull requests were opened in however
    /// long each lookup takes.
    pub(super) fn learn_prs(&self, session_id: &str, links: Vec<String>) {
        if links.is_empty() || tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let waiting = links.len();
        self.0
            .pr_links
            .lock()
            .unwrap()
            .extend(links.into_iter().map(|link| (session_id.to_string(), link)));
        let manager = self.clone();
        tokio::spawn(async move {
            let _one_at_a_time = manager.0.pr_learn_lock.lock().await;
            for _ in 0..waiting {
                let Some((id, link)) = manager.0.pr_links.lock().unwrap().pop_front() else {
                    return;
                };
                let lookup = manager.0.pr_lookup.by_url.clone();
                if let Ok(Some(pr)) = blocking(move || lookup(&link)).await {
                    manager.save_pr(&id, &pr);
                }
            }
        });
    }

    /// Looks the chat's pull requests up again: the one for its worktree's
    /// branch (it may have been opened outside the chat), and every listed
    /// one still open or a draft. Merged and closed ones aren't asked about
    /// again. Each answer updates only its own entry, so one added meanwhile
    /// is kept. Returns the list afterwards.
    async fn refresh_prs(&self, session_id: &str) -> Vec<PrInfo> {
        let Ok(Some(git)) = self.0.store.chat_git(session_id) else {
            return Vec::new();
        };
        let mut asked = None;
        if let Some(wt) = git.worktree.filter(|_| git.status.is_some()) {
            let lookup = self.0.pr_lookup.by_branch.clone();
            if let Ok(Some(pr)) = blocking(move || lookup(&wt.repo_root, &wt.branch)).await {
                self.save_pr(session_id, &pr);
                asked = Some(pr.url);
            }
        }
        for pr in git.prs {
            if !matches!(pr.state.as_str(), "open" | "draft") || asked.as_ref() == Some(&pr.url) {
                continue;
            }
            let lookup = self.0.pr_lookup.by_url.clone();
            if let Ok(Some(pr)) = blocking(move || lookup(&pr.url)).await {
                self.save_pr(session_id, &pr);
            }
        }
        self.0.store.chat_git(session_id).ok().flatten().map(|g| g.prs).unwrap_or_default()
    }

    /// Starts a sweep in the background, unless one is running.
    pub(super) fn sweep_soon(&self) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let manager = self.clone();
        tokio::spawn(async move { manager.sweep().await });
    }

    /// The cleanup pass. For each chat whose process isn't running: open pull
    /// requests are looked up again. Then, for one with a worktree: once all
    /// its work has landed it loses its folder and branches; one with no
    /// message for the idle limit has its folder put away; one put away past
    /// the keep limit loses its branch. Trashed chats past the keep limit are
    /// deleted.
    pub async fn sweep(&self) {
        let Ok(_one_at_a_time) = self.0.sweep_lock.try_lock() else {
            return;
        };
        let Ok(rows) = self.0.store.sweep_list() else {
            return;
        };
        let now = now_ms();
        let idle_limit = self.idle_limit_ms();
        let keep = self.keep_ms();
        for row in rows {
            let id = row.id.clone();
            if self.is_live(&id) {
                continue;
            }
            if row.trashed_at.is_some_and(|at| now - at > keep) {
                self.purge(&row).await;
                continue;
            }
            // Before the lookup, which asks about the branch the folder is on.
            let followed = self.follow_branch(&id).await;
            let prs = if self.pr_due(&id, PR_IN_SWEEP) {
                self.refresh_prs(&id).await
            } else {
                row.prs.clone()
            };
            let (Some(wt), Some(status)) = (followed.or(row.worktree.clone()), row.status.clone()) else {
                continue;
            };
            // Not while a message is starting this chat (it may be restoring).
            let _starting = self.0.start_lock.lock().await;
            if self.is_live(&id) {
                continue;
            }
            let check = wt.clone();
            let chat = id.clone();
            let merged = {
                let prs = prs.clone();
                blocking(move || worktree::is_merged(&check, &prs, &chat)).await
            };
            let changed = |status: &str| {
                self.record(
                    &id,
                    AgentEvent::WorktreeChanged {
                        status: status.into(),
                        branch: Some(wt.branch.clone()),
                    },
                )
            };
            let drop_all = {
                let wt = wt.clone();
                let chat = id.clone();
                move || {
                    worktree::remove_folder(&wt)?;
                    worktree::drop_branch(&wt.repo_root, &wt.branch, &chat);
                    Ok::<(), String>(())
                }
            };
            match merged {
                Ok(Merged::ByGit | Merged::ByPr) => {
                    let done = {
                        let (drop_all, wt) = (drop_all.clone(), wt.clone());
                        blocking(move || {
                            drop_all()?;
                            worktree::drop_landed_branches(&wt, &prs);
                            Ok::<(), String>(())
                        })
                        .await
                    };
                    if done.is_ok() {
                        let _ = self.0.store.clear_worktree(&id);
                        changed("merged");
                    }
                    continue;
                }
                // Its branch was deleted outside the app: nothing to bring back.
                Err(_) if status == "put_away" => {
                    let _ = blocking(drop_all).await;
                    let _ = self.0.store.clear_worktree(&id);
                    changed("gone");
                    continue;
                }
                Err(e) => {
                    eprintln!("harness: sweep skipped a chat: {e}");
                    continue;
                }
                Ok(Merged::No) => {}
            }
            if status == "live" && now - row.last_used_at > idle_limit {
                // On no branch, there would be none to bring it back on.
                let away = wt.clone();
                if blocking(move || worktree::is_detached(&away)).await {
                    continue;
                }
                let away = wt.clone();
                let chat = id.clone();
                match blocking(move || worktree::put_away(&away, &chat)).await {
                    Ok(()) => {
                        let _ = self.0.store.set_worktree(&id, &wt, "put_away");
                        changed("put_away");
                    }
                    Err(e) => eprintln!("harness: couldn't put a worktree away: {e}"),
                }
            } else if status == "put_away" && row.put_away_at.is_some_and(|at| now - at > keep) {
                let _ = blocking(drop_all).await;
                let _ = self.0.store.clear_worktree(&id);
                changed("gone");
            }
        }
    }

    /// Moves a chat to the trash. Its worktree folder, if any, is put away.
    pub(super) async fn trash(&self, session_id: &str) -> Result<(), String> {
        let store = &self.0.store;
        store.chat_git(session_id)?.ok_or("That chat no longer exists")?;
        if let Some(wt) = self.follow_branch(session_id).await {
            let away = wt.clone();
            let chat = session_id.to_string();
            match blocking(move || worktree::put_away(&away, &chat)).await {
                Ok(()) => store.set_worktree(session_id, &wt, "put_away")?,
                Err(e) => eprintln!("harness: couldn't put a worktree away: {e}"),
            }
        }
        store.set_trashed(session_id, Some(now_ms()))
    }

    /// Takes a chat out of the trash. Its worktree comes back with its next message.
    pub fn restore_chat(&self, session_id: &str) -> Result<(), String> {
        self.0.store.set_trashed(session_id, None)
    }

    pub fn list_trash(&self) -> Result<Vec<SessionRow>, String> {
        self.0.store.trash()
    }

    /// Deletes every chat in the trash for good, with their branches.
    pub async fn empty_trash(&self) -> Result<(), String> {
        for row in self.0.store.sweep_list()? {
            if row.trashed_at.is_some() {
                self.purge(&row).await;
            }
        }
        Ok(())
    }

    /// Deletes a chat for good: its branch and saved edits, and its history.
    async fn purge(&self, row: &ChatGit) {
        if let Some(wt) = row.worktree.clone().filter(|_| row.status.is_some()) {
            let chat = row.id.clone();
            blocking(move || {
                let _ = worktree::remove_folder(&wt);
                worktree::drop_branch(&wt.repo_root, &wt.branch, &chat);
            })
            .await;
        }
        if let Err(e) = self.0.store.delete_session(&row.id) {
            eprintln!("harness: couldn't delete a chat: {e}");
        }
        self.0.images.delete_session(&row.id);
        self.0.pr_checked.lock().unwrap().remove(&row.id);
    }
}
