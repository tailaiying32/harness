//! Chats and their worktrees: the agent's request for one, moving a chat into
//! a new worktree, bringing a put-away one back, the trash, and the cleanup
//! sweep. The git work itself is in `crate::worktree`. See PLAN.md, "Worktrees".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{Ask, SessionManager, blocking};
use crate::agent::AppTools;
use crate::events::{AgentEvent, Decision};
use crate::github::PrAsk;
use crate::store::{ChatGit, SessionRow, now_ms};
use crate::worktree::{self, Merged, PrInfo, Worktree};

/// How pull requests are looked up: GitHub in the app, stand-ins in tests.
#[derive(Clone)]
pub struct PrLookup {
    /// The newest pull request for (repo root, branch).
    pub by_branch: Arc<dyn Fn(&Path, &str) -> Result<Option<PrInfo>, String> + Send + Sync>,
    /// The pull request at a link.
    pub by_url: Arc<dyn Fn(&str) -> Result<Option<PrInfo>, String> + Send + Sync>,
    /// Many at once, in one round trip. Without it, each is asked alone.
    pub many: Option<Arc<dyn Fn(&[PrAsk]) -> Vec<Option<PrInfo>> + Send + Sync>>,
}

impl PrLookup {
    pub fn github() -> Self {
        Self {
            by_branch: Arc::new(crate::github::pr_for),
            by_url: Arc::new(crate::github::pr_at),
            many: Some(Arc::new(crate::github::prs)),
        }
    }

    /// The answers to `asks`, in order; None where none was found.
    fn ask_many(&self, asks: &[PrAsk]) -> Vec<Option<PrInfo>> {
        if let Some(many) = &self.many {
            return many(asks);
        }
        asks.iter()
            .map(|ask| match ask {
                PrAsk::Branch { repo_root, branch } => (self.by_branch)(repo_root, branch),
                PrAsk::Url(url) => (self.by_url)(url),
            })
            .map(|answer| answer.ok().flatten())
            .collect()
    }

    /// Finds nothing anywhere.
    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            by_branch: Arc::new(|_, _| Ok(None)),
            by_url: Arc::new(|_| Ok(None)),
            many: None,
        }
    }
}

/// Days without a message before a chat's worktree folder is put away.
const DEFAULT_IDLE_DAYS: u64 = 14;
/// Days a put-away worktree's branch, or a trashed chat, is kept.
const DEFAULT_TRASH_DAYS: u64 = 30;
/// How often the sweep runs while the app is open.
pub const SWEEP_EVERY: Duration = Duration::from_secs(3600);
/// Least time between pull request lookups for one chat when the window comes
/// to the front or the chat is opened, so flicking between windows doesn't
/// ask again and again. After a turn it is always asked.
const PR_GAP: Duration = Duration::from_secs(10);
/// The same, in a sweep.
const PR_IN_SWEEP: Duration = Duration::from_secs(600);

/// The app's worktree tool group, offered to every main chat's agent,
/// whatever the agent: how it asks for a worktree. Claude Code's own
/// worktree tools are turned off (see the adapter), so this is the only way.
pub const SERVER: &str = "tilde_worktree";
/// The tool's one name, and the name the window sees it by, for every agent
/// (Cursor's MCP calls are named this way too, see `acp::name_and_input`).
const TOOL: &str = "ask";
pub const CALL_NAME: &str = "mcp__tilde_worktree__ask";

/// When to ask, in the instructions. Worded firmly ("your first step... is
/// to call"): a softer "ask for a worktree with the tilde_worktree tool"
/// made Haiku skip it for a new module with tests, twice in two runs (CLI
/// 2.1.326); this asked both times, and still not for a one-line fix.
const WHEN: &str = "This app keeps separate pieces of work in separate git worktrees, so chats \
working in the same repo don't get in each other's way. In a git repo, your first step for real work \
(a feature, new files or a module, a fix that touches more than a line or two, a refactor, anything \
you'd put on its own branch) is to call the ask tool of the tilde_worktree server with a short name \
for the work, before you create or change any file, however you'd do it (edit tools or shell \
commands). The user approves or declines. Skip it only for a tiny one-off change, for questions, \
reviews or planning, or once this chat already works in a worktree.";

/// The tool's own description, with the same rule: Cursor hears the
/// instructions only when a conversation starts, so deep in a chat the
/// description is what it still sees.
const DESCRIPTION: &str = "Ask the user to do this work in a new git worktree. Call it before \
creating or changing any file for real work in a git repo: a feature, new files or a module, a fix \
touching more than a line or two, a refactor. Skip it for a tiny one-off change, for questions, \
reviews or planning, or once this chat already works in a worktree. The user approves or declines.";

