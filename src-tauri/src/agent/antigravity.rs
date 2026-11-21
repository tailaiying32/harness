//! Antigravity's command-line agent (`agy`), driven over ACP (`agy acp`; see
//! `acp.rs`). Tested on agy 1.5.11; what it sends is in PLAN.md,
//! "Antigravity over ACP".

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::acp::{self, AcpConfig, Dialect, Intro, PrintCommand, Tool};
use super::acp_sync::{self, SyncRpc, effort_levels, model_option};
use super::{AgentAdapter, EventSink, RunningAgent, SideCopy, SpawnOptions, copy_database, copy_folder, plain_id};
use crate::events::{AgentEvent, CommandInfo, ModelInfo};

pub struct AntigravityAdapter;

const LABEL: &str = "Antigravity";
/// How long listing the models may take in all.
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
/// How long listing the commands may take, from start to its list.
const COMMANDS_TIMEOUT: Duration = Duration::from_secs(20);
/// How long signing in may wait for the user in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
/// How long reading the usage limits may take (about 2 s on agy 1.5.11).
const USAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// How a plan reaches the app's approve card: Antigravity's own plan
/// artifact, marked for feedback, in any mode (it can't enter plan mode
/// itself). Asked for a plan, it sometimes wrote one and sometimes put it in
/// the chat (tested on agy 1.5.12).
const PLAN_INSTRUCTION: &str = "When you present a plan for the user to approve before building, \
write it as your implementation plan artifact with RequestFeedback set, then end your turn. Don't \
write the plan only in the chat: the app shows that artifact with approve buttons and tells you when \
to build.";

/// How Antigravity's ACP differs: its own tool names and capitalized inputs,
/// a system prompt the app can add to, and plans written as files.
pub static DIALECT: Dialect = Dialect {
    name_tool: name_and_input,
    own_tool,
    is_plan,
    clean_output,
    build_mode: "default",
    plan_instruction: PLAN_INSTRUCTION,
    intro: Intro::SystemPrompt,
    session_meta,
    pick_default_model: false,
    forms: true,
    print_commands: PRINT_COMMANDS,
    compacts: true,
};

/// Antigravity's own commands that run in print mode (`agy -p /help`, agy
/// 1.5.12) but not over ACP, where they reach the model. `/model` and
/// `/effort` are left to the app's model menu.
static PRINT_COMMANDS: &[PrintCommand] = &[
    PrintCommand { name: "usage", aliases: &["quota"], description: "View model quota usage" },
    PrintCommand { name: "credits", aliases: &[], description: "Show remaining G1 credits and purchase link" },
    PrintCommand { name: "skills", aliases: &[], description: "List available skills" },
    PrintCommand { name: "agents", aliases: &[], description: "List available custom agents" },
    PrintCommand { name: "plugin", aliases: &[], description: "List plugins" },
    PrintCommand { name: "hooks", aliases: &[], description: "List hook configurations for tool events" },
    PrintCommand { name: "permissions", aliases: &[], description: "List tool permissions" },
    PrintCommand { name: "config", aliases: &["settings"], description: "Show settings" },
    PrintCommand { name: "changelog", aliases: &[], description: "Show release notes and changes" },
];

