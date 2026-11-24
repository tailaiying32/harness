//! Agents that speak ACP, the Agent Client Protocol: JSON-RPC 2.0, one message
//! per line, on the agent's stdin and stdout. This file knows the protocol and
//! turns it into the app's events. `cursor.rs` and `antigravity.rs` say how to
//! start each agent, and fill in a `Dialect`: what differs between them (how
//! tool calls are named, where the app's instructions go, how a plan comes).
//! What each sends was tested live; see PLAN.md, "Cursor over ACP" and
//! "Antigravity over ACP".
//!
//! One task drives the chat in order: `initialize`, then `session/new` or
//! `session/load`, then one `session/prompt` at a time. A reader task routes
//! the agent's lines: replies to our calls, its requests (approvals, plans,
//! questions), and `session/update` notifications (text, tool calls).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use super::rpc::{Out, Rpc, RpcError};
use super::{AppTools, EventSink, ImagePayload, RunningAgent, SpawnOptions, mcp_http};
use crate::events::{AgentEvent, ApprovalKind, Block, Decision, SessionState};

/// How many stderr lines to keep for an error message when the process dies.
const STDERR_TAIL_LINES: usize = 30;
/// How long a stopped process gets before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// ACP's error code for "sign in first".
const AUTH_REQUIRED: i64 = -32000;
/// Told to a side chat's agent that stopped at a refusal without a word.
const REFUSED_NOTE: &str = "(From the app, not the user: That was refused. In this side chat you can read \
and search files, but not change anything or run commands. Answer in words from what you can read.)";
/// How long the agent may go quiet before a follow-up it started on its own
/// counts as finished.
const FOLLOW_UP_QUIET: Duration = Duration::from_secs(8);

/// How to start one ACP agent.
pub struct AcpConfig {
    /// What the user calls it, for messages ("Cursor stopped unexpectedly").
    pub label: &'static str,
    /// Arguments that start it in ACP mode.
    pub args: Vec<String>,
    /// `clientCapabilities._meta`: the agent's own extension switches.
    pub client_meta: Value,
    pub dialect: &'static Dialect,
    /// A side chat: read-only, so every request to ask the user (a
    /// permission, a plan, questions) is refused at once, as a Claude Code
    /// side chat with no way to ask refuses them.
    pub side: bool,
}

/// Where a new conversation's instructions from the app go.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intro {
    /// In the first prompt, after the user's message (Cursor has no system prompt).
    FirstPrompt,
    /// In the agent's system prompt, through `Dialect::session_meta`.
    SystemPrompt,
}

/// What differs between ACP agents.
pub struct Dialect {
    /// The app's name for a tool call and its input, in the shape the window
    /// words Claude Code's tools in (`tools.ts`): "Bash" with `command`, "Read"
    /// with `file_path`, and so on.
    /// Given the app's own tool server names too.
    pub name_tool: fn(&Tool, &[String]) -> (String, Value),
    /// Whether a call is one of the app's own tools, given their server names.
    pub own_tool: fn(&Tool, &[String]) -> bool,
    /// Whether a call writes a plan for the user to approve (an agent whose
    /// plan arrives as a file, not as a request of its own).
    pub is_plan: fn(&Tool) -> bool,
    /// A finished call's output, tidied for the chat.
    pub clean_output: fn(String) -> String,
    /// The mode to switch to once a plan is approved.
    pub build_mode: &'static str,
    /// How the agent is told to present a plan, so it reaches the app's
    /// approve card instead of only the chat. Not for side chats.
    pub plan_instruction: &'static str,
    pub intro: Intro,
    /// `_meta` for `session/new` and `session/load`, given the instructions
    /// (when they go in the system prompt).
    pub session_meta: fn(Option<&str>) -> Value,
    /// A new session starts on the last model used anywhere in the agent, so
    /// the model is set even when the chat picked none ("default").
    pub pick_default_model: bool,
    /// Offer to show question forms (ACP elicitation).
    pub forms: bool,
    /// The agent's own commands that work only in its print mode
    /// (`<binary> -p "/name ..."`), not over ACP, where they would go to the
    /// model. The app runs them that way and shows what they print.
    pub print_commands: &'static [PrintCommand],
    /// Its `/compact` compacts the conversation (a "compacted" line follows).
    pub compacts: bool,
}

/// One of an agent's print-mode commands (see `Dialect::print_commands`).
pub struct PrintCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

impl Dialect {
    /// The print-mode command a message is, if it is one alone.
    fn print_command(&self, text: &str) -> bool {
        let Some(rest) = text.trim().strip_prefix('/') else { return false };
        let name = rest.split_whitespace().next().unwrap_or_default();
        self.print_commands.iter().any(|c| c.name == name || c.aliases.contains(&name))
    }
}

/// How long a print-mode command may take.
const PRINT_TIMEOUT: Duration = Duration::from_secs(60);

/// Runs `<binary> -p "<text>"` in the project and returns what it printed, or
/// why it failed.
fn run_print(binary: &std::path::Path, folder: &std::path::Path, text: &str) -> Result<String, String> {
    use std::io::Read;
    let mut cmd = std::process::Command::new(binary);
    cmd.args(["-p", text.trim()])
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let (mut stdout, mut stderr) = (child.stdout.take().ok_or("no output")?, child.stderr.take().ok_or("no output")?);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut out, mut err) = (String::new(), String::new());
        let _ = stdout.read_to_string(&mut out);
        let _ = stderr.read_to_string(&mut err);
        let _ = tx.send((out, err));
    });
    let Ok((out, err)) = rx.recv_timeout(PRINT_TIMEOUT) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("it took too long".to_string());
    };
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    match (ok, out.trim(), err.trim()) {
        (true, out, _) => Ok(out.to_string()),
        (false, _, err) if !err.is_empty() => Err(err.to_string()),
        (false, out, _) => Err(if out.is_empty() { "it failed".to_string() } else { out.to_string() }),
    }
}

