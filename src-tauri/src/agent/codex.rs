//! OpenAI's Codex, driven over its own app-server protocol (`codex
//! app-server`): JSON-RPC without the `"jsonrpc"` field, one message per line,
//! on stdin and stdout. The same protocol OpenAI's own Codex apps use. What it
//! sends was tested live on codex-cli 0.169.1; see PLAN.md, "Codex over
//! app-server".
//!
//! One task drives the chat in order: `initialize`, then `thread/start` or
//! `thread/resume`, then `turn/start` for each turn. Unlike ACP a turn's call
//! returns at once: the turn runs on and ends with `turn/completed`, so a
//! message sent mid-turn goes in with `turn/steer`. A reader task routes the
//! agent's lines: replies, its requests (approvals, questions), and its
//! notifications (items, turns, usage).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use super::acp::terminate;
use super::claude_code::{BATCH_PROMPT, TITLE_INPUT_CHARS, TITLE_PROMPT, batch_input, clean_title};
use super::rpc::{Out, Rpc, RpcError};
use super::{AgentAdapter, EventSink, ImagePayload, RunningAgent, SideCopy, SpawnOptions, mcp_http, plain_id};
use crate::events::{AgentEvent, ApprovalKind, BackgroundTask, Block, CommandInfo, Decision, ModelInfo, SessionState};

pub struct CodexAdapter;

/// How many stderr lines to keep for an error message when the process dies.
const STDERR_TAIL_LINES: usize = 30;
/// How long a stopped process gets before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// How long listing the models may take.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a title may take.
const TITLE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long `codex login` may wait for the user to finish in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
/// How long the driver waits for more items before starting a turn, so an
/// answer and the message that follows it go together.
const BATCH_WINDOW: Duration = Duration::from_millis(30);

/// The chat's permission modes, as the model menu shows them: Codex's own
/// (read only, the default that asks before leaving the project, full
/// access), plus its plan mode.
pub const MODES: [&str; 4] = ["default", "plan", "read-only", "full-access"];
const DEFAULT_MODE: &str = "default";
/// A side chat's own mode, never offered: the read-only sandbox with no
/// approvals asked, so anything more is refused.
const SIDE_MODE: &str = "side";
/// How long deleting a side chat's thread may take.
const FORGET_TIMEOUT: Duration = Duration::from_secs(20);

/// What `initialize` says about the app. `experimentalApi` turns on plan mode
/// (`collaborationMode`) and questions (`item/tool/requestUserInput`).
fn initialize_params() -> Value {
    json!({
        "clientInfo": { "name": "harness", "title": "Harness", "version": env!("CARGO_PKG_VERSION") },
        "capabilities": { "experimentalApi": true, "requestAttestation": false },
    })
}

/// What the driver does next.
enum Item {
    /// A message from the user.
    User { input: Vec<Value>, id: String },
    /// Wakes the driver: notes are waiting, or a turn ended with messages left.
    Wake,
    /// Finish up and exit.
    Close,
}

/// A request from Codex waiting for the user.
enum Waiting {
    /// A command or file change to allow. `decisions` = what Codex offers.
    Approval { rpc_id: Value, decisions: Vec<Value> },
    /// More access for the sandbox.
    Permissions { rpc_id: Value, permissions: Value },
    /// A call on an MCP server that asks first (not the app's own).
    Elicitation { rpc_id: Value },
    /// Questions; each one's id and text.
    Question { rpc_id: Value, questions: Vec<(String, String)> },
    /// A plan written in plan mode, shown once its turn ended. Not a request
    /// of Codex's: answering it starts the next turn.
    Plan { tool_id: String },
}

impl Waiting {
    /// "Never mind", to Codex.
    fn cancel(&self, rpc: &Rpc) {
        match self {
            Waiting::Approval { rpc_id, .. } => rpc.respond(rpc_id, json!({ "decision": "cancel" })),
            Waiting::Permissions { rpc_id, .. } => {
                rpc.respond(rpc_id, json!({ "permissions": {}, "scope": "turn" }))
            }
            Waiting::Elicitation { rpc_id } => {
                rpc.respond(rpc_id, json!({ "action": "cancel", "content": null, "_meta": null }))
            }
            Waiting::Question { rpc_id, .. } => rpc.respond(rpc_id, json!({ "answers": {} })),
            Waiting::Plan { .. } => {}
        }
    }
}

/// A subagent's thread, shown under the chat's tool call that started it.
struct Child {
    parent_tool: String,
    ended: bool,
}

#[derive(Default)]
struct State {
    thread_id: Option<String>,
    /// The running turn, if any.
    turn_id: Option<String>,
    /// When the turn the user sees began (a plan and its build are one).
    turn_started: Option<Instant>,
    /// The model the thread runs on, as Codex names it.
    model: Option<String>,
    effort: Option<String>,
    mode: String,
    /// The roots the sandbox may write to besides the project.
    writable_roots: Vec<String>,
    /// Messages waiting for a turn, by id, in order. One the user takes back
    /// leaves `queued`, and is skipped.
    pending: Vec<(String, Vec<Value>)>,
    queued: HashSet<String>,
    /// Notes from the app for the next turn: a denial's reason, an approved plan.
    notes: Vec<String>,
    /// The plan written this turn, if any.
    plan: Option<(String, String)>,
    /// The app's requests waiting for the user, by the app's request ID.
    waiting: HashMap<String, Waiting>,
    next_request: u64,
    /// Tool calls already shown, and those with a result.
    shown: HashSet<String>,
    done: HashSet<String>,
    /// To-do list updates so far, for unique tool ids.
    todos: u64,
    /// Subagent threads, by thread ID.
    children: HashMap<String, Child>,
    /// The turn failed and said why already.
    errored: bool,
    /// A turn was stopped by the user.
    stopped: bool,
    /// The running turn is a /compact.
    compacting: bool,
    closing: bool,
}

struct Shared {
    rpc: Rpc,
    state: Mutex<State>,
    sink: EventSink,
    items: mpsc::UnboundedSender<Item>,
    /// Taken by the first `stop()`.
    stop_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// The app's tool server names, whose calls never ask.
    own_tools: Vec<String>,
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

struct CodexProcess {
    shared: Arc<Shared>,
}

impl AgentAdapter for CodexAdapter {
    fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
        spawn(opts, sink)
    }

    fn list_models(&self, binary: &Path, _efforts_for: Option<&[String]>) -> Result<Vec<ModelInfo>, String> {
        let mut rpc = SyncRpc::start(binary)?;
        let deadline = Instant::now() + LIST_TIMEOUT;
        rpc.handshake(deadline)?;
        Ok(parse_models(&rpc.call("model/list", json!({}), deadline)?))
    }

    fn list_commands(&self, _binary: &Path, _folder: &Path) -> Result<Vec<CommandInfo>, String> {
        Ok(vec![CommandInfo {
            name: "compact".to_string(),
            description: "Summarize the conversation to free up context".to_string(),
            hint: None,
            aliases: Vec::new(),
        }])
    }

    fn make_title(&self, binary: &Path, first_message: &str) -> Result<String, String> {
        let message: String = first_message.chars().take(TITLE_INPUT_CHARS).collect();
        small_answer(binary, TITLE_PROMPT, &message)
    }

    fn name_batch(&self, binary: &Path, ask: &str, agents: &[String]) -> Result<String, String> {
        small_answer(binary, BATCH_PROMPT, &batch_input(ask, agents))
    }

    fn sign_in(&self, binary: &Path) -> Result<(), String> {
        use std::io::Read;
        let mut cmd = std::process::Command::new(binary);
        cmd.arg("login")
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Could not start Codex: {e}"))?;
        let mut stderr = child.stderr.take().ok_or("Codex has no stderr")?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            let _ = tx.send(text);
        });
        let Ok(detail) = rx.recv_timeout(SIGN_IN_TIMEOUT) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Sign-in took too long".to_string());
        };
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            let detail = detail.trim();
            Err(if detail.is_empty() { "Sign-in failed".to_string() } else { detail.to_string() })
        }
    }

    /// Through Codex itself (`thread/delete`), which also drops the thread
    /// from its own lists. Codex's program isn't known yet when side chats
    /// left by a crash are deleted at startup, so it is looked for here, off
    /// the caller's thread.
    fn forget_session(&self, agent_session_id: &str) {
        if !plain_id(agent_session_id) {
            return;
        }
        let id = agent_session_id.to_string();
        std::thread::spawn(move || {
            let Some(binary) = crate::locate::find_codex() else { return };
            let deadline = Instant::now() + FORGET_TIMEOUT;
            let deleted = SyncRpc::start(&binary).and_then(|mut rpc| {
                rpc.handshake(deadline)?;
                rpc.call("thread/delete", json!({ "threadId": id }), deadline)
            });
            if let Err(e) = deleted {
                eprintln!("harness: couldn't delete Codex thread {id}: {e}");
            }
        });
    }

    fn has_session(&self, agent_session_id: &str) -> bool {
        plain_id(agent_session_id) && sessions_folder().is_some_and(|folder| has_rollout(&folder, agent_session_id))
    }

    fn side_copy(&self) -> SideCopy {
        SideCopy::Fork
    }
}

/// Where Codex saves threads: `<CODEX_HOME>/sessions/<year>/<month>/<day>/`,
/// one `rollout-<time>-<thread id>.jsonl` each. `CODEX_HOME` replaces `~/.codex`.
fn sessions_folder() -> Option<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| crate::home::user_home().map(|home| home.join(".codex")))?;
    Some(home.join("sessions"))
}

/// Whether a thread's saved file is anywhere under `folder`.
fn has_rollout(folder: &Path, id: &str) -> bool {
    let suffix = format!("-{id}.jsonl");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            has_rollout(&path, id)
        } else {
            entry.file_name().to_string_lossy().ends_with(&suffix)
        }
    })
}