impl AgentAdapter for AntigravityAdapter {
    fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
        let mut opts = opts;
        let side = opts.side.is_some();
        // Antigravity has no mode without edits: a side chat runs in Default,
        // which asks before edits and commands, and every ask is refused
        // (`AcpConfig::side`).
        if side {
            opts.mode = Some("default".to_string());
        }
        // The chat's notes folder, the project's docs and its memory are
        // outside the project: added to the workspace so the agent may write
        // there without being turned away.
        let mut args = vec!["acp".to_string()];
        let notes_dir = opts.notes.as_deref().and_then(Path::parent);
        for dir in [notes_dir, opts.project_docs.as_deref(), opts.memory.as_deref()].into_iter().flatten() {
            args.extend(["--add-dir".to_string(), dir.display().to_string()]);
        }
        let config = AcpConfig {
            label: LABEL,
            args,
            client_meta: json!({}),
            dialect: &DIALECT,
            side,
        };
        if side {
            return acp::spawn(opts.binary.clone(), config, opts, sink);
        }
        // Usage limits aren't sent over ACP: read them now and after each
        // turn (main chats only; a side chat's readings aren't used).
        let usage = UsageReader::new(opts.binary.clone(), sink.clone());
        usage.read();
        let sink: EventSink = Arc::new(move |event| {
            let ended = matches!(event, AgentEvent::TurnEnded { .. });
            sink(event);
            if ended {
                usage.read();
            }
        });
        acp::spawn(opts.binary.clone(), config, opts, sink)
    }

    /// Opens a conversation in an empty folder (nothing is sent, so nothing
    /// is spent) and reads the models in the order Antigravity lists them.
    /// Thinking levels differ by model (Claude Opus stops at High, GPT-OSS
    /// has none) and only show once a model is picked, so each model in
    /// `efforts_for` (all if None) is picked in turn.
    fn list_models(&self, binary: &Path, efforts_for: Option<&[String]>) -> Result<Vec<ModelInfo>, String> {
        let folder = std::env::temp_dir().join("harness-antigravity-models");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let mut rpc = SyncRpc::start(binary, &["acp"], LABEL, &folder)?;
        let deadline = Instant::now() + LIST_TIMEOUT;
        let session = rpc.open_session(json!({}), &folder, deadline)?;
        let session_id = session["sessionId"].as_str().ok_or("Antigravity started no conversation")?.to_string();
        let model_id = model_option(&session["configOptions"])
            .and_then(|o| o["id"].as_str())
            .unwrap_or("model")
            .to_string();
        let mut models = models_from(&session["configOptions"]);
        let current = model_option(&session["configOptions"]).and_then(|o| o["currentValue"].as_str()).map(str::to_owned);
        for info in &mut models {
            if current.as_deref() == Some(info.value.as_str()) {
                continue;
            }
            if efforts_for.is_some_and(|wanted| !wanted.contains(&info.value)) {
                continue;
            }
            let picked = rpc.call(
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": model_id, "value": info.value }),
                deadline,
            );
            if let Ok(reply) = picked {
                let (levels, default) = effort_levels(&reply["configOptions"]).unwrap_or_default();
                info.efforts = levels;
                info.default_effort = default;
            }
        }
        Ok(models)
    }

    /// What ACP lists, and the commands the app runs in print mode.
    fn list_commands(&self, binary: &Path, folder: &Path) -> Result<Vec<CommandInfo>, String> {
        let mut commands = acp_sync::list_commands(binary, &["acp"], LABEL, json!({}), folder, COMMANDS_TIMEOUT)?;
        commands.extend(PRINT_COMMANDS.iter().map(|c| CommandInfo {
            name: c.name.to_string(),
            description: c.description.to_string(),
            hint: None,
            aliases: c.aliases.iter().map(|a| a.to_string()).collect(),
            skill: false,
        }));
        Ok(commands)
    }

    /// Antigravity names each conversation itself (`session_info_update`).
    fn make_title(&self, _binary: &Path, _first_message: &str) -> Result<String, String> {
        Err("Antigravity writes its own titles".to_string())
    }

    /// ACP's own sign-in: Antigravity opens the browser and answers once
    /// the user has signed in with Google. (`agy acp --login` instead wants
    /// a code pasted back into a terminal.)
    fn sign_in(&self, binary: &Path) -> Result<(), String> {
        let folder = std::env::temp_dir();
        let mut rpc = SyncRpc::start(binary, &["acp"], LABEL, &folder)?;
        let deadline = Instant::now() + SIGN_IN_TIMEOUT;
        rpc.call(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
                "clientInfo": { "name": "harness", "version": env!("CARGO_PKG_VERSION") },
            }),
            deadline,
        )?;
        rpc.call("authenticate", json!({ "methodId": "oauth-personal" }), deadline)
            .map(|_| ())
            .map_err(|e| if e.contains("took too long") { "Sign-in took too long".to_string() } else { e })
    }

    fn forget_session(&self, agent_session_id: &str) {
        if let Some(data) = data_folder() {
            forget_in(&data, agent_session_id);
        }
    }

    fn has_session(&self, agent_session_id: &str) -> bool {
        plain_id(agent_session_id)
            && data_folder().is_some_and(|data| data.join("conversations").join(format!("{agent_session_id}.db")).is_file())
    }

    fn side_copy(&self) -> SideCopy {
        SideCopy::Files
    }

    fn copy_session(&self, agent_session_id: &str) -> Result<String, String> {
        let data = data_folder().ok_or("No home folder")?;
        copy_session_in(&data, agent_session_id)
    }
}

