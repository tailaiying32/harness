//! Chats started from another chat: when the user asks, the agent splits a
//! piece of work into parts and asks to start a chat for each. One question
//! covers them all; on "start" each part gets its own worktree and a chat
//! whose first message is the part's brief. After that each is an ordinary
//! chat: the one that asked doesn't follow them. See PLAN.md, "Starting chats
//! from a plan".

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

use super::{SessionManager, blocking};
use crate::agent::{self, AppTools};
use crate::events::{AgentEvent, Decision, NewChatPart, StartedChat};
use crate::worktree;

/// The app's new-chats tool group, offered to every main chat's agent.
pub const SERVER: &str = "harness_new_chats";
const TOOL: &str = "start";
/// The name the window sees the call by, for every agent.
pub const CALL_NAME: &str = "mcp__harness_new_chats__start";

/// Most parts one question can hold.
const MAX_PARTS: usize = 8;

/// When to use it, in the instructions.
const WHEN: &str = "Only when the user explicitly asks you to split work into separate chats, or to \
start chats for parts of a plan, call the start tool of the harness_new_chats server: one call with \
every part. Never use it on your own initiative, and not when the user asks for subagents. Each part's \
brief is the new chat's whole first message and all it will know: give the goal, the decisions made, \
the files to look at, and what done looks like. Leave out \"from\" unless the user asks to branch the \
chats off this chat; if they do, commit your work first, since only commits carry over. Set a part's \
\"agent\" only if the user named one for it.";

/// The tool's own description, with the same rule (Cursor hears the
/// instructions only when a conversation starts).
const DESCRIPTION: &str = "Ask the user to start a separate chat for each part of a piece of work, \
each in its own git worktree. Use it only when the user explicitly asks for separate chats; never on \
your own, and not for subagents. Each brief must stand alone: the new chat knows nothing else. The user \
approves or declines.";

/// What the call returns once the question is on screen. It returns at once,
/// like the worktree tool: Cursor gives up on a tool call after 60 seconds.
const WAITING: &str = "The user is being asked whether to start these chats. End your turn now and \
make no changes. Their answer comes with their next message.";
const ASKED_ALREADY: &str = "The user is already being asked about starting chats. End your turn now \
and make no changes.";
/// Put before the user's message when they write instead of answering.
const REVISE: &str = "The user sent the message below instead of answering whether to start the \
chats you proposed. Go by it: if they still want the chats, change the briefs as they ask and call \
the tool again.";
const DECLINED: &str = "The user declined to start the chats you proposed.";

/// The agent asked to start chats; the question waits for the user.
pub(super) struct ChatsAsk {
    card: String,
    from: String,
    parts: Vec<NewChatPart>,
    repo_root: PathBuf,
}

fn tools() -> Value {
    let part = json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "A short title; the new chat's name." },
            "summary": { "type": "string", "description": "One line: what this part does." },
            "why": { "type": "string", "description": "One line: why this part is needed." },
            "brief": {
                "type": "string",
                "description": "The new chat's first message, complete on its own: the goal, decisions \
                                made, files to look at, and what done looks like.",
            },
            "agent": {
                "type": "string",
                "enum": [agent::CLAUDE, agent::CURSOR, agent::CODEX],
                "description": "The agent for this part. Only if the user named one; else this chat's.",
            },
        },
        "required": ["title", "summary", "why", "brief"],
    });
    json!([{
        "name": TOOL,
        "description": DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "parts": { "type": "array", "items": part, "minItems": 1, "maxItems": MAX_PARTS },
                "from": {
                    "type": "string",
                    "enum": ["main", "this_chat"],
                    "description": "Where the new chats' branches start. \"main\" (the default): where a \
                                    new chat in this project starts. \"this_chat\": this chat's branch, \
                                    only when the user asks.",
                },
            },
            "required": ["parts"],
        },
        // As with the worktree tool: Claude Code otherwise defers it.
        "_meta": { "anthropic/alwaysLoad": true },
    }])
}

