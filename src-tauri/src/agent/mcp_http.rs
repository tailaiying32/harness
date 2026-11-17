//! The app's own tools (browser, terminal) as MCP servers over HTTP, for
//! agents that take MCP servers by URL (Cursor over ACP). Claude Code gets the
//! same tools through its control channel instead (see `claude_code.rs`).
//!
//! One small server for the whole app, on 127.0.0.1 and a port picked by the
//! system. Each chat's process gets its own random token; a tool group is
//! reached at `/mcp/<token>/<server>`. Only plain JSON-RPC over POST is
//! answered (MCP's "Streamable HTTP" with JSON replies); a GET for a stream is
//! refused with 405, which MCP clients accept. Tested with Cursor CLI
//! 2026.11.13.
//!
//! It also answers the `harness-tab` command an agent runs in its shell (see
//! `terminal.rs`): a plain-text POST of a command line to `/tab/<token>`,
//! answered in plain text. Each chat process gets its own token in
//! `HARNESS_TAB_URL`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::AppTools;

/// Largest request body read, in bytes. Tool calls are small.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// Longest command line `harness-tab` may send, in bytes.
const MAX_TAB_COMMAND: usize = 8 * 1024;

/// Runs one `harness-tab` command line for a chat: the text for the agent,
/// or why it couldn't.
pub type TabRun = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync>;

struct Server {
    port: u16,
    chats: Mutex<HashMap<String, Vec<AppTools>>>,
    tabs: Mutex<HashMap<String, TabRun>>,
}

static SERVER: OnceLock<Result<Arc<Server>, String>> = OnceLock::new();

fn server() -> Result<Arc<Server>, String> {
    SERVER
        .get_or_init(|| {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            let port = listener.local_addr().map_err(|e| e.to_string())?.port();
            let server = Arc::new(Server {
                port,
                chats: Mutex::default(),
                tabs: Mutex::default(),
            });
            let serving = server.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(listener) = TcpListener::from_std(listener) else {
                    eprintln!("harness: the tool server couldn't start");
                    return;
                };
                while let Ok((stream, _)) = listener.accept().await {
                    let server = serving.clone();
                    tokio::spawn(async move {
                        let _ = serve(server, stream).await;
                    });
                }
            });
            Ok(server)
        })
        .clone()
}

/// One chat process's tools, reachable while this is kept. Dropping it takes
/// them down.
pub struct Endpoint {
    token: String,
    /// `(server name, URL)` for each tool group.
    pub urls: Vec<(String, String)>,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        if let Some(Ok(server)) = SERVER.get() {
            server.chats.lock().unwrap().remove(&self.token);
        }
    }
}

/// Serves `tools` until the returned endpoint is dropped.
pub fn open(tools: Vec<AppTools>) -> Result<Endpoint, String> {
    let server = server()?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let urls = tools
        .iter()
        .map(|t| {
            (
                t.server.to_string(),
                format!("http://127.0.0.1:{}/mcp/{token}/{}", server.port, t.server),
            )
        })
        .collect();
    server.chats.lock().unwrap().insert(token.clone(), tools);
    Ok(Endpoint { token, urls })
}

/// One chat process's `harness-tab` route, open while this is kept.
pub struct TabEndpoint {
    token: String,
    /// What `HARNESS_TAB_URL` is set to.
    pub url: String,
}

impl Drop for TabEndpoint {
    fn drop(&mut self) {
        if let Some(Ok(server)) = SERVER.get() {
            server.tabs.lock().unwrap().remove(&self.token);
        }
    }
}

/// Answers `harness-tab` with `run` until the returned endpoint is dropped.
pub fn open_tab(run: TabRun) -> Result<TabEndpoint, String> {
    let server = server()?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let url = format!("http://127.0.0.1:{}/tab/{token}", server.port);
    server.tabs.lock().unwrap().insert(token.clone(), run);
    Ok(TabEndpoint { token, url })
}

/// The `harness-tab` route a path names, if its token is live.
fn tab_for(server: &Server, path: &str) -> Option<TabRun> {
    let token = path.strip_prefix("/tab/")?;
    server.tabs.lock().unwrap().get(token).cloned()
}

/// The tool group a path names, if its token is live.
fn tools_for(server: &Server, path: &str) -> Option<AppTools> {
    let rest = path.strip_prefix("/mcp/")?;
    let (token, name) = rest.split_once('/')?;
    let name = name.split(['?', '/']).next()?;
    let chats = server.chats.lock().unwrap();
    chats.get(token)?.iter().find(|t| t.server == name).cloned()
}