/// Where agy keeps its conversations: `~/.gemini/antigravity-cli/`, with
/// `conversations/<id>.db` (the conversation), `annotations/<id>.pbtxt` and a
/// `brain/<id>/` folder for each. Not documented; seen on agy 1.5.12.
fn data_folder() -> Option<PathBuf> {
    crate::home::user_home().map(|home| home.join(".gemini").join("antigravity-cli"))
}

/// A session's saved things, for a plain ID only: its database, notes file
/// and folder.
fn session_paths(data: &Path, id: &str) -> Option<[PathBuf; 3]> {
    plain_id(id).then(|| {
        [
            data.join("conversations").join(format!("{id}.db")),
            data.join("annotations").join(format!("{id}.pbtxt")),
            data.join("brain").join(id),
        ]
    })
}

/// Copies a conversation under a new ID: the database whole (with what is
/// still in its log), its notes file and folder if there are any. The
/// database names its conversation in `trajectory_meta.cascade_id`, which
/// must be the new ID or the copy loads but can't take a message ("trajectory
/// not found"). The old ID also shows up inside the stored steps; tested
/// without changing those: the copy remembers the chat, and neither sees the
/// other's messages after.
fn copy_session_in(data: &Path, id: &str) -> Result<String, String> {
    let [db, notes, brain] = session_paths(data, id).ok_or("Not a session ID")?;
    if !db.is_file() {
        return Err(format!("Antigravity has no saved conversation {id}"));
    }
    let new = uuid::Uuid::new_v4().to_string();
    let [new_db, new_notes, new_brain] = session_paths(data, &new).ok_or("Not a session ID")?;
    let copied = (|| {
        copy_database(&db, &new_db)?;
        let copy = rusqlite::Connection::open(&new_db).map_err(|e| e.to_string())?;
        let named = copy
            .execute("UPDATE trajectory_meta SET cascade_id = ?1", [&new])
            .map_err(|e| format!("Couldn't rename the copy: {e}"))?;
        if named == 0 {
            return Err("The copy names no conversation".to_string());
        }
        drop(copy);
        if notes.is_file() {
            std::fs::copy(&notes, &new_notes).map_err(|e| e.to_string())?;
        }
        if brain.is_dir() {
            copy_folder(&brain, &new_brain).map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    })();
    match copied {
        Ok(()) => Ok(new),
        Err(e) => {
            forget_in(data, &new);
            Err(e)
        }
    }
}

/// Deletes a conversation's saved things, its database's log files too.
fn forget_in(data: &Path, id: &str) {
    let Some([db, notes, brain]) = session_paths(data, id) else {
        return;
    };
    for file in [db.clone(), db.with_extension("db-wal"), db.with_extension("db-shm"), notes] {
        let _ = std::fs::remove_file(file);
    }
    let _ = std::fs::remove_dir_all(brain);
}

/// The models in a `configOptions` list. Only the current model carries the
/// thinking levels from this reply; any other model's are read once it is picked.
fn models_from(options: &Value) -> Vec<ModelInfo> {
    let Some(option) = model_option(options) else { return Vec::new() };
    let current = option["currentValue"].as_str();
    let (efforts, default_effort) = effort_levels(options).unwrap_or_default();
    option["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            let value = o["value"].as_str()?.to_string();
            let name = o["name"].as_str().unwrap_or(&value).to_string();
            let is_current = current == Some(value.as_str());
            Some(ModelInfo {
                value,
                name,
                detail: None,
                efforts: if is_current { efforts.clone() } else { Vec::new() },
                default_effort: if is_current { default_effort.clone() } else { None },
            })
        })
        .collect()
}