/// What the call returns once the question is on screen. It returns at once:
/// Cursor gives up on a tool call after 60 seconds (CLI 2026.11.13, no
/// setting for it over ACP), so the answer comes as the next message.
const WAITING: &str = "The user is being asked whether to do this work in a new worktree. End your \
turn now and make no changes. Their answer comes as your next message.";
/// Asked again while the question is open.
const ASKED_ALREADY: &str = "The user is already being asked about a worktree. End your turn now and \
make no changes. Their answer comes as your next message.";
/// Sent when the user says no.
const STAY_HERE: &str = "The user answered the worktree question: do this work here, in the current \
folder, not in a worktree. Carry on with the task here.";
/// Put before the user's message when they write instead of answering.
/// Nothing is chosen.
const REPLIED: &str = "The user sent the message below instead of answering the worktree question. \
Go by it. If the work still needs file changes, ask for a worktree again before making any.";
/// What the agent is told if making the worktree failed.
const STAYING: &str = "The worktree couldn't be made. Carry on with the task in this folder.";

/// The group's tool definition.
fn tools() -> Value {
    json!([{
        "name": TOOL,
        "description": DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "A short name for the work, like \"bump dependencies\". Used for the branch.",
                },
            },
            "required": ["name"],
        },
        // Claude Code otherwise defers MCP tools behind its tool search,
        // and Haiku skipped asking when the tool wasn't on its list (CLI
        // 2.1.326). Other agents ignore this.
        "_meta": { "anthropic/alwaysLoad": true },
    }])
}

fn days_ms(days: u64) -> i64 {
    (days * 24 * 3600 * 1000) as i64
}

impl SessionManager {
    pub(super) fn worktrees_dir(&self) -> PathBuf {
        self.0.data_dir.join("worktrees")
    }

