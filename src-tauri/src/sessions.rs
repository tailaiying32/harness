//! Keeps track of chats and their live agent processes.
//! Spike scope: start on first message, resume after exit. Idle timers and saved
//! history come in later steps (see PLAN.md, "Session lifecycle").

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::agent::{AgentAdapter, EventSink, RunningAgent, SpawnOptions};
use crate::events::{AgentEvent, Decision, SessionState};

#[derive(Default)]
struct Session {
    /// The live process, if any.
    agent: Option<Box<dyn RunningAgent>>,
    /// The agent's own ID for this chat. Kept after the process exits, for resume.
    agent_session_id: Option<String>,
}

type Sessions = Arc<Mutex<HashMap<String, Session>>>;

pub struct SessionManager {
    adapter: Arc<dyn AgentAdapter>,
    sessions: Sessions,
}

impl SessionManager {
    pub fn new(adapter: Arc<dyn AgentAdapter>) -> Self {
        Self {
            adapter,
            sessions: Arc::default(),
        }
    }

    /// The agent adapter, for calls that don't belong to one chat.
    pub fn adapter(&self) -> Arc<dyn AgentAdapter> {
        self.adapter.clone()
    }

    /// Send a message, starting or resuming the agent if it isn't running.
    /// `mode` and `model` only matter when a process has to be started; a running
    /// one is switched with `set_mode` and `set_model`. `emit` receives every
    /// event for this chat.
    pub fn send_message(
        &self,
        session_id: &str,
        binary: PathBuf,
        project_path: PathBuf,
        mode: Option<String>,
        model: Option<String>,
        text: &str,
        emit: EventSink,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.entry(session_id.to_string()).or_default();

        if session.agent.is_none() {
            // Wrap the sink so the manager learns the resume ID and notices exits.
            let all = self.sessions.clone();
            let id = session_id.to_string();
            let sink: EventSink = Arc::new(move |event: AgentEvent| {
                match &event {
                    AgentEvent::SessionStarted {
                        agent_session_id, ..
                    } => {
                        if let Some(s) = all.lock().unwrap().get_mut(&id) {
                            s.agent_session_id = Some(agent_session_id.clone());
                        }
                    }
                    AgentEvent::Status {
                        state: SessionState::Exited,
                    } => {
                        if let Some(s) = all.lock().unwrap().get_mut(&id) {
                            s.agent = None;
                        }
                    }
                    _ => {}
                }
                emit(event);
            });
            session.agent = Some(self.adapter.spawn(
                SpawnOptions {
                    binary,
                    project_path,
                    resume_id: session.agent_session_id.clone(),
                    mode,
                    model,
                },
                sink,
            )?);
        }

        session
            .agent
            .as_ref()
            .expect("agent was just started")
            .send_user_message(text)
    }

    fn with_agent<T>(
        &self,
        session_id: &str,
        f: impl FnOnce(&dyn RunningAgent) -> Result<T, String>,
    ) -> Result<T, String> {
        let sessions = self.sessions.lock().unwrap();
        let agent = sessions
            .get(session_id)
            .and_then(|s| s.agent.as_deref())
            .ok_or("This chat has no running process")?;
        f(agent)
    }

    pub fn answer_approval(
        &self,
        session_id: &str,
        request_id: &str,
        decision: Decision,
        message: Option<String>,
    ) -> Result<(), String> {
        self.with_agent(session_id, |a| a.answer_approval(request_id, decision, message))
    }

    pub fn interrupt(&self, session_id: &str) -> Result<(), String> {
        self.with_agent(session_id, |a| a.interrupt())
    }

    pub fn set_mode(&self, session_id: &str, mode: &str) -> Result<(), String> {
        self.with_agent(session_id, |a| a.set_mode(mode))
    }

    pub fn set_model(&self, session_id: &str, model: &str) -> Result<(), String> {
        self.with_agent(session_id, |a| a.set_model(model))
    }

    /// Ask the chat's process to finish up and exit. The chat can be resumed later.
    pub fn close(&self, session_id: &str) {
        let _ = self.with_agent(session_id, |a| {
            a.close();
            Ok(())
        });
    }
}
