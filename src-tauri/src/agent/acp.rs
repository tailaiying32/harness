//! Agents that speak ACP, the Agent Client Protocol: JSON-RPC 2.0, one message
//! per line, on the agent's stdin and stdout. This file knows the protocol and
//! turns it into the app's events; `cursor.rs` says how to start Cursor's agent.
//! What Cursor actually sends was tested live; see PLAN.md, "Cursor over ACP".
//!
//! One task drives the chat in order: `initialize`, then `session/new` or
//! `session/load`, then one `session/prompt` at a time. A reader task routes
//! the agent's lines: replies to our calls, its requests (approvals, plans,
//! questions), and `session/update` notifications (text, tool calls).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use super::{AppTools, EventSink, ImagePayload, RunningAgent, SpawnOptions, mcp_http};
use crate::events::{AgentEvent, ApprovalKind, Block, Decision, SessionState};

/// How many stderr lines to keep for an error message when the process dies.
const STDERR_TAIL_LINES: usize = 30;
/// How long a stopped process gets before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// ACP's error code for "sign in first".
const AUTH_REQUIRED: i64 = -32000;

/// How to start one ACP agent.
pub struct AcpConfig {
    /// What the user calls it, for messages ("Cursor stopped unexpectedly").
    pub label: &'static str,
    /// Arguments that start it in ACP mode.
    pub args: Vec<String>,
    /// `clientCapabilities._meta`: the agent's own extension switches.
    pub client_meta: Value,
}

/// A failed call: JSON-RPC's code and message, or the process ending.
#[derive(Debug, Clone)]
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn gone() -> Self {
        Self {
            code: 0,
            message: "the agent is no longer running".into(),
        }
    }

    fn from(value: &Value) -> Self {
        let detail = value["data"]["message"].as_str().or(value["message"].as_str());
        Self {
            code: value["code"].as_i64().unwrap_or(0),
            message: detail.unwrap_or("unknown error").to_string(),
        }
    }
}

enum Out {
    Line(String),
    Close,
}

/// The JSON-RPC side: send calls and wait for their replies.
struct Rpc {
    tx: mpsc::UnboundedSender<Out>,
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>>,
}

impl Rpc {
    fn send(&self, value: Value) -> Result<(), RpcError> {
        self.tx.send(Out::Line(value.to_string())).map_err(|_| RpcError::gone())
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.waiting.lock().unwrap().insert(id, tx);
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        rx.await.unwrap_or_else(|_| Err(RpcError::gone()))
    }

    fn notify(&self, method: &str, params: Value) {
        let _ = self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn respond(&self, id: &Value, result: Value) {
        let _ = self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn respond_error(&self, id: &Value, code: i64, message: &str) {
        let _ = self.send(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }));
    }

    /// A reply arrived: hand it to its caller.
    fn settle(&self, v: &Value) {
        let Some(id) = v["id"].as_u64() else { return };
        if let Some(tx) = self.waiting.lock().unwrap().remove(&id) {
            let result = match v.get("error") {
                Some(error) => Err(RpcError::from(error)),
                None => Ok(v["result"].clone()),
            };
            let _ = tx.send(result);
        }
    }

    /// The process is gone: every waiting call fails.
    fn fail_all(&self) {
        for (_, tx) in self.waiting.lock().unwrap().drain() {
            let _ = tx.send(Err(RpcError::gone()));
        }
    }
}

/// What the chat's driver sends next.
enum Item {
    /// A message from the user.
    User {
        text: String,
        images: Vec<(String, String)>,
        id: String,
    },
    /// Words from the app the agent should read with the next message: the
    /// user's feedback on a plan, an edited plan, a reason for a denial.
    Note(String),
    /// Finish up and exit.
    Close,
}

/// A request from the agent waiting for the user.
enum Waiting {
    Permission {
        rpc_id: Value,
        /// `(optionId, kind)` as offered.
        options: Vec<(String, String)>,
    },
    Plan {
        rpc_id: Value,
    },
    Question {
        rpc_id: Value,
        /// Each question: its id, its text, and its options as `(id, label)`.
        questions: Vec<(String, String, Vec<(String, String)>)>,
    },
}

impl Waiting {
    fn rpc_id(&self) -> &Value {
        match self {
            Waiting::Permission { rpc_id, .. } | Waiting::Plan { rpc_id } | Waiting::Question { rpc_id, .. } => rpc_id,
        }
    }

    /// What answers it as "never mind". (Every answer is wrapped the same way:
    /// `{outcome: {outcome: ...}}`, Cursor's own methods too, which its docs
    /// show unwrapped; Cursor CLI 2026.11.13 reads `outcome.outcome`.)
    fn cancelled(&self) -> Value {
        json!({ "outcome": { "outcome": "cancelled" } })
    }
}

/// One tool call as it fills in: it arrives with an empty input and a generic
/// title ("Terminal"), and an update brings the real ones.
#[derive(Default)]
struct Tool {
    title: String,
    kind: String,
    raw_input: Value,
    /// Its block has been passed on.
    shown: bool,
    /// The name and input last passed on, to tell when fuller ones arrive.
    sent: Option<(String, Value)>,
    /// Its result has been passed on.
    done: bool,
}

/// A subagent's own session: its steps, shown under the chat's tool call
/// that started it.
struct Child {
    parent_tool: String,
    /// Its text so far; thinking is not kept.
    text: String,
    tools: HashMap<String, Tool>,
}

/// The text or thinking being written right now.
struct Run {
    thinking: bool,
    message_id: String,
    text: String,
}

/// The chat's model and effort settings, as the agent last reported them.
#[derive(Default, Clone)]
struct Config {
    model: Option<String>,
    /// The id of the current model's effort option (`effort`,
    /// `reasoning_effort`, `reasoning`...), and the levels it takes.
    effort: Option<(String, Vec<String>)>,
    modes: Vec<String>,
}

impl Config {
    /// Reads a `configOptions` list (from `session/new`, `session/load`, a
    /// `session/set_config_option` reply or a `config_option_update`).
    fn read(&mut self, options: &Value) {
        let Some(options) = options.as_array() else { return };
        self.effort = None;
        for option in options {
            let values = || -> Vec<String> {
                option["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| o["value"].as_str().map(str::to_owned))
                    .collect()
            };
            match option["category"].as_str().unwrap_or_default() {
                "model" => self.model = option["currentValue"].as_str().map(str::to_owned),
                "mode" => self.modes = values(),
                "thought_level" => {
                    if let Some(id) = option["id"].as_str() {
                        self.effort = Some((id.to_string(), values()));
                    }
                }
                _ => {}
            }
        }
    }
}

#[derive(Default)]
struct State {
    session_id: Option<String>,
    /// While `session/load` replays history, which the chat has saved already.
    loading: bool,
    run: Option<Run>,
    tools: HashMap<String, Tool>,
    /// Running subagents, by their session ID.
    children: HashMap<String, Child>,
    /// The agent's requests waiting for the user, by the app's request ID.
    waiting: HashMap<String, Waiting>,
    next_request: u64,
    config: Config,
    mode: Option<String>,
    /// The effort level picked for this chat, re-applied after a model change.
    effort: Option<String>,
    /// Notes waiting for the next prompt.
    notes: Vec<String>,
    /// A plan was approved: leave plan mode before the next prompt.
    build_after_turn: bool,
    /// Messages sent but not yet put in a prompt. One the user takes back
    /// leaves this set, and the driver skips it.
    queued: HashSet<String>,
    /// What the app's own tools answered, until each call's result is saved.
    served: ServedLog,
}

/// What one of the app's own tools answered. Cursor only says such a call
/// succeeded, so the app keeps its own answer to save as the call's result.
struct Served {
    /// As the chat names the tool: `mcp__<server>__<tool>`.
    name: String,
    args: Value,
    text: String,
    is_error: bool,
}

type ServedLog = Arc<Mutex<VecDeque<Served>>>;

/// The most answers kept waiting for their call to finish.
const SERVED_KEPT: usize = 32;

/// The app's tools, with every answer also written to `log`.
fn recording(tools: &[AppTools], log: &ServedLog) -> Vec<AppTools> {
    tools
        .iter()
        .map(|group| {
            let (call, log, server) = (group.call.clone(), log.clone(), group.server);
            AppTools {
                call: Arc::new(move |tool, args| {
                    let (call, log) = (call.clone(), log.clone());
                    Box::pin(async move {
                        let name = format!("mcp__{server}__{tool}");
                        let result = call(tool, args.clone()).await;
                        let (text, is_error) = match &result {
                            Ok(content) => {
                                let texts: Vec<&str> = content.iter().filter_map(|part| part["text"].as_str()).collect();
                                (texts.join("\n"), false)
                            }
                            Err(message) => (message.clone(), true),
                        };
                        let mut log = log.lock().unwrap();
                        if log.len() >= SERVED_KEPT {
                            log.pop_front();
                        }
                        log.push_back(Served { name, args, text, is_error });
                        result
                    })
                }),
                ..group.clone()
            }
        })
        .collect()
}

/// The app's own answer to a finished call: the oldest one kept for that tool
/// with the same input, or failing that the oldest for that tool.
fn take_served(log: &ServedLog, name: &str, args: &Value) -> Option<Served> {
    let mut log = log.lock().unwrap();
    let at = log
        .iter()
        .position(|s| s.name == name && &s.args == args)
        .or_else(|| log.iter().position(|s| s.name == name))?;
    log.remove(at)
}

struct Shared {
    rpc: Rpc,
    state: Mutex<State>,
    sink: EventSink,
    items: mpsc::UnboundedSender<Item>,
    /// Taken by the first `stop()`.
    stop_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// The app's tool server names, whose calls are allowed without a card.
    own_tools: Vec<String>,
    label: &'static str,
}

impl Shared {
    fn emit(&self, event: AgentEvent) {
        (self.sink)(event);
    }

