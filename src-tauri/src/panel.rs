//! The side panel, opened by the agent: a markdown file as a tab, or one of
//! the panel's views. For when the user asks to see something, so it comes up
//! without a click on a link.

use std::path::Path;

use serde_json::{Value, json};

use crate::events::{AgentEvent, Envelope};
use crate::sessions::Out;

/// The name the agent knows the tool group by.
pub const SERVER: &str = "harness_panel";

/// The line added to the agent's instructions.
pub const INSTRUCTION: &str = "When the user asks to see, show, open or pull up something the side \
panel can show (a markdown file, the plan, their notes, the browser, the terminal, chat info), \
open it with the harness_panel show tool instead of pasting it into the chat or only linking it.";

/// The views the tool can open besides a file.
const VIEWS: [&str; 5] = ["notes", "plan", "browser", "terminal", "info"];

/// Runs the group's one tool for a chat.
pub fn call(out: &Out, chat_id: &str, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
    if tool != "show" {
        return Err(format!("No tool named {tool}."));
    }
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|s| !s.is_empty());
    let (path, view, said) = match (text("path"), text("view")) {
        (Some(_), Some(_)) => return Err("Give `path` or `view`, not both.".into()),
        (Some(path), None) => {
            if !Path::new(path).is_absolute() {
                return Err("Give the file's full path.".into());
            }
            // The same check the panel makes when it reads the file.
            crate::docs::read_bytes(Path::new(path))?;
            (Some(path.to_string()), None, format!("Opened {path} in the side panel."))
        }
        (None, Some(view)) => {
            if !VIEWS.contains(&view) {
                return Err(format!("`view` is one of: {}.", VIEWS.join(", ")));
            }
            (None, Some(view.to_string()), format!("Opened the {view} view in the side panel."))
        }
        (None, None) => return Err("Give `path` (a markdown file) or `view`.".into()),
    };
    out(Envelope { session_id: chat_id.to_string(), event: AgentEvent::ShowPanel { path, view, tab: None } });
    Ok(vec![json!({ "type": "text", "text": said })])
}

pub fn tools() -> Value {
    json!([
        {
            "name": "show",
            "description": "Open something in the side panel beside the chat, opening the panel if it is \
hidden. Use it when the user asks to see, show, open or pull up something: give `path` for a markdown file \
(it opens as a tab), or `view` for one of the panel's views: notes (the user's notes for this chat), plan \
(the latest plan), browser, terminal, or info (chat info: context, tasks, subagents). Other files don't show \
in the panel; link them instead.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The full path of a markdown file (.md or .markdown)." },
                    "view": { "type": "string", "enum": VIEWS },
                },
            },
        },
    ])
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn rig() -> (Out, Arc<Mutex<Vec<Envelope>>>) {
        let seen: Arc<Mutex<Vec<Envelope>>> = Arc::default();
        let out: Out = {
            let seen = seen.clone();
            Arc::new(move |e: Envelope| seen.lock().unwrap().push(e))
        };
        (out, seen)
    }

    #[test]
    fn opens_a_markdown_file() {
        let dir = std::env::temp_dir().join(format!("harness-panel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("TODO.md");
        std::fs::write(&file, "# to-do").unwrap();
        let (out, seen) = rig();
        let path = file.to_str().unwrap();
        call(&out, "c1", "show", &json!({ "path": path })).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].session_id, "c1");
        assert_eq!(seen[0].event, AgentEvent::ShowPanel { path: Some(path.into()), view: None, tab: None });
    }

    #[test]
    fn opens_a_view() {
        let (out, seen) = rig();
        call(&out, "c1", "show", &json!({ "view": "terminal" })).unwrap();
        assert_eq!(seen.lock().unwrap()[0].event, AgentEvent::ShowPanel { path: None, view: Some("terminal".into()), tab: None });
    }

    #[test]
    fn refuses_what_the_panel_cant_show() {
        let (out, seen) = rig();
        assert!(call(&out, "c1", "show", &json!({ "path": "relative/TODO.md" })).is_err());
        assert!(call(&out, "c1", "show", &json!({ "path": "/etc/hosts" })).is_err());
        assert!(call(&out, "c1", "show", &json!({ "path": "/no/such/file.md" })).is_err());
        assert!(call(&out, "c1", "show", &json!({ "view": "settings" })).is_err());
        assert!(call(&out, "c1", "show", &json!({})).is_err());
        assert!(call(&out, "c1", "show", &json!({ "path": "/a.md", "view": "plan" })).is_err());
        assert!(seen.lock().unwrap().is_empty());
    }
}
