//! The boundary between the app and any coding agent.
//! `claude_code` is the only adapter in v1; a second agent means a second file here.

pub mod claude_code;

use std::path::PathBuf;
use std::sync::Arc;

use crate::events::{AgentEvent, Decision};

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
}

/// A live agent process. Every method only queues a message, so none of them block.
pub trait RunningAgent: Send + Sync {
    fn send_user_message(&self, text: &str) -> Result<(), String>;
    fn answer_approval(
        &self,
        request_id: &str,
        decision: Decision,
        message: Option<String>,
    ) -> Result<(), String>;
    /// Stop the current turn. The chat stays usable.
    fn interrupt(&self) -> Result<(), String>;
    fn set_mode(&self, mode: &str) -> Result<(), String>;
    /// Ask the process to finish up and exit on its own.
    fn close(&self);
}

pub trait AgentAdapter: Send + Sync {
    /// Start the agent. Must be called from inside the async runtime.
    fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String>;
}
