//! JSON-RPC over an agent's stdin and stdout, one message per line: send calls
//! and wait for their replies. Shared by ACP agents (`acp.rs`) and Codex's
//! app-server (`codex.rs`), which leaves out the `"jsonrpc"` field.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

/// A failed call: JSON-RPC's code and message, or the process ending.
#[derive(Debug, Clone)]
pub(crate) struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub fn gone() -> Self {
        Self {
            code: 0,
            message: "the agent is no longer running".into(),
        }
    }

    pub fn from(value: &Value) -> Self {
        let detail = value["data"]["message"].as_str().or(value["message"].as_str());
        Self {
            code: value["code"].as_i64().unwrap_or(0),
            message: detail.unwrap_or("unknown error").to_string(),
        }
    }
}

/// What the writer task does next.
pub(crate) enum Out {
    Line(String),
    Close,
}

/// The JSON-RPC side: send calls and wait for their replies.
pub(crate) struct Rpc {
    pub tx: mpsc::UnboundedSender<Out>,
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>>,
    /// Whether each message carries `"jsonrpc": "2.0"` (ACP does; Codex's
    /// app-server leaves it out).
    jsonrpc: bool,
}

impl Rpc {
    pub fn new(tx: mpsc::UnboundedSender<Out>, jsonrpc: bool) -> Self {
        Self {
            tx,
            next: AtomicU64::new(1),
            waiting: Mutex::default(),
            jsonrpc,
        }
    }

    fn send(&self, mut value: Value) -> Result<(), RpcError> {
        if self.jsonrpc {
            value["jsonrpc"] = json!("2.0");
        }
        self.tx.send(Out::Line(value.to_string())).map_err(|_| RpcError::gone())
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.waiting.lock().unwrap().insert(id, tx);
        self.send(json!({ "id": id, "method": method, "params": params }))?;
        rx.await.unwrap_or_else(|_| Err(RpcError::gone()))
    }

    pub fn notify(&self, method: &str, params: Value) {
        let _ = self.send(json!({ "method": method, "params": params }));
    }

    pub fn respond(&self, id: &Value, result: Value) {
        let _ = self.send(json!({ "id": id, "result": result }));
    }

    pub fn respond_error(&self, id: &Value, code: i64, message: &str) {
        let _ = self.send(json!({ "id": id, "error": { "code": code, "message": message } }));
    }

    /// A reply arrived: hand it to its caller.
    pub fn settle(&self, v: &Value) {
        let Some(id) = v["id"].as_u64() else { return };
        if let Some(tx) = self.waiting.lock().unwrap().remove(&id) {
            let result = match v.get("error") {
                Some(error) => Err(RpcError::from(error)),
                None => Ok(v["result"].clone()),
            };
            let _ = tx.send(result);
        }
    }

    /// The process is gone: every waiting call fails.
    pub fn fail_all(&self) {
        for (_, tx) in self.waiting.lock().unwrap().drain() {
            let _ = tx.send(Err(RpcError::gone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(rx: &mut mpsc::UnboundedReceiver<Out>) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(Out::Line(line)) = rx.try_recv() {
            out.push(serde_json::from_str(&line).unwrap());
        }
        out
    }

    #[test]
    fn messages_carry_the_jsonrpc_field_only_when_asked() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let acp = Rpc::new(tx, true);
        acp.notify("a", json!({}));
        acp.respond(&json!(3), json!({ "ok": true }));
        let sent = lines(&mut rx);
        assert!(sent.iter().all(|m| m["jsonrpc"] == "2.0"));
        assert_eq!(sent[1]["id"], 3);

        let (tx, mut rx) = mpsc::unbounded_channel();
        let codex = Rpc::new(tx, false);
        codex.notify("initialized", json!({}));
        codex.respond_error(&json!("x"), -32601, "no");
        let sent = lines(&mut rx);
        assert!(sent.iter().all(|m| m.get("jsonrpc").is_none()));
        assert_eq!(sent[0]["method"], "initialized");
        assert_eq!(sent[1]["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn a_reply_settles_its_call() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let rpc = std::sync::Arc::new(Rpc::new(tx, false));
        let caller = {
            let rpc = rpc.clone();
            tokio::spawn(async move { rpc.call("model/list", json!({})).await })
        };
        tokio::task::yield_now().await;
        let sent = loop {
            if let Ok(Out::Line(line)) = rx.try_recv() {
                break serde_json::from_str::<Value>(&line).unwrap();
            }
            tokio::task::yield_now().await;
        };
        rpc.settle(&json!({ "id": sent["id"], "error": { "code": -1, "message": "nope" } }));
        let error = caller.await.unwrap().unwrap_err();
        assert_eq!((error.code, error.message.as_str()), (-1, "nope"));
    }
}
