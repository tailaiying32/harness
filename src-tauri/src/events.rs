//! The app's own event model. Nothing here knows about any particular agent.
//! Adapters under `agent/` turn their agent's wire format into these.

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
    },
    UserMessage {
        text: String,
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
    Error {
        message: String,
    },
}

/// What the window receives: an event plus the chat it belongs to.
#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub session_id: String,
    pub event: AgentEvent,
}
