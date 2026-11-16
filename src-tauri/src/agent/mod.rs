//! The boundary between the app and any coding agent. Each agent is one
//! adapter here: `claude_code` (Claude Code's own channel) and `cursor`
//! (Cursor's agent over ACP, see `acp`).

pub mod acp;
pub mod claude_code;
pub mod cursor;
pub mod mcp_http;

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::events::{AgentEvent, Decision, ModelInfo};

/// Agent IDs, as saved with each chat.
pub const CLAUDE: &str = "claude-code";
pub const CURSOR: &str = "cursor";

/// What the user sees an agent called.
pub fn label(agent: &str) -> &'static str {
    match agent {
        CURSOR => "Cursor",
        _ => "Claude Code",
    }
}

/// Every agent the app knows, with where its program was found (None = not
/// installed, or not looked for yet).
#[derive(Clone, Default)]
pub struct Agents(Arc<Mutex<BTreeMap<String, (Arc<dyn AgentAdapter>, Option<PathBuf>)>>>);

impl Agents {
    pub fn add(&self, id: &str, adapter: Arc<dyn AgentAdapter>, binary: Option<PathBuf>) {
        self.0.lock().unwrap().insert(id.to_string(), (adapter, binary));
    }

    pub fn set_binary(&self, id: &str, binary: Option<PathBuf>) {
        if let Some(entry) = self.0.lock().unwrap().get_mut(id) {
            entry.1 = binary;
        }
    }

    /// The agent and its program, or why it can't be used.
    pub fn get(&self, id: &str) -> Result<(Arc<dyn AgentAdapter>, PathBuf), String> {
        let agents = self.0.lock().unwrap();
        let (adapter, binary) = agents.get(id).ok_or_else(|| format!("Unknown agent: {id}"))?;
        let binary = binary.clone().ok_or_else(|| format!("{} couldn't be found", label(id)))?;
        Ok((adapter.clone(), binary))
    }
}

/// Where an adapter delivers events. Called from background tasks.
pub type EventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

pub struct SpawnOptions {
    /// Path to the agent's program.
    pub binary: PathBuf,
    /// Folder the agent works in.
    pub project_path: PathBuf,
    /// The agent's own session ID, to pick an earlier chat back up.
    pub resume_id: Option<String>,
    /// Permission mode to start in ("default", "acceptEdits", "plan"). None = the agent's default.
    pub mode: Option<String>,
    /// Model to start on: an alias such as "opus" or a full name. None or "default" = the agent's default.
    pub model: Option<String>,
    /// Effort level to start on ("low" ... "max"). None or "default" = the agent's default for the model.
    pub effort: Option<String>,
    /// Set for a side chat: a read-only conversation that starts as a copy of a chat.
    pub side: Option<SideOptions>,
    /// Tools the app itself offers this chat (the side panel's browser and
    /// terminal), one group per server.
    pub tools: Vec<AppTools>,
    /// The chat's notes file, which the user writes in the side panel. The
    /// agent is told where it is and may read it, and a main chat may write
    /// it too; it may not exist yet.
    pub notes: Option<PathBuf>,
}

/// Where the chat's notes are, for the agent's instructions. Read only when the
/// user mentions them, so nothing changes from turn to turn. `writable`: a main
/// chat may change them when asked; a side chat can't change files.
pub fn notes_instruction(notes: &Path, writable: bool) -> String {
    let rule = if writable {
        "The user sees it in the app's side panel and may edit it at any time. Read it when they \
mention their notes. Write to it when they ask you to put something in their notes: read it first, \
then change only what they asked for and keep the rest of their text."
    } else {
        "Read it when they mention their notes. Don't write to it."
    };
    format!(
        "The user keeps notes for this chat in {} (markdown; it may not exist yet). {rule}",
        notes.display()
    )
}

/// A running tool call's result: MCP content items, or an error message.
pub type ToolFuture = Pin<Box<dyn Future<Output = Result<Vec<Value>, String>> + Send>>;

/// Tools the app offers the agent, served by the app itself. Agent-agnostic:
/// the adapter decides how the agent reaches them.
#[derive(Clone)]
pub struct AppTools {
    /// The name the agent knows the group by.
    pub server: &'static str,
    /// A line added to the agent's instructions about when to use them.
    pub instruction: &'static str,
    /// MCP tool definitions.
    pub list: Value,
    /// Runs a call: tool name and arguments.
    pub call: Arc<dyn Fn(String, Value) -> ToolFuture + Send + Sync>,
}