/// `model/list`'s visible models, Codex's default first.
fn parse_models(reply: &Value) -> Vec<ModelInfo> {
    let mut models: Vec<(bool, ModelInfo)> = reply["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["hidden"] != true)
        .filter_map(|m| {
            let value = m["id"].as_str()?.to_string();
            let efforts = m["supportedReasoningEfforts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e["reasoningEffort"].as_str().map(str::to_owned))
                .collect();
            Some((
                m["isDefault"] == true,
                ModelInfo {
                    name: m["displayName"].as_str().unwrap_or(&value).to_string(),
                    detail: m["description"].as_str().filter(|d| !d.is_empty()).map(str::to_owned),
                    efforts,
                    default_effort: m["defaultReasoningEffort"].as_str().map(str::to_owned),
                    value,
                },
            ))
        })
        .collect();
    models.sort_by_key(|(default, _)| !default);
    models.into_iter().map(|(_, m)| m).collect()
}

/// The model to write titles with: the first one Codex calls fast or
/// affordable, else its default; and that model's lowest effort.
fn title_model(models: &[ModelInfo]) -> Option<(String, Option<String>)> {
    let cheap = |m: &&ModelInfo| {
        let detail = m.detail.as_deref().unwrap_or_default().to_lowercase();
        detail.contains("fast") || detail.contains("affordable")
    };
    let model = models.iter().find(cheap).or_else(|| models.first())?;
    Some((model.value.clone(), model.efforts.first().cloned()))
}

/// One short answer from Codex's cheapest model, in a throwaway thread with
/// no tools of the user's: a title, or a label for a batch of subagents.
fn small_answer(binary: &Path, instructions: &str, input: &str) -> Result<String, String> {
    let mut rpc = SyncRpc::start(binary)?;
    let deadline = Instant::now() + TITLE_TIMEOUT;
    rpc.handshake(deadline)?;
    let models = parse_models(&rpc.call("model/list", json!({}), deadline)?);
    let (model, effort) = title_model(&models).ok_or("Codex listed no models")?;
    let thread = rpc.call(
        "thread/start",
        json!({
            "cwd": std::env::temp_dir().to_string_lossy(),
            "model": model,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "baseInstructions": instructions,
            "ephemeral": true,
            "config": { "mcp_servers": {} },
        }),
        deadline,
    )?;
    let thread_id = thread["thread"]["id"].as_str().ok_or("Codex started no thread")?.to_string();
    rpc.call(
        "turn/start",
        json!({
            "threadId": thread_id,
            "input": [{ "type": "text", "text": input, "text_elements": [] }],
            "effort": effort,
        }),
        deadline,
    )?;
    let mut answer = String::new();
    loop {
        let notice = rpc.notice(deadline).ok_or("No title in time")?;
        match notice["method"].as_str() {
            Some("item/completed") if notice["params"]["item"]["type"] == "agentMessage" => {
                answer = notice["params"]["item"]["text"].as_str().unwrap_or_default().to_string();
            }
            Some("turn/completed") => break,
            _ => {}
        }
    }
    let title = clean_title(&answer);
    // Markup or a long reply means the model did something other than name it.
    if title.is_empty() || title.contains(['<', '>']) || title.split_whitespace().count() > 10 {
        return Err("No usable title".to_string());
    }
    Ok(title)
}

/// A blocking JSON-RPC client for short jobs (listing models, a title).
struct SyncRpc {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    lines: std::sync::mpsc::Receiver<String>,
    next: u64,
    /// Notifications that came while waiting for a reply, oldest first.
    notices: Vec<Value>,
}

impl SyncRpc {
    fn start(binary: &Path) -> Result<Self, String> {
        let mut cmd = std::process::Command::new(binary);
        cmd.arg("app-server")
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Could not start Codex: {e}"))?;
        let stdin = child.stdin.take().ok_or("Codex has no stdin")?;
        let stdout = child.stdout.take().ok_or("Codex has no stdout")?;
        let (tx, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self { child, stdin, lines, next: 1, notices: Vec::new() })
    }

    fn write(&mut self, value: Value) -> Result<(), String> {
        writeln!(self.stdin, "{value}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }

    fn handshake(&mut self, deadline: Instant) -> Result<(), String> {
        self.call("initialize", initialize_params(), deadline)?;
        self.write(json!({ "method": "initialized" }))
    }

    fn call(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        self.write(json!({ "id": id, "method": method, "params": params }))?;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).map_err(|_| "Codex took too long".to_string())?;
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if v.get("method").is_some() {
                self.take_message(v)?;
                continue;
            }
            if v["id"] == id {
                return match v.get("error") {
                    Some(error) => Err(RpcError::from(error).message),
                    None => Ok(v["result"].clone()),
                };
            }
        }
    }

    /// The next notification, waiting until `deadline`.
    fn notice(&mut self, deadline: Instant) -> Option<Value> {
        loop {
            if !self.notices.is_empty() {
                return Some(self.notices.remove(0));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).ok()?;
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if v.get("method").is_some() {
                    self.take_message(v).ok()?;
                }
            }
        }
    }

    /// Its own requests are refused (a throwaway thread asks for nothing);
    /// notifications are kept.
    fn take_message(&mut self, v: Value) -> Result<(), String> {
        if v.get("id").is_some() {
            return self.write(json!({ "id": v["id"], "error": { "code": -32601, "message": "not available" } }));
        }
        self.notices.push(v);
        Ok(())
    }
}

impl Drop for SyncRpc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What the agent is told on every start and resume, after Codex's own
/// instructions: the app's tools, the chat's notes and docs, the project's
/// memory and the user's preferences. Sent each time like Claude Code's
/// system prompt, so it outlasts compaction.
fn instructions(opts: &SpawnOptions) -> Option<String> {
    let side = opts.side.is_some();
    let mut lines: Vec<String> = Vec::new();
    // A side chat reads and answers: told so, and given no way to write notes or memory.
    if side {
        lines.push(super::SIDE_INSTRUCTION.to_string());
    }
    lines.extend(opts.tools.iter().map(|t| t.instruction.to_string()));
    if let Some(notes) = &opts.notes {
        lines.push(super::notes_instruction(notes, opts.project_docs.as_deref(), !side));
    }
    // Codex's own memory isn't shared with the app's other agents, so it is
    // told about the project's.
    if let Some(memory) = opts.memory.as_ref().filter(|_| !side) {
        lines.push(super::memory_instruction(memory));
    }
    if let Some(preferences) = &opts.preferences {
        lines.push(super::preferences_instruction(preferences));
    }
    let lines: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The folders the sandbox may write to besides the project: the chat's own
/// folder (notes and docs), the project's docs and its memory.
fn writable_roots(opts: &SpawnOptions) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    let folders = [
        opts.notes.as_deref().and_then(Path::parent),
        opts.project_docs.as_deref(),
        opts.memory.as_deref(),
    ];
    for folder in folders.into_iter().flatten() {
        let folder = folder.to_string_lossy().into_owned();
        if !roots.contains(&folder) {
            roots.push(folder);
        }
    }
    roots
}

/// A mode's approval policy and sandbox, as `turn/start` takes them.
fn mode_policy(mode: &str, roots: &[String]) -> (&'static str, Value) {
    match mode {
        SIDE_MODE => ("never", json!({ "type": "readOnly", "networkAccess": false })),
        "read-only" => ("on-request", json!({ "type": "readOnly", "networkAccess": false })),
        "full-access" => ("never", json!({ "type": "dangerFullAccess" })),
        _ => (
            "on-request",
            json!({
                "type": "workspaceWrite",
                "writableRoots": roots,
                "networkAccess": false,
                "excludeTmpdirEnvVar": false,
                "excludeSlashTmp": false,
            }),
        ),
    }
}

/// The same, as `thread/start` and `thread/resume` take them.
fn mode_sandbox(mode: &str) -> (&'static str, &'static str) {
    match mode {
        SIDE_MODE => ("never", "read-only"),
        "read-only" => ("on-request", "read-only"),
        "full-access" => ("never", "danger-full-access"),
        _ => ("on-request", "workspace-write"),
    }
}

/// Starts Codex for one chat.
fn spawn(opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
    let mut cmd = Command::new(&opts.binary);
    cmd.arg("app-server")
        .current_dir(&opts.project_path)
        .envs(opts.env.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash

    let mut child = cmd.spawn().map_err(|e| format!("Could not start Codex: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("Codex has no stdin")?;
    let stdout = child.stdout.take().ok_or("Codex has no stdout")?;
    let stderr = child.stderr.take().ok_or("Codex has no stderr")?;

    // The app's tools, served for as long as this process lives.
    let endpoint = if opts.tools.is_empty() {
        None
    } else {
        match mcp_http::open(opts.tools.clone()) {
            Ok(endpoint) => Some(endpoint),
            Err(e) => {
                eprintln!("harness: the app's tools aren't available to Codex: {e}");
                None
            }
        }
    };
    let servers: Vec<(String, String)> = endpoint.iter().flat_map(|e| e.urls.iter().cloned()).collect();

    let mode = if opts.side.is_some() {
        SIDE_MODE.to_string()
    } else {
        opts.mode.clone().filter(|m| MODES.contains(&m.as_str())).unwrap_or_else(|| DEFAULT_MODE.to_string())
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<Out>();
    let (items_tx, items_rx) = mpsc::unbounded_channel::<Item>();
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let shared = Arc::new(Shared {
        rpc: Rpc::new(tx, false),
        state: Mutex::new(State {
            effort: opts.effort.clone().filter(|e| e != "default"),
            model: opts.model.clone().filter(|m| m != "default"),
            mode,
            writable_roots: writable_roots(&opts),
            ..State::default()
        }),
        sink,
        items: items_tx,
        stop_tx: Mutex::new(Some(stop_tx)),
        own_tools: opts.tools.iter().map(|t| t.server.to_string()).collect(),
    });
    shared.emit(AgentEvent::Status {
        state: SessionState::Starting,
    });

    // Writer: owns stdin. When it ends, stdin closes and Codex exits.
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

    let stderr_tail: Arc<Mutex<std::collections::VecDeque<String>>> = Arc::default();
    {
        let tail = stderr_tail.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut tail = tail.lock().unwrap();
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });
    }

    // Reader: Codex's lines, until it exits.
    {
        let shared = shared.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
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
                    message: format!("Codex stopped unexpectedly:\n{detail}"),
                });
            }
            shared.emit(AgentEvent::Status {
                state: SessionState::Exited,
            });
        });
    }

    tokio::spawn(drive(shared.clone(), opts, servers, items_rx));
    Ok(Box::new(CodexProcess { shared }))
}

/// Config for a thread: the app's tool servers, which never ask before a
/// call (as with Claude Code), and the sandbox's extra folders. A side chat's
/// list replaces the user's own tool servers, so only the app's load.
fn thread_config(servers: &[(String, String)], roots: &[String], side: bool) -> Value {
    if side {
        let servers: serde_json::Map<String, Value> = servers
            .iter()
            .map(|(name, url)| (name.clone(), json!({ "url": url, "default_tools_approval_mode": "approve" })))
            .collect();
        return json!({ "mcp_servers": servers });
    }
    let mut config = serde_json::Map::new();
    for (name, url) in servers {
        config.insert(format!("mcp_servers.{name}.url"), json!(url));
        config.insert(format!("mcp_servers.{name}.default_tools_approval_mode"), json!("approve"));
    }
    config.insert("sandbox_workspace_write.writable_roots".to_string(), json!(roots));
    Value::Object(config)
}

