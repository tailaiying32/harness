//! The app's own event model. Nothing here knows about any particular agent.
//! Adapters under `agent/` turn their agent's wire format into these.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Starting,
    Working,
    WaitingApproval,
    Idle,
    Exited,
}

/// One finished piece of an assistant reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    Thinking { text: String },
    ToolCall { tool_id: String, name: String, input: Value },
}

/// One command or agent running in the background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub id: String,
    /// The agent's own word for it, e.g. "local_bash" or "local_agent".
    pub kind: String,
    pub description: String,
}

/// One part of what fills the context window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPart {
    pub name: String,
    pub tokens: u64,
}

/// What kind of answer an approval request wants from the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    /// Allow or deny a tool call.
    Tool,
    /// Approve a plan, or keep planning.
    Plan,
    /// Pick an answer to a question the agent asked.
    Question,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    AlwaysAllow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Sent at the start of every turn. `agent_session_id` is what resume needs.
    SessionStarted {
        agent_session_id: String,
        model: Option<String>,
        mode: Option<String>,
    },
    Status {
        state: SessionState,
    },
    /// A live piece of text. Not saved; the matching `AssistantBlock` is the final truth.
    TextDelta {
        message_id: String,
        block_index: u64,
        text: String,
    },
    AssistantBlock {
        message_id: String,
        block: Block,
        /// Set when this came from a subagent started by that tool call.
        parent_tool_id: Option<String>,
    },
    ToolResult {
        tool_id: String,
        content: Value,
        is_error: bool,
        parent_tool_id: Option<String>,
    },
    /// A tool call's name and input, when they came after the call was first
    /// shown: Cursor sends a call empty and fills it in. Saved, so the call
    /// still reads right when the chat is reopened or read by another chat.
    ToolInput {
        tool_id: String,
        name: String,
        input: Value,
        parent_tool_id: Option<String>,
    },
    ApprovalRequest {
        request_id: String,
        approval: ApprovalKind,
        tool_name: String,
        title: String,
        input: Value,
        tool_id: Option<String>,
        /// What "always allow" would do here, as button text. None = nothing
        /// to remember, so the choice isn't offered.
        always_label: Option<String>,
    },
    /// The agent withdrew a request, e.g. because the turn was stopped.
    ApprovalCancelled {
        request_id: String,
    },
    /// The user answered an approval. Recorded by the core, so a reloaded chat
    /// shows how each request went.
    ApprovalAnswered {
        request_id: String,
        decision: Decision,
        /// For a question: each question's text mapped to the chosen answer.
        answers: Option<BTreeMap<String, String>>,
    },
    /// The permission mode changed, by the user or as a side effect of an approval.
    ModeChanged {
        mode: String,
    },
    /// The user pressed stop. The turn's end follows as `TurnEnded`.
    Interrupted,
    UserMessage {
        text: String,
        /// Names the message, so `MessageSeen` can point at it. Absent in
        /// history saved before messages had IDs.
        #[serde(default)]
        id: Option<String>,
        /// Images sent with it, in the order their `[image n]` words appear.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<crate::images::ImageRef>,
    },
    /// The agent took a message in. One sent mid-turn waits until the running
    /// step is done.
    MessageSeen {
        id: String,
    },
    /// The user took back a message the agent hadn't read yet, to edit it.
    /// `ok` is false when it was too late: the agent had taken it in already.
    /// Only a taken-back message is saved, so history drops it.
    MessageWithdrawn {
        id: String,
        ok: bool,
    },
    TurnEnded {
        ok: bool,
        duration_ms: Option<u64>,
    },
    /// How much of a usage limit is spent. `window` names the limit, e.g.
    /// "seven_day". One event per limit the agent reported.
    UsageUpdate {
        window: Option<String>,
        /// 0.0 to 1.0.
        utilization: f64,
        /// When the limit starts over, in seconds since 1970.
        resets_at: Option<u64>,
    },
    /// How many background commands and agents the agent has running. Sent
    /// whenever that changes. A chat with any running is not closed when idle.
    BackgroundWork {
        running: u32,
        /// How many of those are agents. A chat with any shows as working.
        agents: u32,
        /// What each one is, for the chat info view.
        #[serde(default)]
        tasks: Vec<BackgroundTask>,
    },
    /// A subagent (or other agent task) started. `tool_id` is the main chat's
    /// tool call that started it; its steps carry that as `parent_tool_id`.
    TaskStarted {
        task_id: String,
        tool_id: Option<String>,
        description: String,
        /// The kind of subagent, e.g. "general-purpose".
        agent_type: Option<String>,
        /// Runs on after its tool call returns; only `TaskEnded` says it's done.
        background: bool,
    },
    /// What a running task is doing now. Frequent, so not saved.
    TaskProgress {
        task_id: String,
        tool_id: Option<String>,
        /// A short live line, e.g. "Reading src/one.txt".
        description: Option<String>,
        tool_uses: Option<u32>,
        last_tool: Option<String>,
    },
    /// A task finished: "completed", "failed" or "stopped". `summary` is the
    /// answer it handed back, when the agent sends one.
    TaskEnded {
        task_id: String,
        tool_id: Option<String>,
        status: String,
        summary: Option<String>,
        /// How long it ran, when the agent says.
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    /// How full the context window is, read after each turn. Saved, so a
    /// reopened chat shows its last reading.
    ContextUsage {
        used: u64,
        max: u64,
        /// What takes the space, largest first; empty when not known.
        #[serde(default)]
        parts: Vec<ContextPart>,
    },
    Error {
        message: String,
    },
    /// The agent started or finished compacting the conversation.
    Compacting {
        active: bool,
    },
    /// The conversation was compacted: the agent now sees a summary of what
    /// came before. `auto` = the agent did it on its own as context filled up.
    Compacted {
        auto: bool,
    },
    /// The agent isn't signed in, or its login couldn't be renewed. `message`
    /// is the agent's own wording.
    SignInNeeded {
        message: String,
    },
    /// A batch of subagents got a written label for chat info. `tool_id` is
    /// the batch's first subagent. Saved, so a reopened chat keeps it.
    BatchTitled {
        tool_id: String,
        title: String,
    },
    /// The user saved a doc from the chat's folder to the project: it moved
    /// from `from` to `to`. Saved, so links and tabs to `from` still find it
    /// after the chat is reopened.
    DocSaved {
        from: String,
        to: String,
    },
    /// The chat got a written title. Not saved: the store holds the title.
    TitleChanged {
        title: String,
    },
    /// The agent asked to work in a worktree, and waits while the user picks:
    /// a new worktree, or this folder.
    WorktreeAsk {
        request_id: String,
        repo_root: String,
        /// The request's tool call, so the chat can fold it away.
        tool_id: Option<String>,
        /// The name the agent suggested, used for the branch.
        #[serde(default)]
        name: Option<String>,
    },
    /// The user's pick: "worktree" or "here".
    WorktreeChosen {
        choice: String,
    },
    /// The chat's worktree changed: "created", "restored", "put_away",
    /// "merged" or "gone".
    WorktreeChanged {
        status: String,
        branch: Option<String>,
    },
    /// The chat's pull requests changed: all of them, oldest first. Not
    /// saved: the store holds them.
    PrsChanged {
        prs: Vec<crate::worktree::PrInfo>,
    },
    /// The chat was started in a folder that turned out to be a project
    /// whose folder had moved from `from`: its chats, docs and memory came
    /// along to `to`. Saved, so the chat keeps the line that says so.
    ProjectMoved {
        from: String,
        to: String,
    },
    /// The user edited a plan before approving it; the agent works from this
    /// text. Saved, so the plan shows as approved after a restart.
    PlanEdited {
        request_id: String,
        plan: String,
    },
    /// The chat's side-panel browser: every tab, and the one on show. Sent
    /// whole on every change; an empty list means the browser closed. Not saved.
    BrowserTabs {
        tabs: Vec<BrowserTab>,
        shown: Option<String>,
    },
    /// A project's terminal tabs changed: one opened, closed, or what runs in
    /// it changed. Sent with an empty session ID. Not saved.
    TerminalTabs {
        project: String,
        tabs: Vec<crate::terminal::TabInfo>,
    },
    /// A tab key from the Mac menu: Cmd+T ("new"), Cmd+W ("close"), Cmd+Shift+[
    /// ("prev") or Cmd+Shift+] ("next"). The window applies it to the panel
    /// view that has the keys; Cmd+W with none closes the window. Menu keys
    /// arrive even while a browser page holds the keys. Sent with an empty
    /// session ID. Not saved.
    TabKey { action: String },
    /// Keep awake started, changed or ended: its mode and latest end (ms
    /// since 1970), both empty once it ended. Sent with an empty session ID.
    /// Not saved.
    KeepAwake {
        mode: Option<crate::keep_awake::Mode>,
        ends_at: Option<u64>,
    },
}