    fn stop(&self) {
        if let Some(stop) = self.stop_tx.lock().unwrap().take() {
            let _ = stop.send(());
        }
    }
}

struct AcpProcess {
    shared: Arc<Shared>,
}

fn terminate(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: plain signal send to our own child's pid.
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    #[cfg(windows)]
    let _ = child.start_kill();
}

/// Starts an ACP agent for one chat.
pub fn spawn(binary: PathBuf, cfg: AcpConfig, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
    let label = cfg.label;
    let mut cmd = Command::new(&binary);
    cmd.args(&cfg.args)
        .current_dir(&opts.project_path)
        .envs(opts.env.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash

    let mut child = cmd.spawn().map_err(|e| format!("Could not start {label}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or_else(|| format!("{label} has no stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| format!("{label} has no stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| format!("{label} has no stderr"))?;

    // The app's tools, served for as long as this process lives.
    let served = ServedLog::default();
    let endpoint = if opts.tools.is_empty() {
        None
    } else {
        match mcp_http::open(recording(&opts.tools, &served)) {
            Ok(endpoint) => Some(endpoint),
            Err(e) => {
                eprintln!("harness: the app's tools aren't available to {label}: {e}");
                None
            }
        }
    };

    // How the agent reaches them: one MCP server per tool group.
    let mcp_servers: Value = endpoint
        .iter()
        .flat_map(|e| e.urls.iter())
        .map(|(name, url)| json!({ "type": "http", "name": name, "url": url, "headers": [] }))
        .collect();

    let (tx, mut rx) = mpsc::unbounded_channel::<Out>();
    let (items_tx, items_rx) = mpsc::unbounded_channel::<Item>();
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let shared = Arc::new(Shared {
        rpc: Rpc {
            tx,
            next: AtomicU64::new(1),
            waiting: Mutex::default(),
        },
        state: Mutex::new(State {
            effort: opts.effort.clone().filter(|e| e != "default"),
            served,
            ..State::default()
        }),
        sink,
        items: items_tx,
        stop_tx: Mutex::new(Some(stop_tx)),
        own_tools: opts.tools.iter().map(|t| t.server.to_string()).collect(),
        label,
    });
    shared.emit(AgentEvent::Status {
        state: SessionState::Starting,
    });

    // Writer: owns stdin. When it ends, stdin closes and the agent exits.
    tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            let Out::Line(line) = out else { break };
            let ok = stdin.write_all(line.as_bytes()).await.is_ok()
                && stdin.write_all(b"\n").await.is_ok()
                && stdin.flush().await.is_ok();
            if !ok {
                break;
            }
        }
    });

    let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::default();
    {
        let tail = stderr_tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut tail = tail.lock().unwrap();
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });
    }

    // Reader: the agent's lines, until it exits.
    {
        let shared = shared.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut kill_at: Option<tokio::time::Instant> = None;
            let mut killed = false;
            loop {
                let line = tokio::select! {
                    line = lines.next_line() => match line {
                        Ok(Some(line)) => line,
                        _ => break,
                    },
                    _ = &mut stop_rx, if kill_at.is_none() => {
                        kill_at = Some(tokio::time::Instant::now() + STOP_GRACE);
                        terminate(&mut child);
                        continue;
                    },
                    _ = tokio::time::sleep_until(kill_at.unwrap_or_else(tokio::time::Instant::now)),
                        if kill_at.is_some() && !killed =>
                    {
                        killed = true;
                        let _ = child.start_kill();
                        continue;
                    },
                };
                let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
                    continue;
                };
                receive(&shared, &value);
            }
            let clean_exit = matches!(child.wait().await, Ok(status) if status.success());
            shared.rpc.fail_all();
            drop(endpoint);
            let cancelled: Vec<String> = shared.state.lock().unwrap().waiting.drain().map(|(id, _)| id).collect();
            for request_id in cancelled {
                shared.emit(AgentEvent::ApprovalCancelled { request_id });
            }
            if !clean_exit && kill_at.is_none() {
                let tail: Vec<String> = stderr_tail.lock().unwrap().iter().cloned().collect();
                let detail = if tail.is_empty() { "no error output".to_string() } else { tail.join("\n") };
                shared.emit(AgentEvent::Error {
                    message: format!("{} stopped unexpectedly:\n{detail}", shared.label),
                });
            }
            shared.emit(AgentEvent::Status {
                state: SessionState::Exited,
            });
        });
    }

    // Driver: the chat's calls, in order.
    tokio::spawn(drive(shared.clone(), cfg.client_meta, opts, mcp_servers, items_rx));

    Ok(Box::new(AcpProcess { shared }))
}