/// Sets up the thread, then starts turns as messages come.
async fn drive(shared: Arc<Shared>, opts: SpawnOptions, servers: Vec<(String, String)>, mut items: mpsc::UnboundedReceiver<Item>) {
    let fail = |message: String| {
        shared.emit(AgentEvent::Error { message });
        shared.stop();
    };
    if let Err(e) = shared.rpc.call("initialize", initialize_params()).await {
        return fail(format!("Codex didn't start: {}", e.message));
    }
    shared.rpc.notify("initialized", json!({}));

    // Signed out: say so and end, so the next message starts fresh.
    if let Ok(account) = shared.rpc.call("account/read", json!({})).await {
        if account["account"].is_null() && account["requiresOpenaiAuth"] == true {
            shared.emit(AgentEvent::SignInNeeded {
                message: "Codex isn't signed in.".to_string(),
            });
            shared.stop();
            return;
        }
    }
    // Usage so far; updates follow as turns run.
    if let Ok(limits) = shared.rpc.call("account/rateLimits/read", json!({})).await {
        for event in usage_events(&limits["rateLimits"]) {
            shared.emit(event);
        }
    }

    let cwd = opts.project_path.to_string_lossy().into_owned();
    let (mode, model, roots) = {
        let state = shared.state.lock().unwrap();
        (state.mode.clone(), state.model.clone(), state.writable_roots.clone())
    };
    let (approval, sandbox) = mode_sandbox(&mode);
    let mut params = json!({
        "cwd": cwd,
        "approvalPolicy": approval,
        "sandbox": sandbox,
        "config": thread_config(&servers, &roots, opts.side.is_some()),
    });
    if let Some(model) = &model {
        params["model"] = json!(model);
    }
    if let Some(text) = instructions(&opts) {
        params["developerInstructions"] = json!(text);
    }

    // A side chat's first start forks the chat's thread; later ones resume
    // the fork. Either failing ends it: a new thread would know nothing.
    let side = opts.side.clone();
    let mut started = None;
    if let Some(side) = &side {
        let (method, id) = match &side.fork_from {
            Some(from) => ("thread/fork", Some(from)),
            None => ("thread/resume", opts.resume_id.as_ref()),
        };
        if let Some(id) = id {
            let mut side_params = params.clone();
            side_params["threadId"] = json!(id);
            side_params["excludeTurns"] = json!(true);
            match shared.rpc.call(method, side_params).await {
                Ok(reply) => started = Some(reply),
                Err(e) => return fail(format!("Codex couldn't copy the chat into this side chat: {}", e.message)),
            }
        }
    } else if let Some(resume) = &opts.resume_id {
        let mut resume_params = params.clone();
        resume_params["threadId"] = json!(resume);
        resume_params["excludeTurns"] = json!(true);
        match shared.rpc.call("thread/resume", resume_params).await {
            Ok(reply) => started = Some(reply),
            Err(e) => shared.emit(AgentEvent::Error {
                message: format!(
                    "Couldn't pick up this chat's earlier Codex conversation ({}). It continues as a new one.",
                    e.message
                ),
            }),
        }
    }
    let reply = match started {
        Some(reply) => reply,
        None => match shared.rpc.call("thread/start", params).await {
            Ok(reply) => reply,
            Err(e) if e.message.to_lowercase().contains("auth") => {
                shared.emit(AgentEvent::SignInNeeded { message: e.message });
                shared.stop();
                return;
            }
            Err(e) => return fail(format!("Codex couldn't start a conversation: {}", e.message)),
        },
    };
    let Some(thread_id) = reply["thread"]["id"].as_str().map(str::to_owned) else {
        return fail("Codex started no conversation".to_string());
    };
    let model = {
        let mut state = shared.state.lock().unwrap();
        state.thread_id = Some(thread_id.clone());
        if let Some(model) = reply["model"].as_str() {
            state.model = Some(model.to_string());
        }
        if state.effort.is_none() {
            state.effort = reply["reasoningEffort"].as_str().map(str::to_owned);
        }
        state.model.clone()
    };
    // No "idle" here: the message that started the process is already queued.
    shared.emit(AgentEvent::SessionStarted {
        agent_session_id: thread_id.clone(),
        model,
        mode: Some(mode),
    });

    while let Some(first) = items.recv().await {
        let mut batch = vec![first];
        tokio::time::sleep(BATCH_WINDOW).await;
        while let Ok(item) = items.try_recv() {
            batch.push(item);
        }
        let closing = batch.iter().any(|i| matches!(i, Item::Close));
        {
            let mut state = shared.state.lock().unwrap();
            for item in batch {
                if let Item::User { input, id } = item {
                    state.pending.push((id, input));
                }
            }
            if closing {
                state.closing = true;
            }
        }
        if closing {
            break;
        }
        next_turn(&shared, &thread_id).await;
    }
    // Closing stdin makes Codex exit.
    let _ = shared.rpc.tx.send(Out::Close);
}

/// What's waiting goes to Codex: into the running turn, or as a new one.
async fn next_turn(shared: &Arc<Shared>, thread_id: &str) {
    let (turn, messages, notes) = {
        let mut state = shared.state.lock().unwrap();
        // A plan waiting for its answer holds everything back.
        if state.waiting.values().any(|w| matches!(w, Waiting::Plan { .. })) {
            return;
        }
        let pending = std::mem::take(&mut state.pending);
        let messages: Vec<(String, Vec<Value>)> =
            pending.into_iter().filter(|(id, _)| state.queued.contains(id)).collect();
        (state.turn_id.clone(), messages, std::mem::take(&mut state.notes))
    };
    if messages.is_empty() && notes.is_empty() {
        return;
    }
    let mut input: Vec<Value> = notes.iter().map(|n| text_input(n)).collect();
    for (_, parts) in &messages {
        input.extend(parts.iter().cloned());
    }
    let ids: Vec<String> = messages.iter().map(|(id, _)| id.clone()).collect();

    // Mid-turn: Codex reads it while it works. If it can't take it now, it
    // waits for the turn to end.
    if let Some(turn_id) = turn {
        let steer = shared
            .rpc
            .call("turn/steer", json!({ "threadId": thread_id, "expectedTurnId": turn_id, "input": input }))
            .await;
        match steer {
            Ok(_) => seen(shared, ids),
            Err(_) => {
                let mut state = shared.state.lock().unwrap();
                let mut back = messages;
                back.append(&mut state.pending);
                state.pending = back;
                let mut kept = notes;
                kept.append(&mut state.notes);
                state.notes = kept;
                // The turn may have ended meanwhile: try again as a new one.
                if state.turn_id.is_none() {
                    let _ = shared.items.send(Item::Wake);
                }
            }
        }
        return;
    }

    // `/compact` alone asks Codex to compact the thread.
    let compact = notes.is_empty() && messages.len() == 1 && is_compact(&messages[0].1);
    if compact {
        shared.state.lock().unwrap().compacting = true;
        seen(shared, ids);
        working(shared);
        if let Err(e) = shared.rpc.call("thread/compact/start", json!({ "threadId": thread_id })).await {
            shared.state.lock().unwrap().compacting = false;
            failed_start(shared, e);
        }
        return;
    }

    let params = {
        let mut state = shared.state.lock().unwrap();
        state.errored = false;
        state.stopped = false;
        state.plan = None;
        state.turn_started.get_or_insert_with(Instant::now);
        let (approval, sandbox) = mode_policy(&state.mode, &state.writable_roots);
        let collaboration = if state.mode == "plan" { "plan" } else { "default" };
        let mut params = json!({
            "threadId": thread_id,
            "input": input,
            "approvalPolicy": approval,
            "sandboxPolicy": sandbox,
        });
        if let Some(model) = &state.model {
            params["model"] = json!(model);
            params["collaborationMode"] = json!({
                "mode": collaboration,
                "settings": { "model": model, "reasoning_effort": state.effort, "developer_instructions": null },
            });
        }
        if let Some(effort) = &state.effort {
            params["effort"] = json!(effort);
        }
        params
    };
    seen(shared, ids);
    working(shared);
    match shared.rpc.call("turn/start", params).await {
        Ok(reply) => {
            if let Some(id) = reply["turn"]["id"].as_str() {
                let mut state = shared.state.lock().unwrap();
                // Not if it already ended.
                if !state.done.contains(&format!("turn:{id}")) {
                    state.turn_id = Some(id.to_string());
                }
            }
        }
        Err(e) => failed_start(shared, e),
    }
}

fn text_input(text: &str) -> Value {
    json!({ "type": "text", "text": text, "text_elements": [] })
}

fn is_compact(input: &[Value]) -> bool {
    input.len() == 1 && input[0]["type"] == "text" && input[0]["text"].as_str().map(str::trim) == Some("/compact")
}

fn seen(shared: &Shared, ids: Vec<String>) {
    let mut state = shared.state.lock().unwrap();
    for id in &ids {
        state.queued.remove(id);
    }
    drop(state);
    for id in ids {
        shared.emit(AgentEvent::MessageSeen { id });
    }
}

fn working(shared: &Shared) {
    shared.emit(AgentEvent::Status {
        state: SessionState::Working,
    });
}

/// A turn that couldn't start: said so, and the chat is idle again.
fn failed_start(shared: &Shared, e: RpcError) {
    shared.emit(AgentEvent::Error {
        message: format!("Codex: {}", e.message),
    });
    finish_turn(shared, false);
}

/// The turn the user sees is over.
fn finish_turn(shared: &Shared, ok: bool) {
    let started = shared.state.lock().unwrap().turn_started.take();
    shared.emit(AgentEvent::TurnEnded {
        ok,
        duration_ms: started.map(|s| s.elapsed().as_millis() as u64),
    });
    shared.emit(AgentEvent::Status {
        state: SessionState::Idle,
    });
}

/// One line from Codex.
fn receive(shared: &Shared, v: &Value) {
    let method = v["method"].as_str();
    match (method, v.get("id")) {
        (None, Some(_)) => shared.rpc.settle(v),
        (Some(method), Some(id)) => request(shared, method, id, &v["params"]),
        (Some(method), None) => {
            for event in notification(shared, method, &v["params"]) {
                shared.emit(event);
            }
        }
        _ => {}
    }
}

/// Keeps a request until the user answers; returns the app's ID for it.
fn wait(state: &mut State, waiting: Waiting) -> String {
    state.next_request += 1;
    let id = format!("codex-{}", state.next_request);
    state.waiting.insert(id.clone(), waiting);
    id
}