/// What a print-mode command printed, for the chat. Its tab-split lines
/// (`Gemini Models\tWeekly Limit Remaining\t97%\t...`) become a list, the
/// first cell in bold; anything else is shown as it came.
fn print_reply(out: &str) -> String {
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return "Nothing to show.".to_string();
    }
    if !lines.iter().all(|l| l.contains('\t')) {
        return format!("```\n{}\n```", out.trim_end());
    }
    lines
        .iter()
        .map(|line| {
            let mut cells = line.split('\t').map(str::trim).filter(|c| !c.is_empty());
            let first = cells.next().unwrap_or_default();
            let rest: Vec<&str> = cells.collect();
            match rest.is_empty() {
                true => format!("- **{first}**"),
                false => format!("- **{first}** · {}", rest.join(" · ")),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    /// A plan to approve. No `rpc_id` when the agent wrote it as a file and
    /// ended its turn: the answer goes back as the next prompt.
    Plan {
        rpc_id: Option<Value>,
    },
    Question {
        rpc_id: Value,
        /// Each question: its id, its text, and its options as `(id, label)`.
        questions: Vec<(String, String, Vec<(String, String)>)>,
    },
    /// Questions asked as a form (ACP elicitation), one field each.
    Form {
        rpc_id: Value,
        fields: Vec<FormField>,
    },
}

/// One question in an elicitation form: its field, its text, its choices as
/// `(value, label)`, and the field for a free-form answer, if there is one.
struct FormField {
    key: String,
    text: String,
    options: Vec<(String, String)>,
    multi: bool,
    other: Option<String>,
}

/// The questions in an elicitation form's `requestedSchema`: every field
/// with choices (`oneOf`/`enum`, or an array of them for several answers).
/// A field `<key>_other` is that question's free-form answer.
fn form_fields(schema: &Value) -> Vec<FormField> {
    let Some(props) = schema["properties"].as_object() else { return Vec::new() };
    let choices = |field: &Value| -> Vec<(String, String)> {
        if let Some(one_of) = field["oneOf"].as_array().or(field["anyOf"].as_array()) {
            return one_of
                .iter()
                .filter_map(|o| {
                    let value = o["const"].as_str()?.to_string();
                    Some((value.clone(), o["title"].as_str().unwrap_or(&value).to_string()))
                })
                .collect();
        }
        field["enum"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .enumerate()
            .map(|(i, v)| {
                let label = field["enumNames"][i].as_str().unwrap_or(v);
                (v.to_string(), label.to_string())
            })
            .collect()
    };
    props
        .iter()
        .filter(|(key, _)| !key.ends_with("_other"))
        .filter_map(|(key, field)| {
            let multi = field["type"] == "array";
            let options = choices(if multi { &field["items"] } else { field });
            if options.is_empty() {
                return None;
            }
            let other = format!("{key}_other");
            Some(FormField {
                key: key.clone(),
                text: field["title"].as_str().or(field["description"].as_str()).unwrap_or(key).to_string(),
                options,
                multi,
                other: props.contains_key(&other).then_some(other),
            })
        })
        .collect()
}

/// The form's answer from the card's: each question's picked labels as
/// their values, or what the user typed in its free-form field.
fn form_content(fields: &[FormField], answers: &BTreeMap<String, String>) -> Value {
    let mut content = serde_json::Map::new();
    for field in fields {
        let Some(answer) = answers.get(&field.text) else { continue };
        let picked: Vec<&str> = answer
            .split(", ")
            .filter_map(|label| field.options.iter().find(|(_, l)| l == label).map(|(v, _)| v.as_str()))
            .collect();
        if picked.is_empty() {
            if let Some(other) = &field.other {
                content.insert(other.clone(), json!(answer));
            }
        } else if field.multi {
            content.insert(field.key.clone(), json!(picked));
        } else {
            content.insert(field.key.clone(), json!(picked[0]));
        }
    }
    Value::Object(content)
}

impl Waiting {
    fn rpc_id(&self) -> Option<&Value> {
        match self {
            Waiting::Permission { rpc_id, .. } | Waiting::Question { rpc_id, .. } | Waiting::Form { rpc_id, .. } => {
                Some(rpc_id)
            }
            Waiting::Plan { rpc_id } => rpc_id.as_ref(),
        }
    }

    /// What answers it as "never mind". (Every answer is wrapped the same way:
    /// `{outcome: {outcome: ...}}`, Cursor's own methods too, which its docs
    /// show unwrapped; Cursor CLI 2026.11.13 reads `outcome.outcome`. A form
    /// is answered ACP's elicitation way, `{action: ...}`.)
    fn cancelled(&self) -> Value {
        match self {
            Waiting::Form { .. } => json!({ "action": "cancel" }),
            _ => json!({ "outcome": { "outcome": "cancelled" } }),
        }
    }
}

/// One tool call as it fills in: it arrives with an empty input and a generic
/// title ("Terminal"), and an update brings the real ones.
#[derive(Default)]
pub struct Tool {
    /// The agent's own name for the tool, when it sends one (Antigravity).
    pub name: String,
    pub title: String,
    pub kind: String,
    pub raw_input: Value,
    /// The files it touches, as `locations` lists them.
    pub locations: Vec<String>,
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
    /// A side chat: something was refused this turn and no words came after.
    refused: bool,
    /// A side chat: the agent was told once, since the user's last message,
    /// why something was refused.
    told_refused: bool,
    /// A plan was approved: leave plan mode before the next prompt.
    build_after_turn: bool,
    /// A plan written this turn as a file, to offer for approval when the
    /// turn ends: its tool call's ID, its text and where it is.
    written_plan: Option<(String, String, String)>,
    /// A prompt is being answered.
    in_turn: bool,
    /// The agent is talking without a prompt (Antigravity reports a
    /// subagent's result after its turn ended): when it last said something.
    follow_up: Option<Instant>,
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
    dialect: &'static Dialect,
    /// A side chat: nothing waits for the user (`AcpConfig::side`).
    side: bool,
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

pub(crate) fn terminate(child: &mut tokio::process::Child) {
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
                eprintln!("tilde: the app's tools aren't available to {label}: {e}");
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
        rpc: Rpc::new(tx, true),
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
        dialect: cfg.dialect,
        side: cfg.side,
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

    // A follow-up the agent started on its own ends when it goes quiet, if
    // its closing update never came.
    {
        let weak = Arc::downgrade(&shared);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(shared) = weak.upgrade() else { break };
                let closed = {
                    let mut state = shared.state.lock().unwrap();
                    match state.follow_up {
                        Some(at) if at.elapsed() >= FOLLOW_UP_QUIET && state.waiting.is_empty() => {
                            state.follow_up = None;
                            Some(close_run(&mut state))
                        }
                        _ => None,
                    }
                };
                if let Some(block) = closed {
                    if let Some(block) = block {
                        shared.emit(block);
                    }
                    end_follow_up(&shared);
                }
            }
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
    let dialect = shared.dialect;
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
                    "elicitation": if dialect.forms { json!({ "form": {} }) } else { Value::Null },
                    "_meta": client_meta,
                },
                "clientInfo": { "name": "tilde", "version": env!("CARGO_PKG_VERSION") },
            }),
        )
        .await;
    let init = match init {
        Ok(init) => init,
        Err(e) => return fail(format!("{label} didn't start: {}", e.message)),
    };
    let cwd = opts.project_path.to_string_lossy().into_owned();
    // Told again on every start when it goes in the system prompt, which a
    // loaded session doesn't keep.
    let told = instructions(&opts, dialect.plan_instruction);
    let meta = (dialect.session_meta)(told.as_deref().filter(|_| dialect.intro == Intro::SystemPrompt));
    let with_meta = |mut params: Value| {
        if meta.as_object().is_some_and(|m| !m.is_empty()) {
            params["_meta"] = meta.clone();
        }
        params
    };

    // Pick up the earlier chat if there is one; else a new one.
    let mut fresh = true;
    let mut session = None;
    if let Some(resume) = opts.resume_id.clone().filter(|_| init["agentCapabilities"]["loadSession"] == true) {
        shared.state.lock().unwrap().loading = true;
        let loaded = shared
            .rpc
            .call("session/load", with_meta(json!({ "sessionId": resume, "cwd": cwd, "mcpServers": mcp_servers })))
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
            .call("session/new", with_meta(json!({ "cwd": cwd, "mcpServers": mcp_servers })))
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

    // The chat's own settings.
    let model = match opts.model.clone().filter(|m| m != "default") {
        Some(model) => Some(model),
        None if dialect.pick_default_model => Some("default".to_string()),
        None => None,
    };
    if let Some(model) = model.filter(|m| shared.state.lock().unwrap().config.model.as_deref() != Some(m.as_str())) {
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
    // A side chat's first start is told too: it loaded a copy of a chat that
    // heard other instructions.
    let new_side = opts.side.as_ref().is_some_and(|s| s.first);
    let mut intro = if (fresh || new_side) && dialect.intro == Intro::FirstPrompt { told } else { None };

    while let Some(first) = items.recv().await {
        let mut batch = vec![first];
        while let Ok(item) = items.try_recv() {
            batch.push(item);
        }
        let closing = batch.iter().any(|i| matches!(i, Item::Close));
        // One of the agent's print-mode commands, alone: run it that way.
        if let [Item::User { text, images, id }] = batch.as_slice() {
            if images.is_empty() && dialect.print_command(text) {
                if shared.state.lock().unwrap().queued.remove(id) {
                    run_print_command(&shared, &opts, text.clone(), id.clone()).await;
                }
                continue;
            }
        }
        let mut body: Vec<Value> = Vec::new();
        let mut seen = Vec::new();
        // `/compact` alone, on an agent whose `/compact` compacts.
        let compact = dialect.compacts
            && matches!(batch.as_slice(), [Item::User { text, images, .. }] if images.is_empty() && text.trim() == "/compact");
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
                    shared.state.lock().unwrap().told_refused = false;
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
            let to = dialect.build_mode;
            // Only from plan mode: a plan made in Auto is built in Auto.
            let leave_plan = build && {
                let state = shared.state.lock().unwrap();
                state.mode.as_deref() == Some("plan") && state.config.modes.iter().any(|m| m == to)
            };
            if leave_plan
                && shared
                    .rpc
                    .call("session/set_mode", json!({ "sessionId": session_id, "modeId": to }))
                    .await
                    .is_ok()
            {
                shared.state.lock().unwrap().mode = Some(to.to_string());
                shared.emit(AgentEvent::ModeChanged { mode: to.to_string() });
            }
            prompt.extend(notes.into_iter().map(|note| json!({ "type": "text", "text": note })));
            prompt.extend(body);
            // The intro goes last: Cursor titles a chat from the start of its
            // first prompt, and with the intro first the title was about the
            // app's notes ("Tilde Agent"), not what the user asked.
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
            {
                // A follow-up still going is folded into this turn.
                let mut state = shared.state.lock().unwrap();
                state.in_turn = true;
                state.follow_up = None;
            }
            let reply = shared
                .rpc
                .call("session/prompt", json!({ "sessionId": session_id, "prompt": prompt }))
                .await;
            shared.state.lock().unwrap().in_turn = false;
            end_run(&shared);
            // Nothing can still be waiting once the turn is over.
            let left: Vec<(String, Waiting)> = shared.state.lock().unwrap().waiting.drain().collect();
            for (request_id, waiting) in left {
                if let Some(rpc_id) = waiting.rpc_id() {
                    shared.rpc.respond(rpc_id, waiting.cancelled());
                }
                shared.emit(AgentEvent::ApprovalCancelled { request_id });
            }
            let written_plan = shared.state.lock().unwrap().written_plan.take();
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
            // A side chat whose agent stopped at a refusal without a word
            // (Antigravity does) is told why once, and answers in the same
            // turn, as Claude Code does on its own.
            if shared.side && ok && !stopped && !closing {
                let mut state = shared.state.lock().unwrap();
                if std::mem::take(&mut state.refused) && !state.told_refused {
                    state.told_refused = true;
                    state.notes.push(REFUSED_NOTE.to_string());
                }
            }
            // Notes from the turn (an approved plan, say) carry it on at once:
            // to the user it is one turn, as with Claude Code. Not after a
            // stop, a failure, or when closing.
            let carry_on = ok && !stopped && !closing && !shared.state.lock().unwrap().notes.is_empty();
            // A plan written as a file waits for the user, as a plan asked
            // about with a request does; the answer starts the next turn.
            let plan = written_plan.filter(|_| ok && !stopped && !closing && !carry_on);
            if let Some((tool_id, plan, path)) = plan {
                offer_plan(&shared, tool_id, plan, path);
            } else if carry_on {
                let _ = shared.items.send(Item::Note(String::new()));
            } else {
                shared.state.lock().unwrap().notes.clear();
                if compact && ok && !stopped {
                    shared.emit(AgentEvent::Compacted { auto: false });
                }
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

/// Runs a print-mode command and answers with what it printed, as a turn of
/// its own.
async fn run_print_command(shared: &Arc<Shared>, opts: &SpawnOptions, text: String, id: String) {
    shared.emit(AgentEvent::MessageSeen { id: id.clone() });
    shared.emit(AgentEvent::Status { state: SessionState::Working });
    let started = Instant::now();
    let (binary, folder) = (opts.binary.clone(), opts.project_path.clone());
    let typed = text.clone();
    let result = tokio::task::spawn_blocking(move || run_print(&binary, &folder, &typed))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    let ok = result.is_ok();
    match result {
        Ok(out) => shared.emit(AgentEvent::AssistantBlock {
            message_id: format!("{id}-reply"),
            block: Block::Text { text: print_reply(&out) },
            parent_tool_id: None,
        }),
        Err(e) => shared.emit(AgentEvent::Error {
            message: format!("{} couldn't run {}: {e}", shared.label, text.trim()),
        }),
    }
    shared.emit(AgentEvent::TurnEnded { ok, duration_ms: Some(started.elapsed().as_millis() as u64) });
    shared.emit(AgentEvent::Status { state: SessionState::Idle });
}

/// Asks the user to approve a plan the agent wrote as a file and then
/// stopped. The chat stays busy until they answer.
fn offer_plan(shared: &Shared, tool_id: String, plan: String, path: String) {
    let request_id = wait(shared, Waiting::Plan { rpc_id: None });
    shared.emit(AgentEvent::ApprovalRequest {
        request_id,
        approval: ApprovalKind::Plan,
        tool_name: "ExitPlanMode".to_string(),
        title: "Plan".to_string(),
        input: json!({ "plan": plan, "planFilePath": path }),
        tool_id: Some(tool_id),
        always_label: None,
    });
    shared.emit(AgentEvent::Status {
        state: SessionState::WaitingApproval,
    });
}

/// The agent says it isn't signed in. The process is ended, so the next
/// message, after signing in, starts a fresh one.
fn sign_in_needed(shared: &Shared, e: RpcError) {
    shared.emit(AgentEvent::SignInNeeded { message: e.message });
    shared.stop();
}

/// What a new conversation is told: the app's tools, the chat's notes, the
/// project's memory and the user's preferences. Where it goes is the
/// dialect's `intro`.
fn instructions(opts: &SpawnOptions, plan: &str) -> Option<String> {
    let side = opts.side.is_some();
    let mut lines: Vec<String> = Vec::new();
    // A side chat reads and answers: told so, and given no way to write notes or memory.
    if side {
        lines.push(super::SIDE_INSTRUCTION.to_string());
    } else {
        lines.push(plan.to_string());
    }
    lines.extend(opts.tools.iter().map(|t| t.instruction.to_string()));
    if let Some(notes) = &opts.notes {
        lines.push(super::notes_instruction(notes, opts.project_docs.as_deref(), !side));
    }
    // Cursor keeps no memory of its own, and Antigravity's own isn't shared
    // with the app's other agents, so each is told about the project's.
    if let Some(memory) = opts.memory.as_ref().filter(|_| !side) {
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
            let (events, started, ended) = {
                let mut state = shared.state.lock().unwrap();
                let session = v["params"]["sessionId"].as_str().unwrap_or_default();
                let u = &v["params"]["update"];
                match state.children.get_mut(session) {
                    Some(child) => (child_update(child, shared.dialect, &shared.own_tools, u), false, false),
                    None => {
                        let mut events = update(&mut state, shared.dialect, &shared.own_tools, u);
                        let (started, ended) = follow_up(&mut state, u);
                        if ended {
                            events.extend(close_run(&mut state));
                        }
                        (events, started, ended)
                    }
                }
            };
            if started {
                shared.emit(AgentEvent::Status { state: SessionState::Working });
            }
            for event in events {
                shared.emit(event);
            }
            if ended {
                end_follow_up(shared);
            }
        }
        _ => {}
    }
}

/// Whether an update outside any prompt starts the agent talking on its own,
/// or ends that: its closing `usage_update`.
fn follow_up(state: &mut State, u: &Value) -> (bool, bool) {
    if state.in_turn || state.loading || state.session_id.is_none() {
        return (false, false);
    }
    let kind = u["sessionUpdate"].as_str().unwrap_or_default();
    let talk = matches!(kind, "agent_message_chunk" | "agent_thought_chunk" | "tool_call" | "tool_call_update");
    if talk {
        let started = state.follow_up.is_none();
        state.follow_up = Some(Instant::now());
        return (started, false);
    }
    let ended = kind == "usage_update" && state.waiting.is_empty() && state.follow_up.take().is_some();
    (false, ended)
}

fn end_follow_up(shared: &Shared) {
    shared.emit(AgentEvent::TurnEnded { ok: true, duration_ms: None });
    shared.emit(AgentEvent::Status { state: SessionState::Idle });
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
                if let Some(title) = call["title"].as_str() {
                    tool.title = title.to_string();
                }
                fill_tool(tool, call);
                let d = shared.dialect;
                let (name, input) = (d.name_tool)(tool, &shared.own_tools);
                let own = (d.own_tool)(tool, &shared.own_tools);
                if let Some(event) = show_tool(&tool_id, tool, true, d, &shared.own_tools) {
                    events.push(event);
                }
                (name, input, own)
            };
            for event in events {
                shared.emit(event);
            }
            // A side chat changes nothing: refused without asking.
            if shared.side && !own {
                shared.state.lock().unwrap().refused = true;
                let reject = options
                    .iter()
                    .find(|(_, kind)| kind == "reject_once")
                    .or_else(|| options.iter().find(|(_, kind)| kind.starts_with("reject")));
                let result = match reject {
                    Some((option, _)) => json!({ "outcome": { "outcome": "selected", "optionId": option } }),
                    None => json!({ "outcome": { "outcome": "cancelled" } }),
                };
                shared.rpc.respond(rpc_id, result);
                return;
            }
            // The app's own tools never ask, as with Claude Code.
            if own {
                let allow = options
                    .iter()
                    .find(|(_, kind)| kind == "allow_once")
                    .or_else(|| options.iter().find(|(id, _)| id == "allow"));
                if let Some((option, _)) = allow {
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
        "cursor/create_plan" if shared.side => shared.rpc.respond(
            rpc_id,
            json!({ "outcome": { "outcome": "rejected", "reason": "This is a read-only side chat; answer in words." } }),
        ),
        "cursor/create_plan" => {
            let tool_id = params["toolCallId"].as_str().unwrap_or_default().to_string();
            let plan = params["plan"].as_str().unwrap_or_default().to_string();
            let shown = {
                let mut state = shared.state.lock().unwrap();
                let tool = state.tools.entry(tool_id.clone()).or_default();
                tool.raw_input = json!({ "_toolName": "createPlan", "plan": plan });
                show_tool(&tool_id, tool, true, shared.dialect, &shared.own_tools)
            };
            if let Some(event) = shown {
                shared.emit(event);
            }
            let request_id = wait(shared, Waiting::Plan { rpc_id: Some(rpc_id.clone()) });
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
        "cursor/ask_question" if shared.side => shared.rpc.respond(
            rpc_id,
            json!({ "outcome": { "outcome": "skipped", "reason": "This is a side chat; answer without asking." } }),
        ),
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

        // Questions as a form (ACP elicitation), shown like Claude Code's. A
        // form the card can't show (no choices) is declined, so the agent
        // carries on without an answer.
        "elicitation/create" => {
            let fields = form_fields(&params["requestedSchema"]);
            if params["mode"] != "form" || fields.is_empty() || shared.side {
                shared.rpc.respond(rpc_id, json!({ "action": "decline" }));
                return;
            }
            let shown: Vec<Value> = fields
                .iter()
                .map(|f| {
                    json!({
                        "question": f.text,
                        "header": "",
                        "multiSelect": f.multi,
                        "options": f.options.iter().map(|(_, label)| json!({ "label": label, "description": "" })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            let tool_id = params["toolCallId"].as_str().map(str::to_owned);
            let request_id = wait(shared, Waiting::Form { rpc_id: rpc_id.clone(), fields });
            shared.emit(AgentEvent::ApprovalRequest {
                request_id,
                approval: ApprovalKind::Question,
                tool_name: "AskUserQuestion".to_string(),
                title: params["message"].as_str().unwrap_or("Questions").to_string(),
                input: json!({ "questions": shown }),
                tool_id,
                always_label: None,
            });
            shared.emit(AgentEvent::Status {
                state: SessionState::WaitingApproval,
            });
        }

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

/// Takes what a tool call's update says about it beyond the standard
/// fields: the agent's own name for it and the files it touches.
fn fill_tool(tool: &mut Tool, u: &Value) {
    if let Some(name) = u["name"].as_str() {
        tool.name = name.to_string();
    }
    let locations: Vec<String> = u["locations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["path"].as_str().map(str::to_owned))
        .collect();
    if !locations.is_empty() {
        tool.locations = locations;
    }
}

/// The tool's block, once there is enough to show: its input has arrived, it
/// started, or it is asking (`now`).
fn show_tool(tool_id: &str, tool: &mut Tool, now: bool, d: &Dialect, own: &[String]) -> Option<AgentEvent> {
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
    let (name, input) = (d.name_tool)(tool, own);
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
fn refresh_tool(tool_id: &str, tool: &mut Tool, d: &Dialect, own: &[String]) -> Option<AgentEvent> {
    if !tool.shown || tool.done || tool.raw_input["_toolName"] == "createPlan" {
        return None;
    }
    let now = (d.name_tool)(tool, own);
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
fn update(state: &mut State, d: &Dialect, own: &[String], u: &Value) -> Vec<AgentEvent> {
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
            if !thinking {
                state.refused = false;
            }
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
            // A plan the agent went on past (outside plan mode, it may build
            // in the same turn) is no longer waiting for the user.
            let planning = state.mode.as_deref() == Some("plan");
            if kind == "tool_call" && !planning && state.written_plan.as_ref().is_some_and(|(id, ..)| id != tool_id) {
                state.written_plan = None;
            }
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
            fill_tool(tool, u);
            let status = u["status"].as_str().unwrap_or_default();
            let started = matches!(status, "in_progress" | "completed" | "failed");
            match show_tool(tool_id, tool, started && tool.raw_input["_toolName"] != "createPlan", d, own) {
                Some(shown) => out.push(shown),
                None if started => out.extend(refresh_tool(tool_id, tool, d, own)),
                None => {}
            }
            let finished = matches!(status, "completed" | "failed");
            // The agent switched its own mode (Cursor's SwitchMode, which
            // sends no `current_mode_update`).
            if status == "completed" {
                if let Some(to) = tool.raw_input["targetModeId"].as_str().filter(|m| state.config.modes.iter().any(|x| x == m)) {
                    if state.mode.as_deref() != Some(to) {
                        state.mode = Some(to.to_string());
                        out.push(AgentEvent::ModeChanged { mode: to.to_string() });
                    }
                }
            }
            // A plan written as a file and marked for the user's feedback,
            // offered for approval once the turn ends, in any mode: an agent
            // that can't enter plan mode itself (Antigravity) still marks it.
            if finished && status == "completed" && (d.is_plan)(tool) {
                let (_, input) = (d.name_tool)(tool, own);
                let plan = input["plan"].as_str().unwrap_or_default().to_string();
                let path = input["planFilePath"].as_str().unwrap_or_default().to_string();
                let plan = if plan.is_empty() { std::fs::read_to_string(&path).unwrap_or_default() } else { plan };
                if !plan.trim().is_empty() {
                    state.written_plan = Some((tool_id.to_string(), plan, path));
                }
            }
            let tool = state.tools.get_mut(tool_id).expect("added above");
            if finished && !tool.done && tool.shown {
                tool.done = true;
                let mut text = (d.clean_output)(result_text(u));
                let mut is_error = status == "failed";
                // One of the app's own tools: the agent passes on no output,
                // so the answer the app gave is the result.
                if text.is_empty() {
                    let (name, input) = (d.name_tool)(tool, own);
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
        // How full the context window is.
        "usage_update" => {
            if let (Some(used), Some(max)) = (u["used"].as_u64(), u["size"].as_u64()) {
                if used > 0 && max > 0 {
                    out.push(AgentEvent::ContextUsage { used, max, parts: Vec::new() });
                }
            }
        }
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
fn child_update(child: &mut Child, d: &Dialect, own: &[String], u: &Value) -> Vec<AgentEvent> {
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
            fill_tool(tool, u);
            let status = u["status"].as_str().unwrap_or_default();
            let started = matches!(status, "in_progress" | "completed" | "failed");
            match show_tool(tool_id, tool, started, d, own) {
                Some(shown) => out.push(tag(shown, &parent)),
                None if started => out.extend(refresh_tool(tool_id, tool, d, own).map(|event| tag(event, &parent))),
                None => {}
            }
            if matches!(status, "completed" | "failed") && !tool.done && tool.shown {
                tool.done = true;
                out.push(AgentEvent::ToolResult {
                    tool_id: tool_id.to_string(),
                    content: json!((d.clean_output)(result_text(u))),
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
                    if let Some(rpc_id) = rpc_id {
                        rpc.respond(rpc_id, json!({ "outcome": { "outcome": "accepted" } }));
                    }
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
                    match rpc_id {
                        Some(rpc_id) => rpc.respond(rpc_id, json!({ "outcome": { "outcome": "rejected", "reason": reason } })),
                        // The agent's turn is over: the reason is the next prompt.
                        None => notes.push(reason),
                    }
                }
            }
            Waiting::Form { rpc_id, fields } => {
                let reply = if allow {
                    json!({ "action": "accept", "content": form_content(fields, &answers.clone().unwrap_or_default()) })
                } else {
                    if let Some(message) = message.clone() {
                        notes.push(format!("The user skipped your questions. What they said: {message}"));
                    }
                    json!({ "action": "decline" })
                };
                rpc.respond(rpc_id, reply);
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
        // A plan whose turn is over: nothing will carry the notes on, so the
        // driver is woken for them.
        if matches!(waiting, Waiting::Plan { rpc_id: None }) {
            let _ = self.shared.items.send(Item::Note(String::new()));
        }
        self.shared.emit(AgentEvent::Status {
            state: SessionState::Working,
        });
        Ok(())
    }

    fn open_requests(&self) -> Vec<String> {
        self.shared.state.lock().unwrap().waiting.keys().cloned().collect()
    }

    fn open_plans(&self) -> Vec<String> {
        let state = self.shared.state.lock().unwrap();
        state.waiting.iter().filter(|(_, w)| matches!(w, Waiting::Plan { .. })).map(|(id, _)| id.clone()).collect()
    }

    fn interrupt(&self) -> Result<(), String> {
        let session_id = self.session_id().ok_or("Nothing to stop yet")?;
        self.shared.rpc.notify("session/cancel", json!({ "sessionId": session_id }));
        // The agent doesn't withdraw what it asked; the app does.
        let left: Vec<(String, Waiting)> = self.shared.state.lock().unwrap().waiting.drain().collect();
        for (request_id, waiting) in left {
            if let Some(rpc_id) = waiting.rpc_id() {
                self.shared.rpc.respond(rpc_id, waiting.cancelled());
            }
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
        let told = instructions(&opts, "").unwrap();
        assert!(told.contains("That folder, /data/notes/chat-1, is this chat's own"));
        assert!(told.contains("\"put it in the docs\" means write it here"));
        assert!(told.contains("write it into the project's docs folder, /home/projects/-work/docs,"));
        assert!(!told.contains("Claude Docs"), "only Claude has that connector");
    }

    #[test]
    fn a_new_conversation_is_told_about_the_projects_memory() {
        let dir = std::env::temp_dir().join(format!("tilde-acp-memory-{}", uuid::Uuid::new_v4()));
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
        let told = instructions(&opts(Some(dir.clone())), "").unwrap();
        assert!(told.contains(&format!("This project has a shared memory in {}", dir.display())));
        assert!(told.contains("frontmatter (`name`, `description`, and `metadata.type`"));
        // The index as it is now, cut where Claude Code cuts its own.
        assert!(told.contains("- [Note 200](n200.md) — fact 200") && !told.contains("Note 201"));

        // The user's preferences come with it.
        let mut with_preferences = opts(None);
        with_preferences.preferences =
            Some(crate::home::Preferences { path: "/home/preferences.md".into(), text: "- Be brief.".into() });
        let told = instructions(&with_preferences, "").unwrap();
        assert!(told.contains("standing instructions, kept in /home/preferences.md") && told.ends_with("- Be brief."));

        // No index yet: said so. No memory folder: nothing about memory.
        std::fs::remove_file(dir.join("MEMORY.md")).unwrap();
        assert!(instructions(&opts(Some(dir.clone())), "").unwrap().ends_with("The index now:\n(empty)"));
        assert!(instructions(&opts(None), "").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A connection with no process: what it writes and what it reports are kept.
    fn rig() -> (Arc<Shared>, mpsc::UnboundedReceiver<Out>, Arc<Mutex<Vec<AgentEvent>>>) {
        rig_for(&crate::agent::cursor::DIALECT).0
    }

    type Rig = (Arc<Shared>, mpsc::UnboundedReceiver<Out>, Arc<Mutex<Vec<AgentEvent>>>);

    /// The rig for one agent's dialect, and what the driver would be sent.
    fn rig_for(dialect: &'static Dialect) -> (Rig, mpsc::UnboundedReceiver<Item>) {
        rig_as(dialect, false)
    }

    /// The same, for a side chat or not.
    fn rig_as(dialect: &'static Dialect, side: bool) -> (Rig, mpsc::UnboundedReceiver<Item>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (items, items_rx) = mpsc::unbounded_channel();
        let seen: Arc<Mutex<Vec<AgentEvent>>> = Arc::default();
        let sink: EventSink = {
            let seen = seen.clone();
            Arc::new(move |e| seen.lock().unwrap().push(e))
        };
        let shared = Arc::new(Shared {
            rpc: Rpc::new(tx, true),
            state: Mutex::default(),
            sink,
            items,
            stop_tx: Mutex::new(None),
            own_tools: vec!["tilde_browser".into()],
            label: "Cursor",
            dialect,
            side,
        });
        ((shared, rx, seen), items_rx)
    }

    #[test]
    fn print_mode_output_reads_as_a_list() {
        let usage = "Gemini Models\tWeekly Limit Remaining\t97%\t2026-11-25T22:40:16Z\nRemaining credits\t0\n";
        assert_eq!(
            print_reply(usage),
            "- **Gemini Models** · Weekly Limit Remaining · 97% · 2026-11-25T22:40:16Z\n- **Remaining credits** · 0"
        );
        assert_eq!(print_reply("  \n"), "Nothing to show.");
        assert_eq!(print_reply("Version 1.5.12\n- fixes"), "```\nVersion 1.5.12\n- fixes\n```");
        let dialect = Dialect {
            print_commands: &[PrintCommand { name: "usage", aliases: &["quota"], description: "" }],
            ..super::super::cursor::DIALECT
        };
        assert!(dialect.print_command("/usage"));
        assert!(dialect.print_command(" /quota daily"));
        assert!(!dialect.print_command("/usages"));
        assert!(!dialect.print_command("what is /usage"));
    }

    #[test]
    fn talk_after_a_turn_is_a_follow_up_ended_by_its_usage_update() {
        let ((shared, _rx, seen), _items) = rig_for(&crate::agent::antigravity::DIALECT);
        shared.state.lock().unwrap().session_id = Some("s".into());
        upd(&shared, r#"{"sessionUpdate":"usage_update","used":10,"size":100}"#);
        assert!(matches!(take(&seen)[..], [AgentEvent::ContextUsage { .. }]), "no follow-up from usage alone");
        upd(&shared, r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"The subagent read it."}}"#);
        let events = take(&seen);
        assert_eq!(events[0], AgentEvent::Status { state: SessionState::Working });
        assert!(matches!(&events[1], AgentEvent::TextDelta { .. }));
        upd(&shared, r#"{"sessionUpdate":"usage_update","used":12,"size":100}"#);
        let events = take(&seen);
        assert!(matches!(&events[1], AgentEvent::AssistantBlock { block: Block::Text { text }, .. } if text == "The subagent read it."));
        assert_eq!(events[2], AgentEvent::TurnEnded { ok: true, duration_ms: None });
        assert_eq!(events[3], AgentEvent::Status { state: SessionState::Idle });

        // Inside a turn the same updates are the turn's own.
        shared.state.lock().unwrap().in_turn = true;
        upd(&shared, r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"x"}}"#);
        upd(&shared, r#"{"sessionUpdate":"usage_update","used":13,"size":100}"#);
        assert!(!take(&seen).iter().any(|e| matches!(e, AgentEvent::TurnEnded { .. } | AgentEvent::Status { .. })));
    }

    #[test]
    fn a_question_form_is_shown_as_questions_and_answered_as_a_form() {
        let ((shared, mut rx, seen), _items) = rig_for(&crate::agent::antigravity::DIALECT);
        // As agy 1.5.11 sent it (trimmed).
        line(&shared, r#"{"jsonrpc":"2.0","method":"elicitation/create","id":"3","params":{"sessionId":"s","toolCallId":"call_9","mode":"form","message":"The agent has 2 questions.","requestedSchema":{"type":"object","properties":{
            "q1":{"type":"string","title":"Purpose?","oneOf":[{"const":"1","title":"Sandbox"},{"const":"2","title":"Demo"}]},
            "q1_other":{"type":"string","title":"Other"},
            "q2":{"type":"array","title":"Sections?","items":{"anyOf":[{"const":"a","title":"Usage"},{"const":"b","title":"Files"}]}},
            "q2_other":{"type":"string","title":"Other"}}}}}"#);
        let events = take(&seen);
        let AgentEvent::ApprovalRequest { request_id, approval: ApprovalKind::Question, input, tool_id, .. } = &events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(tool_id.as_deref(), Some("call_9"));
        assert_eq!(input["questions"][0]["question"], "Purpose?");
        assert_eq!(input["questions"][0]["options"][1]["label"], "Demo");
        assert_eq!(input["questions"][1]["multiSelect"], true);
        let process = AcpProcess { shared: shared.clone() };
        let answers = BTreeMap::from([
            ("Purpose?".to_string(), "A scratch folder".to_string()),
            ("Sections?".to_string(), "Usage, Files".to_string()),
        ]);
        process.answer_approval(request_id, Decision::Allow, None, Some(answers), None).unwrap();
        let reply = &written(&mut rx)[0];
        assert_eq!(reply["id"], "3");
        assert_eq!(
            reply["result"],
            json!({ "action": "accept", "content": { "q1_other": "A scratch folder", "q2": ["a", "b"] } })
        );

        // Skipped: declined, and what the user said reaches the agent.
        take(&seen);
        line(&shared, r#"{"jsonrpc":"2.0","method":"elicitation/create","id":"4","params":{"sessionId":"s","mode":"form","message":"Q","requestedSchema":{"type":"object","properties":{"q1":{"type":"string","title":"Purpose?","oneOf":[{"const":"1","title":"Sandbox"}]}}}}}"#);
        let request_id = match &take(&seen)[0] {
            AgentEvent::ApprovalRequest { request_id, .. } => request_id.clone(),
            other => panic!("{other:?}"),
        };
        process.answer_approval(&request_id, Decision::Deny, Some("Just pick".into()), None, None).unwrap();
        assert_eq!(written(&mut rx)[0]["result"], json!({ "action": "decline" }));
        assert_eq!(shared.state.lock().unwrap().notes, vec!["The user skipped your questions. What they said: Just pick".to_string()]);

        // A form with no choices can't be shown: declined at once.
        line(&shared, r#"{"jsonrpc":"2.0","method":"elicitation/create","id":"5","params":{"sessionId":"s","mode":"form","message":"Name?","requestedSchema":{"type":"object","properties":{"name":{"type":"string"}}}}}"#);
        assert_eq!(written(&mut rx)[0]["result"], json!({ "action": "decline" }));
    }

    #[test]
    fn the_agent_switching_its_own_mode_switches_the_chat() {
        // Cursor's SwitchMode, as Cursor CLI 2026.11.19 sends it: no `current_mode_update` follows.
        let ((shared, _rx, seen), _items) = rig_for(&crate::agent::cursor::DIALECT);
        {
            let mut state = shared.state.lock().unwrap();
            state.config.modes = vec!["agent".into(), "plan".into(), "ask".into()];
            state.mode = Some("agent".into());
        }
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"t1","title":"Switch Mode: unknown","kind":"switch_mode","status":"pending","rawInput":{}}"#);
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"t1","title":"Switch Mode: plan","rawInput":{"targetModeId":"plan","explanation":"Plan first."}}"#);
        assert_eq!(shared.state.lock().unwrap().mode.as_deref(), Some("agent"), "not until it's done");
        take(&seen);
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"completed"}"#);
        assert_eq!(shared.state.lock().unwrap().mode.as_deref(), Some("plan"));
        assert!(take(&seen).iter().any(|e| matches!(e, AgentEvent::ModeChanged { mode } if mode == "plan")));

        // A mode the session doesn't have is ignored.
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"t2","status":"completed","rawInput":{"targetModeId":"debug"}}"#);
        assert_eq!(shared.state.lock().unwrap().mode.as_deref(), Some("plan"));
    }

    #[test]
    fn a_plan_written_as_a_file_is_offered_once_the_turn_ends() {
        let ((shared, mut rx, seen), mut items) = rig_for(&crate::agent::antigravity::DIALECT);
        let plan = r##"{"sessionUpdate":"tool_call","toolCallId":"call_5","name":"write_file","status":"in_progress","kind":"edit","title":"File plan.md","rawInput":{"ArtifactMetadata":{"RequestFeedback":true},"Content":"# Plan","Path":"/brain/s/plan.md"}}"##;
        let done = plan.replace("in_progress", "completed");
        // Outside plan mode it is a plan too, until the agent goes on past it.
        upd(&shared, plan);
        upd(&shared, &done);
        assert!(shared.state.lock().unwrap().written_plan.as_ref().is_some_and(|(id, ..)| id == "call_5"));
        let events = take(&seen);
        assert!(matches!(&events[0], AgentEvent::AssistantBlock { block: Block::ToolCall { name, .. }, .. } if name == "ExitPlanMode"));
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"call_9","name":"run_command","status":"in_progress","kind":"execute","title":"ls","rawInput":{"CommandLine":"ls"}}"#);
        assert!(shared.state.lock().unwrap().written_plan.is_none(), "built on: no card");

        // In plan mode a file written after the plan doesn't drop it.
        upd(&shared, r#"{"sessionUpdate":"current_mode_update","currentModeId":"plan"}"#);
        upd(&shared, &plan.replace("call_5", "call_6"));
        upd(&shared, &done.replace("call_5", "call_6"));
        upd(&shared, r#"{"sessionUpdate":"tool_call","toolCallId":"call_8","name":"write_file","status":"in_progress","kind":"edit","title":"File task.md","rawInput":{"Content":"- [ ] one","Path":"/brain/s/task.md"}}"#);
        let (tool_id, text, path) = shared.state.lock().unwrap().written_plan.take().unwrap();
        assert_eq!((tool_id.as_str(), text.as_str(), path.as_str()), ("call_6", "# Plan", "/brain/s/plan.md"));
        take(&seen);

        // The turn ended: the plan waits for the user, with no request to answer.
        offer_plan(&shared, tool_id, text, path);
        let events = take(&seen);
        let AgentEvent::ApprovalRequest { request_id, approval: ApprovalKind::Plan, input, .. } = &events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(input["planFilePath"], "/brain/s/plan.md");
        let process = AcpProcess { shared: shared.clone() };
        process
            .answer_approval(request_id, Decision::Allow, None, None, None)
            .unwrap();
        assert!(written(&mut rx).is_empty(), "nothing to reply to");
        let state = shared.state.lock().unwrap();
        assert!(state.build_after_turn);
        assert_eq!(state.notes, vec!["The user approved the plan. Build it now.".to_string()]);
        drop(state);
        assert!(matches!(items.try_recv(), Ok(Item::Note(text)) if text.is_empty()), "the driver is woken");

        // Not approved: what they said is the next prompt.
        shared.state.lock().unwrap().notes.clear();
        take(&seen);
        offer_plan(&shared, "call_7".into(), "# Plan".into(), String::new());
        let request_id = match &take(&seen)[0] {
            AgentEvent::ApprovalRequest { request_id, .. } => request_id.clone(),
            other => panic!("{other:?}"),
        };
        process
            .answer_approval(&request_id, Decision::Deny, Some("Smaller steps".into()), None, None)
            .unwrap();
        assert_eq!(
            shared.state.lock().unwrap().notes,
            vec!["The user didn't approve the plan yet. What they said: Smaller steps".to_string()]
        );
        assert!(items.try_recv().is_ok());
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
                tool("other", "tilde_browser: open", json!({ "providerIdentifier": "tilde_browser", "toolName": "open", "args": { "url": "x" } })),
                "mcp__tilde_browser__open",
                json!({ "url": "x" }),
            ),
            (tool("other", "Create Plan", json!({ "_toolName": "createPlan", "plan": "P" })), "ExitPlanMode", json!({ "plan": "P" })),
        ];
        for (tool, name, input) in cases {
            assert_eq!(crate::agent::cursor::name_and_input(&tool, &[]), (name.to_string(), input));
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
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"t","title":"tilde_browser: open","kind":"other","rawInput":{"providerIdentifier":"tilde_browser","toolName":"open","args":{}}}"#);
        take(&seen);
        line(&shared, r#"{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t","title":"tilde_browser-open: open","kind":"other"},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}}"#);
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
            server: "tilde_chats",
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
                r#"{{"sessionUpdate":"tool_call","toolCallId":"{id}","title":"tilde_chats: {tool}","kind":"other","status":"completed","rawInput":{{"providerIdentifier":"tilde_chats","toolName":"{tool}","args":{args}}},"rawOutput":{{"success":true}}}}"#
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

    #[test]
    fn a_side_chat_refuses_every_ask_at_once_but_lets_the_apps_tools_run() {
        let ((shared, mut rx, seen), _) = rig_as(&crate::agent::cursor::DIALECT, true);
        line(&shared, r#"{"jsonrpc":"2.0","id":1,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t","title":"`rm x`","kind":"execute"},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}}"#);
        line(&shared, r#"{"jsonrpc":"2.0","id":2,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"u","title":"x","kind":"edit"},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"}]}}"#);
        line(&shared, r#"{"jsonrpc":"2.0","id":3,"method":"cursor/ask_question","params":{"toolCallId":"q","questions":[{"id":"a","prompt":"Which?","options":[{"id":"1","label":"One"}]}]}}"#);
        line(&shared, r#"{"jsonrpc":"2.0","id":4,"method":"cursor/create_plan","params":{"toolCallId":"p","plan":"do it"}}"#);
        upd(&shared, r#"{"sessionUpdate":"tool_call_update","toolCallId":"b","title":"tilde_browser: open","kind":"other","rawInput":{"providerIdentifier":"tilde_browser","toolName":"open","args":{}}}"#);
        line(&shared, r#"{"jsonrpc":"2.0","id":5,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"b","title":"tilde_browser-open: open","kind":"other"},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"}]}}"#);
        assert!(take(&seen).iter().all(|e| !matches!(e, AgentEvent::ApprovalRequest { .. })));
        assert!(shared.state.lock().unwrap().waiting.is_empty());
        let out = written(&mut rx);
        assert_eq!(out[0]["result"]["outcome"]["optionId"], "reject-once");
        assert_eq!(out[1]["result"]["outcome"]["outcome"], "cancelled");
        assert_eq!(out[2]["result"]["outcome"]["outcome"], "skipped");
        assert_eq!(out[3]["result"]["outcome"]["outcome"], "rejected");
        assert_eq!(out[4]["result"]["outcome"]["optionId"], "allow-once");
    }

    #[test]
    fn a_side_chat_is_told_it_is_one_and_may_only_read_the_notes() {
        let dir = std::env::temp_dir().join(format!("tilde-acp-side-{}", uuid::Uuid::new_v4()));
        let opts = SpawnOptions {
            binary: "agent".into(),
            project_path: "/p".into(),
            resume_id: None,
            mode: None,
            model: None,
            effort: None,
            side: Some(crate::agent::SideOptions { fork_from: None, first: true }),
            tools: Vec::new(),
            notes: Some(dir.join("notes.md")),
            project_docs: None,
            memory: Some(dir.join("memory")),
            preferences: None,
            env: Vec::new(),
        };
        let told = instructions(&opts, "").unwrap();
        assert!(told.contains(crate::agent::SIDE_INSTRUCTION));
        assert_eq!(told, told.replace(&crate::agent::notes_instruction(&dir.join("notes.md"), None, true), ""));
        assert!(told.contains(&crate::agent::notes_instruction(&dir.join("notes.md"), None, false)));
        assert!(!told.contains(&crate::agent::memory_instruction(&dir.join("memory"))));
    }
}