/// Answers one MCP JSON-RPC message for the app's own tools. None for a
/// notification, which gets no reply. Shared by every adapter: Claude Code
/// passes these over its control channel, ACP agents over HTTP.
pub async fn answer_mcp(tools: &AppTools, message: &Value) -> Option<Value> {
    let method = message["method"].as_str().unwrap_or_default();
    if message.get("id").is_none() || method.starts_with("notifications/") {
        return None;
    }
    let result = match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": message["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": tools.server, "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(serde_json::json!({})),
        "tools/list" => Ok(serde_json::json!({ "tools": tools.list })),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or_default().to_string();
            let args = message["params"]["arguments"].clone();
            Ok(match (tools.call)(name, args).await {
                Ok(content) => serde_json::json!({ "content": content }),
                Err(error) => serde_json::json!({ "content": [{ "type": "text", "text": error }], "isError": true }),
            })
        }
        method => Err(serde_json::json!({ "code": -32601, "message": format!("Method not found: {method}") })),
    };
    let mut response = serde_json::json!({ "jsonrpc": "2.0", "id": message["id"] });
    match result {
        Ok(result) => response["result"] = result,
        Err(error) => response["error"] = error,
    }
    Some(response)
}

/// How a side chat starts.
#[derive(Clone, Debug, Default)]
pub struct SideOptions {
    /// The main chat's agent session ID, copied on the side chat's first start.
    /// None when the main chat hasn't started one yet.
    pub fork_from: Option<String>,
}

/// An image to send with a message.
pub struct ImagePayload {
    /// "image/png", "image/jpeg", "image/gif" or "image/webp".
    pub media_type: String,
    /// The image's bytes, base64-encoded.
    pub data_b64: String,
}

/// A live agent process. Every method only queues a message, so none of them block.
pub trait RunningAgent: Send + Sync {
    /// `id` names the message; `MessageSeen` carries it back when the agent takes the message in.
    fn send_user_message(&self, text: &str, images: &[ImagePayload], id: &str) -> Result<(), String>;
    /// `message` goes with a denial. `answers` goes with a question: each
    /// question's text mapped to the chosen answer. `plan` goes with an
    /// approved plan the user edited: the agent works from that text instead.
    fn answer_approval(
        &self,
        request_id: &str,
        decision: Decision,
        message: Option<String>,
        answers: Option<BTreeMap<String, String>>,
        plan: Option<String>,
    ) -> Result<(), String>;
    /// The requests still waiting for the user's answer, by request ID.
    fn open_requests(&self) -> Vec<String>;
    /// Stop the current turn. The chat stays usable.
    fn interrupt(&self) -> Result<(), String>;
    fn set_mode(&self, mode: &str) -> Result<(), String>;
    /// Switch model for the turns that follow. "default" goes back to the agent's default.
    fn set_model(&self, model: &str) -> Result<(), String>;
    /// Switch effort level for the turns that follow.
    fn set_effort(&self, effort: &str) -> Result<(), String>;
    /// Ask the process to finish up and exit on its own.
    fn close(&self);
    /// End the process now. On Mac and Linux it is asked to terminate first, so
    /// it can end its own background commands; it is killed if still running
    /// 5 seconds later.
    fn stop(&self);
}

pub trait AgentAdapter: Send + Sync {
    /// Start the agent. Must be called from inside the async runtime.
    fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String>;

    /// The models the agent offers right now, in the order it lists them.
    /// `efforts_for` names the models whose effort levels to read when that
    /// takes a call per model (Cursor); None = all. Blocks for up to a few
    /// seconds; call it off the main thread.
    fn list_models(&self, binary: &Path, efforts_for: Option<&[String]>) -> Result<Vec<ModelInfo>, String>;

    /// A short title for a chat that starts with `first_message`, written by a
    /// small model. Blocks for a few seconds; call it off the main thread.
    fn make_title(&self, binary: &Path, first_message: &str) -> Result<String, String>;

    /// Sign the agent in, the way its own login command does (for Claude Code
    /// it opens the browser). Blocks until done; call it off the main thread.
    fn sign_in(&self, binary: &Path) -> Result<(), String>;

    /// Delete what the agent saved for a session, so it can't be resumed or
    /// listed. Used when a side chat ends. Failures are ignored.
    fn forget_session(&self, agent_session_id: &str);
}