/// Sets up the session, then sends one prompt at a time.
async fn drive(
    shared: Arc<Shared>,
    client_meta: Value,
    opts: SpawnOptions,
    mcp_servers: Value,
    mut items: mpsc::UnboundedReceiver<Item>,
) {
    let label = shared.label;
    let fail = |message: String| {
        shared.emit(AgentEvent::Error { message });
        shared.stop();
    };
    let init = shared
        .rpc
        .call(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false,
                    "_meta": client_meta,
                },
                "clientInfo": { "name": "harness", "version": env!("CARGO_PKG_VERSION") },
            }),
        )
        .await;
    let init = match init {
        Ok(init) => init,
        Err(e) => return fail(format!("{label} didn't start: {}", e.message)),
    };
    let cwd = opts.project_path.to_string_lossy().into_owned();

    // Pick up the earlier chat if there is one; else a new one.
    let mut fresh = true;
    let mut session = None;
    if let Some(resume) = opts.resume_id.clone().filter(|_| init["agentCapabilities"]["loadSession"] == true) {
        shared.state.lock().unwrap().loading = true;
        let loaded = shared
            .rpc
            .call("session/load", json!({ "sessionId": resume, "cwd": cwd, "mcpServers": mcp_servers }))
            .await;
        {
            let mut state = shared.state.lock().unwrap();
            state.loading = false;
            state.run = None;
            state.tools.clear();
        }
        match loaded {
            Ok(result) => {
                session = Some((resume, result));
                fresh = false;
            }
            Err(e) if e.code == AUTH_REQUIRED => return sign_in_needed(&shared, e),
            Err(e) => shared.emit(AgentEvent::Error {
                message: format!(
                    "Couldn't pick up this chat's earlier {label} conversation ({}). It continues as a new one.",
                    e.message
                ),
            }),
        }
    }
    let (session_id, result) = match session {
        Some(session) => session,
        None => match shared
            .rpc
            .call("session/new", json!({ "cwd": cwd, "mcpServers": mcp_servers }))
            .await
        {
            Ok(result) => match result["sessionId"].as_str() {
                Some(id) => (id.to_string(), result),
                None => return fail(format!("{label} started no conversation")),
            },
            Err(e) if e.code == AUTH_REQUIRED => return sign_in_needed(&shared, e),
            Err(e) => return fail(format!("{label} couldn't start a conversation: {}", e.message)),
        },
    };
    {
        let mut state = shared.state.lock().unwrap();
        state.session_id = Some(session_id.clone());
        state.config.read(&result["configOptions"]);
        state.mode = result["modes"]["currentModeId"].as_str().map(str::to_owned);
        if state.config.modes.is_empty() {
            state.config.modes = result["modes"]["availableModes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str().map(str::to_owned))
                .collect();
        }
    }

    // The chat's own settings. A new session starts on the last model used
    // anywhere in the agent, so the model is always set.
    let model = opts.model.clone().unwrap_or_else(|| "default".to_string());
    if shared.state.lock().unwrap().config.model.as_deref() != Some(model.as_str()) {
        if let Err(e) = set_model(&shared, &session_id, &model).await {
            shared.emit(AgentEvent::Error {
                message: format!("{label} couldn't switch to {model}: {}", e.message),
            });
        }
    }
    apply_effort(&shared, &session_id).await;
    let mode = opts.mode.clone().filter(|m| {
        let state = shared.state.lock().unwrap();
        state.config.modes.contains(m) && state.mode.as_ref() != Some(m)
    });
    if let Some(mode) = mode {
        if shared
            .rpc
            .call("session/set_mode", json!({ "sessionId": session_id, "modeId": mode }))
            .await
            .is_ok()
        {
            shared.state.lock().unwrap().mode = Some(mode);
        }
    }
    let (model, mode) = {
        let state = shared.state.lock().unwrap();
        (state.config.model.clone(), state.mode.clone())
    };
    // No "idle" here: the message that started the process is already queued.
    shared.emit(AgentEvent::SessionStarted {
        agent_session_id: session_id.clone(),
        model,
        mode,
    });

    // What a new conversation is told once, with its first message.
    let mut intro = if fresh { instructions(&opts) } else { None };

    while let Some(first) = items.recv().await {
        let mut batch = vec![first];
        while let Ok(item) = items.try_recv() {
            batch.push(item);
        }
        let closing = batch.iter().any(|i| matches!(i, Item::Close));
        let mut body: Vec<Value> = Vec::new();
        let mut seen = Vec::new();
        for item in batch {
            match item {
                // An empty note only wakes the loop for notes saved during a turn.
                Item::Note(text) if text.is_empty() => {}
                Item::Note(text) => body.push(json!({ "type": "text", "text": text })),
                Item::User { text, images, id } => {
                    // Taken back by the user while it waited.
                    if !shared.state.lock().unwrap().queued.remove(&id) {
                        continue;
                    }
                    for (media_type, data) in images {
                        body.push(json!({ "type": "image", "mimeType": media_type, "data": data }));
                    }
                    body.push(json!({ "type": "text", "text": text }));
                    seen.push(id);
                }
                Item::Close => {}
            }
        }
        let notes = std::mem::take(&mut shared.state.lock().unwrap().notes);
        // Nothing to say (every message was taken back): no turn, and the
        // intro and a pending plan switch wait for the next real prompt.
        let mut prompt: Vec<Value> = Vec::new();
        if !body.is_empty() || !notes.is_empty() {
            let intro = intro.take();
            let build = std::mem::take(&mut shared.state.lock().unwrap().build_after_turn);
            let leave_plan = build && shared.state.lock().unwrap().config.modes.iter().any(|m| m == "agent");
            if leave_plan
                && shared
                    .rpc
                    .call("session/set_mode", json!({ "sessionId": session_id, "modeId": "agent" }))
                    .await
                    .is_ok()
            {
                shared.state.lock().unwrap().mode = Some("agent".to_string());
                shared.emit(AgentEvent::ModeChanged { mode: "agent".to_string() });
            }
            prompt.extend(notes.into_iter().map(|note| json!({ "type": "text", "text": note })));
            prompt.extend(body);
            // The intro goes last: Cursor titles a chat from the start of its
            // first prompt, and with the intro first the title was about the
            // app's notes ("Harness Agent"), not what the user asked.
            if let Some(text) = intro {
                prompt.push(json!({ "type": "text", "text": text }));
            }
        }
        if !prompt.is_empty() {
            for id in seen {
                shared.emit(AgentEvent::MessageSeen { id });
            }
            shared.emit(AgentEvent::Status {
                state: SessionState::Working,
            });
            let started = Instant::now();
            let reply = shared
                .rpc
                .call("session/prompt", json!({ "sessionId": session_id, "prompt": prompt }))
                .await;
            end_run(&shared);
            // Nothing can still be waiting once the turn is over.
            let left: Vec<(String, Waiting)> = shared.state.lock().unwrap().waiting.drain().collect();
            for (request_id, waiting) in left {
                shared.rpc.respond(waiting.rpc_id(), waiting.cancelled());
                shared.emit(AgentEvent::ApprovalCancelled { request_id });
            }
            let stopped = reply.as_ref().is_ok_and(|r| r["stopReason"] == "cancelled");
            let ok = match reply {
                Ok(reply) => {
                    if reply["stopReason"] == "refusal" {
                        shared.emit(AgentEvent::Error {
                            message: format!("{label} refused to answer this."),
                        });
                    }
                    reply["stopReason"] != "refusal"
                }
                Err(e) if e.code == AUTH_REQUIRED => {
                    sign_in_needed(&shared, e);
                    false
                }
                Err(e) => {
                    shared.emit(AgentEvent::Error {
                        message: format!("{label}: {}", e.message),
                    });
                    false
                }
            };
            // Notes from the turn (an approved plan, say) carry it on at once:
            // to the user it is one turn, as with Claude Code. Not after a
            // stop, a failure, or when closing.
            let carry_on = ok && !stopped && !closing && !shared.state.lock().unwrap().notes.is_empty();
            if carry_on {
                let _ = shared.items.send(Item::Note(String::new()));
            } else {
                shared.state.lock().unwrap().notes.clear();
                shared.emit(AgentEvent::TurnEnded {
                    ok,
                    duration_ms: Some(started.elapsed().as_millis() as u64),
                });
                shared.emit(AgentEvent::Status {
                    state: SessionState::Idle,
                });
            }
        }
        if closing {
            break;
        }
    }
    // Closing stdin makes the agent exit.
    let _ = shared.rpc.tx.send(Out::Close);
}

/// The agent says it isn't signed in. The process is ended, so the next
/// message, after signing in, starts a fresh one.
fn sign_in_needed(shared: &Shared, e: RpcError) {
    shared.emit(AgentEvent::SignInNeeded { message: e.message });
    shared.stop();
}

/// What a new conversation is told with its first message: the app's tools
/// and the chat's notes. ACP has no system prompt.
fn instructions(opts: &SpawnOptions) -> Option<String> {
    let mut lines: Vec<String> = opts.tools.iter().map(|t| t.instruction.to_string()).collect();
    if let Some(notes) = &opts.notes {
        lines.push(super::notes_instruction(notes, opts.project_docs.as_deref(), true));
    }
    // Cursor keeps no memory of its own, so it is told about the project's.
    if let Some(memory) = &opts.memory {
        lines.push(super::memory_instruction(memory));
    }
    if let Some(preferences) = &opts.preferences {
        lines.push(super::preferences_instruction(preferences));
    }
    let lines: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();
    (!lines.is_empty()).then(|| format!("(Notes from the app you run in, not from the user.)\n{}", lines.join("\n")))
}

async fn set_model(shared: &Shared, session_id: &str, model: &str) -> Result<(), RpcError> {
    let reply = shared
        .rpc
        .call(
            "session/set_config_option",
            json!({ "sessionId": session_id, "configId": "model", "value": model }),
        )
        .await?;
    shared.state.lock().unwrap().config.read(&reply["configOptions"]);
    Ok(())
}

/// Sets the chat's effort level, if one was picked and the model takes it.
async fn apply_effort(shared: &Shared, session_id: &str) {
    let target = {
        let state = shared.state.lock().unwrap();
        match (&state.effort, &state.config.effort) {
            (Some(effort), Some((id, levels))) if levels.contains(effort) => Some((id.clone(), effort.clone())),
            _ => None,
        }
    };
    let Some((id, effort)) = target else { return };
    if let Ok(reply) = shared
        .rpc
        .call(
            "session/set_config_option",
            json!({ "sessionId": session_id, "configId": id, "value": effort }),
        )
        .await
    {
        shared.state.lock().unwrap().config.read(&reply["configOptions"]);
    }
}

/// One line from the agent.
fn receive(shared: &Shared, v: &Value) {
    let method = v["method"].as_str();
    match (method, v.get("id")) {
        (None, Some(_)) => shared.rpc.settle(v),
        (Some(method), Some(id)) => request(shared, method, id, &v["params"]),
        (Some("session/update"), None) => {
            let events = {
                let mut state = shared.state.lock().unwrap();
                let session = v["params"]["sessionId"].as_str().unwrap_or_default();
                match state.children.get_mut(session) {
                    Some(child) => child_update(child, &v["params"]["update"]),
                    None => update(&mut state, &v["params"]["update"]),
                }
            };
            for event in events {
                shared.emit(event);
            }
        }
        _ => {}
    }
}