/// The parts in a call, or what is wrong with them.
fn read_parts(args: &Value) -> Result<Vec<NewChatPart>, String> {
    let list = args["parts"].as_array().filter(|l| !l.is_empty()).ok_or("Give at least one part.")?;
    if list.len() > MAX_PARTS {
        return Err(format!("At most {MAX_PARTS} parts at once."));
    }
    let text = |v: &Value, key: &str| v[key].as_str().map(str::trim).unwrap_or("").to_string();
    let mut parts = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let part = NewChatPart {
            title: text(v, "title"),
            summary: text(v, "summary"),
            why: text(v, "why"),
            brief: text(v, "brief"),
            agent: v["agent"].as_str().map(str::trim).filter(|a| !a.is_empty()).map(str::to_owned),
        };
        if part.title.is_empty() || part.brief.is_empty() {
            return Err(format!("Part {} needs a title and a brief.", i + 1));
        }
        parts.push(part);
    }
    Ok(parts)
}

impl SessionManager {
    /// The new-chats tool group for one main chat.
    pub(super) fn new_chats_tools(&self, session_id: &str) -> AppTools {
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
                    let text = manager.ask_new_chats(&id, &args);
                    Ok(vec![json!({ "type": "text", "text": text })])
                })
            }),
        }
    }

    /// The agent asks to start chats. The question goes on screen and the
    /// call returns at once; a request that can't be met gets the reason.
    fn ask_new_chats(&self, session_id: &str, args: &Value) -> String {
        self.load_open_chats_ask(session_id);
        let parts = match read_parts(args) {
            Ok(parts) => parts,
            Err(e) => return e,
        };
        let installed = self.0.agents.installed();
        if let Some(part) = parts.iter().find(|p| p.agent.as_ref().is_some_and(|a| !installed.contains(a))) {
            return format!(
                "\"{}\" asks for agent {}, which isn't installed here. Installed: {}.",
                part.title,
                part.agent.as_deref().unwrap_or(""),
                installed.join(", ")
            );
        }
        let from = match args["from"].as_str() {
            Some("this_chat") => "this_chat",
            _ => "main",
        };
        let (card, tool_id) = {
            let mut sessions = self.0.sessions.lock().unwrap();
            let Some(session) = sessions.get_mut(session_id) else {
                return "This chat isn't running.".to_string();
            };
            if session.chats_ask.is_some() {
                return ASKED_ALREADY.to_string();
            }
            let Some(repo_root) = session.repo_root.clone() else {
                return "This folder isn't in a git repo, so separate chats can't each get a worktree."
                    .to_string();
            };
            let card = uuid::Uuid::new_v4().to_string();
            session.chats_ask = Some(ChatsAsk {
                card: card.clone(),
                from: from.to_string(),
                parts: parts.clone(),
                repo_root,
            });
            (card, session.new_chats_tool_id.take())
        };
        self.record(
            session_id,
            AgentEvent::NewChatsAsk {
                request_id: card,
                tool_id,
                from: from.to_string(),
                parts,
            },
        );
        WAITING.to_string()
    }

    /// Brings back a new-chats question left open when the app last quit.
    /// Checked once per chat per run.
    fn load_open_chats_ask(&self, session_id: &str) {
        {
            let mut sessions = self.0.sessions.lock().unwrap();
            let session = sessions.entry(session_id.to_string()).or_default();
            if std::mem::replace(&mut session.chats_ask_checked, true) || session.chats_ask.is_some() {
                return;
            }
        }
        let Ok(events) = self.0.store.events(session_id) else {
            return;
        };
        let mut open: Option<(String, String, Vec<NewChatPart>)> = None;
        for event in events {
            match event {
                AgentEvent::NewChatsAsk {
                    request_id, from, parts, ..
                } => open = Some((request_id, from, parts)),
                AgentEvent::NewChatsChosen { request_id, .. }
                | AgentEvent::ApprovalAnswered { request_id, .. }
                | AgentEvent::ApprovalCancelled { request_id }
                    if open.as_ref().is_some_and(|(card, ..)| *card == request_id) =>
                {
                    open = None
                }
                _ => {}
            }
        }
        let Some((card, from, parts)) = open else {
            return;
        };
        // The repo is the chat's project's, found again.
        let Some(project) = self.0.store.session(session_id).ok().flatten().map(|s| s.project_path) else {
            return;
        };
        let Some((repo_root, _)) = worktree::repo_of(Path::new(&project)) else {
            return;
        };
        if let Some(session) = self.0.sessions.lock().unwrap().get_mut(session_id) {
            if session.chats_ask.is_none() {
                session.chats_ask = Some(ChatsAsk {
                    card,
                    from,
                    parts,
                    repo_root,
                });
            }
        }
    }

    /// The user wrote instead of answering the new-chats question: it closes
    /// with nothing started. Returns the note to put before their message.
    pub(super) fn reply_closes_chats_ask(&self, session_id: &str) -> Option<&'static str> {
        self.load_open_chats_ask(session_id);
        let ask = self.0.sessions.lock().unwrap().get_mut(session_id).and_then(|s| s.chats_ask.take())?;
        self.record(
            session_id,
            AgentEvent::ApprovalAnswered {
                request_id: ask.card,
                decision: Decision::Deny,
                answers: None,
            },
        );
        Some(REVISE)
    }

    /// The user's answer to the request to start chats: "start" or "no".
    /// "start" returns at once; the chats are made one by one in the
    /// background, and `NewChatsStarted` lists them when all are done.
    pub async fn choose_new_chats(&self, session_id: &str, request_id: &str, choice: &str) -> Result<(), String> {
        if choice != "start" && choice != "no" {
            return Err(format!("Unknown choice: {choice}"));
        }
        self.load_open_chats_ask(session_id);
        let ask = {
            let mut sessions = self.0.sessions.lock().unwrap();
            let session = sessions
                .get_mut(session_id)
                .filter(|s| s.chats_ask.as_ref().is_some_and(|a| a.card == request_id))
                .ok_or("This question is no longer waiting for an answer")?;
            session.chats_ask.take().unwrap()
        };
        self.record(
            session_id,
            AgentEvent::NewChatsChosen {
                request_id: request_id.to_string(),
                choice: choice.to_string(),
            },
        );
        if choice == "no" {
            return self.0.store.add_pending_note(session_id, DECLINED);
        }
        let manager = self.clone();
        let parent = session_id.to_string();
        let work = async move { manager.start_children(&parent, ask).await };
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(work);
        }
        Ok(())
    }

    /// Starts a chat for each part, in order, then lists them on the asking
    /// chat and leaves its agent a note naming them.
    async fn start_children(&self, parent: &str, ask: ChatsAsk) {
        let mut chats = Vec::new();
        for part in &ask.parts {
            let started = self.start_child(parent, part, &ask.from, &ask.repo_root).await;
            if let Err(e) = &started {
                eprintln!("harness: couldn't start a chat for \"{}\": {e}", part.title);
            }
            chats.push(StartedChat {
                title: part.title.clone(),
                session_id: started.as_ref().ok().cloned(),
                error: started.err(),
            });
        }
        let links: Vec<String> = chats
            .iter()
            .filter_map(|c| c.session_id.as_ref().map(|id| format!("[@{}](<chat:{id}>)", c.title)))
            .collect();
        let failed = chats.iter().filter(|c| c.error.is_some()).count();
        let mut note = if links.is_empty() {
            "None of the chats you proposed could be started.".to_string()
        } else {
            format!("The user started the chats you proposed: {}.", links.join(", "))
        };
        if failed > 0 && !links.is_empty() {
            note.push_str(&format!(" {failed} couldn't be started."));
        }
        let _ = self.0.store.add_pending_note(parent, &note);
        self.record(
            parent,
            AgentEvent::NewChatsStarted {
                request_id: ask.card,
                chats,
            },
        );
    }

    /// Starts one chat: its worktree, its saved row, then its first message,
    /// the part's brief. Returns its ID.
    async fn start_child(&self, parent: &str, part: &NewChatPart, from: &str, repo_root: &Path) -> Result<String, String> {
        let store = &self.0.store;
        let saved = store.session(parent)?.ok_or("The chat that asked no longer exists")?;
        let project = PathBuf::from(&saved.project_path);

        // The parent's agent and its settings, unless the part names another.
        let agent_id = part.agent.clone().unwrap_or_else(|| saved.agent.clone());
        let same = agent_id == saved.agent;
        let (mode, model) = if same {
            (saved.mode.clone(), saved.model.clone())
        } else {
            ("default".to_string(), "default".to_string())
        };
        self.0.agents.get(&agent_id)?;

        // Where its branch starts: the parent's branch when asked, else where
        // a new chat here would start.
        let parent_branch = if from == "this_chat" {
            self.follow_branch(parent).await.map(|wt| wt.branch)
        } else {
            None
        };
        let (base_branch, base_commit) = match parent_branch {
            Some(branch) => (Some(branch), None),
            None => {
                let folder = project.clone();
                let (branch, commit) = blocking(move || worktree::head_of(&folder))
                    .await
                    .ok_or("The project's folder has no commit to start from")?;
                (branch, Some(commit))
            }
        };

        let made = {
            let (root, project, dir) = (repo_root.to_path_buf(), project.clone(), self.worktrees_dir());
            let (title, branch, commit) = (part.title.clone(), base_branch.clone(), base_commit.clone());
            blocking(move || {
                let sub = worktree::repo_of(&project).map(|(_, s)| s).unwrap_or_default();
                worktree::create(&root, &sub, &title, branch.as_deref(), commit.as_deref(), &dir)
            })
            .await?
        };

        let child = uuid::Uuid::new_v4().to_string();
        let saved_row = (|| {
            store.touch_session(&child, &saved.project_path, &part.brief, &mode, &model)?;
            store.set_agent(&child, &agent_id)?;
            store.set_start(&child, made.base_branch.as_deref(), &made.base_commit)?;
            store.rename_session(&child, &part.title)?;
            store.set_worktree(&child, &made, "live")?;
            store.set_choice(&child, Some("worktree"))?;
            if same {
                store.set_effort(&child, &saved.effort)?;
            }
            Ok::<(), String>(())
        })();
        let sent = match saved_row {
            Ok(()) => {
                self.record(
                    &child,
                    AgentEvent::WorktreeChanged {
                        status: "created".into(),
                        branch: Some(made.branch.clone()),
                    },
                );
                let note = format!(
                    "This chat was started from another chat ([@{}](<chat:{parent}>)) to do one part of a \
                     larger piece of work. The message below is your brief. Read it and look at the code \
                     it points to, then reply with what you understood, how you'd do it, and any \
                     questions. Don't create or change files until the user says to go ahead.",
                    saved.title
                );
                let message_id = uuid::Uuid::new_v4().to_string();
                self.send_inner(
                    &child,
                    &agent_id,
                    project.clone(),
                    Some(mode),
                    Some(model),
                    None,
                    &part.brief,
                    &[],
                    &message_id,
                    true,
                    &[note],
                )
                .await
            }
            Err(e) => Err(e),
        };
        if let Err(e) = sent {
            // Nothing half made is left behind.
            self.close(&child);
            let _ = self.wait_for_exit(&child).await;
            let _ = store.delete_session(&child);
            let wt = made.clone();
            let chat = child.clone();
            blocking(move || {
                let _ = worktree::remove_folder(&wt);
                worktree::drop_branch(&wt.repo_root, &wt.branch, &chat);
            })
            .await;
            return Err(e);
        }
        Ok(child)
    }
}
