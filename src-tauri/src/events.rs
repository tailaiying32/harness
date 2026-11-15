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
    },
    /// The agent took a message in. One sent mid-turn waits until the running
    /// step is done.
    MessageSeen {
        id: String,
    },
    TurnEnded {
        ok: bool,
        duration_ms: Option<u64>,
    },
    /// How much of a usage limit is spent. `window` names the limit, e.g. "seven_day".
    UsageUpdate {
        window: Option<String>,
        /// 0.0 to 1.0.
        utilization: f64,
    },
    /// How many background commands and agents the agent has running. Sent
    /// whenever that changes. A chat with any running is not closed when idle.
    BackgroundWork {
        running: u32,
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
    /// The worktree branch's pull request changed. Not saved: the store holds it.
    PrChanged {
        pr: Option<crate::worktree::PrInfo>,
    },
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
                | AgentEvent::TitleChanged { .. }
                | AgentEvent::PrChanged { .. }
        )
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
        assert!(AgentEvent::UserMessage { text: "hi".into(), id: None }.is_saved());
        assert!(AgentEvent::MessageSeen { id: "u1".into() }.is_saved());
        // History saved before messages had IDs still loads.
        assert_eq!(
            serde_json::from_str::<AgentEvent>(r#"{"kind":"user_message","text":"hi"}"#).unwrap(),
            AgentEvent::UserMessage { text: "hi".into(), id: None }
        );
        assert!(!AgentEvent::Status { state: SessionState::Idle }.is_saved());
        assert!(!AgentEvent::BackgroundWork { running: 1 }.is_saved());
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