/// A request from the agent. Every one is answered, now or once the user
/// has: an unanswered request holds the agent up.
fn request(shared: &Shared, method: &str, rpc_id: &Value, params: &Value) {
    match method {
        "session/request_permission" => {
            let call = &params["toolCall"];
            let tool_id = call["toolCallId"].as_str().unwrap_or_default().to_string();
            let options: Vec<(String, String)> = params["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|o| Some((o["optionId"].as_str()?.to_string(), o["kind"].as_str()?.to_string())))
                .collect();
            let mut events = Vec::new();
            let (name, input, own) = {
                let mut state = shared.state.lock().unwrap();
                let tool = state.tools.entry(tool_id.clone()).or_default();
                if tool.kind.is_empty() {
                    tool.kind = call["kind"].as_str().unwrap_or("other").to_string();
                }
                let (name, input) = name_and_input(tool);
                let own = own_tool(tool, &shared.own_tools, call["title"].as_str().unwrap_or_default());
                if let Some(event) = show_tool(&tool_id, tool, true) {
                    events.push(event);
                }
                (name, input, own)
            };
            for event in events {
                shared.emit(event);
            }
            // The app's own tools never ask, as with Claude Code.
            if own {
                if let Some((option, _)) = options.iter().find(|(_, kind)| kind == "allow_once") {
                    shared.rpc.respond(rpc_id, json!({ "outcome": { "outcome": "selected", "optionId": option } }));
                    return;
                }
            }
            let always_label = options.iter().any(|(_, kind)| kind == "allow_always").then(|| {
                let content = call["content"][0]["content"]["text"].as_str().unwrap_or_default();
                match content.strip_prefix("Not in allowlist: ") {
                    Some(commands) if !commands.trim().is_empty() => format!("Always allow {}", commands.trim()),
                    _ => "Always allow".to_string(),
                }
            });
            let title = call["title"].as_str().unwrap_or(&name).trim_matches('`').to_string();
            let request_id = wait(shared, Waiting::Permission { rpc_id: rpc_id.clone(), options });
            shared.emit(AgentEvent::ApprovalRequest {
                request_id,
                approval: ApprovalKind::Tool,
                tool_name: name,
                title,
                input,
                tool_id: Some(tool_id),
                always_label,
            });
            shared.emit(AgentEvent::Status {
                state: SessionState::WaitingApproval,
            });
        }

        // Cursor's plan approval. A rejection's reason reaches the model; an
        // edited plan follows as a note.
        "cursor/create_plan" => {
            let tool_id = params["toolCallId"].as_str().unwrap_or_default().to_string();
            let plan = params["plan"].as_str().unwrap_or_default().to_string();
            let shown = {
                let mut state = shared.state.lock().unwrap();
                let tool = state.tools.entry(tool_id.clone()).or_default();
                tool.raw_input = json!({ "_toolName": "createPlan", "plan": plan });
                show_tool(&tool_id, tool, true)
            };
            if let Some(event) = shown {
                shared.emit(event);
            }
            let request_id = wait(shared, Waiting::Plan { rpc_id: rpc_id.clone() });
            let name = params["name"].as_str().filter(|n| !n.is_empty()).unwrap_or("Plan");
            shared.emit(AgentEvent::ApprovalRequest {
                request_id,
                approval: ApprovalKind::Plan,
                tool_name: "ExitPlanMode".to_string(),
                title: name.to_string(),
                input: json!({ "plan": plan }),
                tool_id: Some(tool_id),
                always_label: None,
            });
            shared.emit(AgentEvent::Status {
                state: SessionState::WaitingApproval,
            });
        }

        // Cursor's multiple-choice questions, shown like Claude Code's.
        "cursor/ask_question" => {
            let mut questions = Vec::new();
            let mut shown = Vec::new();
            for q in params["questions"].as_array().into_iter().flatten() {
                let options: Vec<(String, String)> = q["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| Some((o["id"].as_str()?.to_string(), o["label"].as_str()?.to_string())))
                    .collect();
                let text = q["prompt"].as_str().unwrap_or_default().to_string();
                shown.push(json!({
                    "question": text,
                    "header": "",
                    "multiSelect": q["allowMultiple"] == true,
                    "options": options.iter().map(|(_, label)| json!({ "label": label, "description": "" })).collect::<Vec<_>>(),
                }));
                questions.push((q["id"].as_str().unwrap_or_default().to_string(), text, options));
            }
            let tool_id = params["toolCallId"].as_str().map(str::to_owned);
            let request_id = wait(shared, Waiting::Question { rpc_id: rpc_id.clone(), questions });
            shared.emit(AgentEvent::ApprovalRequest {
                request_id,
                approval: ApprovalKind::Question,
                tool_name: "AskUserQuestion".to_string(),
                title: params["title"].as_str().unwrap_or("Questions").to_string(),
                input: json!({ "questions": shown }),
                tool_id,
                always_label: None,
            });
            shared.emit(AgentEvent::Status {
                state: SessionState::WaitingApproval,
            });
        }

        // Sent after a subagent's tool call finished, with what it did. The
        // chat has it already from the updates.
        "cursor/task" => shared.rpc.respond(rpc_id, json!({})),

        // The app offers no file system or terminal of its own.
        _ => shared.rpc.respond_error(rpc_id, -32601, &format!("Method not found: {method}")),
    }
}

/// Keeps a request until the user answers; returns the app's ID for it.
fn wait(shared: &Shared, waiting: Waiting) -> String {
    let mut state = shared.state.lock().unwrap();
    state.next_request += 1;
    let id = format!("acp-{}", state.next_request);
    state.waiting.insert(id.clone(), waiting);
    id
}

/// Whether a tool call is one of the app's own tools.
fn own_tool(tool: &Tool, own: &[String], title: &str) -> bool {
    let provider = tool.raw_input["providerIdentifier"].as_str();
    own.iter()
        .any(|server| provider == Some(server.as_str()) || title.starts_with(&format!("{server}-")))
}

/// The app's name for a tool call and its input, in the shape the window
/// words Claude Code's tools in (`tools.ts`): "Bash" with `command`, "Read"
/// with `file_path`, and so on. Cursor's raw inputs were read live.
fn name_and_input(tool: &Tool) -> (String, Value) {
    let raw = &tool.raw_input;
    let path = raw["path"].as_str().unwrap_or_default();
    let named = |name: &str, input: Value| (name.to_string(), input);
    if let Some(provider) = raw["providerIdentifier"].as_str() {
        let tool_name = raw["toolName"].as_str().unwrap_or_default();
        return named(&format!("mcp__{provider}__{tool_name}"), raw["args"].clone());
    }
    match raw["_toolName"].as_str() {
        Some("createPlan") => return named("ExitPlanMode", json!({ "plan": raw["plan"] })),
        Some("todoWrite") | Some("updateTodos") => return named("TodoWrite", raw.clone()),
        // A subagent, worded and followed like Claude Code's.
        Some("task") => {
            return named(
                "Agent",
                json!({ "description": raw["description"], "prompt": raw["prompt"] }),
            );
        }
        _ => {}
    }
    match tool.kind.as_str() {
        "execute" => named("Bash", json!({ "command": raw["command"] })),
        "read" => named("Read", json!({ "file_path": path })),
        "edit" => named("Edit", json!({ "file_path": path })),
        "delete" => named("Delete", json!({ "file_path": path })),
        "move" => named("Move", json!({ "file_path": path })),
        "search" if tool.title.starts_with("Find") => {
            named("Glob", json!({ "pattern": raw["pattern"], "path": raw["path"] }))
        }
        "search" => named("Grep", json!({ "pattern": raw["pattern"], "path": raw["path"] })),
        "fetch" => {
            if raw["url"].is_string() {
                named("WebFetch", json!({ "url": raw["url"] }))
            } else {
                named("WebSearch", json!({ "query": raw["query"].as_str().unwrap_or(&tool.title) }))
            }
        }
        _ => {
            let title = tool.title.trim();
            named(if title.is_empty() { "a tool" } else { title }, raw.clone())
        }
    }
}

/// The tool's block, once there is enough to show: its input has arrived, it
/// started, or it is asking (`now`).
fn show_tool(tool_id: &str, tool: &mut Tool, now: bool) -> Option<AgentEvent> {
    if tool.shown {
        return None;
    }
    let planning = tool.raw_input["_toolName"] == "createPlan";
    let filled = tool
        .raw_input
        .as_object()
        .is_some_and(|o| o.keys().any(|k| k != "_toolName"));
    if !now && (planning || !filled) {
        return None;
    }
    tool.shown = true;
    let (name, input) = name_and_input(tool);
    tool.sent = Some((name.clone(), input.clone()));
    Some(AgentEvent::AssistantBlock {
        message_id: format!("tool-{tool_id}"),
        block: Block::ToolCall {
            tool_id: tool_id.to_string(),
            name,
            input,
        },
        parent_tool_id: None,
    })
}

/// A shown call's name and input again, when a later update brought fuller
/// ones: a call often starts before Cursor has sent what it is for. A plan
/// is left out; its text reaches the chat with the request to approve it.
fn refresh_tool(tool_id: &str, tool: &mut Tool) -> Option<AgentEvent> {
    if !tool.shown || tool.done || tool.raw_input["_toolName"] == "createPlan" {
        return None;
    }
    let now = name_and_input(tool);
    if tool.sent.as_ref() == Some(&now) {
        return None;
    }
    tool.sent = Some(now.clone());
    Some(AgentEvent::ToolInput {
        tool_id: tool_id.to_string(),
        name: now.0,
        input: now.1,
        parent_tool_id: None,
    })
}

/// A tool's output as text, from what Cursor sends: a command's output, a
/// file's text, a search's count, or content blocks.
fn result_text(update: &Value) -> String {
    let raw = &update["rawOutput"];
    if raw.get("exitCode").is_some() || raw.get("stdout").is_some() {
        let mut text = raw["stdout"].as_str().unwrap_or_default().to_string();
        if let Some(err) = raw["stderr"].as_str().filter(|e| !e.is_empty()) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(err);
        }
        return text;
    }
    if let Some(content) = raw["content"].as_str() {
        return content.to_string();
    }
    if let Some(n) = raw["totalMatches"].as_u64() {
        return format!("{n} matches");
    }
    if let Some(n) = raw["totalFiles"].as_u64() {
        return format!("{n} files");
    }
    let mut parts = Vec::new();
    for item in update["content"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("content") => {
                if let Some(text) = item["content"]["text"].as_str() {
                    parts.push(text.to_string());
                }
            }
            Some("diff") => parts.push(format!("Edited {}", item["path"].as_str().unwrap_or("a file"))),
            _ => {}
        }
    }
    parts.join("\n")
}

/// Ends the text or thinking being written, as a finished block.
fn close_run(state: &mut State) -> Option<AgentEvent> {
    let run = state.run.take()?;
    if run.text.trim().is_empty() {
        return None;
    }
    let block = if run.thinking {
        Block::Thinking { text: run.text }
    } else {
        Block::Text { text: run.text }
    };
    Some(AgentEvent::AssistantBlock {
        message_id: run.message_id,
        block,
        parent_tool_id: None,
    })
}

fn end_run(shared: &Shared) {
    let event = close_run(&mut shared.state.lock().unwrap());
    if let Some(event) = event {
        shared.emit(event);
    }
}