/// One connection: requests one after another, kept open between them.
async fn serve(server: Arc<Server>, stream: TcpStream) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();
        let mut length = 0usize;
        let mut close = false;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).await? == 0 {
                return Ok(());
            }
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                let value = value.trim();
                match name.trim().to_ascii_lowercase().as_str() {
                    "content-length" => length = value.parse().unwrap_or(0),
                    "connection" => close = value.eq_ignore_ascii_case("close"),
                    _ => {}
                }
            }
        }
        if length > MAX_BODY {
            reply(&mut write, 413, None).await?;
            return Ok(());
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).await?;

        if path.starts_with("/tab/") {
            match (method.as_str(), tab_for(&server, &path)) {
                (_, None) => reply(&mut write, 404, None).await?,
                ("POST", Some(run)) => {
                    let (status, text) = match String::from_utf8(body) {
                        Ok(command) if command.trim().is_empty() => (400, "Give a command to run.".to_string()),
                        Ok(command) if command.len() > MAX_TAB_COMMAND => (400, "That command is too long.".to_string()),
                        Ok(command) => match run(command).await {
                            Ok(text) => (200, text),
                            Err(text) => (400, text),
                        },
                        Err(_) => (400, "The command isn't text.".to_string()),
                    };
                    reply_text(&mut write, status, &text).await?
                }
                _ => reply(&mut write, 405, None).await?,
            }
            if close {
                return Ok(());
            }
            continue;
        }
        let tools = tools_for(&server, &path);
        match (method.as_str(), tools) {
            (_, None) => reply(&mut write, 404, None).await?,
            ("POST", Some(tools)) => match serde_json::from_slice::<Value>(&body) {
                Ok(message) => match super::answer_mcp(&tools, &message).await {
                    Some(response) => reply(&mut write, 200, Some(response)).await?,
                    None => reply(&mut write, 202, None).await?,
                },
                Err(_) => reply(&mut write, 400, None).await?,
            },
            ("DELETE", Some(_)) => reply(&mut write, 200, None).await?,
            _ => reply(&mut write, 405, None).await?,
        }
        if close {
            return Ok(());
        }
    }
}

async fn reply(write: &mut tokio::net::tcp::OwnedWriteHalf, status: u16, body: Option<Value>) -> std::io::Result<()> {
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    send(write, status, "application/json", &body).await
}

/// A plain-text reply, ending in a newline so a shell prints it on its own line.
async fn reply_text(write: &mut tokio::net::tcp::OwnedWriteHalf, status: u16, text: &str) -> std::io::Result<()> {
    send(write, status, "text/plain; charset=utf-8", &format!("{}\n", text.trim_end())).await
}

async fn send(write: &mut tokio::net::tcp::OwnedWriteHalf, status: u16, kind: &str, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Payload Too Large",
    };
    let content_type = if body.is_empty() { String::new() } else { format!("Content-Type: {kind}\r\n") };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n{content_type}Content-Length: {}\r\n\r\n",
        body.len()
    );
    write.write_all(head.as_bytes()).await?;
    write.write_all(body.as_bytes()).await?;
    write.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn echo() -> AppTools {
        AppTools {
            server: "harness_echo",
            instruction: String::new(),
            list: json!([{ "name": "echo", "inputSchema": { "type": "object" } }]),
            call: Arc::new(|_, args| Box::pin(async move { Ok(vec![json!({ "type": "text", "text": args["say"] })]) })),
        }
    }

    async fn post(url: &str, body: Value) -> (u16, String) {
        post_text(url, &body.to_string()).await
    }

    async fn post_text(url: &str, body: &str) -> (u16, String) {
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut stream = TcpStream::connect(host).await.unwrap();
        let request = format!(
            "POST /{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).await.unwrap();
        let status = text[9..12].parse().unwrap();
        let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
        (status, body)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tools_are_listed_and_called_over_http_until_the_endpoint_closes() {
        let endpoint = open(vec![echo()]).unwrap();
        let (name, url) = endpoint.urls[0].clone();
        assert_eq!(name, "harness_echo");

        let (status, body) = post(&url, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).await;
        assert_eq!(status, 200);
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["result"]["tools"][0]["name"], "echo");

        let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "echo", "arguments": { "say": "hi" } } });
        let (_, body) = post(&url, call.clone()).await;
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["result"]["content"][0]["text"], "hi");

        let (status, _) = post(&url, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
        assert_eq!(status, 202);

        drop(endpoint);
        let (status, _) = post(&url, call).await;
        assert_eq!(status, 404);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn harness_tab_posts_a_command_until_the_endpoint_closes() {
        let run: TabRun = Arc::new(|command| {
            Box::pin(async move { if command == "bad" { Err("no".to_string()) } else { Ok(format!("ran {command}")) } })
        });
        let endpoint = open_tab(run).unwrap();
        let url = endpoint.url.clone();
        assert_eq!(post_text(&url, "npm run dev").await, (200, "ran npm run dev\n".to_string()));
        assert_eq!(post_text(&url, "bad").await, (400, "no\n".to_string()));
        assert_eq!(post_text(&url, "  ").await.0, 400);
        assert_eq!(post_text(&url, &"x".repeat(MAX_TAB_COMMAND + 1)).await.0, 400);
        drop(endpoint);
        assert_eq!(post_text(&url, "npm run dev").await.0, 404);
    }
}