/// One tab of a chat's browser. `url` is empty for a new tab with no page yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
}

impl AgentEvent {
    /// Whether this belongs in a chat's saved history. Live text pieces, usage
    /// readings and process states are only for the moment.
    pub fn is_saved(&self) -> bool {
        !matches!(
            self,
            AgentEvent::TextDelta { .. }
                | AgentEvent::UsageUpdate { .. }
                | AgentEvent::Status { .. }
                | AgentEvent::Compacting { .. }
                | AgentEvent::BackgroundWork { .. }
                | AgentEvent::TaskProgress { .. }
                | AgentEvent::TitleChanged { .. }
                | AgentEvent::PrsChanged { .. }
                | AgentEvent::BrowserTabs { .. }
                | AgentEvent::TerminalTabs { .. }
                | AgentEvent::TabKey { .. }
                | AgentEvent::KeepAwake { .. }
                | AgentEvent::MessageWithdrawn { ok: false, .. }
        )
    }

    /// A tool result's text, with its parts joined. None for other events.
    pub fn tool_result_text(&self) -> Option<String> {
        let AgentEvent::ToolResult { content, .. } = self else {
            return None;
        };
        Some(match content {
            Value::String(text) => text.clone(),
            Value::Array(parts) => {
                let texts = parts.iter().filter_map(|part| part.get("text")?.as_str());
                texts.collect::<Vec<_>>().join("\n")
            }
            _ => String::new(),
        })
    }