/// Reads a chat's usage limits with `agy -p /usage`, which prints them
/// without asking the model or saving a conversation. One read at a time.
struct UsageReader {
    binary: PathBuf,
    sink: EventSink,
    busy: Arc<AtomicBool>,
}

impl UsageReader {
    fn new(binary: PathBuf, sink: EventSink) -> Self {
        UsageReader { binary, sink, busy: Arc::default() }
    }

    /// Starts a read in the background; skipped while one is running.
    fn read(&self) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let (binary, sink, busy) = (self.binary.clone(), self.sink.clone(), self.busy.clone());
        std::thread::spawn(move || {
            if let Some(text) = run_usage(&binary) {
                for event in usage_events(&text) {
                    sink(event);
                }
            }
            busy.store(false, Ordering::SeqCst);
        });
    }
}

/// What `agy -p /usage` prints, or None if it failed or took too long.
fn run_usage(binary: &Path) -> Option<String> {
    use std::io::Read;
    let mut cmd = Command::new(binary);
    cmd.args(["-p", "/usage"])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let text = rx.recv_timeout(USAGE_TIMEOUT).ok();
    if text.is_none() {
        let _ = child.kill();
    }
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    text.filter(|_| ok)
}

/// The limits in `/usage`'s table, as the app's usage readings. Each line is
/// group, limit, percent left, reset time (UTC), split by tabs:
/// `Gemini Models\tFive Hour Limit Remaining\t96%\t2026-11-19T20:03:27Z`.
/// Named "gemini 5h", "claude and gpt week", "gemini 4 argon week"; each
/// group's shorter window first.
fn usage_events(text: &str) -> Vec<AgentEvent> {
    let mut groups: Vec<String> = Vec::new();
    let mut readings: Vec<(usize, u8, AgentEvent)> = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').map(str::trim).collect();
        let [group, limit, left, resets] = cols[..] else { continue };
        let Some(left) = left.strip_suffix('%').and_then(|n| n.trim().parse::<f64>().ok()) else { continue };
        let group = group.to_lowercase();
        let group = group.strip_suffix(" models").unwrap_or(&group).to_string();
        let (period, rank) = period(&limit.to_lowercase());
        let index = groups.iter().position(|g| *g == group).unwrap_or_else(|| {
            groups.push(group.clone());
            groups.len() - 1
        });
        let resets_at = chrono::DateTime::parse_from_rfc3339(resets).ok().and_then(|t| u64::try_from(t.timestamp()).ok());
        readings.push((
            index,
            rank,
            AgentEvent::UsageUpdate {
                window: Some(format!("{group} {period}")),
                utilization: (1.0 - left / 100.0).clamp(0.0, 1.0),
                resets_at,
            },
        ));
    }
    readings.sort_by_key(|(group, rank, _)| (*group, *rank));
    readings.into_iter().map(|(_, _, event)| event).collect()
}

/// A limit's window from its name ("Five Hour Limit Remaining" is "5h"),
/// with a rank that puts shorter windows first.
fn period(limit: &str) -> (String, u8) {
    if limit.contains("five hour") {
        ("5h".to_string(), 0)
    } else if limit.contains("daily") || limit.contains("day") {
        ("day".to_string(), 1)
    } else if limit.contains("week") {
        ("week".to_string(), 2)
    } else if limit.contains("month") {
        ("month".to_string(), 3)
    } else {
        let name = limit.replace("limit", "").replace("remaining", "");
        (name.split_whitespace().collect::<Vec<_>>().join(" "), 4)
    }
}

/// The app's instructions, added to Antigravity's system prompt.
fn session_meta(instructions: Option<&str>) -> Value {
    match instructions {
        Some(text) => json!({ "antigravity": { "systemPrompt": { "append": text } } }),
        None => Value::Null,
    }
}

