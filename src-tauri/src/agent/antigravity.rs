//! Antigravity's command-line agent (`agy`), driven over ACP (`agy acp`; see
//! `acp.rs`). Tested on agy 1.5.11; what it sends is in PLAN.md,
//! "Antigravity over ACP".

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::acp::{self, AcpConfig, Dialect, Intro, Tool};
use super::acp_sync::{self, SyncRpc, effort_levels, model_option};
use super::{AgentAdapter, EventSink, RunningAgent, SpawnOptions};
use crate::events::{CommandInfo, ModelInfo};

pub struct AntigravityAdapter;

const LABEL: &str = "Antigravity";
/// How long listing the models may take in all.
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
/// How long listing the commands may take, from start to its list.
const COMMANDS_TIMEOUT: Duration = Duration::from_secs(20);
/// How long signing in may wait for the user in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// How Antigravity's ACP differs: its own tool names and capitalized inputs,
/// a system prompt the app can add to, and plans written as files.
pub static DIALECT: Dialect = Dialect {
    name_tool: name_and_input,
    own_tool,
    is_plan,
    clean_output,
    build_mode: "default",
    intro: Intro::SystemPrompt,
    session_meta,
    pick_default_model: false,
    forms: true,
};

impl AgentAdapter for AntigravityAdapter {
    fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
        if opts.side.is_some() {
            return Err("Side chats work with Claude Code only".to_string());
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
        };
        acp::spawn(opts.binary.clone(), config, opts, sink)
    }

    /// Opens a conversation in an empty folder (nothing is sent, so nothing
    /// is spent) and reads the models in the order Antigravity lists them.
    /// Thinking levels differ by model (Claude Opus stops at High, GPT-OSS
    /// has none) and only show once a model is picked, so each is picked in
    /// turn; there are about ten.
    fn list_models(&self, binary: &Path, _efforts_for: Option<&[String]>) -> Result<Vec<ModelInfo>, String> {
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

    fn list_commands(&self, binary: &Path, folder: &Path) -> Result<Vec<CommandInfo>, String> {
        acp_sync::list_commands(binary, &["acp"], LABEL, json!({}), folder, COMMANDS_TIMEOUT)
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

    fn forget_session(&self, _agent_session_id: &str) {}
}

/// The models in a `configOptions` list. The current model's thinking
/// levels are given to all; each other model's are read once it is picked.
fn models_from(options: &Value) -> Vec<ModelInfo> {
    let Some(option) = model_option(options) else { return Vec::new() };
    let (efforts, default_effort) = effort_levels(options).unwrap_or_default();
    option["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            let value = o["value"].as_str()?.to_string();
            let name = o["name"].as_str().unwrap_or(&value).to_string();
            Some(ModelInfo {
                value,
                name,
                detail: None,
                efforts: efforts.clone(),
                default_effort: default_effort.clone(),
            })
        })
        .collect()
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
    fn every_model_gets_the_thinking_levels() {
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
        assert_eq!(models[1].name, "Claude Opus 5.5");
        assert_eq!(models[1].efforts, ["low", "medium", "high", "xhigh"]);
        assert_eq!(models[1].default_effort.as_deref(), Some("high"));
    }

    #[test]
    fn instructions_go_in_the_system_prompt() {
        assert_eq!(session_meta(Some("Be brief.")), json!({ "antigravity": { "systemPrompt": { "append": "Be brief." } } }));
        assert_eq!(session_meta(None), Value::Null);
    }
}