    /// Cuts a very long tool result. The window shows only a few lines of one,
    /// and whole file dumps would bloat both memory and the saved history.
    pub fn cap_tool_result(&mut self, max_chars: usize) {
        let AgentEvent::ToolResult { content, .. } = self else {
            return;
        };
        let mut left = max_chars;
        let mut cut = |text: &mut String| {
            let count = text.chars().count();
            if count > left {
                *text = text.chars().take(left).collect::<String>() + "\n… (cut)";
                left = 0;
            } else {
                left -= count;
            }
        };
        match content {
            Value::String(text) => cut(text),
            Value::Array(parts) => {
                for part in parts {
                    if let Some(Value::String(text)) = part.get_mut("text") {
                        cut(text);
                    }
                }
            }
            _ => {}
        }
    }
}

/// One model the agent offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelInfo {
    /// What to pass back to pick it: an alias, a full name, or "default".
    pub value: String,
    /// What to show, e.g. "Opus 5.5" or "Default (Fable 5.2)".
    pub name: String,
    /// A short description, if the agent gives one.
    pub detail: Option<String>,
    /// Effort levels it takes, lowest first. Empty = no effort setting.
    pub efforts: Vec<String>,
    /// The level it uses when none is picked, if known.
    pub default_effort: Option<String>,
}

/// What the window receives: an event plus the chat it belongs to.
#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub session_id: String,
    pub event: AgentEvent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(content: Value) -> AgentEvent {
        AgentEvent::ToolResult {
            tool_id: "t".into(),
            content,
            is_error: false,
            parent_tool_id: None,
        }
    }

    #[test]
    fn long_tool_results_are_cut_and_short_ones_kept() {
        let mut long = result(json!("abcdefghij"));
        long.cap_tool_result(4);
        assert_eq!(long, result(json!("abcd\n… (cut)")));

        let mut short = result(json!("abc"));
        short.cap_tool_result(4);
        assert_eq!(short, result(json!("abc")));

        // The limit is shared across the parts of a multi-part result.
        let mut parts = result(json!([{ "type": "text", "text": "abc" }, { "type": "text", "text": "defg" }]));
        parts.cap_tool_result(5);
        assert_eq!(
            parts,
            result(json!([{ "type": "text", "text": "abc" }, { "type": "text", "text": "de\n… (cut)" }]))
        );
    }

    #[test]
    fn only_lasting_events_are_saved() {
        assert!(AgentEvent::Interrupted.is_saved());
        assert!(AgentEvent::UserMessage { text: "hi".into(), id: None, images: vec![] }.is_saved());
        assert!(AgentEvent::MessageSeen { id: "u1".into() }.is_saved());
        // A message taken back leaves history; a failed try leaves no trace.
        assert!(AgentEvent::MessageWithdrawn { id: "u1".into(), ok: true }.is_saved());
        assert!(!AgentEvent::MessageWithdrawn { id: "u1".into(), ok: false }.is_saved());
        // History saved before messages had IDs still loads.
        assert_eq!(
            serde_json::from_str::<AgentEvent>(r#"{"kind":"user_message","text":"hi"}"#).unwrap(),
            AgentEvent::UserMessage { text: "hi".into(), id: None, images: vec![] }
        );
        assert!(!AgentEvent::Status { state: SessionState::Idle }.is_saved());
        assert!(!AgentEvent::BackgroundWork { running: 1, agents: 0, tasks: vec![] }.is_saved());
        assert!(!AgentEvent::TaskProgress {
            task_id: "a".into(),
            tool_id: None,
            description: None,
            tool_uses: None,
            last_tool: None
        }
        .is_saved());
        assert!(AgentEvent::ContextUsage { used: 1, max: 2, parts: vec![] }.is_saved());
        // Background readings saved before the task list existed still load.
        assert_eq!(
            serde_json::from_str::<AgentEvent>(r#"{"kind":"background_work","running":1,"agents":0}"#).unwrap(),
            AgentEvent::BackgroundWork { running: 1, agents: 0, tasks: vec![] }
        );
        assert!(!AgentEvent::Compacting { active: true }.is_saved());
        assert!(AgentEvent::Compacted { auto: false }.is_saved());
        assert!(AgentEvent::SignInNeeded { message: "x".into() }.is_saved());
        assert!(
            !AgentEvent::TextDelta {
                message_id: "m".into(),
                block_index: 0,
                text: "x".into()
            }
            .is_saved()
        );
    }
}
