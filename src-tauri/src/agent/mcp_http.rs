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

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::AppTools;

/// Largest request body read, in bytes. Tool calls are small.
const MAX_BODY: usize = 4 * 1024 * 1024;

struct Server {
    port: u16,
    chats: Mutex<HashMap<String, Vec<AppTools>>>,
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
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Payload Too Large",
    };
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let content_type = if body.is_empty() { "" } else { "Content-Type: application/json\r\n" };
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
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut stream = TcpStream::connect(host).await.unwrap();
        let body = body.to_string();
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
}
