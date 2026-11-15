//! The boundary between the app and any coding agent.
//! `claude_code` is the only adapter in v1; a second agent means a second file here.

pub mod claude_code;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::events::{AgentEvent, Decision, ModelInfo};

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
    /// question's text mapped to the chosen answer.
    fn answer_approval(
        &self,
        request_id: &str,
        decision: Decision,
        message: Option<String>,
        answers: Option<BTreeMap<String, String>>,
    ) -> Result<(), String>;
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
    /// Blocks for up to a few seconds; call it off the main thread.
    fn list_models(&self, binary: &Path) -> Result<Vec<ModelInfo>, String>;

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