/// Which tool call a thread's events go under: None for the chat's own
/// thread, the spawning call for a subagent's; Err for a thread not ours.
fn parent_of(state: &State, params: &Value) -> Result<Option<String>, ()> {
    let Some(thread) = params["threadId"].as_str() else {
        return Ok(None);
    };
    if state.thread_id.as_deref() == Some(thread) {
        return Ok(None);
    }
    match state.children.get(thread) {
        Some(child) => Ok(Some(child.parent_tool.clone())),
        None => Err(()),
    }
}

/// A request from Codex. Every one is answered, now or once the user has: an
/// unanswered request holds Codex up.
fn request(shared: &Shared, method: &str, rpc_id: &Value, params: &Value) {
    let mut events = Vec::new();
    {
        let mut state = shared.state.lock().unwrap();
        let parent = parent_of(&state, params).unwrap_or(None);
        let item_id = params["itemId"].as_str().unwrap_or_default().to_string();
        match method {
            "item/commandExecution/requestApproval" => {
                let command = plain_command(&params["command"], &params["commandActions"]);
                let (name, input) = command_tool(&command, &params["commandActions"]);
                events.extend(show(&mut state, &item_id, &name, input.clone(), parent));
                let decisions = decisions(params);
                let always_label = always_label(&decisions);
                let request_id = wait(&mut state, Waiting::Approval { rpc_id: rpc_id.clone(), decisions });
                let mut input = input;
                if let Some(reason) = params["reason"].as_str() {
                    input["description"] = json!(reason);
                }
                events.push(AgentEvent::ApprovalRequest {
                    request_id,
                    approval: ApprovalKind::Tool,
                    tool_name: name,
                    title: command,
                    input,
                    tool_id: Some(item_id),
                    always_label,
                });
            }
            "item/fileChange/requestApproval" => {
                let decisions = decisions(params);
                let always_label = always_label(&decisions);
                let request_id = wait(&mut state, Waiting::Approval { rpc_id: rpc_id.clone(), decisions });
                let mut input = json!({});
                if let Some(reason) = params["reason"].as_str() {
                    input["description"] = json!(reason);
                }
                events.push(AgentEvent::ApprovalRequest {
                    request_id,
                    approval: ApprovalKind::Tool,
                    tool_name: "Edit".to_string(),
                    title: params["reason"].as_str().unwrap_or("Change files").to_string(),
                    input,
                    tool_id: state.shown.contains(&change_id(&item_id, 0)).then(|| change_id(&item_id, 0)),
                    always_label,
                });
            }
            "item/permissions/requestApproval" => {
                let reason = params["reason"].as_str().unwrap_or("More access for the sandbox").to_string();
                let request_id = wait(
                    &mut state,
                    Waiting::Permissions { rpc_id: rpc_id.clone(), permissions: params["permissions"].clone() },
                );
                events.push(AgentEvent::ApprovalRequest {
                    request_id,
                    approval: ApprovalKind::Tool,
                    tool_name: "Permissions".to_string(),
                    title: reason,
                    input: json!({ "permissions": params["permissions"] }),
                    tool_id: None,
                    always_label: Some("Allow for this chat".to_string()),
                });
            }
            "mcpServer/elicitation/request" => {
                let server = params["serverName"].as_str().unwrap_or_default();
                // The app's own tools never ask, as with Claude Code.
                if shared.own_tools.iter().any(|s| s == server) {
                    shared.rpc.respond(rpc_id, json!({ "action": "accept", "content": {}, "_meta": null }));
                    return;
                }
                let meta = &params["_meta"];
                let tool = meta["tool_name"].as_str().unwrap_or_default();
                let request_id = wait(&mut state, Waiting::Elicitation { rpc_id: rpc_id.clone() });
                events.push(AgentEvent::ApprovalRequest {
                    request_id,
                    approval: ApprovalKind::Tool,
                    tool_name: format!("mcp__{server}__{tool}"),
                    title: params["message"].as_str().unwrap_or("Allow this tool call?").to_string(),
                    input: meta["tool_params"].clone(),
                    tool_id: None,
                    always_label: None,
                });
            }
            "item/tool/requestUserInput" => {
                let mut questions = Vec::new();
                let mut shown = Vec::new();
                for q in params["questions"].as_array().into_iter().flatten() {
                    let text = q["question"].as_str().unwrap_or_default().to_string();
                    let options: Vec<Value> = q["options"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|o| json!({ "label": o["label"], "description": o["description"].as_str().unwrap_or_default() }))
                        .collect();
                    shown.push(json!({
                        "question": text,
                        "header": q["header"].as_str().unwrap_or_default(),
                        "multiSelect": false,
                        "options": options,
                    }));
                    questions.push((q["id"].as_str().unwrap_or_default().to_string(), text));
                }
                let request_id = wait(&mut state, Waiting::Question { rpc_id: rpc_id.clone(), questions });
                events.push(AgentEvent::ApprovalRequest {
                    request_id,
                    approval: ApprovalKind::Question,
                    tool_name: "AskUserQuestion".to_string(),
                    title: "Questions".to_string(),
                    input: json!({ "questions": shown }),
                    tool_id: None,
                    always_label: None,
                });
            }
            _ => {
                shared.rpc.respond_error(rpc_id, -32601, &format!("Method not found: {method}"));
                return;
            }
        }
    }
    for event in events {
        shared.emit(event);
    }
    shared.emit(AgentEvent::Status {
        state: SessionState::WaitingApproval,
    });
}

/// What Codex offers as answers; its full set when it doesn't say.
fn decisions(params: &Value) -> Vec<Value> {
    match params["availableDecisions"].as_array() {
        Some(list) if !list.is_empty() => list.clone(),
        _ => vec![json!("accept"), json!("acceptForSession"), json!("decline"), json!("cancel")],
    }
}

/// What "always allow" would do, if Codex offers a way to remember.
fn always_label(decisions: &[Value]) -> Option<String> {
    if decisions.iter().any(|d| d == "acceptForSession") {
        return Some("Allow for this chat".to_string());
    }
    let rule = decisions.iter().find_map(|d| d["acceptWithExecpolicyAmendment"]["execpolicy_amendment"].as_array())?;
    let words: Vec<&str> = rule.iter().filter_map(Value::as_str).collect();
    // `["/bin/zsh", "-lc", "<command>"]`: the command is what's remembered.
    let command = match words.as_slice() {
        [_, flag, command] if flag.starts_with('-') => command.to_string(),
        words => words.join(" "),
    };
    Some(format!("Always allow {command}"))
}

/// The answer to send for the user's decision, from what Codex offers.
fn pick_decision(decisions: &[Value], decision: Decision) -> Value {
    let has = |name: &str| decisions.iter().any(|d| d == name);
    match decision {
        Decision::Allow => json!("accept"),
        Decision::AlwaysAllow => {
            if has("acceptForSession") {
                json!("acceptForSession")
            } else {
                decisions
                    .iter()
                    .find(|d| d.get("acceptWithExecpolicyAmendment").is_some())
                    .cloned()
                    .unwrap_or_else(|| json!("accept"))
            }
        }
        Decision::Deny => json!(if has("decline") { "decline" } else { "cancel" }),
    }
}

/// A command as it was asked for, without the shell around it: what
/// Codex's own parse says, else the shell's argument.
fn plain_command(command: &Value, actions: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    for action in actions.as_array().into_iter().flatten() {
        if let Some(c) = action["command"].as_str() {
            if !parts.iter().any(|p| p == c) {
                parts.push(c.to_string());
            }
        }
    }
    if !parts.is_empty() {
        return parts.join(" && ");
    }
    let full = command.as_str().unwrap_or_default();
    // `/bin/zsh -lc '<command>'`
    match full.split_once(" -lc ") {
        Some((_, rest)) => {
            let rest = rest.trim();
            let quoted = rest.len() >= 2 && (rest.starts_with('\'') && rest.ends_with('\'') || rest.starts_with('"') && rest.ends_with('"'));
            if quoted { rest[1..rest.len() - 1].to_string() } else { rest.to_string() }
        }
        None => full.to_string(),
    }
}

/// A command's tool name and input, as the window words Claude Code's
/// (`tools.ts`): a plain read is "Read", a search "Grep", the rest "Bash".
fn command_tool(command: &str, actions: &Value) -> (String, Value) {
    let actions: Vec<&Value> = actions.as_array().into_iter().flatten().collect();
    if let [action] = actions.as_slice() {
        match action["type"].as_str() {
            Some("read") if action["path"].is_string() => {
                return ("Read".to_string(), json!({ "file_path": action["path"] }));
            }
            Some("search") if action["query"].is_string() => {
                return ("Grep".to_string(), json!({ "pattern": action["query"], "path": action["path"] }));
            }
            _ => {}
        }
    }
    ("Bash".to_string(), json!({ "command": command }))
}

/// A file change item has one tool call per file.
fn change_id(item_id: &str, index: usize) -> String {
    if index == 0 { item_id.to_string() } else { format!("{item_id}#{index}") }
}

/// A tool call's block, once.
fn show(state: &mut State, tool_id: &str, name: &str, input: Value, parent: Option<String>) -> Option<AgentEvent> {
    if !state.shown.insert(tool_id.to_string()) {
        return None;
    }
    Some(AgentEvent::AssistantBlock {
        message_id: format!("tool-{tool_id}"),
        block: Block::ToolCall {
            tool_id: tool_id.to_string(),
            name: name.to_string(),
            input,
        },
        parent_tool_id: parent,
    })
}

/// A tool call's result, once.
fn result(state: &mut State, tool_id: &str, text: String, is_error: bool, parent: Option<String>) -> Option<AgentEvent> {
    if !state.shown.contains(tool_id) || !state.done.insert(tool_id.to_string()) {
        return None;
    }
    Some(AgentEvent::ToolResult {
        tool_id: tool_id.to_string(),
        content: json!(text),
        is_error,
        parent_tool_id: parent,
    })
}