    /// A length in ms: an environment override in seconds (for tests), else a
    /// setting in days, else the default.
    fn period_ms(&self, env: &str, setting: &str, default_days: u64) -> i64 {
        if let Some(seconds) = crate::home::app_env_text(env).and_then(|v| v.trim().parse::<i64>().ok()) {
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
        self.period_ms("WORKTREE_IDLE_SECONDS", "worktree_idle_days", DEFAULT_IDLE_DAYS)
    }

    fn keep_ms(&self) -> i64 {
        if let Some(fixed) = self.0.timings.keep {
            return fixed.as_millis() as i64;
        }
        self.period_ms("TRASH_SECONDS", "trash_days", DEFAULT_TRASH_DAYS)
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

    /// The worktree tool group for one main chat.
    pub(super) fn worktree_tools(&self, session_id: &str) -> AppTools {
        let manager = self.clone();
        let id = session_id.to_string();
        AppTools {
            server: SERVER,
            instruction: WHEN.to_string(),
            list: tools(),
            call: Arc::new(move |tool, args| {
                let manager = manager.clone();
                let id = id.clone();
                Box::pin(async move {
                    if tool != TOOL {
                        return Err(format!("Unknown tool: {tool}"));
                    }
                    let text = manager.ask_worktree(&id, &args);
                    Ok(vec![json!({ "type": "text", "text": text })])
                })
            }),
        }
    }

    /// The agent asks for a worktree. The question goes on screen and the
    /// call returns at once, telling the agent to end its turn: the answer
    /// comes as its next message (see `choose_worktree`). A request that
    /// can't be met gets the reason instead, with no question.
    fn ask_worktree(&self, session_id: &str, args: &Value) -> String {
        self.load_open_ask(session_id);
        let name = args["name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_owned);
        let has_worktree = self
            .0
            .store
            .chat_git(session_id)
            .ok()
            .flatten()
            .and_then(|g| g.worktree.filter(|_| g.status.is_some()));
        let (card, repo_root, tool_id) = {
            let mut sessions = self.0.sessions.lock().unwrap();
            let Some(session) = sessions.get_mut(session_id) else {
                return "This chat isn't running.".to_string();
            };
            if session.ask.is_some() {
                return ASKED_ALREADY.to_string();
            }
            if let Some(wt) = &has_worktree {
                return format!(
                    "This chat already works in its own git worktree at {}. Carry on there.",
                    wt.work_path().display()
                );
            }
            let Some(repo_root) = session.repo_root.clone() else {
                return "This folder isn't in a git repo, so there is no worktree to make. Carry on here."
                    .to_string();
            };
            let card = uuid::Uuid::new_v4().to_string();
            session.ask = Some(Ask {
                repo_root: repo_root.clone(),
                card: card.clone(),
                name: name.clone(),
            });
            (card, repo_root, session.worktree_tool_id.take())
        };
        self.record(
            session_id,
            AgentEvent::WorktreeAsk {
                request_id: card,
                repo_root: repo_root.to_string_lossy().into_owned(),
                tool_id,
                name,
            },
        );
        WAITING.to_string()
    }

    /// Brings back a worktree question left open when the app last quit: the
    /// saved history has it, memory doesn't. Checked once per chat per run.
    fn load_open_ask(&self, session_id: &str) {
        {
            let mut sessions = self.0.sessions.lock().unwrap();
            let session = sessions.entry(session_id.to_string()).or_default();
            if std::mem::replace(&mut session.ask_checked, true) || session.ask.is_some() {
                return;
            }
        }
        let Ok(events) = self.0.store.events(session_id) else {
            return;
        };
        let mut open: Option<Ask> = None;
        for event in events {
            match event {
                AgentEvent::WorktreeAsk {
                    request_id,
                    repo_root,
                    name,
                    ..
                } => {
                    open = Some(Ask {
                        repo_root: PathBuf::from(repo_root),
                        card: request_id,
                        name,
                    })
                }
                AgentEvent::WorktreeChosen { .. } => open = None,
                AgentEvent::ApprovalAnswered { request_id, .. } | AgentEvent::ApprovalCancelled { request_id }
                    if open.as_ref().is_some_and(|a| a.card == request_id) =>
                {
                    open = None
                }
                _ => {}
            }
        }
        if let Some(session) = self.0.sessions.lock().unwrap().get_mut(session_id) {
            if session.ask.is_none() {
                session.ask = open;
            }
        }
    }

    /// The user wrote instead of answering the worktree question. It closes
    /// with nothing chosen. Returns the note to put before their message,
    /// telling the agent to go by it and ask again if it still needs a
    /// worktree; None when no question was open.
    pub(super) fn reply_closes_ask(&self, session_id: &str) -> Option<&'static str> {
        self.load_open_ask(session_id);
        let ask = self.0.sessions.lock().unwrap().get_mut(session_id).and_then(|s| s.ask.take())?;
        self.record(
            session_id,
            AgentEvent::ApprovalAnswered {
                request_id: ask.card,
                decision: Decision::Deny,
                answers: None,
            },
        );
        Some(REPLIED)
    }

    /// Sends the agent a note from the app that the chat doesn't show,
    /// starting its process if it isn't running.
    async fn send_note(&self, session_id: &str, note: &str) -> Result<(), String> {
        let saved = self.0.store.session(session_id)?.ok_or("That chat no longer exists")?;
        let message_id = uuid::Uuid::new_v4().to_string();
        self.send_inner(
            session_id,
            &saved.agent,
            PathBuf::from(&saved.project_path),
            Some(saved.mode),
            Some(saved.model),
            None,
            note,
            &[],
            &message_id,
            false,
            &[],
        )
        .await
    }
    /// The user's answer to the agent's request for a worktree: "worktree" or "here".
    ///
    /// "here": the agent is sent a note to carry on in the folder it is in.
    ///
    /// "worktree": the process is stopped (if it still runs), a worktree made,
    /// and the same chat resumes inside it with a note telling the agent
    /// where it now is.
    pub async fn choose_worktree(
        &self,
        session_id: &str,
        request_id: &str,
        choice: &str,
    ) -> Result<(), String> {
        if choice != "worktree" && choice != "here" {
            return Err(format!("Unknown choice: {choice}"));
        }
        self.load_open_ask(session_id);
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

        if choice == "here" {
            return self.send_note(session_id, STAY_HERE).await;
        }

        // The agent was told to end its turn; stop it in case it hasn't.
        if let Ok(agent) = self.agent(session_id) {
            let _ = agent.interrupt();
            agent.stop();
            self.wait_for_exit(session_id).await?;
        }

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
                     the old one. Don't ask for a worktree again in this chat.",
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
        self.send_note(session_id, &note).await
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
            manager.pr_due(&id, Duration::ZERO); // Marks it looked up.
            manager.refresh_prs(&id).await;
        });
    }

    /// The window came to the front: the pull requests of every chat that may
    /// have one changing are looked up, all in one go, so one merged or
    /// closed meanwhile shows at once.
    pub async fn check_all_prs(&self) {
        let Ok(rows) = self.0.store.sweep_list() else {
            return;
        };
        let ids: Vec<String> = rows
            .into_iter()
            .filter(|row| row.trashed_at.is_none() && self.pr_due(&row.id, PR_GAP))
            .map(|row| row.id)
            .collect();
        self.refresh_prs_of(&ids).await;
    }

