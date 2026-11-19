//! Cursor's command-line agent (`agent`, also installed as `cursor-agent`),
//! driven over ACP (`agent acp`; see `acp.rs`). Tested on Cursor CLI
//! 2026.11.13; what it sends is in PLAN.md, "Cursor over ACP".

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::acp::{self, AcpConfig, Dialect, Intro, Tool};
use super::acp_sync::{self, SyncRpc, effort_levels, model_option};
#[cfg(test)]
use super::acp_sync::parse_commands;
use super::{AgentAdapter, EventSink, RunningAgent, SpawnOptions};
use crate::events::{CommandInfo, ModelInfo};

pub struct CursorAdapter;

/// How Cursor's ACP differs: its own plan request and field names, and no
/// system prompt (the app's instructions go in the first prompt).
pub static DIALECT: Dialect = Dialect {
    name_tool: name_and_input,
    own_tool,
    is_plan: |_| false,
    clean_output: |text| text,
    build_mode: "agent",
    intro: Intro::FirstPrompt,
    session_meta: |_| Value::Null,
    pick_default_model: true,
    forms: false,
    print_commands: &[],
    compacts: false,
};

/// Asks for plain model ids with names, and each model's effort levels as
/// options of their own, instead of one id per combination.
fn client_meta() -> Value {
    // `subagents`: each subagent runs as its own session, announced on the
    // chat's session (`subagent_spawned`, `subagent_state_update`).
    json!({ "parameterizedModelPicker": true, "subagents": true })
}

/// How long listing the models may take in all (one call per model).
const LIST_TIMEOUT: Duration = Duration::from_secs(90);
/// How long listing the commands may take, from start to Cursor's list.
const COMMANDS_TIMEOUT: Duration = Duration::from_secs(20);
/// How long `agent login` may wait for the user in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

impl AgentAdapter for CursorAdapter {
    fn spawn(&self, opts: SpawnOptions, sink: EventSink) -> Result<Box<dyn RunningAgent>, String> {
        if opts.side.is_some() {
            return Err("Side chats work with Claude Code only".to_string());
        }
        let config = AcpConfig {
            label: "Cursor",
            args: vec!["acp".to_string()],
            client_meta: client_meta(),
            dialect: &DIALECT,
        };
        acp::spawn(opts.binary.clone(), config, opts, sink)
    }

    /// Starts a short-lived process, opens a conversation in an empty folder
    /// (nothing is sent, so nothing is spent) and reads every model, Auto
    /// first. Which ones the menu shows is the user's pick, in Settings. A
    /// model's effort levels only show once it is picked, so each model in
    /// `efforts_for` is picked in turn (about a second each); the model that
    /// was current is put back after, since Cursor starts every new session on
    /// the last one.
    fn list_models(&self, binary: &Path, efforts_for: Option<&[String]>) -> Result<Vec<ModelInfo>, String> {
        let folder = std::env::temp_dir().join("harness-cursor-models");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let mut rpc = SyncRpc::start(binary, &["acp"], "Cursor", &folder)?;
        let deadline = Instant::now() + LIST_TIMEOUT;
        let session = rpc.open_session(client_meta(), &folder, deadline)?;
        let session_id = session["sessionId"].as_str().ok_or("Cursor started no conversation")?.to_string();
        let model_option = model_option(&session["configOptions"]).ok_or("Cursor listed no models")?;
        let current = model_option["currentValue"].as_str().map(str::to_owned);
        let listed: Vec<(String, String)> = model_option["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| {
                let value = o["value"].as_str()?.to_string();
                let name = o["name"].as_str().unwrap_or(&value).to_string();
                Some((value, name))
            })
            .collect();
        let mut listed = listed;
        listed.sort_by_key(|(value, _)| value != "default");

        let mut models = Vec::new();
        for (value, name) in listed {
            let mut info = ModelInfo {
                value: value.clone(),
                name,
                detail: None,
                efforts: Vec::new(),
                default_effort: None,
            };
            if efforts_for.is_some_and(|wanted| !wanted.contains(&value)) {
                models.push(info);
                continue;
            }
            let picked = rpc.call(
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": "model", "value": value }),
                deadline,
            );
            if let Ok(reply) = picked {
                if let Some((levels, default)) = effort_levels(&reply["configOptions"]) {
                    info.efforts = levels;
                    info.default_effort = default;
                }
            }
            models.push(info);
        }
        if let Some(current) = current {
            let _ = rpc.call(
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": "model", "value": current }),
                deadline,
            );
        }
        Ok(models)
    }

    /// Cursor sends its commands right after a conversation opens.
    fn list_commands(&self, binary: &Path, folder: &Path) -> Result<Vec<CommandInfo>, String> {
        acp_sync::list_commands(binary, &["acp"], "Cursor", client_meta(), folder, COMMANDS_TIMEOUT)
    }

    /// Cursor names each conversation itself (`session_info_update`), which
    /// the chat takes as its title; no model call is needed.
    fn make_title(&self, _binary: &Path, _first_message: &str) -> Result<String, String> {
        Err("Cursor writes its own titles".to_string())
    }

    fn sign_in(&self, binary: &Path) -> Result<(), String> {
        acp_sync::sign_in(binary, &["login"], "Cursor", SIGN_IN_TIMEOUT)
    }

    fn forget_session(&self, _agent_session_id: &str) {}
}

/// Whether a tool call is one of the app's own tools.
fn own_tool(tool: &Tool, own: &[String]) -> bool {
    let provider = tool.raw_input["providerIdentifier"].as_str();
    own.iter()
        .any(|server| provider == Some(server.as_str()) || tool.title.starts_with(&format!("{server}-")))
}

/// The app's name for a tool call and its input, in the shape the window
/// words Claude Code's tools in (`tools.ts`): "Bash" with `command`, "Read"
/// with `file_path`, and so on. Cursor's raw inputs were read live.
pub(crate) fn name_and_input(tool: &Tool, _own: &[String]) -> (String, Value) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_levels_come_from_the_thought_level_option() {
        let options = json!([
            { "id": "model", "category": "model", "currentValue": "gpt-5.5", "options": [] },
            { "id": "reasoning", "category": "thought_level", "currentValue": "medium",
              "options": [{ "value": "low" }, { "value": "medium" }, { "value": "high" }] },
            { "id": "fast", "category": "model_config", "currentValue": "false", "options": [] },
        ]);
        assert_eq!(
            effort_levels(&options),
            Some((vec!["low".into(), "medium".into(), "high".into()], Some("medium".into())))
        );
        assert_eq!(effort_levels(&json!([])), None);
    }

    #[test]
    fn commands_come_from_the_update() {
        let update = json!({ "sessionUpdate": "available_commands_update", "availableCommands": [
            { "name": "autopilot", "description": "Keep a PR merge-ready. (builtin skill)" },
            { "name": "shell", "description": "Runs the rest as a shell command.", "input": { "hint": "command" } },
            { "description": "No name, so skipped" },
        ]});
        assert_eq!(
            parse_commands(&update),
            vec![
                CommandInfo {
                    name: "autopilot".into(),
                    description: "Keep a PR merge-ready. (builtin skill)".into(),
                    hint: None,
                    aliases: vec![],
                    skill: false,
                },
                CommandInfo {
                    name: "shell".into(),
                    description: "Runs the rest as a shell command.".into(),
                    hint: Some("command".into()),
                    aliases: vec![],
                    skill: false,
                },
            ]
        );
    }
}
