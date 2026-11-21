//! Short, blocking ACP jobs: listing an agent's models and commands,
//! signing in, and a few words from a cheap model. Shared by the ACP agents (`cursor.rs`, `antigravity.rs`).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::claude_code::clean_title;
use crate::events::CommandInfo;

/// Runs a sign-in command (`args`) until it ends or `timeout` passes. The
/// agent opens the browser itself; its error output is the error.
pub fn sign_in(binary: &Path, args: &[&str], label: &str, timeout: Duration) -> Result<(), String> {
    use std::io::Read;
    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Could not start {label}: {e}"))?;
    let mut stderr = child.stderr.take().ok_or(format!("{label} has no stderr"))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let Ok(detail) = rx.recv_timeout(timeout) else {
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

/// The commands the agent sends right after a session opens in `folder`
/// (nothing is sent, so nothing is spent). None in time means none.
pub fn list_commands(
    binary: &Path,
    args: &[&str],
    label: &'static str,
    client_meta: Value,
    folder: &Path,
    timeout: Duration,
) -> Result<Vec<CommandInfo>, String> {
    let mut rpc = SyncRpc::start(binary, args, label, folder)?;
    let deadline = Instant::now() + timeout;
    rpc.open_session(client_meta, folder, deadline)?;
    // It comes a moment after the reply.
    let wait = deadline.min(Instant::now() + Duration::from_secs(10));
    Ok(rpc
        .notice(|v| v["params"]["update"]["sessionUpdate"] == "available_commands_update", wait)
        .map(|v| parse_commands(&v["params"]["update"]))
        .unwrap_or_default())
}

/// How an agent gives a short answer (a label for a batch of subagents).
pub struct SmallAsk {
    pub label: &'static str,
    pub client_meta: Value,
    /// The cheap model to ask, from the `model` option; None keeps the one
    /// the conversation starts on.
    pub model: fn(&Value) -> Option<String>,
    /// A mode without edits or commands, if the agent has one.
    pub mode: Option<&'static str>,
    /// The agent starts every new conversation on the model picked last
    /// (Cursor), so the one that was current is picked again after.
    pub put_model_back: bool,
    pub timeout: Duration,
}

/// A few words from the agent's cheap model: one prompt in a throwaway
/// conversation in an empty folder, at the lowest effort, with every ask of
/// the agent's refused. `forget` deletes the conversation once the agent has
/// stopped.
pub fn small_answer(binary: &Path, ask: &SmallAsk, prompt: &str, forget: impl Fn(&str)) -> Result<String, String> {
    let folder = std::env::temp_dir().join("tilde-labels");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut rpc = SyncRpc::start(binary, &["acp"], ask.label, &folder)?;
    let deadline = Instant::now() + ask.timeout;
    let session = rpc.open_session(ask.client_meta.clone(), &folder, deadline)?;
    let session_id = session["sessionId"].as_str().ok_or(format!("{} started no conversation", ask.label))?.to_string();
    let answer = answer_in(&mut rpc, ask, &session, &session_id, prompt, deadline);
    drop(rpc);
    forget(&session_id);
    let title = clean_title(&answer?);
    // Markup or a long reply means the model did something other than name it.
    if title.is_empty() || title.contains(['<', '>']) || title.split_whitespace().count() > 10 {
        return Err("No usable title".to_string());
    }
    Ok(title)
}

/// Sets the conversation up as `ask` says, sends `prompt` and returns the
/// agent's whole reply.
fn answer_in(
    rpc: &mut SyncRpc,
    ask: &SmallAsk,
    session: &Value,
    session_id: &str,
    prompt: &str,
    deadline: Instant,
) -> Result<String, String> {
    let set = |rpc: &mut SyncRpc, id: &str, value: &str| {
        rpc.call("session/set_config_option", json!({ "sessionId": session_id, "configId": id, "value": value }), deadline)
    };
    let mut options = session["configOptions"].clone();
    let model = model_option(&options).cloned().unwrap_or_default();
    let model_id = model["id"].as_str().unwrap_or("model").to_string();
    let current = model["currentValue"].as_str().map(str::to_owned);
    let wanted = (ask.model)(&model).filter(|m| Some(m) != current.as_ref());
    if let Some(wanted) = &wanted {
        options = set(rpc, &model_id, wanted)?["configOptions"].clone();
    }
    let answer = (|| {
        if let Some(option) = options.as_array().and_then(|all| all.iter().find(|o| o["category"] == "thought_level")) {
            let lowest = option["options"][0]["value"].as_str();
            if let (Some(id), Some(lowest)) = (option["id"].as_str(), lowest) {
                if option["currentValue"].as_str() != Some(lowest) {
                    let _ = set(rpc, id, lowest);
                }
            }
        }
        if let Some(mode) = ask.mode {
            rpc.call("session/set_mode", json!({ "sessionId": session_id, "modeId": mode }), deadline)?;
        }
        rpc.call(
            "session/prompt",
            json!({ "sessionId": session_id, "prompt": [{ "type": "text", "text": prompt }] }),
            deadline,
        )?;
        Ok(message_text(&rpc.notices, session_id))
    })();
    if let (true, Some(_), Some(current)) = (ask.put_model_back, &wanted, &current) {
        let _ = set(rpc, &model_id, current);
    }
    answer
}

/// What the agent said in `session_id`, from its `agent_message_chunk`s.
fn message_text(notices: &[Value], session_id: &str) -> String {
    notices
        .iter()
        .filter(|v| v["params"]["sessionId"] == session_id)
        .map(|v| &v["params"]["update"])
        .filter(|u| u["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|u| u["content"]["text"].as_str())
        .collect()
}

/// The `model` entry of a `configOptions` list.
pub fn model_option(options: &Value) -> Option<&Value> {
    options.as_array()?.iter().find(|o| o["category"] == "model")
}

/// The current model's effort levels and its default, from a `configOptions`
/// list: the option in the `thought_level` category (named `effort`,
/// `reasoning_effort` or `reasoning`, depending on the model).
pub fn effort_levels(options: &Value) -> Option<(Vec<String>, Option<String>)> {
    let option = options.as_array()?.iter().find(|o| o["category"] == "thought_level")?;
    let levels: Vec<String> = option["options"]
        .as_array()?
        .iter()
        .filter_map(|o| o["value"].as_str().map(str::to_owned))
        .collect();
    (!levels.is_empty()).then(|| (levels, option["currentValue"].as_str().map(str::to_owned)))
}

/// The commands in an `available_commands_update`: `name`, `description`,
/// and maybe `input.hint`.
pub fn parse_commands(update: &Value) -> Vec<CommandInfo> {
    update["availableCommands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let name = c["name"].as_str().filter(|n| !n.is_empty())?.to_string();
            Some(CommandInfo {
                name,
                description: c["description"].as_str().unwrap_or_default().to_string(),
                hint: c["input"]["hint"].as_str().filter(|h| !h.trim().is_empty()).map(str::to_owned),
                aliases: Vec::new(),
                skill: false,
            })
        })
        .collect()
}

/// A blocking JSON-RPC client for short jobs (listing models and commands).
pub struct SyncRpc {
    label: &'static str,
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    next: u64,
    /// Notifications that came while waiting for a reply, oldest first.
    notices: Vec<Value>,
}

impl SyncRpc {
    /// Starts the agent in ACP mode (`args`) in `folder`.
    pub fn start(binary: &Path, args: &[&str], label: &'static str, folder: &Path) -> Result<Self, String> {
        let mut cmd = Command::new(binary);
        cmd.args(args)
            .current_dir(folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Could not start {label}: {e}"))?;
        let stdin = child.stdin.take().ok_or(format!("{label} has no stdin"))?;
        let stdout = child.stdout.take().ok_or(format!("{label} has no stdout"))?;
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self { label, child, stdin, lines, next: 1, notices: Vec::new() })
    }

    /// `initialize`, then `session/new` in `cwd`; returns the new session.
    pub fn open_session(&mut self, client_meta: Value, cwd: &Path, deadline: Instant) -> Result<Value, String> {
        self.call(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false, "_meta": client_meta },
                "clientInfo": { "name": "tilde", "version": env!("CARGO_PKG_VERSION") },
            }),
            deadline,
        )?;
        self.call("session/new", json!({ "cwd": cwd.to_string_lossy(), "mcpServers": [] }), deadline)
    }

    pub fn call(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        writeln!(self.stdin, "{line}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).map_err(|_| format!("{} took too long", self.label))?;
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            // Its own requests get a refusal; notifications are kept for `notice`.
            if v.get("method").is_some() {
                self.take_message(v);
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

impl SyncRpc {
    /// Refuses one of the agent's own requests, or keeps a notification.
    fn take_message(&mut self, v: Value) {
        if v.get("id").is_some() {
            let reply = json!({ "jsonrpc": "2.0", "id": v["id"], "error": { "code": -32601, "message": "Not supported" } });
            let _ = writeln!(self.stdin, "{reply}");
        } else {
            self.notices.push(v);
        }
    }

    /// The first notification `wanted` accepts, whether it came already or
    /// comes before `deadline`.
    pub fn notice(&mut self, wanted: impl Fn(&Value) -> bool, deadline: Instant) -> Option<Value> {
        if let Some(i) = self.notices.iter().position(&wanted) {
            return Some(self.notices.remove(i));
        }
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).ok()?;
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if v.get("method").is_none() {
                continue;
            }
            if v.get("id").is_none() && wanted(&v) {
                return Some(v);
            }
            self.take_message(v);
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
    fn a_reply_is_its_message_chunks_in_this_session() {
        let chunk = |session: &str, kind: &str, text: &str| {
            json!({ "method": "session/update", "params": { "sessionId": session, "update": { "sessionUpdate": kind, "content": { "type": "text", "text": text } } } })
        };
        let notices = [
            chunk("s", "agent_thought_chunk", "hmm"),
            chunk("s", "agent_message_chunk", "Checking idle "),
            chunk("other", "agent_message_chunk", "no"),
            chunk("s", "agent_message_chunk", "chats"),
        ];
        assert_eq!(message_text(&notices, "s"), "Checking idle chats");
    }

    /// Asks the real agents (signed in, on this machine) for a label:
    /// `cargo test real_agents_label -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_agents_label_a_batch() {
        use crate::agent::AgentAdapter;
        let tasks = ["Find where idle chats get closed".to_string(), "Measure memory of an idle chat".to_string()];
        let home = crate::home::user_home().unwrap().join(".local/bin");
        let cursor = crate::agent::cursor::CursorAdapter.name_batch(&home.join("agent"), "check idle chats", &tasks);
        let agy = crate::agent::antigravity::AntigravityAdapter.name_batch(&home.join("agy"), "check idle chats", &tasks);
        println!("Cursor: {cursor:?}\nAntigravity: {agy:?}");
        assert!(cursor.is_ok() && agy.is_ok());
    }
}
