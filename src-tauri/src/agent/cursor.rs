//! Cursor's command-line agent (`agent`, also installed as `cursor-agent`),
//! driven over ACP (`agent acp`; see `acp.rs`). Tested on Cursor CLI
//! 2026.11.13; what it sends is in PLAN.md, "Cursor over ACP".

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::acp::{self, AcpConfig};
use super::{AgentAdapter, EventSink, RunningAgent, SpawnOptions};
use crate::events::ModelInfo;

pub struct CursorAdapter;

/// Asks for plain model ids with names, and each model's effort levels as
/// options of their own, instead of one id per combination.
fn client_meta() -> Value {
    json!({ "parameterizedModelPicker": true })
}

/// How long listing the models may take in all (one call per model).
const LIST_TIMEOUT: Duration = Duration::from_secs(90);
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
        let mut rpc = SyncRpc::start(binary, &folder)?;
        let deadline = Instant::now() + LIST_TIMEOUT;
        rpc.call(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false, "_meta": client_meta() },
                "clientInfo": { "name": "harness", "version": env!("CARGO_PKG_VERSION") },
            }),
            deadline,
        )?;
        let session = rpc.call(
            "session/new",
            json!({ "cwd": folder.to_string_lossy(), "mcpServers": [] }),
            deadline,
        )?;
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

    /// Cursor names each conversation itself (`session_info_update`), which
    /// the chat takes as its title; no model call is needed.
    fn make_title(&self, _binary: &Path, _first_message: &str) -> Result<String, String> {
        Err("Cursor writes its own titles".to_string())
    }

    fn sign_in(&self, binary: &Path) -> Result<(), String> {
        use std::io::Read;
        let mut cmd = Command::new(binary);
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
        let mut child = cmd.spawn().map_err(|e| format!("Could not start Cursor: {e}"))?;
        let mut stderr = child.stderr.take().ok_or("Cursor has no stderr")?;
        let (tx, rx) = mpsc::channel();
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

    fn forget_session(&self, _agent_session_id: &str) {}
}

/// The `model` entry of a `configOptions` list.
fn model_option(options: &Value) -> Option<&Value> {
    options.as_array()?.iter().find(|o| o["category"] == "model")
}

/// The current model's effort levels and its default, from a `configOptions`
/// list: the option in the `thought_level` category (named `effort`,
/// `reasoning_effort` or `reasoning`, depending on the model).
fn effort_levels(options: &Value) -> Option<(Vec<String>, Option<String>)> {
    let option = options.as_array()?.iter().find(|o| o["category"] == "thought_level")?;
    let levels: Vec<String> = option["options"]
        .as_array()?
        .iter()
        .filter_map(|o| o["value"].as_str().map(str::to_owned))
        .collect();
    (!levels.is_empty()).then(|| (levels, option["currentValue"].as_str().map(str::to_owned)))
}

/// A blocking JSON-RPC client for short jobs (listing models).
struct SyncRpc {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    next: u64,
}

impl SyncRpc {
    fn start(binary: &Path, folder: &PathBuf) -> Result<Self, String> {
        let mut cmd = Command::new(binary);
        cmd.arg("acp")
            .current_dir(folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Could not start Cursor: {e}"))?;
        let stdin = child.stdin.take().ok_or("Cursor has no stdin")?;
        let stdout = child.stdout.take().ok_or("Cursor has no stdout")?;
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self { child, stdin, lines, next: 1 })
    }

    fn call(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        writeln!(self.stdin, "{line}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).map_err(|_| "Cursor took too long".to_string())?;
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            // Its own requests get a refusal; notifications are skipped.
            if v.get("method").is_some() {
                if v.get("id").is_some() {
                    let reply = json!({ "jsonrpc": "2.0", "id": v["id"], "error": { "code": -32601, "message": "Not supported" } });
                    let _ = writeln!(self.stdin, "{reply}");
                }
                continue;
            }
            if v["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = v.get("error") {
                let message = error["data"]["message"].as_str().or(error["message"].as_str()).unwrap_or("error");
                return Err(message.to_string());
            }
            return Ok(v["result"].clone());
        }
    }
}

impl Drop for SyncRpc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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
}