/// One `session/update` into the app's events.
fn update(state: &mut State, u: &Value) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    if state.loading {
        return out;
    }
    let kind = u["sessionUpdate"].as_str().unwrap_or_default();
    match kind {
        "agent_message_chunk" | "agent_thought_chunk" => {
            let thinking = kind == "agent_thought_chunk";
            let Some(text) = u["content"]["text"].as_str() else {
                return out;
            };
            if state.run.as_ref().is_some_and(|r| r.thinking != thinking) {
                out.extend(close_run(state));
            }
            let run = state.run.get_or_insert_with(|| Run {
                thinking,
                message_id: uuid::Uuid::new_v4().to_string(),
                text: String::new(),
            });
            run.text.push_str(text);
            if !thinking {
                out.push(AgentEvent::TextDelta {
                    message_id: run.message_id.clone(),
                    block_index: 0,
                    text: text.to_string(),
                });
            }
        }
        "tool_call" | "tool_call_update" => {
            out.extend(close_run(state));
            let Some(tool_id) = u["toolCallId"].as_str() else {
                return out;
            };
            let served = state.served.clone();
            let tool = state.tools.entry(tool_id.to_string()).or_default();
            if let Some(title) = u["title"].as_str() {
                tool.title = title.to_string();
            }
            if let Some(kind) = u["kind"].as_str() {
                tool.kind = kind.to_string();
            }
            if let Some(input) = u.get("rawInput").filter(|i| i.is_object()) {
                tool.raw_input = input.clone();
            }
            let status = u["status"].as_str().unwrap_or_default();
            let started = matches!(status, "in_progress" | "completed" | "failed");
            match show_tool(tool_id, tool, started && tool.raw_input["_toolName"] != "createPlan") {
                Some(shown) => out.push(shown),
                None if started => out.extend(refresh_tool(tool_id, tool)),
                None => {}
            }
            if matches!(status, "completed" | "failed") && !tool.done && tool.shown {
                tool.done = true;
                let mut text = result_text(u);
                let mut is_error = status == "failed";
                // One of the app's own tools: Cursor passes on no output, so
                // the answer the app gave is the result.
                if text.is_empty() {
                    let (name, input) = name_and_input(tool);
                    if let Some(own) = take_served(&served, &name, &input) {
                        text = own.text;
                        is_error |= own.is_error;
                    }
                }
                out.push(AgentEvent::ToolResult {
                    tool_id: tool_id.to_string(),
                    content: json!(text),
                    is_error,
                    parent_tool_id: None,
                });
            }
        }
        "current_mode_update" => {
            if let Some(mode) = u["currentModeId"].as_str() {
                state.mode = Some(mode.to_string());
                out.push(AgentEvent::ModeChanged { mode: mode.to_string() });
            }
        }
        "config_option_update" => state.config.read(&u["configOptions"]),
        // A subagent started: its own session reports its steps (see `child_update`).
        "subagent_spawned" => {
            let (Some(session), Some(tool)) = (
                u["subagentSessionId"].as_str(),
                u["_meta"]["cursor"]["toolCallId"].as_str(),
            ) else {
                return out;
            };
            state.children.insert(
                session.to_string(),
                Child { parent_tool: tool.to_string(), text: String::new(), tools: HashMap::new() },
            );
            out.push(AgentEvent::TaskStarted {
                task_id: session.to_string(),
                tool_id: Some(tool.to_string()),
                description: u["name"].as_str().unwrap_or_default().to_string(),
                agent_type: u["name"].as_str().map(str::to_owned),
                background: false,
            });
        }
        "subagent_state_update" => {
            let Some(session) = u["subagentSessionId"].as_str() else {
                return out;
            };
            let status = match u["state"].as_str() {
                Some("completed") => "completed",
                Some("failed") => "failed",
                Some("running") | None => return out,
                _ => "stopped",
            };
            let tool_id = u["_meta"]["cursor"]["toolCallId"].as_str().map(str::to_owned);
            if let Some(mut child) = state.children.remove(session) {
                out.extend(close_child_text(&mut child));
            }
            out.push(AgentEvent::TaskEnded {
                task_id: session.to_string(),
                tool_id,
                status: status.to_string(),
                summary: None,
                duration_ms: None,
            });
        }
        "session_info_update" => {
            if let Some(title) = u["title"].as_str().filter(|t| !t.trim().is_empty()) {
                out.push(AgentEvent::TitleChanged { title: title.trim().to_string() });
            }
        }
        _ => {}
    }
    out
}

/// A subagent's text so far, as one finished block under its tool call.
fn close_child_text(child: &mut Child) -> Option<AgentEvent> {
    let text = std::mem::take(&mut child.text);
    if text.trim().is_empty() {
        return None;
    }
    Some(AgentEvent::AssistantBlock {
        message_id: uuid::Uuid::new_v4().to_string(),
        block: Block::Text { text },
        parent_tool_id: Some(child.parent_tool.clone()),
    })
}

/// One `session/update` from a subagent's session: its text and tool calls,
/// tagged with the chat's tool call that started it. Nothing streams live.
fn child_update(child: &mut Child, u: &Value) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    let tag = |event: AgentEvent, parent: &str| match event {
        AgentEvent::AssistantBlock { message_id, block, .. } => AgentEvent::AssistantBlock {
            message_id,
            block,
            parent_tool_id: Some(parent.to_string()),
        },
        AgentEvent::ToolResult { tool_id, content, is_error, .. } => AgentEvent::ToolResult {
            tool_id,
            content,
            is_error,
            parent_tool_id: Some(parent.to_string()),
        },
        AgentEvent::ToolInput { tool_id, name, input, .. } => AgentEvent::ToolInput {
            tool_id,
            name,
            input,
            parent_tool_id: Some(parent.to_string()),
        },
        other => other,
    };
    match u["sessionUpdate"].as_str().unwrap_or_default() {
        "agent_message_chunk" => {
            if let Some(text) = u["content"]["text"].as_str() {
                child.text.push_str(text);
            }
        }
        "tool_call" | "tool_call_update" => {
            out.extend(close_child_text(child));
            let Some(tool_id) = u["toolCallId"].as_str() else {
                return out;
            };
            let parent = child.parent_tool.clone();
            let tool = child.tools.entry(tool_id.to_string()).or_default();
            if let Some(title) = u["title"].as_str() {
                tool.title = title.to_string();
            }
            if let Some(kind) = u["kind"].as_str() {
                tool.kind = kind.to_string();
            }
            if let Some(input) = u.get("rawInput").filter(|i| i.is_object()) {
                tool.raw_input = input.clone();
            }
            let status = u["status"].as_str().unwrap_or_default();
            let started = matches!(status, "in_progress" | "completed" | "failed");
            match show_tool(tool_id, tool, started) {
                Some(shown) => out.push(tag(shown, &parent)),
                None if started => out.extend(refresh_tool(tool_id, tool).map(|event| tag(event, &parent))),
                None => {}
            }
            if matches!(status, "completed" | "failed") && !tool.done && tool.shown {
                tool.done = true;
                out.push(AgentEvent::ToolResult {
                    tool_id: tool_id.to_string(),
                    content: json!(result_text(u)),
                    is_error: status == "failed",
                    parent_tool_id: Some(parent),
                });
            }
        }
        _ => {}
    }
    out
}

impl AcpProcess {
    fn session_id(&self) -> Option<String> {
        self.shared.state.lock().unwrap().session_id.clone()
    }

    /// Runs a call in the background; a failure is shown in the chat.
    fn background(&self, method: &'static str, params: Value, after: impl FnOnce(&Shared, Value) + Send + 'static) {
        let shared = self.shared.clone();
        tauri::async_runtime::spawn(async move {
            match shared.rpc.call(method, params).await {
                Ok(reply) => after(&shared, reply),
                Err(e) => shared.emit(AgentEvent::Error {
                    message: format!("{}: {}", shared.label, e.message),
                }),
            }
        });
    }
}

impl RunningAgent for AcpProcess {
    fn send_user_message(&self, text: &str, images: &[ImagePayload], id: &str) -> Result<(), String> {
        let images = images.iter().map(|i| (i.media_type.clone(), i.data_b64.clone())).collect();
        // Before sending: the driver drops a message it finds missing here.
        self.shared.state.lock().unwrap().queued.insert(id.to_string());
        let item = Item::User {
            text: text.to_string(),
            images,
            id: id.to_string(),
        };
        if self.shared.items.send(item).is_err() {
            self.shared.state.lock().unwrap().queued.remove(id);
            return Err(format!("{} is no longer running", self.shared.label));
        }
        self.shared.emit(AgentEvent::Status {
            state: SessionState::Working,
        });
        Ok(())
    }