/// An item's tool calls: what was called, and its input.
fn item_tools(item: &Value) -> Vec<(String, String, Value)> {
    let id = item["id"].as_str().unwrap_or_default();
    match item["type"].as_str().unwrap_or_default() {
        "commandExecution" => {
            let command = plain_command(&item["command"], &item["commandActions"]);
            let (name, input) = command_tool(&command, &item["commandActions"]);
            vec![(id.to_string(), name, input)]
        }
        "fileChange" => item["changes"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, change)| {
                let path = change["path"].as_str().unwrap_or_default();
                let (name, input) = match change["kind"]["type"].as_str() {
                    Some("add") => ("Write", json!({ "file_path": path })),
                    Some("delete") => ("Bash", json!({ "command": format!("rm {path}") })),
                    _ => ("Edit", json!({ "file_path": path })),
                };
                (change_id(id, i), name.to_string(), input)
            })
            .collect(),
        "mcpToolCall" => {
            let name = format!(
                "mcp__{}__{}",
                item["server"].as_str().unwrap_or_default(),
                item["tool"].as_str().unwrap_or_default()
            );
            vec![(id.to_string(), name, item["arguments"].clone())]
        }
        "webSearch" => vec![(id.to_string(), "WebSearch".to_string(), json!({ "query": item["query"] }))],
        "imageView" => vec![(id.to_string(), "Read".to_string(), json!({ "file_path": item["path"] }))],
        "collabAgentToolCall" if item["tool"] == "spawnAgent" => {
            let prompt = item["prompt"].as_str().unwrap_or_default();
            let description: String = prompt.lines().next().unwrap_or_default().chars().take(80).collect();
            vec![(id.to_string(), "Agent".to_string(), json!({ "description": description, "prompt": prompt }))]
        }
        _ => Vec::new(),
    }
}

/// A finished item's results, one per tool call.
fn item_results(item: &Value) -> Vec<(String, String, bool)> {
    let id = item["id"].as_str().unwrap_or_default();
    let failed = matches!(item["status"].as_str(), Some("failed") | Some("declined"));
    match item["type"].as_str().unwrap_or_default() {
        "commandExecution" => {
            let output = item["aggregatedOutput"].as_str().unwrap_or_default().to_string();
            let is_error = failed || item["exitCode"].as_i64().is_some_and(|c| c != 0);
            vec![(id.to_string(), output, is_error)]
        }
        "fileChange" => item["changes"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, change)| (change_id(id, i), change["diff"].as_str().unwrap_or_default().to_string(), failed))
            .collect(),
        "mcpToolCall" => {
            let text = if let Some(error) = item["error"].as_object() {
                error.get("message").and_then(Value::as_str).unwrap_or("failed").to_string()
            } else {
                let texts: Vec<&str> = item["result"]["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["text"].as_str())
                    .collect();
                texts.join("\n")
            };
            vec![(id.to_string(), text, failed || !item["error"].is_null())]
        }
        "webSearch" | "imageView" => vec![(id.to_string(), String::new(), failed)],
        "collabAgentToolCall" if item["tool"] == "spawnAgent" => {
            vec![(id.to_string(), "Started".to_string(), failed)]
        }
        _ => Vec::new(),
    }
}

/// Codex's rate-limit windows as the app's usage readings, named for how
/// long each one runs ("5h", "week", "month").
fn usage_events(limits: &Value) -> Vec<AgentEvent> {
    ["primary", "secondary"]
        .iter()
        .filter_map(|key| {
            let window = &limits[key];
            let used = window["usedPercent"].as_f64()?;
            Some(AgentEvent::UsageUpdate {
                window: Some(window_name(window["windowDurationMins"].as_u64())),
                utilization: (used / 100.0).clamp(0.0, 1.0),
                resets_at: window["resetsAt"].as_u64(),
            })
        })
        .collect()
}

fn window_name(minutes: Option<u64>) -> String {
    match minutes {
        Some(m) if m >= 1440 * 28 => "month".to_string(),
        Some(m) if m >= 1440 * 7 => "week".to_string(),
        Some(m) if m >= 1440 && m % 1440 == 0 => format!("{} days", m / 1440),
        Some(m) if m >= 60 => format!("{}h", m / 60),
        Some(m) => format!("{m}m"),
        None => "limit".to_string(),
    }
}

/// The running subagents, for the chat's background count.
fn background(state: &State) -> AgentEvent {
    let tasks: Vec<BackgroundTask> = state
        .children
        .iter()
        .filter(|(_, c)| !c.ended)
        .map(|(id, c)| BackgroundTask { id: id.clone(), kind: "agent".to_string(), description: c.parent_tool.clone() })
        .collect();
    let running = tasks.len() as u32;
    AgentEvent::BackgroundWork { running, agents: running, tasks }
}

/// A subagent thread started under the call `tool_id`.
fn child_started(state: &mut State, thread: &str, tool_id: &str, item: &Value, out: &mut Vec<AgentEvent>) {
    if state.children.contains_key(thread) {
        return;
    }
    state.children.insert(thread.to_string(), Child { parent_tool: tool_id.to_string(), ended: false });
    let prompt = item["prompt"].as_str().unwrap_or_default();
    out.push(AgentEvent::TaskStarted {
        task_id: thread.to_string(),
        tool_id: Some(tool_id.to_string()),
        description: prompt.lines().next().unwrap_or_default().chars().take(80).collect(),
        agent_type: item["agentRole"].as_str().map(str::to_owned).or(Some("agent".to_string())),
        background: true,
    });
    out.push(background(state));
}

fn child_ended(state: &mut State, thread: &str, status: &str, out: &mut Vec<AgentEvent>) {
    let Some(child) = state.children.get_mut(thread) else { return };
    if child.ended {
        return;
    }
    child.ended = true;
    let tool_id = child.parent_tool.clone();
    out.push(AgentEvent::TaskEnded {
        task_id: thread.to_string(),
        tool_id: Some(tool_id),
        status: status.to_string(),
        summary: None,
        duration_ms: None,
    });
    out.push(background(state));
}

/// A subagent's state as the app's task status, once it's over.
fn ended_status(status: &str) -> Option<&'static str> {
    match status {
        "completed" | "shutdown" => Some("completed"),
        "errored" | "notFound" => Some("failed"),
        "interrupted" => Some("stopped"),
        _ => None,
    }
}

