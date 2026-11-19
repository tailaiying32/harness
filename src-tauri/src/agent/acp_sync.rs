//! Short, blocking ACP jobs: listing an agent's models and commands, and
//! signing in. Shared by the ACP agents (`cursor.rs`, `antigravity.rs`).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

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
                "clientInfo": { "name": "harness", "version": env!("CARGO_PKG_VERSION") },
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