    fn answer_approval(
        &self,
        request_id: &str,
        decision: Decision,
        message: Option<String>,
        answers: Option<BTreeMap<String, String>>,
        plan: Option<String>,
    ) -> Result<(), String> {
        let waiting = self
            .shared
            .state
            .lock()
            .unwrap()
            .waiting
            .remove(request_id)
            .ok_or("This request is no longer waiting for an answer")?;
        let rpc = &self.shared.rpc;
        let allow = decision != Decision::Deny;
        let mut notes = Vec::new();
        match &waiting {
            Waiting::Permission { rpc_id, options } => {
                let kind = match decision {
                    Decision::Allow => "allow_once",
                    Decision::AlwaysAllow => "allow_always",
                    Decision::Deny => "reject_once",
                };
                let option = options
                    .iter()
                    .find(|(_, k)| k == kind)
                    .or_else(|| options.iter().find(|(_, k)| k.starts_with(if allow { "allow" } else { "reject" })));
                let result = match option {
                    Some((id, _)) => json!({ "outcome": { "outcome": "selected", "optionId": id } }),
                    None => json!({ "outcome": { "outcome": "cancelled" } }),
                };
                rpc.respond(rpc_id, result);
                if let Some(message) = message.filter(|_| !allow) {
                    notes.push(message);
                }
            }
            Waiting::Plan { rpc_id } => {
                if allow {
                    rpc.respond(rpc_id, json!({ "outcome": { "outcome": "accepted" } }));
                    // Cursor stops after a plan; the app carries on, as Claude Code does.
                    notes.push(match plan {
                        Some(plan) => format!(
                            "The user approved the plan after editing it. Build it now, from this version, \
not your original:\n\n{plan}"
                        ),
                        None => "The user approved the plan. Build it now.".to_string(),
                    });
                    // Planning is over; the agent needs its full tools to build.
                    // Switched once the turn ends: switching while the plan
                    // call finishes makes Cursor report the plan as cancelled.
                    self.shared.state.lock().unwrap().build_after_turn = true;
                } else {
                    let message = message.unwrap_or_else(|| "Keep planning.".to_string());
                    let reason = format!("The user didn't approve the plan yet. What they said: {message}");
                    rpc.respond(rpc_id, json!({ "outcome": { "outcome": "rejected", "reason": reason } }));
                }
            }
            Waiting::Question { rpc_id, questions } => {
                if allow {
                    let answers = answers.unwrap_or_default();
                    let picked: Vec<Value> = questions
                        .iter()
                        .map(|(qid, text, options)| {
                            let chosen: Vec<&str> = answers.get(text).map(|a| a.split(", ").collect()).unwrap_or_default();
                            let ids: Vec<&String> = options
                                .iter()
                                .filter(|(_, label)| chosen.contains(&label.as_str()))
                                .map(|(id, _)| id)
                                .collect();
                            json!({ "questionId": qid, "selectedOptionIds": ids })
                        })
                        .collect();
                    rpc.respond(rpc_id, json!({ "outcome": { "outcome": "answered", "answers": picked } }));
                } else {
                    let reason = message.unwrap_or_else(|| "The user skipped these questions.".to_string());
                    rpc.respond(rpc_id, json!({ "outcome": { "outcome": "skipped", "reason": reason } }));
                }
            }
        }
        if !notes.is_empty() {
            self.shared.state.lock().unwrap().notes.extend(notes);
        }
        self.shared.emit(AgentEvent::Status {
            state: SessionState::Working,
        });
        Ok(())
    }

    fn open_requests(&self) -> Vec<String> {
        self.shared.state.lock().unwrap().waiting.keys().cloned().collect()
    }

    fn interrupt(&self) -> Result<(), String> {
        let session_id = self.session_id().ok_or("Nothing to stop yet")?;
        self.shared.rpc.notify("session/cancel", json!({ "sessionId": session_id }));
        // The agent doesn't withdraw what it asked; the app does.
        let left: Vec<(String, Waiting)> = self.shared.state.lock().unwrap().waiting.drain().collect();
        for (request_id, waiting) in left {
            self.shared.rpc.respond(waiting.rpc_id(), waiting.cancelled());
            self.shared.emit(AgentEvent::ApprovalCancelled { request_id });
        }
        Ok(())
    }

    fn set_mode(&self, mode: &str) -> Result<(), String> {
        let session_id = self.session_id().ok_or("Not started yet")?;
        let mode = mode.to_string();
        self.background(
            "session/set_mode",
            json!({ "sessionId": session_id, "modeId": mode }),
            move |shared, _| shared.state.lock().unwrap().mode = Some(mode),
        );
        Ok(())
    }

    fn set_model(&self, model: &str) -> Result<(), String> {
        let session_id = self.session_id().ok_or("Not started yet")?;
        let shared = self.shared.clone();
        let model = model.to_string();
        tauri::async_runtime::spawn(async move {
            match set_model(&shared, &session_id, &model).await {
                Ok(()) => apply_effort(&shared, &session_id).await,
                Err(e) => shared.emit(AgentEvent::Error {
                    message: format!("{} couldn't switch to {model}: {}", shared.label, e.message),
                }),
            }
        });
        Ok(())
    }

    fn set_effort(&self, effort: &str) -> Result<(), String> {
        self.shared.state.lock().unwrap().effort = Some(effort.to_string()).filter(|e| e != "default");
        let session_id = self.session_id().ok_or("Not started yet")?;
        let shared = self.shared.clone();
        tauri::async_runtime::spawn(async move { apply_effort(&shared, &session_id).await });
        Ok(())
    }

    // The message waits in this app's own queue, so taking it back is only
    // a matter of the driver skipping it.
    fn withdraw_message(&self, id: &str) -> Result<(), String> {
        let ok = self.shared.state.lock().unwrap().queued.remove(id);
        self.shared.emit(AgentEvent::MessageWithdrawn { id: id.to_string(), ok });
        Ok(())
    }

    fn close(&self) {
        let _ = self.shared.items.send(Item::Close);
    }

    fn stop(&self) {
        self.shared.stop();
    }
}