/// One notification into the app's events.
fn notification(shared: &Shared, method: &str, params: &Value) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    let mut state = shared.state.lock().unwrap();
    let Ok(parent) = parent_of(&state, params) else {
        return out;
    };
    match method {
        "item/agentMessage/delta" if parent.is_none() => {
            if let (Some(id), Some(delta)) = (params["itemId"].as_str(), params["delta"].as_str()) {
                out.push(AgentEvent::TextDelta {
                    message_id: id.to_string(),
                    block_index: 0,
                    text: delta.to_string(),
                });
            }
        }
        "item/started" => {
            let item = &params["item"];
            if item["type"] == "contextCompaction" {
                out.push(AgentEvent::Compacting { active: true });
            }
            for (id, name, input) in item_tools(item) {
                out.extend(show(&mut state, &id, &name, input, parent.clone()));
            }
        }
        "item/completed" => {
            let item = &params["item"];
            let id = item["id"].as_str().unwrap_or_default().to_string();
            match item["type"].as_str().unwrap_or_default() {
                "agentMessage" => {
                    if let Some(text) = item["text"].as_str().filter(|t| !t.trim().is_empty()) {
                        out.push(AgentEvent::AssistantBlock {
                            message_id: id.clone(),
                            block: Block::Text { text: text.to_string() },
                            parent_tool_id: parent.clone(),
                        });
                    }
                }
                "reasoning" if parent.is_none() => {
                    let parts: Vec<&str> = item["summary"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                    let text = parts.join("\n\n");
                    if !text.trim().is_empty() {
                        out.push(AgentEvent::AssistantBlock {
                            message_id: id.clone(),
                            block: Block::Thinking { text },
                            parent_tool_id: None,
                        });
                    }
                }
                "plan" if parent.is_none() => {
                    if let Some(text) = item["text"].as_str().filter(|t| !t.trim().is_empty()) {
                        state.plan = Some((id.clone(), text.to_string()));
                    }
                }
                "contextCompaction" => {
                    let manual = std::mem::take(&mut state.compacting);
                    out.push(AgentEvent::Compacting { active: false });
                    out.push(AgentEvent::Compacted { auto: !manual });
                }
                "subAgentActivity" => {
                    if let Some(thread) = item["agentThreadId"].as_str() {
                        match item["kind"].as_str() {
                            Some("completed") => child_ended(&mut state, thread, "completed", &mut out),
                            Some("interrupted") => child_ended(&mut state, thread, "stopped", &mut out),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            // A call shown only now (it came finished) still gets its block.
            for (tool_id, name, input) in item_tools(item) {
                out.extend(show(&mut state, &tool_id, &name, input, parent.clone()));
            }
            for (tool_id, text, is_error) in item_results(item) {
                out.extend(result(&mut state, &tool_id, text, is_error, parent.clone()));
            }
            if item["type"] == "collabAgentToolCall" {
                let receivers: Vec<String> = item["receiverThreadIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t.as_str().map(str::to_owned))
                    .collect();
                if item["tool"] == "spawnAgent" {
                    for thread in &receivers {
                        child_started(&mut state, thread, &id, item, &mut out);
                    }
                }
                if let Some(states) = item["agentsStates"].as_object() {
                    for (thread, s) in states {
                        if let Some(status) = s["status"].as_str().and_then(ended_status) {
                            child_ended(&mut state, thread, status, &mut out);
                        }
                    }
                }
            }
        }
        "turn/plan/updated" if parent.is_none() => {
            let todos: Vec<Value> = params["plan"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|step| {
                    let status = match step["status"].as_str() {
                        Some("inProgress") => "in_progress",
                        Some("completed") => "completed",
                        _ => "pending",
                    };
                    json!({ "content": step["step"], "status": status, "activeForm": step["step"] })
                })
                .collect();
            if !todos.is_empty() {
                state.todos += 1;
                let tool_id = format!("todo-{}-{}", params["turnId"].as_str().unwrap_or_default(), state.todos);
                out.extend(show(&mut state, &tool_id, "TodoWrite", json!({ "todos": todos }), None));
                out.extend(result(&mut state, &tool_id, String::new(), false, None));
            }
        }
        "thread/tokenUsage/updated" if parent.is_none() => {
            let usage = &params["tokenUsage"];
            if let (Some(used), Some(max)) = (usage["last"]["totalTokens"].as_u64(), usage["modelContextWindow"].as_u64()) {
                out.push(AgentEvent::ContextUsage { used, max, parts: Vec::new() });
            }
        }
        "account/rateLimits/updated" => out.extend(usage_events(&params["rateLimits"])),
        "error" => {
            if params["willRetry"] == true {
                return out;
            }
            let error = &params["error"];
            let message = error["message"].as_str().unwrap_or("Codex hit an error").to_string();
            let info = &error["codexErrorInfo"];
            let unauthorized = info == "unauthorized" || info.get("unauthorized").is_some();
            if parent.is_none() {
                state.errored = true;
            }
            if unauthorized {
                out.push(AgentEvent::SignInNeeded { message });
                drop(state);
                shared.stop();
                return out;
            }
            out.push(AgentEvent::Error { message: format!("Codex: {message}") });
        }
        "turn/started" if parent.is_none() => {
            if let Some(id) = params["turn"]["id"].as_str() {
                state.turn_id = Some(id.to_string());
            }
        }
        "turn/completed" => {
            let turn = &params["turn"];
            // A subagent's turn ending ends its task.
            if parent.is_some() {
                if let Some(thread) = params["threadId"].as_str() {
                    let status = match turn["status"].as_str() {
                        Some("failed") => "failed",
                        Some("interrupted") => "stopped",
                        _ => "completed",
                    };
                    child_ended(&mut state, thread, status, &mut out);
                }
                return out;
            }
            if let Some(id) = turn["id"].as_str() {
                state.done.insert(format!("turn:{id}"));
            }
            state.turn_id = None;
            state.compacting = false;
            let status = turn["status"].as_str().unwrap_or_default();
            let stopped = status == "interrupted" || std::mem::take(&mut state.stopped);
            if status == "failed" && !state.errored {
                let message = turn["error"]["message"].as_str().unwrap_or("the turn failed");
                out.push(AgentEvent::Error { message: format!("Codex: {message}") });
            }
            let ok = status == "completed" && !state.errored;
            // Nothing Codex asked can still be waiting once its turn is over.
            let left: Vec<String> = state
                .waiting
                .iter()
                .filter(|(_, w)| !matches!(w, Waiting::Plan { .. }))
                .map(|(id, _)| id.clone())
                .collect();
            for request_id in left {
                if let Some(waiting) = state.waiting.remove(&request_id) {
                    waiting.cancel(&shared.rpc);
                }
                out.push(AgentEvent::ApprovalCancelled { request_id });
            }
            // A plan: shown for approval; the turn the user sees goes on.
            if let Some((tool_id, plan)) = state.plan.take().filter(|_| ok && !stopped && state.mode == "plan") {
                out.extend(show(&mut state, &tool_id, "ExitPlanMode", json!({ "plan": plan }), None));
                let request_id = wait(&mut state, Waiting::Plan { tool_id: tool_id.clone() });
                out.push(AgentEvent::ApprovalRequest {
                    request_id,
                    approval: ApprovalKind::Plan,
                    tool_name: "ExitPlanMode".to_string(),
                    title: plan_title(&plan),
                    input: json!({ "plan": plan }),
                    tool_id: Some(tool_id),
                    always_label: None,
                });
                out.push(AgentEvent::Status {
                    state: SessionState::WaitingApproval,
                });
                return out;
            }
            // Messages or notes left: they go on at once, as one turn to the
            // user. Not after a stop or a failure.
            let more = !state.pending.is_empty() || !state.notes.is_empty();
            if more && ok && !stopped && !state.closing {
                let _ = shared.items.send(Item::Wake);
                return out;
            }
            state.notes.clear();
            let started = state.turn_started.take();
            out.push(AgentEvent::TurnEnded {
                ok,
                duration_ms: started.map(|s| s.elapsed().as_millis() as u64),
            });
            out.push(AgentEvent::Status {
                state: SessionState::Idle,
            });
            // Messages sent mid-turn that Codex couldn't take start the next turn.
            if !state.pending.is_empty() && !state.closing {
                let _ = shared.items.send(Item::Wake);
            }
        }
        _ => {}
    }
    out
}

/// A plan's first heading, for the card's title.
fn plan_title(plan: &str) -> String {
    plan.lines()
        .find_map(|l| l.strip_prefix('#').map(|h| h.trim_start_matches('#').trim().to_string()))
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "Plan".to_string())
}

impl RunningAgent for CodexProcess {
    fn send_user_message(&self, text: &str, images: &[ImagePayload], id: &str) -> Result<(), String> {
        let mut input: Vec<Value> = images
            .iter()
            .map(|i| json!({ "type": "image", "url": format!("data:{};base64,{}", i.media_type, i.data_b64) }))
            .collect();
        input.push(text_input(text));
        // Before sending: the driver drops a message it finds missing here.
        self.shared.state.lock().unwrap().queued.insert(id.to_string());
        if self.shared.items.send(Item::User { input, id: id.to_string() }).is_err() {
            self.shared.state.lock().unwrap().queued.remove(id);
            return Err("Codex is no longer running".to_string());
        }
        working(&self.shared);
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
        match waiting {
            Waiting::Approval { rpc_id, decisions } => {
                rpc.respond(&rpc_id, json!({ "decision": pick_decision(&decisions, decision) }));
                notes.extend(message.filter(|_| !allow));
            }
            Waiting::Permissions { rpc_id, permissions } => {
                let answer = match decision {
                    Decision::Allow => json!({ "permissions": permissions, "scope": "turn" }),
                    Decision::AlwaysAllow => json!({ "permissions": permissions, "scope": "session" }),
                    Decision::Deny => json!({ "permissions": {}, "scope": "turn" }),
                };
                rpc.respond(&rpc_id, answer);
                notes.extend(message.filter(|_| !allow));
            }
            Waiting::Elicitation { rpc_id } => {
                let action = if allow { "accept" } else { "decline" };
                rpc.respond(&rpc_id, json!({ "action": action, "content": if allow { json!({}) } else { Value::Null }, "_meta": null }));
                notes.extend(message.filter(|_| !allow));
            }
            Waiting::Question { rpc_id, questions } => {
                let answers = answers.unwrap_or_default();
                let picked: serde_json::Map<String, Value> = questions
                    .iter()
                    .filter(|_| allow)
                    .filter_map(|(id, text)| {
                        let chosen: Vec<&str> = answers.get(text)?.split(", ").collect();
                        Some((id.clone(), json!({ "answers": chosen })))
                    })
                    .collect();
                rpc.respond(&rpc_id, json!({ "answers": picked }));
                notes.extend(message.filter(|_| !allow));
            }
            Waiting::Plan { tool_id } => {
                let mut state = self.shared.state.lock().unwrap();
                let (text, events) = if allow {
                    // Planning is over: build in the default mode, from the
                    // plan as the user left it.
                    state.mode = DEFAULT_MODE.to_string();
                    notes.push(match plan {
                        Some(plan) => format!(
                            "The user approved the plan after editing it. Build it now, from this version, \
not your original:\n\n{plan}"
                        ),
                        None => "The user approved the plan. Build it now.".to_string(),
                    });
                    ("The user approved the plan.".to_string(), vec![AgentEvent::ModeChanged { mode: DEFAULT_MODE.to_string() }])
                } else {
                    let message = message.unwrap_or_else(|| "Keep planning.".to_string());
                    notes.push(format!("The user didn't approve the plan yet. What they said: {message}"));
                    (format!("Not approved: {message}"), Vec::new())
                };
                let done = result(&mut state, &tool_id, text, !allow, None);
                drop(state);
                for event in events.into_iter().chain(done) {
                    self.shared.emit(event);
                }
            }
        }
        if !notes.is_empty() {
            self.shared.state.lock().unwrap().notes.extend(notes);
            let _ = self.shared.items.send(Item::Wake);
        }
        working(&self.shared);
        Ok(())
    }

    fn open_requests(&self) -> Vec<String> {
        self.shared.state.lock().unwrap().waiting.keys().cloned().collect()
    }

    fn withdraw_message(&self, id: &str) -> Result<(), String> {
        let ok = {
            let mut state = self.shared.state.lock().unwrap();
            state.pending.retain(|(pending, _)| pending != id);
            state.queued.remove(id)
        };
        self.shared.emit(AgentEvent::MessageWithdrawn { id: id.to_string(), ok });
        Ok(())
    }

    fn interrupt(&self) -> Result<(), String> {
        let (thread, turn, left, plan_waiting) = {
            let mut state = self.shared.state.lock().unwrap();
            state.stopped = true;
            state.notes.clear();
            let left: Vec<(String, Waiting)> = state.waiting.drain().collect();
            let plan_waiting = left.iter().any(|(_, w)| matches!(w, Waiting::Plan { .. }));
            (state.thread_id.clone(), state.turn_id.clone(), left, plan_waiting)
        };
        // Codex doesn't withdraw what it asked; the app does.
        for (request_id, waiting) in left {
            waiting.cancel(&self.shared.rpc);
            self.shared.emit(AgentEvent::ApprovalCancelled { request_id });
        }
        match (thread, turn) {
            (Some(thread), Some(turn)) => {
                let shared = self.shared.clone();
                tauri::async_runtime::spawn(async move {
                    // A process being shut down (a worktree move, say) is stopped anyway.
                    let reply = shared.rpc.call("turn/interrupt", json!({ "threadId": thread, "turnId": turn })).await;
                    match reply {
                        Err(e) if e.message != RpcError::gone().message => {
                            shared.emit(AgentEvent::Error { message: format!("Codex couldn't stop: {}", e.message) })
                        }
                        _ => {}
                    }
                });
            }
            // Only a plan was waiting: no turn of Codex's to stop.
            _ if plan_waiting => {
                self.shared.state.lock().unwrap().stopped = false;
                finish_turn(&self.shared, true);
            }
            _ => return Err("Nothing to stop".to_string()),
        }
        Ok(())
    }

    fn set_mode(&self, mode: &str) -> Result<(), String> {
        if !MODES.contains(&mode) {
            return Err(format!("Codex has no mode {mode}"));
        }
        self.shared.state.lock().unwrap().mode = mode.to_string();
        Ok(())
    }

    fn set_model(&self, model: &str) -> Result<(), String> {
        let mut state = self.shared.state.lock().unwrap();
        if model != "default" {
            state.model = Some(model.to_string());
        }
        Ok(())
    }

    fn set_effort(&self, effort: &str) -> Result<(), String> {
        self.shared.state.lock().unwrap().effort = Some(effort.to_string()).filter(|e| e != "default");
        Ok(())
    }

    fn close(&self) {
        let _ = self.shared.items.send(Item::Close);
    }

    fn stop(&self) {
        self.shared.stop();
    }
}

impl Drop for CodexProcess {
    fn drop(&mut self) {
        // A dropped handle (a deleted chat) ends the process.
        let _ = self.shared.items.send(Item::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rig {
        shared: Arc<Shared>,
        out: mpsc::UnboundedReceiver<Out>,
        items: mpsc::UnboundedReceiver<Item>,
        seen: Arc<Mutex<Vec<AgentEvent>>>,
    }

    /// A connection with no process: what it writes and reports are kept.
    fn rig() -> Rig {
        let (tx, out) = mpsc::unbounded_channel();
        let (items_tx, items) = mpsc::unbounded_channel();
        let seen: Arc<Mutex<Vec<AgentEvent>>> = Arc::default();
        let sink: EventSink = {
            let seen = seen.clone();
            Arc::new(move |e| seen.lock().unwrap().push(e))
        };
        let shared = Arc::new(Shared {
            rpc: Rpc::new(tx, false),
            state: Mutex::new(State {
                thread_id: Some("t1".into()),
                mode: DEFAULT_MODE.into(),
                ..State::default()
            }),
            sink,
            items: items_tx,
            stop_tx: Mutex::new(None),
            own_tools: vec!["harness_browser".into()],
        });
        Rig { shared, out, items, seen }
    }

    impl Rig {
        fn feed(&self, v: Value) {
            receive(&self.shared, &v);
        }
        fn events(&self) -> Vec<AgentEvent> {
            std::mem::take(&mut *self.seen.lock().unwrap())
        }
        fn sent(&mut self) -> Vec<Value> {
            let mut sent = Vec::new();
            while let Ok(Out::Line(line)) = self.out.try_recv() {
                sent.push(serde_json::from_str(&line).unwrap());
            }
            sent
        }
        fn woken(&mut self) -> bool {
            let mut woken = false;
            while let Ok(item) = self.items.try_recv() {
                woken |= matches!(item, Item::Wake);
            }
            woken
        }
        fn process(&self) -> CodexProcess {
            CodexProcess { shared: self.shared.clone() }
        }
        fn request_id(&self, events: &[AgentEvent]) -> String {
            events
                .iter()
                .find_map(|e| match e {
                    AgentEvent::ApprovalRequest { request_id, .. } => Some(request_id.clone()),
                    _ => None,
                })
                .expect("an approval request")
        }
    }

    fn note(method: &str, params: Value) -> Value {
        json!({ "method": method, "params": params })
    }

    #[test]
    fn text_streams_then_lands_as_a_block() {
        let r = rig();
        r.feed(note("item/agentMessage/delta", json!({ "threadId": "t1", "itemId": "m1", "delta": "Hel" })));
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": { "type": "agentMessage", "id": "m1", "text": "Hello" } })));
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": { "type": "reasoning", "id": "r1", "summary": ["Thinking it over"] } })));
        // Another thread's lines (not this chat's, nor a subagent's) are ignored.
        r.feed(note("item/agentMessage/delta", json!({ "threadId": "other", "itemId": "m9", "delta": "x" })));
        let events = r.events();
        assert_eq!(
            events,
            vec![
                AgentEvent::TextDelta { message_id: "m1".into(), block_index: 0, text: "Hel".into() },
                AgentEvent::AssistantBlock {
                    message_id: "m1".into(),
                    block: Block::Text { text: "Hello".into() },
                    parent_tool_id: None
                },
                AgentEvent::AssistantBlock {
                    message_id: "r1".into(),
                    block: Block::Thinking { text: "Thinking it over".into() },
                    parent_tool_id: None
                },
            ]
        );
    }

    #[test]
    fn a_command_asks_and_each_answer_maps_to_what_codex_offers() {
        let mut r = rig();
        let ask = json!({
            "id": 7,
            "method": "item/commandExecution/requestApproval",
            "params": {
                "threadId": "t1", "turnId": "u1", "itemId": "exec-1",
                "command": "/bin/zsh -lc 'touch $HOME/x'",
                "commandActions": [{ "type": "unknown", "command": "touch $HOME/x" }],
                "reason": "Outside the project",
                "availableDecisions": ["accept", { "acceptWithExecpolicyAmendment": { "execpolicy_amendment": ["/bin/zsh", "-lc", "touch $HOME/x"] } }, "cancel"],
            },
        });
        r.feed(ask.clone());
        let events = r.events();
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { tool_id, name, input }, .. }
            if tool_id == "exec-1" && name == "Bash" && input["command"] == "touch $HOME/x"));
        match &events[1] {
            AgentEvent::ApprovalRequest { approval, title, always_label, tool_id, .. } => {
                assert_eq!(*approval, ApprovalKind::Tool);
                assert_eq!(title, "touch $HOME/x");
                assert_eq!(always_label.as_deref(), Some("Always allow touch $HOME/x"));
                assert_eq!(tool_id.as_deref(), Some("exec-1"));
            }
            other => panic!("expected a card, got {other:?}"),
        }
        let id = r.request_id(&events);
        r.process().answer_approval(&id, Decision::AlwaysAllow, None, None, None).unwrap();
        assert_eq!(r.sent()[0]["result"]["decision"]["acceptWithExecpolicyAmendment"]["execpolicy_amendment"][2], "touch $HOME/x");

        // No "decline" offered: a denial cancels, and its reason waits for the next turn.
        r.feed(ask.clone());
        let id = r.request_id(&r.events());
        r.process().answer_approval(&id, Decision::Deny, Some("not there".into()), None, None).unwrap();
        assert_eq!(r.sent()[0]["result"]["decision"], "cancel");
        assert_eq!(r.shared.state.lock().unwrap().notes, vec!["not there".to_string()]);
        assert!(r.woken());
        r.events();

        // The item starting after the answer doesn't show the call twice; it finishing gives the result.
        r.feed(note("item/started", json!({ "threadId": "t1", "item": { "type": "commandExecution", "id": "exec-1", "command": "x", "commandActions": [{ "type": "unknown", "command": "touch $HOME/x" }] } })));
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": { "type": "commandExecution", "id": "exec-1", "command": "x", "commandActions": [{ "type": "unknown", "command": "touch $HOME/x" }], "aggregatedOutput": null, "exitCode": 1, "status": "failed" } })));
        assert_eq!(
            r.events(),
            vec![AgentEvent::ToolResult { tool_id: "exec-1".into(), content: json!(""), is_error: true, parent_tool_id: None }]
        );
    }

    #[test]
    fn file_changes_are_one_call_per_file() {
        let r = rig();
        let item = json!({ "type": "fileChange", "id": "fc", "status": "completed", "changes": [
            { "path": "/p/new.txt", "kind": { "type": "add" }, "diff": "hello\n" },
            { "path": "/p/old.txt", "kind": { "type": "update", "move_path": null }, "diff": "@@ -1 +1 @@" },
        ] });
        r.feed(note("item/started", json!({ "threadId": "t1", "item": item })));
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": item })));
        let events = r.events();
        let calls: Vec<(String, String, Value)> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::AssistantBlock { block: Block::ToolCall { tool_id, name, input }, .. } => {
                    Some((tool_id.clone(), name.clone(), input["file_path"].clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            calls,
            vec![
                ("fc".into(), "Write".into(), json!("/p/new.txt")),
                ("fc#1".into(), "Edit".into(), json!("/p/old.txt")),
            ]
        );
        assert!(events.contains(&AgentEvent::ToolResult { tool_id: "fc#1".into(), content: json!("@@ -1 +1 @@"), is_error: false, parent_tool_id: None }));
    }

    #[test]
    fn the_apps_own_tools_never_ask() {
        let mut r = rig();
        r.feed(json!({ "id": 3, "method": "mcpServer/elicitation/request", "params": { "threadId": "t1", "serverName": "harness_browser", "mode": "form", "message": "Allow?", "_meta": { "tool_name": "open" } } }));
        assert!(r.events().is_empty());
        assert_eq!(r.sent()[0]["result"]["action"], "accept");
        // Another server's tool asks with a card.
        r.feed(json!({ "id": 4, "method": "mcpServer/elicitation/request", "params": { "threadId": "t1", "serverName": "github", "mode": "form", "message": "Allow the github MCP server to run tool \"merge\"?", "_meta": { "tool_name": "merge", "tool_params": { "pr": 1 } } } }));
        let events = r.events();
        assert!(matches!(&events[0], AgentEvent::ApprovalRequest { tool_name, .. } if tool_name == "mcp__github__merge"));
        // Its result comes from Codex's item.
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": { "type": "mcpToolCall", "id": "c1", "server": "harness_browser", "tool": "read", "arguments": {}, "status": "completed", "error": null, "result": { "content": [{ "type": "text", "text": "page text" }] } } })));
        let events = r.events();
        assert!(events.contains(&AgentEvent::ToolResult { tool_id: "c1".into(), content: json!("page text"), is_error: false, parent_tool_id: None }));
        // An unknown request is refused, so Codex never waits on it.
        r.feed(json!({ "id": 5, "method": "attestation/generate", "params": {} }));
        assert_eq!(r.sent().last().unwrap()["error"]["code"], -32601);
    }

    #[test]
    fn a_plan_is_shown_once_its_turn_ends_and_approving_builds_it() {
        let mut r = rig();
        r.shared.state.lock().unwrap().mode = "plan".into();
        r.feed(note("turn/started", json!({ "threadId": "t1", "turn": { "id": "u1" } })));
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": { "type": "plan", "id": "u1-plan", "text": "# Add hello.py\n\nSteps" } })));
        r.feed(note("turn/completed", json!({ "threadId": "t1", "turn": { "id": "u1", "status": "completed" } })));
        let events = r.events();
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, .. }, .. } if name == "ExitPlanMode"));
        match &events[1] {
            AgentEvent::ApprovalRequest { approval, title, input, .. } => {
                assert_eq!(*approval, ApprovalKind::Plan);
                assert_eq!(title, "Add hello.py");
                assert_eq!(input["plan"], "# Add hello.py\n\nSteps");
            }
            other => panic!("expected the plan card, got {other:?}"),
        }
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::TurnEnded { .. })), "the turn goes on");
        let id = r.request_id(&events);
        r.process().answer_approval(&id, Decision::Allow, None, None, Some("# Edited".into())).unwrap();
        let events = r.events();
        assert!(events.contains(&AgentEvent::ModeChanged { mode: "default".into() }));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolResult { tool_id, is_error: false, .. } if tool_id == "u1-plan")));
        let state = r.shared.state.lock().unwrap();
        assert_eq!(state.mode, "default");
        assert!(state.notes[0].contains("from this version") && state.notes[0].ends_with("# Edited"));
        drop(state);
        assert!(r.woken());
        assert!(r.sent().is_empty(), "nothing goes to Codex for the plan card itself");
    }

    #[test]
    fn stopping_while_a_plan_waits_ends_the_turn() {
        let r = rig();
        r.shared.state.lock().unwrap().mode = "plan".into();
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": { "type": "plan", "id": "p", "text": "Plan" } })));
        r.feed(note("turn/completed", json!({ "threadId": "t1", "turn": { "id": "u1", "status": "completed" } })));
        r.events();
        r.process().interrupt().unwrap();
        let events = r.events();
        assert!(matches!(events[0], AgentEvent::ApprovalCancelled { .. }));
        assert!(matches!(events[1], AgentEvent::TurnEnded { ok: true, .. }));
        assert_eq!(events[2], AgentEvent::Status { state: SessionState::Idle });
    }

    #[test]
    fn questions_answer_by_question_id() {
        let mut r = rig();
        r.feed(json!({ "id": 9, "method": "item/tool/requestUserInput", "params": { "threadId": "t1", "itemId": "q", "questions": [
            { "id": "greeting", "header": "Greeting", "question": "Which greeting?", "isOther": true, "isSecret": false,
              "options": [{ "label": "Hello", "description": "Plain" }, { "label": "Hi", "description": "Casual" }] },
        ] } }));
        let events = r.events();
        match &events[0] {
            AgentEvent::ApprovalRequest { approval, input, .. } => {
                assert_eq!(*approval, ApprovalKind::Question);
                assert_eq!(input["questions"][0]["question"], "Which greeting?");
                assert_eq!(input["questions"][0]["options"][1]["label"], "Hi");
            }
            other => panic!("expected a question, got {other:?}"),
        }
        let id = r.request_id(&events);
        let answers = BTreeMap::from([("Which greeting?".to_string(), "Hi".to_string())]);
        r.process().answer_approval(&id, Decision::Allow, None, Some(answers), None).unwrap();
        assert_eq!(r.sent()[0], json!({ "id": 9, "result": { "answers": { "greeting": { "answers": ["Hi"] } } } }));
    }

    #[test]
    fn a_turn_ends_with_its_status_and_cancels_what_still_waits() {
        let mut r = rig();
        r.shared.state.lock().unwrap().turn_started = Some(Instant::now());
        r.feed(json!({ "id": 1, "method": "item/fileChange/requestApproval", "params": { "threadId": "t1", "itemId": "f" } }));
        r.events();
        r.feed(note("turn/completed", json!({ "threadId": "t1", "turn": { "id": "u1", "status": "failed", "error": { "message": "boom" } } })));
        let events = r.events();
        assert!(matches!(&events[0], AgentEvent::Error { message } if message == "Codex: boom"));
        assert!(matches!(events[1], AgentEvent::ApprovalCancelled { .. }));
        assert!(matches!(events[2], AgentEvent::TurnEnded { ok: false, duration_ms: Some(_) }));
        assert_eq!(r.sent()[0]["result"]["decision"], "cancel");

        // Messages Codex couldn't take mid-turn go on as the next turn.
        {
            let mut state = r.shared.state.lock().unwrap();
            state.queued.insert("m2".into());
            state.pending.push(("m2".into(), vec![text_input("later")]));
        }
        r.feed(note("turn/completed", json!({ "threadId": "t1", "turn": { "id": "u2", "status": "completed" } })));
        assert!(!r.events().iter().any(|e| matches!(e, AgentEvent::TurnEnded { .. })), "one turn to the user");
        assert!(r.woken());
    }

    #[test]
    fn subagents_show_under_the_call_that_started_them() {
        let r = rig();
        let spawn = json!({ "type": "collabAgentToolCall", "id": "spawn1", "tool": "spawnAgent", "status": "completed",
            "prompt": "Find the bug\nin detail", "receiverThreadIds": ["child"], "agentsStates": { "child": { "status": "running" } } });
        r.feed(note("item/completed", json!({ "threadId": "t1", "item": spawn })));
        let events = r.events();
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, input, .. }, .. }
            if name == "Agent" && input["description"] == "Find the bug"));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TaskStarted { task_id, tool_id: Some(t), background: true, .. } if task_id == "child" && t == "spawn1")));
        assert!(events.contains(&AgentEvent::BackgroundWork {
            running: 1,
            agents: 1,
            tasks: vec![BackgroundTask { id: "child".into(), kind: "agent".into(), description: "spawn1".into() }]
        }));
        // Its steps carry the call's id; its turn ending ends the task.
        r.feed(note("item/completed", json!({ "threadId": "child", "item": { "type": "agentMessage", "id": "cm", "text": "Found it" } })));
        r.feed(note("turn/completed", json!({ "threadId": "child", "turn": { "id": "cu", "status": "completed" } })));
        let events = r.events();
        assert_eq!(
            events[0],
            AgentEvent::AssistantBlock {
                message_id: "cm".into(),
                block: Block::Text { text: "Found it".into() },
                parent_tool_id: Some("spawn1".into())
            }
        );
        assert!(matches!(&events[1], AgentEvent::TaskEnded { status, .. } if status == "completed"));
        assert_eq!(events[2], AgentEvent::BackgroundWork { running: 0, agents: 0, tasks: vec![] });
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::TurnEnded { .. })), "the chat's own turn is not over");
    }

    #[test]
    fn usage_context_and_sign_in() {
        let r = rig();
        r.feed(note("thread/tokenUsage/updated", json!({ "threadId": "t1", "tokenUsage": { "last": { "totalTokens": 16378 }, "modelContextWindow": 258400 } })));
        r.feed(note("account/rateLimits/updated", json!({ "rateLimits": {
            "primary": { "usedPercent": 25, "windowDurationMins": 300, "resetsAt": 100 },
            "secondary": { "usedPercent": 10, "windowDurationMins": 10080, "resetsAt": 200 },
        } })));
        r.feed(note("error", json!({ "threadId": "t1", "willRetry": true, "error": { "message": "retrying" } })));
        r.feed(note("error", json!({ "threadId": "t1", "willRetry": false, "error": { "message": "Sign in again", "codexErrorInfo": "unauthorized" } })));
        assert_eq!(
            r.events(),
            vec![
                AgentEvent::ContextUsage { used: 16378, max: 258400, parts: vec![] },
                AgentEvent::UsageUpdate { window: Some("5h".into()), utilization: 0.25, resets_at: Some(100) },
                AgentEvent::UsageUpdate { window: Some("week".into()), utilization: 0.1, resets_at: Some(200) },
                AgentEvent::SignInNeeded { message: "Sign in again".into() },
            ]
        );
        assert_eq!(window_name(Some(43200)), "month");
    }

    #[test]
    fn a_to_do_list_reads_like_claude_codes() {
        let r = rig();
        r.feed(note("turn/plan/updated", json!({ "threadId": "t1", "turnId": "u1", "plan": [
            { "step": "Read", "status": "completed" }, { "step": "Write", "status": "inProgress" },
        ] })));
        let events = r.events();
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, input, .. }, .. }
            if name == "TodoWrite" && input["todos"][1]["status"] == "in_progress"));
        assert!(matches!(&events[1], AgentEvent::ToolResult { .. }));
    }

    #[test]
    fn commands_lose_their_shell() {
        assert_eq!(plain_command(&json!("/bin/zsh -lc 'ls -la'"), &json!([])), "ls -la");
        assert_eq!(plain_command(&json!("/bin/zsh -lc \"rg x\""), &json!(null)), "rg x");
        assert_eq!(
            plain_command(&json!("x"), &json!([{ "command": "ls" }, { "command": "cat a" }, { "command": "ls" }])),
            "ls && cat a"
        );
        let (name, input) = command_tool("cat a.rs", &json!([{ "type": "read", "command": "cat a.rs", "path": "/p/a.rs" }]));
        assert_eq!((name.as_str(), input["file_path"].as_str()), ("Read", Some("/p/a.rs")));
    }

    #[test]
    fn models_put_the_default_first_and_titles_use_a_fast_one() {
        let models = parse_models(&json!({ "data": [
            { "id": "terra", "displayName": "Terra", "description": "Balanced model", "hidden": false, "isDefault": false,
              "supportedReasoningEfforts": [{ "reasoningEffort": "low" }, { "reasoningEffort": "high" }], "defaultReasoningEffort": "high" },
            { "id": "secret", "displayName": "Secret", "hidden": true, "isDefault": false, "supportedReasoningEfforts": [] },
            { "id": "luna", "displayName": "Luna", "description": "Fast and affordable", "hidden": false, "isDefault": true,
              "supportedReasoningEfforts": [{ "reasoningEffort": "low" }], "defaultReasoningEffort": "low" },
        ] }));
        let values: Vec<&str> = models.iter().map(|m| m.value.as_str()).collect();
        assert_eq!(values, ["luna", "terra"]);
        assert_eq!(models[1].efforts, ["low", "high"]);
        assert_eq!(title_model(&models), Some(("luna".to_string(), Some("low".to_string()))));
    }

    #[test]
    fn each_mode_sets_codexs_own_policy() {
        let roots = vec!["/notes".to_string()];
        assert_eq!(mode_policy("default", &roots).1["writableRoots"], json!(["/notes"]));
        assert_eq!(mode_policy("plan", &roots).1["type"], "workspaceWrite");
        assert_eq!(mode_policy("read-only", &roots), ("on-request", json!({ "type": "readOnly", "networkAccess": false })));
        assert_eq!(mode_policy("full-access", &roots), ("never", json!({ "type": "dangerFullAccess" })));
        let config = thread_config(&[("harness_chats".into(), "http://x/mcp".into())], &roots, false);
        assert_eq!(config["mcp_servers.harness_chats.default_tools_approval_mode"], "approve");
        assert!(is_compact(&[text_input(" /compact ")]));
    }

    #[test]
    fn a_side_chat_reads_only_and_loads_only_the_apps_tools() {
        assert_eq!(mode_sandbox(SIDE_MODE), ("never", "read-only"));
        let (approval, sandbox) = mode_policy(SIDE_MODE, &["/notes".into()]);
        assert_eq!((approval, sandbox["type"].as_str()), ("never", Some("readOnly")));
        let config = thread_config(&[("harness_chats".into(), "http://x/mcp".into())], &["/notes".into()], true);
        assert_eq!(
            config,
            json!({ "mcp_servers": { "harness_chats": { "url": "http://x/mcp", "default_tools_approval_mode": "approve" } } })
        );
        // No tools: none of the user's either.
        assert_eq!(thread_config(&[], &[], true), json!({ "mcp_servers": {} }));
    }

    #[test]
    fn a_threads_saved_file_is_found_by_its_id_in_any_day_folder() {
        let folder = std::env::temp_dir().join(format!("harness-codex-rollouts-{}", uuid::Uuid::new_v4()));
        let day = folder.join("2026/11/19");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(day.join("rollout-2026-11-19T10-44-09-01a1f47a-c80f.jsonl"), "").unwrap();
        assert!(has_rollout(&folder, "01a1f47a-c80f"));
        assert!(!has_rollout(&folder, "c80f-01a1f47a"));
        assert!(!has_rollout(&folder.join("missing"), "01a1f47a-c80f"));
        let _ = std::fs::remove_dir_all(folder);
    }
}