    /// A chat was opened: its pull requests are looked up.
    pub async fn check_prs(&self, session_id: &str) {
        if self.pr_due(session_id, PR_GAP) {
            self.refresh_prs(session_id).await;
        }
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
    pub(super) async fn follow_branch(&self, session_id: &str) -> Option<Worktree> {
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

    /// Looks the chat's pull requests up again (see `refresh_prs_of`).
    /// Returns the list afterwards.
    async fn refresh_prs(&self, session_id: &str) -> Vec<PrInfo> {
        self.refresh_prs_of(&[session_id.to_string()]).await;
        self.0.store.chat_git(session_id).ok().flatten().map(|g| g.prs).unwrap_or_default()
    }

    /// Looks these chats' pull requests up again, all in one lookup: for
    /// each, the one for its worktree's branch (it may have been opened
    /// outside the chat), and every listed one still open or a draft. Merged
    /// and closed ones aren't asked about again. Each answer updates only its
    /// own entry, so one added meanwhile is kept.
    async fn refresh_prs_of(&self, ids: &[String]) {
        let (mut asks, mut whose) = (Vec::new(), Vec::new());
        for id in ids {
            let Ok(Some(git)) = self.0.store.chat_git(id) else {
                continue;
            };
            if let Some(wt) = git.worktree.filter(|_| git.status.is_some()) {
                asks.push(PrAsk::Branch {
                    repo_root: wt.repo_root,
                    branch: wt.branch,
                });
                whose.push(id.clone());
            }
            for pr in git.prs.into_iter().filter(|pr| matches!(pr.state.as_str(), "open" | "draft")) {
                asks.push(PrAsk::Url(pr.url));
                whose.push(id.clone());
            }
        }
        if asks.is_empty() {
            return;
        }
        let lookup = self.0.pr_lookup.clone();
        let answers = blocking(move || lookup.ask_many(&asks)).await;
        for (id, pr) in whose.iter().zip(answers) {
            if let Some(pr) = pr {
                self.save_pr(id, &pr);
            }
        }
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
            // A repo that isn't there (its folder moved, or its drive is
            // unplugged) says nothing about the branch. Leave the chat as it
            // is for when the repo is back.
            if !wt.repo_root.is_dir() {
                continue;
            }
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
                        self.terminal_gone(&wt.path, "branch merged");
                    }
                    continue;
                }
                // Its branch was deleted outside the app: nothing to bring back.
                Err(_) if status == "put_away" => {
                    let _ = blocking(drop_all).await;
                    let _ = self.0.store.clear_worktree(&id);
                    changed("gone");
                    self.terminal_gone(&wt.path, "cleaned up");
                    continue;
                }
                Err(e) => {
                    eprintln!("tilde: sweep skipped a chat: {e}");
                    continue;
                }
                Ok(Merged::No) => {}
            }
            if status == "live" && now - row.last_used_at > idle_limit {
                // Not while the user has a shell in it.
                if self.terminal_in(&wt.path) {
                    continue;
                }
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
                    Err(e) => eprintln!("tilde: couldn't put a worktree away: {e}"),
                }
            } else if status == "put_away" && row.put_away_at.is_some_and(|at| now - at > keep) {
                let _ = blocking(drop_all).await;
                let _ = self.0.store.clear_worktree(&id);
                changed("gone");
                self.terminal_gone(&wt.path, "cleaned up");
            }
        }
    }

    /// Moves a chat to the trash. Its worktree folder, if any, is put away,
    /// and the idle terminal tabs its agent opened are closed.
    pub(super) async fn trash(&self, session_id: &str) -> Result<(), String> {
        let store = &self.0.store;
        store.chat_git(session_id)?.ok_or("That chat no longer exists")?;
        if let Some(wt) = self.follow_branch(session_id).await {
            let away = wt.clone();
            let chat = session_id.to_string();
            match blocking(move || worktree::put_away(&away, &chat)).await {
                Ok(()) => {
                    store.set_worktree(session_id, &wt, "put_away")?;
                    self.terminal_gone(&wt.path, "chat deleted");
                }
                Err(e) => eprintln!("tilde: couldn't put a worktree away: {e}"),
            }
        }
        self.terminal_chat_gone(session_id);
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
            self.terminal_gone(&wt.path, "cleaned up");
            let chat = row.id.clone();
            blocking(move || {
                let _ = worktree::remove_folder(&wt);
                worktree::drop_branch(&wt.repo_root, &wt.branch, &chat);
            })
            .await;
        }
        if let Err(e) = self.0.store.delete_session(&row.id) {
            eprintln!("tilde: couldn't delete a chat: {e}");
        }
        self.0.images.delete_session(&row.id);
        self.0.notes.delete_session(&row.id, &row.project_path);
        self.0.pr_checked.lock().unwrap().remove(&row.id);
    }
}