impl Drop for AcpProcess {
    fn drop(&mut self) {
        // A dropped handle (a deleted chat) ends the process.
        let _ = self.shared.items.send(Item::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_conversation_is_told_where_docs_go() {
        let opts = SpawnOptions {
            binary: "agent".into(),
            project_path: "/work".into(),
            resume_id: None,
            mode: None,
            model: None,
            effort: None,
            side: None,
            tools: Vec::new(),
            notes: Some("/data/notes/chat-1/notes.md".into()),
            project_docs: Some("/home/projects/-work/docs".into()),
            memory: None,
            preferences: None,
            env: Vec::new(),
        };
        let told = instructions(&opts).unwrap();
        assert!(told.contains("That folder, /data/notes/chat-1, is this chat's own"));
        assert!(told.contains("\"put it in the docs\" means write it here"));
        assert!(told.contains("write it into the project's docs folder, /home/projects/-work/docs,"));
        assert!(!told.contains("Claude Docs"), "only Claude has that connector");
    }

    #[test]
    fn a_new_conversation_is_told_about_the_projects_memory() {
        let dir = std::env::temp_dir().join(format!("harness-acp-memory-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let index: String = (1..=250).map(|n| format!("- [Note {n}](n{n}.md) — fact {n}\n")).collect();
        std::fs::write(dir.join("MEMORY.md"), index).unwrap();
        let opts = |memory: Option<PathBuf>| SpawnOptions {
            binary: "agent".into(),
            project_path: "/work".into(),
            resume_id: None,
            mode: None,
            model: None,
            effort: None,
            side: None,
            tools: Vec::new(),
            notes: None,
            project_docs: None,
            memory,
            preferences: None,
            env: Vec::new(),
        };
        let told = instructions(&opts(Some(dir.clone()))).unwrap();
        assert!(told.contains(&format!("This project has a shared memory in {}", dir.display())));
        assert!(told.contains("frontmatter (`name`, `description`, and `metadata.type`"));
        // The index as it is now, cut where Claude Code cuts its own.
        assert!(told.contains("- [Note 200](n200.md) — fact 200") && !told.contains("Note 201"));

        // The user's preferences come with it.
        let mut with_preferences = opts(None);
        with_preferences.preferences =
            Some(crate::home::Preferences { path: "/home/preferences.md".into(), text: "- Be brief.".into() });
        let told = instructions(&with_preferences).unwrap();
        assert!(told.contains("standing instructions, kept in /home/preferences.md") && told.ends_with("- Be brief."));

        // No index yet: said so. No memory folder: nothing about memory.
        std::fs::remove_file(dir.join("MEMORY.md")).unwrap();
        assert!(instructions(&opts(Some(dir.clone()))).unwrap().ends_with("The index now:\n(empty)"));
        assert!(instructions(&opts(None)).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A connection with no process: what it writes and what it reports are kept.
    fn rig() -> (Arc<Shared>, mpsc::UnboundedReceiver<Out>, Arc<Mutex<Vec<AgentEvent>>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (items, _) = mpsc::unbounded_channel();
        let seen: Arc<Mutex<Vec<AgentEvent>>> = Arc::default();
        let sink: EventSink = {
            let seen = seen.clone();
            Arc::new(move |e| seen.lock().unwrap().push(e))
        };
        let shared = Arc::new(Shared {
            rpc: Rpc {
                tx,
                next: AtomicU64::new(1),
                waiting: Mutex::default(),
            },
            state: Mutex::default(),
            sink,
            items,
            stop_tx: Mutex::new(None),
            own_tools: vec!["harness_browser".into()],
            label: "Cursor",
        });
        (shared, rx, seen)
    }

    fn written(rx: &mut mpsc::UnboundedReceiver<Out>) -> Vec<Value> {
        let mut lines = Vec::new();
        while let Ok(Out::Line(line)) = rx.try_recv() {
            lines.push(serde_json::from_str(&line).unwrap());
        }
        lines
    }

    fn line(shared: &Shared, text: &str) {
        receive(shared, &serde_json::from_str(text).unwrap());
    }

    fn take(seen: &Mutex<Vec<AgentEvent>>) -> Vec<AgentEvent> {
        std::mem::take(&mut seen.lock().unwrap())
    }

    #[test]
    fn a_waiting_message_can_be_taken_back_once() {
        let (shared, _rx, seen) = rig();
        let process = AcpProcess { shared: shared.clone() };
        // As `send_user_message` leaves it (the rig has no driver to send to).
        shared.state.lock().unwrap().queued.insert("m1".into());
        process.withdraw_message("m1").unwrap();
        assert_eq!(take(&seen), vec![AgentEvent::MessageWithdrawn { id: "m1".into(), ok: true }]);
        assert!(shared.state.lock().unwrap().queued.is_empty(), "the driver will skip it");
        process.withdraw_message("m1").unwrap();
        assert_eq!(take(&seen), vec![AgentEvent::MessageWithdrawn { id: "m1".into(), ok: false }]);
    }

    const SHELL: &str = r#"{"sessionUpdate":"tool_call","toolCallId":"call-1\nfc_1","title":"Terminal","kind":"execute","status":"pending","rawInput":{}}"#;

    fn upd(shared: &Shared, update: &str) {
        line(shared, &format!(r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s","update":{update}}}}}"#));
    }

    #[test]
    fn thinking_then_text_become_blocks_and_text_streams() {
        let (shared, _rx, seen) = rig();
        upd(&shared, r#"{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"Hmm"}}"#);
        upd(&shared, r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hel"}}"#);
        upd(&shared, r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"lo"}}"#);
        end_run(&shared);
        let events = take(&seen);
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::Thinking { text }, .. } if text == "Hmm"));
        assert!(matches!(&events[1], AgentEvent::TextDelta { text, .. } if text == "Hel"));
        assert!(matches!(&events[2], AgentEvent::TextDelta { text, .. } if text == "lo"));
        let (AgentEvent::TextDelta { message_id: a, .. }, AgentEvent::AssistantBlock { message_id: b, block, .. }) =
            (&events[1], &events[3])
        else {
            panic!("{events:?}")
        };
        assert_eq!(a, b);
        assert_eq!(block, &Block::Text { text: "Hello".into() });
    }

    #[test]
    fn a_tool_call_is_shown_once_its_input_arrives_and_its_output_follows() {
        let (shared, _rx, seen) = rig();
        upd(&shared, SHELL);
        assert!(take(&seen).is_empty(), "nothing to show with an empty input");
        upd(
            &shared,
            r#"{"sessionUpdate":"tool_call_update","toolCallId":"call-1\nfc_1","title":"`ls`","rawInput":{"command":"ls"}}"#,
        );
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"call-1\nfc_1","status":"in_progress"}"#);
        upd(
            &shared,
            r#"{"sessionUpdate":"tool_call_update","toolCallId":"call-1\nfc_1","status":"completed","rawOutput":{"exitCode":0,"stdout":"a\n","stderr":"oops"}}"#,
        );
        let events = take(&seen);
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, input, .. }, .. }
            if name == "Bash" && input["command"] == "ls"));
        assert!(matches!(&events[1], AgentEvent::ToolResult { content, is_error: false, .. } if content == "a\noops"));
    }

    // Lines below are from a live run on Cursor CLI 2026.11.13 (trimmed).
    #[test]
    fn a_subagents_session_is_shown_under_its_tool_call() {
        let (shared, mut rx, seen) = rig();
        let child = |update: &str| {
            line(&shared, &format!(r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"kid","update":{update}}}}}"#));
        };
        upd(&shared, r#"{"sessionUpdate":"subagent_spawned","subagentSessionId":"kid","name":"generalPurpose","task":"Read these two files","capabilities":{},"_meta":{"cursor":{"toolCallId":"call-6\nfc_0","agentId":"kid","model":"default"}}}"#);
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"call-6\nfc_0","title":"Task: Read package.json","kind":"other","status":"pending","rawInput":{"_toolName":"task","prompt":"Read these two files","description":"Read package.json"}}"#);
        child(r#"{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"The user wants"}}"#);
        child(r#"{"sessionUpdate":"tool_call","toolCallId":"call-3","title":"Read File","kind":"read","status":"pending","rawInput":{}}"#);
        child(r#"{"sessionUpdate":"tool_call_update","toolCallId":"call-3","title":"Read /p/package.json","rawInput":{"path":"/p/package.json"}}"#);
        child(r#"{"sessionUpdate":"tool_call_update","toolCallId":"call-3","status":"completed","rawOutput":{"content":"{}\n"}}"#);
        child(r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Both "}}"#);
        child(r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"read."}}"#);
        upd(&shared, r#"{"sessionUpdate":"subagent_state_update","subagentSessionId":"kid","state":"completed","_meta":{"cursor":{"toolCallId":"call-6\nfc_0","agentId":"kid"}}}"#);
        line(&shared, r#"{"jsonrpc":"2.0","id":7,"method":"cursor/task","params":{"toolCallId":"call-6\nfc_0"}}"#);

        let events = take(&seen);
        assert!(matches!(&events[0], AgentEvent::TaskStarted { tool_id: Some(t), .. } if t == "call-6\nfc_0"));
        assert!(matches!(&events[1], AgentEvent::AssistantBlock { block: Block::ToolCall { name, input, .. }, parent_tool_id: None, .. }
            if name == "Agent" && input["prompt"] == "Read these two files"));
        let under: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::AssistantBlock { parent_tool_id: Some(_), .. } | AgentEvent::ToolResult { parent_tool_id: Some(_), .. }))
            .collect();
        assert_eq!(under.len(), 3, "{events:?}");
        assert!(matches!(under[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, .. }, .. } if name == "Read"));
        assert!(matches!(under[2], AgentEvent::AssistantBlock { block: Block::Text { text }, .. } if text == "Both read."));
        assert!(matches!(events.last().unwrap(), AgentEvent::TaskEnded { status, .. } if status == "completed"));
        // Nothing of the subagent's leaks into the chat's own text.
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::TextDelta { .. })));
        assert_eq!(written(&mut rx).last().unwrap()["result"], json!({}));
    }

    #[test]
    fn tools_get_the_names_the_window_words() {
        let tool = |kind: &str, title: &str, raw: Value| Tool {
            title: title.into(),
            kind: kind.into(),
            raw_input: raw,
            ..Tool::default()
        };
        let cases = [
            (tool("read", "Read a", json!({ "path": "/p/a.md" })), "Read", json!({ "file_path": "/p/a.md" })),
            (tool("edit", "Edit", json!({ "path": "/p/a.md" })), "Edit", json!({ "file_path": "/p/a.md" })),
            (tool("search", "grep \"x\"", json!({ "pattern": "x" })), "Grep", json!({ "pattern": "x", "path": null })),
            (tool("search", "Find `*.md`", json!({ "pattern": "*.md" })), "Glob", json!({ "pattern": "*.md", "path": null })),
            (
                tool("other", "harness_browser: open", json!({ "providerIdentifier": "harness_browser", "toolName": "open", "args": { "url": "x" } })),
                "mcp__harness_browser__open",
                json!({ "url": "x" }),
            ),
            (tool("other", "Create Plan", json!({ "_toolName": "createPlan", "plan": "P" })), "ExitPlanMode", json!({ "plan": "P" })),
        ];
        for (tool, name, input) in cases {
            assert_eq!(name_and_input(&tool), (name.to_string(), input));
        }
    }

    #[test]
    fn a_permission_becomes_a_card_and_each_answer_picks_its_option() {
        let (shared, mut rx, seen) = rig();
        upd(&shared, SHELL);
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"call-1\nfc_1","title":"`echo hi`","rawInput":{"command":"echo hi"}}"#);
        take(&seen);
        let ask = r#"{"jsonrpc":"2.0","id":0,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"call-1\nfc_1","title":"`echo hi`","kind":"execute","status":"pending","content":[{"type":"content","content":{"type":"text","text":"Not in allowlist: echo"}}]},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"allow-always","name":"Allow always","kind":"allow_always"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}}"#;
        line(&shared, ask);
        let events = take(&seen);
        let AgentEvent::ApprovalRequest { request_id, approval, tool_name, title, always_label, .. } = &events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(*approval, ApprovalKind::Tool);
        assert_eq!(tool_name, "Bash");
        assert_eq!(title, "echo hi");
        assert_eq!(always_label.as_deref(), Some("Always allow echo"));
        let process = AcpProcess { shared: shared.clone() };
        assert_eq!(process.open_requests(), vec![request_id.clone()]);
        process.answer_approval(request_id, Decision::AlwaysAllow, None, None, None).unwrap();
        assert_eq!(written(&mut rx), vec![json!({ "jsonrpc": "2.0", "id": 0, "result": { "outcome": { "outcome": "selected", "optionId": "allow-always" } } })]);

        take(&seen);
        line(&shared, ask);
        let AgentEvent::ApprovalRequest { request_id, .. } = take(&seen).remove(0) else { panic!() };
        process.answer_approval(&request_id, Decision::Deny, Some("not that".into()), None, None).unwrap();
        assert_eq!(written(&mut rx)[0]["result"]["outcome"]["optionId"], "reject-once");
        // The reason reaches the agent with the next message.
        assert_eq!(shared.state.lock().unwrap().notes, vec!["not that".to_string()]);
    }

    #[test]
    fn the_apps_own_tools_are_allowed_without_a_card() {
        let (shared, mut rx, seen) = rig();
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"t","title":"harness_browser: open","kind":"other","rawInput":{"providerIdentifier":"harness_browser","toolName":"open","args":{}}}"#);
        take(&seen);
        line(&shared, r#"{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t","title":"harness_browser-open: open","kind":"other"},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}}"#);
        assert!(take(&seen).iter().all(|e| !matches!(e, AgentEvent::ApprovalRequest { .. })));
        assert_eq!(written(&mut rx)[0]["result"]["outcome"]["optionId"], "allow-once");
    }

    #[test]
    fn a_call_that_started_empty_gets_its_real_input_when_it_arrives() {
        let (shared, _rx, seen) = rig();
        // Started before Cursor said what it searches for.
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"t","title":"Grep","kind":"search","status":"in_progress"}"#);
        let events = take(&seen);
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, input, .. }, .. }
            if name == "Grep" && input["pattern"].is_null()));
        // The input arrives while it waits: nothing yet, it may still change.
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"t","rawInput":{"pattern":"fn main"}}"#);
        assert!(take(&seen).is_empty());
        // It finishes: the real input, then the result.
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed","rawOutput":{"totalMatches":2}}"#);
        let events = take(&seen);
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(&events[0], AgentEvent::ToolInput { tool_id, name, input, parent_tool_id: None }
            if tool_id == "t" && name == "Grep" && input["pattern"] == "fn main"));
        assert!(matches!(&events[1], AgentEvent::ToolResult { tool_id, .. } if tool_id == "t"));

        // A call whose input was there from the start says it once.
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"u","title":"Read","kind":"read","rawInput":{"path":"/a.rs"},"status":"in_progress"}"#);
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"u","status":"completed","rawOutput":{"content":"x"}}"#);
        let events = take(&seen);
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(events.iter().all(|e| !matches!(e, AgentEvent::ToolInput { .. })));
    }

    #[tokio::test]
    async fn the_apps_own_answer_is_saved_as_its_tools_result() {
        let (shared, _rx, seen) = rig();
        let echo = AppTools {
            server: "harness_chats",
            instruction: String::new(),
            list: json!([]),
            call: Arc::new(|tool, args| {
                Box::pin(async move {
                    match tool.as_str() {
                        "read" => Ok(vec![json!({ "type": "text", "text": format!("read {}", args["id"]) })]),
                        _ => Err("No such tool".to_string()),
                    }
                })
            }),
        };
        let log = shared.state.lock().unwrap().served.clone();
        let tools = recording(&[echo], &log);
        // Two calls answered by the app, as Cursor's would be over HTTP.
        (tools[0].call)("read".into(), json!({ "id": "aaaa" })).await.unwrap();
        (tools[0].call)("read".into(), json!({ "id": "bbbb" })).await.unwrap();
        (tools[0].call)("nope".into(), json!({})).await.unwrap_err();

        // Cursor then reports each as done, with no output of its own.
        let done = |id: &str, tool: &str, args: &str| {
            upd(&shared, &format!(
                r#"{{"sessionUpdate":"tool_call","toolCallId":"{id}","title":"harness_chats: {tool}","kind":"other","status":"completed","rawInput":{{"providerIdentifier":"harness_chats","toolName":"{tool}","args":{args}}},"rawOutput":{{"success":true}}}}"#
            ));
            let events = take(&seen);
            let Some(AgentEvent::ToolResult { content, is_error, .. }) = events.last().cloned() else {
                panic!("no result: {events:?}");
            };
            (content, is_error)
        };
        // Matched by input, not by order.
        assert_eq!(done("t2", "read", r#"{"id":"bbbb"}"#), (json!("read \"bbbb\""), false));
        assert_eq!(done("t1", "read", r#"{"id":"aaaa"}"#), (json!("read \"aaaa\""), false));
        assert_eq!(done("t3", "nope", "{}"), (json!("No such tool"), true));
        // Nothing kept for it: the result stays empty.
        assert_eq!(done("t4", "read", r#"{"id":"cccc"}"#), (json!(""), false));
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn a_plan_is_a_plan_card_and_approving_it_says_to_build() {
        let (shared, mut rx, seen) = rig();
        line(&shared, r##"{"jsonrpc":"2.0","id":3,"method":"cursor/create_plan","params":{"toolCallId":"p1","name":"Add docs","overview":"o","plan":"# Plan\n1. x","todos":[]}}"##);
        let events = take(&seen);
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, .. }, .. } if name == "ExitPlanMode"));
        let AgentEvent::ApprovalRequest { request_id, approval: ApprovalKind::Plan, input, title, .. } = &events[1] else {
            panic!("{events:?}")
        };
        assert_eq!(input["plan"], "# Plan\n1. x");
        assert_eq!(title, "Add docs");
        let process = AcpProcess { shared: shared.clone() };
        process
            .answer_approval(request_id, Decision::Allow, None, None, Some("# Edited".into()))
            .unwrap();
        assert_eq!(written(&mut rx)[0]["result"], json!({ "outcome": { "outcome": "accepted" } }));
        let notes = shared.state.lock().unwrap().notes.clone();
        assert!(notes[0].contains("# Edited"), "{notes:?}");

        line(&shared, r#"{"jsonrpc":"2.0","id":4,"method":"cursor/create_plan","params":{"toolCallId":"p2","plan":"P"}}"#);
        let AgentEvent::ApprovalRequest { request_id, .. } = take(&seen).into_iter().find(|e| matches!(e, AgentEvent::ApprovalRequest { .. })).unwrap() else {
            panic!()
        };
        process.answer_approval(&request_id, Decision::Deny, Some("Shorter".into()), None, None).unwrap();
        let answer = &written(&mut rx)[0]["result"]["outcome"];
        assert_eq!(answer["outcome"], "rejected");
        assert!(answer["reason"].as_str().unwrap().contains("Shorter"));
    }

    #[test]
    fn a_question_is_answered_with_option_ids() {
        let (shared, mut rx, seen) = rig();
        line(&shared, r#"{"jsonrpc":"2.0","id":5,"method":"cursor/ask_question","params":{"toolCallId":"q","questions":[{"id":"color","prompt":"Which colour?","options":[{"id":"r","label":"Red"},{"id":"b","label":"Blue"}]}]}}"#);
        let AgentEvent::ApprovalRequest { request_id, approval: ApprovalKind::Question, input, .. } = take(&seen).remove(0) else {
            panic!()
        };
        assert_eq!(input["questions"][0]["question"], "Which colour?");
        assert_eq!(input["questions"][0]["options"][1]["label"], "Blue");
        let answers = BTreeMap::from([("Which colour?".to_string(), "Blue".to_string())]);
        AcpProcess { shared: shared.clone() }
            .answer_approval(&request_id, Decision::Allow, None, Some(answers), None)
            .unwrap();
        assert_eq!(
            written(&mut rx)[0]["result"],
            json!({ "outcome": { "outcome": "answered", "answers": [{ "questionId": "color", "selectedOptionIds": ["b"] }] } })
        );
    }

    #[test]
    fn stopping_withdraws_what_waits_and_sends_cancel() {
        let (shared, mut rx, seen) = rig();
        shared.state.lock().unwrap().session_id = Some("s".into());
        line(&shared, r#"{"jsonrpc":"2.0","id":9,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t","title":"x","kind":"execute"},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"}]}}"#);
        take(&seen);
        AcpProcess { shared: shared.clone() }.interrupt().unwrap();
        let lines = written(&mut rx);
        assert_eq!(lines[0]["method"], "session/cancel");
        assert_eq!(lines[1], json!({ "jsonrpc": "2.0", "id": 9, "result": { "outcome": { "outcome": "cancelled" } } }));
        assert!(take(&seen).iter().any(|e| matches!(e, AgentEvent::ApprovalCancelled { .. })));
    }

    #[test]
    fn replayed_history_is_skipped_and_mode_and_title_pass_through() {
        let (shared, _rx, seen) = rig();
        shared.state.lock().unwrap().loading = true;
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"replay-0-1","title":"`ls`","kind":"execute","rawInput":{"command":"ls"}}"#);
        assert!(take(&seen).is_empty());
        shared.state.lock().unwrap().loading = false;
        upd(&shared, r#"{"sessionUpdate":"current_mode_update","currentModeId":"plan"}"#);
        upd(&shared, r#"{"sessionUpdate":"session_info_update","title":"Fix the login"}"#);
        assert_eq!(
            take(&seen),
            vec![
                AgentEvent::ModeChanged { mode: "plan".into() },
                AgentEvent::TitleChanged { title: "Fix the login".into() }
            ]
        );
    }

    #[test]
    fn effort_options_are_read_per_model() {
        let mut config = Config::default();
        config.read(&json!([
            { "id": "mode", "category": "mode", "currentValue": "agent", "options": [{ "value": "agent" }, { "value": "plan" }] },
            { "id": "model", "category": "model", "currentValue": "gpt-5.5", "options": [] },
            { "id": "reasoning", "category": "thought_level", "currentValue": "medium", "options": [{ "value": "low" }, { "value": "high" }] },
        ]));
        assert_eq!(config.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(config.modes, vec!["agent", "plan"]);
        assert_eq!(config.effort, Some(("reasoning".into(), vec!["low".into(), "high".into()])));
    }
}