/// A plan: a file it writes asking for the user's feedback.
fn is_plan(tool: &Tool) -> bool {
    tool.raw_input["ArtifactMetadata"]["RequestFeedback"] == true && tool.raw_input["Content"].is_string()
}

/// A command's output starts with when it ran and how it ended; the output
/// itself follows "Command output:". Lines end in `\r\n` (a terminal's).
fn clean_output(text: String) -> String {
    let text = match text.split_once("Command output:\n") {
        Some((_, output)) => output.to_string(),
        None => text,
    };
    text.replace("\r\n", "\n")
}

/// The app's server name and tool name in `mcp_<server>_<tool>`. Server
/// names have underscores of their own, so only known ones are matched.
fn own_parts<'a>(name: &'a str, own: &'a [String]) -> Option<(&'a str, &'a str)> {
    let rest = name.strip_prefix("mcp_")?;
    own.iter()
        .find_map(|server| Some((server.as_str(), rest.strip_prefix(server.as_str())?.strip_prefix('_')?)))
}

fn own_tool(tool: &Tool, own: &[String]) -> bool {
    own_parts(&tool.name, own).is_some()
}

/// The first of `keys` the input has, as a string.
fn first<'a>(raw: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| raw[*k].as_str())
}

/// The app's name for a tool call and its input, in the shape the window
/// words Claude Code's tools in (`tools.ts`). Antigravity names its tools
/// (`run_command`, `read_file`, `write_file`, `mcp_<server>_<tool>`) and
/// capitalizes their inputs (`CommandLine`, `Path`).
fn name_and_input(tool: &Tool, own: &[String]) -> (String, Value) {
    let raw = &tool.raw_input;
    let named = |name: &str, input: Value| (name.to_string(), input);
    let path = first(raw, &["Path", "AbsolutePath", "TargetFile", "File"])
        .map(str::to_owned)
        .or_else(|| tool.locations.first().cloned())
        .unwrap_or_default();
    if is_plan(tool) {
        return named("ExitPlanMode", json!({ "plan": raw["Content"], "planFilePath": path }));
    }
    // An MCP tool: the app's own, or one the user set up for Antigravity.
    if let Some(rest) = tool.name.strip_prefix("mcp_") {
        let mut input = raw.clone();
        if let Some(fields) = input.as_object_mut() {
            fields.remove("toolSummary");
        }
        let full = match own_parts(&tool.name, own) {
            Some((server, name)) => format!("mcp__{server}__{name}"),
            None => format!("mcp__{rest}"),
        };
        return (full, input);
    }
    let name = tool.name.as_str();
    match name {
        // Its questions; the answer comes from the form it sends with them.
        "ask_question" => {
            let questions: Vec<Value> = raw["questions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|q| {
                    json!({
                        "question": q["question"],
                        "header": "",
                        "multiSelect": q["is_multi_select"] == true,
                        "options": q["options"].as_array().into_iter().flatten()
                            .map(|o| json!({ "label": o, "description": "" })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            return named("AskUserQuestion", json!({ "questions": questions }));
        }
        // A subagent, worded like Claude Code's. Its own steps aren't sent.
        "invoke_subagent" => {
            return named(
                "Agent",
                json!({ "description": raw["Role"], "prompt": raw["Prompt"], "subagent_type": raw["TypeName"] }),
            );
        }
        _ => {}
    }
    match (name, tool.kind.as_str()) {
        ("run_command", _) | (_, "execute") => named("Bash", json!({ "command": first(raw, &["CommandLine", "Command"]) })),
        (_, "read") => named("Read", json!({ "file_path": path })),
        ("write_file" | "write_to_file", _) => named("Write", json!({ "file_path": path })),
        (_, "edit") => named("Edit", json!({ "file_path": path })),
        (_, "delete") => named("Delete", json!({ "file_path": path })),
        (_, "move") => named("Move", json!({ "file_path": path })),
        (_, "search") => {
            let pattern = first(raw, &["Query", "Pattern", "SearchPattern"]);
            let dir = first(raw, &["SearchPath", "SearchDirectory", "DirectoryPath", "Path"]);
            let glob = name.contains("find") || tool.title.starts_with("Find");
            named(if glob { "Glob" } else { "Grep" }, json!({ "pattern": pattern, "path": dir }))
        }
        (_, "fetch") => match first(raw, &["Url", "URL", "url"]) {
            Some(url) => named("WebFetch", json!({ "url": url })),
            None => named("WebSearch", json!({ "query": first(raw, &["Query", "query"]).unwrap_or(&tool.title) })),
        },
        _ => {
            let title = tool.title.trim();
            named(if title.is_empty() { "a tool" } else { title }, raw.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(line: &str) -> Tool {
        let u: Value = serde_json::from_str(line).unwrap();
        let mut tool = Tool::default();
        tool.name = u["name"].as_str().unwrap_or_default().into();
        tool.title = u["title"].as_str().unwrap_or_default().into();
        tool.kind = u["kind"].as_str().unwrap_or_default().into();
        tool.raw_input = u["rawInput"].clone();
        tool.locations = u["locations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["path"].as_str().map(str::to_owned))
            .collect();
        tool
    }

    // Lines as agy 1.5.11 sent them.
    const LS: &str = r#"{"sessionUpdate":"tool_call","toolCallId":"call_1","name":"run_command","args":{"CommandLine":"ls -la","WaitMsBeforeAsync":5000,"toolSummary":"List directory contents"},"status":"in_progress","kind":"execute","title":"ls -la","rawInput":{"CommandLine":"ls -la","WaitMsBeforeAsync":5000,"toolSummary":"List directory contents"}}"#;
    const READ: &str = r#"{"sessionUpdate":"tool_call","toolCallId":"call_2","name":"read_file","status":"in_progress","kind":"read","title":"File hello.txt","locations":[{"path":"/w/hello.txt"}],"rawInput":{"Path":"/w/hello.txt"}}"#;
    const WRITE: &str = r#"{"sessionUpdate":"tool_call","toolCallId":"call_3","name":"write_file","status":"in_progress","kind":"edit","title":"File out.txt","locations":[{"path":"/w/out.txt"}],"rawInput":{"Content":"hi","Path":"/w/out.txt"}}"#;
    const MCP: &str = r#"{"sessionUpdate":"tool_call","toolCallId":"call_4","name":"mcp_harness_worktree_ask","status":"in_progress","kind":"other","title":"Run mcp_harness_worktree_ask","rawInput":{"name":"fix it","toolSummary":"Ask for a worktree"}}"#;
    const PLAN: &str = r##"{"sessionUpdate":"tool_call","toolCallId":"call_5","name":"write_file","status":"in_progress","kind":"edit","title":"File readme_plan.md","rawInput":{"ArtifactMetadata":{"RequestFeedback":true,"Summary":"Plan","UserFacing":true},"Content":"# Plan\n- do it","Path":"/brain/s/readme_plan.md"}}"##;

    #[test]
    fn tools_get_the_names_the_window_words() {
        let own = vec!["harness_worktree".to_string(), "harness_new_chats".to_string()];
        let cases = [
            (LS, "Bash", json!({ "command": "ls -la" })),
            (READ, "Read", json!({ "file_path": "/w/hello.txt" })),
            (WRITE, "Write", json!({ "file_path": "/w/out.txt" })),
            (MCP, "mcp__harness_worktree__ask", json!({ "name": "fix it" })),
            (PLAN, "ExitPlanMode", json!({ "plan": "# Plan\n- do it", "planFilePath": "/brain/s/readme_plan.md" })),
        ];
        for (line, name, input) in cases {
            assert_eq!(name_and_input(&tool(line), &own), (name.to_string(), input), "{line}");
        }
        let ask = r#"{"name":"ask_question","kind":"other","rawInput":{"questions":[{"is_multi_select":false,"options":["A","B"],"question":"Which?"}]}}"#;
        assert_eq!(
            name_and_input(&tool(ask), &own),
            (
                "AskUserQuestion".to_string(),
                json!({ "questions": [{ "question": "Which?", "header": "", "multiSelect": false,
                    "options": [{ "label": "A", "description": "" }, { "label": "B", "description": "" }] }] })
            )
        );
        let sub = r#"{"name":"invoke_subagent","kind":"other","rawInput":{"Prompt":"Read hello.txt","Role":"File Reader","TypeName":"research"}}"#;
        assert_eq!(
            name_and_input(&tool(sub), &own),
            ("Agent".to_string(), json!({ "description": "File Reader", "prompt": "Read hello.txt", "subagent_type": "research" }))
        );
        // Someone else's MCP server keeps its own name.
        let other = MCP.replace("mcp_harness_worktree_ask", "mcp_github_list_issues");
        assert_eq!(name_and_input(&tool(&other), &own).0, "mcp__github_list_issues");
    }

    #[test]
    fn the_apps_own_tools_are_known_by_their_whole_server_name() {
        let own = vec!["harness_new_chats".to_string(), "harness_chats".to_string()];
        assert_eq!(own_parts("mcp_harness_new_chats_start", &own), Some(("harness_new_chats", "start")));
        assert_eq!(own_parts("mcp_harness_chats_read", &own), Some(("harness_chats", "read")));
        assert_eq!(own_parts("mcp_harness_chatsy_read", &own), None);
        assert!(own_tool(&tool(MCP), &["harness_worktree".to_string()]));
        assert!(!own_tool(&tool(LS), &["harness_worktree".to_string()]));
    }

    #[test]
    fn only_a_file_asking_for_feedback_is_a_plan() {
        assert!(is_plan(&tool(PLAN)));
        assert!(!is_plan(&tool(WRITE)));
    }

    #[test]
    fn command_output_loses_its_header() {
        let raw = "Created At: 2026-11-18T17:44:09-05:00\nCompleted At: 2026-11-18T17:44:09-05:00\nCommand exited with code 0.\n\nCommand output:\ntotal 8\r\n-rw-r--r-- hello.txt\r\n";
        assert_eq!(clean_output(raw.to_string()), "total 8\n-rw-r--r-- hello.txt\n");
        assert_eq!(clean_output("Edited /w/out.txt".to_string()), "Edited /w/out.txt");
    }

    #[test]
    fn only_the_current_model_starts_with_thinking_levels() {
        let options = json!([
            { "id": "model", "category": "model", "currentValue": "gemini-4-neon", "options": [
                { "value": "gemini-4-neon", "name": "Gemini 4 Neon" },
                { "value": "claude-opus-5-5", "name": "Claude Opus 5.5" },
            ]},
            { "id": "thought_level", "category": "thought_level", "currentValue": "high", "options": [
                { "value": "low" }, { "value": "medium" }, { "value": "high" }, { "value": "xhigh" },
            ]},
            { "id": "mode", "category": "mode", "currentValue": "default", "options": [] },
        ]);
        let models = models_from(&options);
        assert_eq!(models.iter().map(|m| m.value.as_str()).collect::<Vec<_>>(), ["gemini-4-neon", "claude-opus-5-5"]);
        assert_eq!(models[0].efforts, ["low", "medium", "high", "xhigh"]);
        assert_eq!(models[0].default_effort.as_deref(), Some("high"));
        assert_eq!(models[1].name, "Claude Opus 5.5");
        assert!(models[1].efforts.is_empty());
        assert_eq!(models[1].default_effort, None);
    }

    #[test]
    fn instructions_go_in_the_system_prompt() {
        assert_eq!(session_meta(Some("Be brief.")), json!({ "antigravity": { "systemPrompt": { "append": "Be brief." } } }));
        assert_eq!(session_meta(None), Value::Null);
    }

    #[test]
    fn usage_table_becomes_readings() {
        // As printed by `agy -p /usage` on agy 1.5.11.
        let text = "Gemini Models\tWeekly Limit Remaining\t97%\t2026-11-25T22:40:16Z\n\
            Gemini Models\tFive Hour Limit Remaining\t96%\t2026-11-19T20:03:27Z\n\
            Claude and GPT models\tWeekly Limit Remaining\t100%\t2026-11-26T15:35:06Z\n\
            Claude and GPT models\tFive Hour Limit Remaining\t100%\t2026-11-19T20:35:06Z\n\
            Gemini 4 Argon\tGemini 4 Argon Weekly Limit\t100%\t2026-11-26T15:35:06Z\n\
            not a limit\n";
        let got: Vec<(String, f64, Option<u64>)> = usage_events(text)
            .into_iter()
            .map(|e| match e {
                AgentEvent::UsageUpdate { window, utilization, resets_at } => {
                    (window.unwrap(), (utilization * 100.0).round(), resets_at)
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("gemini 5h".to_string(), 4.0, Some(1795118607)),
                ("gemini week".to_string(), 3.0, Some(1795646416)),
                ("claude and gpt 5h".to_string(), 0.0, Some(1795120506)),
                ("claude and gpt week".to_string(), 0.0, Some(1795707306)),
                ("gemini 4 argon week".to_string(), 0.0, Some(1795707306)),
            ]
        );
        assert!(usage_events("").is_empty());
    }

    #[test]
    fn a_conversation_is_copied_under_a_new_id_that_its_database_names_and_forgotten_whole() {
        let data = std::env::temp_dir().join(format!("harness-agy-copy-{}", uuid::Uuid::new_v4()));
        for folder in ["conversations", "annotations", "brain/c-1"] {
            std::fs::create_dir_all(data.join(folder)).unwrap();
        }
        let db = rusqlite::Connection::open(data.join("conversations/c-1.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE trajectory_meta (trajectory_id TEXT, cascade_id TEXT);
             INSERT INTO trajectory_meta VALUES ('t-1', 'c-1');",
        )
        .unwrap();
        drop(db);
        std::fs::write(data.join("annotations/c-1.pbtxt"), "x").unwrap();
        std::fs::write(data.join("brain/c-1/scratch"), "y").unwrap();

        let new = copy_session_in(&data, "c-1").unwrap();
        let named: String = rusqlite::Connection::open(data.join(format!("conversations/{new}.db")))
            .unwrap()
            .query_row("SELECT cascade_id FROM trajectory_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(named, new);
        assert_eq!(std::fs::read_to_string(data.join(format!("annotations/{new}.pbtxt"))).unwrap(), "x");
        assert_eq!(std::fs::read_to_string(data.join(format!("brain/{new}/scratch"))).unwrap(), "y");
        // The original is untouched.
        let original: String = rusqlite::Connection::open(data.join("conversations/c-1.db"))
            .unwrap()
            .query_row("SELECT cascade_id FROM trajectory_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(original, "c-1");

        forget_in(&data, &new);
        assert!(!data.join(format!("conversations/{new}.db")).exists());
        assert!(!data.join(format!("annotations/{new}.pbtxt")).exists());
        assert!(!data.join(format!("brain/{new}")).exists());
        assert!(data.join("conversations/c-1.db").exists());
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_conversation_with_only_a_database_still_copies_and_a_bad_one_leaves_nothing() {
        let data = std::env::temp_dir().join(format!("harness-agy-copy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(data.join("conversations")).unwrap();
        let db = rusqlite::Connection::open(data.join("conversations/c-1.db")).unwrap();
        db.execute_batch("CREATE TABLE trajectory_meta (trajectory_id TEXT, cascade_id TEXT); INSERT INTO trajectory_meta VALUES ('t', 'c-1');")
            .unwrap();
        drop(db);
        assert!(copy_session_in(&data, "c-1").is_ok());
        // A database that names no conversation: refused, and the copy removed.
        let empty = rusqlite::Connection::open(data.join("conversations/c-2.db")).unwrap();
        empty.execute_batch("CREATE TABLE trajectory_meta (trajectory_id TEXT, cascade_id TEXT);").unwrap();
        drop(empty);
        assert!(copy_session_in(&data, "c-2").is_err());
        assert_eq!(std::fs::read_dir(data.join("conversations")).unwrap().count(), 3);
        assert!(copy_session_in(&data, "../c-1").is_err());
        let _ = std::fs::remove_dir_all(data);
    }
}
