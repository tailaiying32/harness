//! Earlier chats, for agents: a tool group that searches, lists and reads the
//! chats of the asking chat's project, from the app's own saved history. It
//! works the same for every agent, and a chat on one agent can read a chat
//! that ran on another. Chats in the trash and chats of other projects are
//! out of reach.
//!
//! A chat is read as what the user and the agent said, with each tool call as
//! one line (marked when it failed), its plans and answered questions, and
//! the names of the docs it wrote. Tool output is left out: it is most of a
//! saved chat and little of its meaning. The one-line tool calls stay because
//! they are where a tried-and-dropped attempt shows.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Value, json};

use crate::events::{AgentEvent, ApprovalKind, Block, Decision};
use crate::notes::Notes;
use crate::store::{SessionRow, Store, now_ms, search_words};
use crate::worktree;

/// The name the agent knows the tool group by.
pub const SERVER: &str = "tilde_chats";

/// The line added to the agent's instructions. The same on every launch, so
/// it never disturbs prompt caching; the list of recent chats goes with a
/// chat's first message instead (`titles_note`).
pub const INSTRUCTION: &str = "Earlier chats in this project are saved. When the user refers to \
earlier work, a past discussion, a decision or something that was tried, and it is not in this chat, \
look it up with the tilde_chats tools (search, recent, read) before you answer. Don't look when the \
message stands on its own. A message may name a chat as [@Title](<chat:ID>): read that chat by its ID.";

/// How many chats `recent` lists, and how many titles a new chat is told.
const RECENT: usize = 15;
/// The most one `read` hands back, in characters (about 15,000 tokens).
const READ_CHARS: usize = 60_000;
/// The longest a tool call's one line gets.
const LINE_CHARS: usize = 110;

/// When to look, said in each tool's own description: Claude Code loads the
/// app's tools late, and Cursor hears `INSTRUCTION` only when a conversation
/// starts, so deep in a chat the descriptions are what an agent still sees.
const WHEN: &str = "Use it when the user refers to earlier work, a past discussion, a decision, or \
something that was tried, that is not in this chat, even in passing (\"same as we did for…\", \"like \
we tried in…\"). Don't use it when the message stands on its own.";

/// The group's tool definitions.
pub fn tools() -> Value {
    json!([
        {
            "name": "search",
            "description": format!(
                "Search this project's earlier chats, from any agent, by what they were about. {WHEN} \
A search costs little, so try a few wordings. Then read the chats that clearly match; when several \
cover the same thing, the newest wins. Give `chat` to search inside one chat and get its matching turns."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "A few words, like: tab reordering." },
                    "chat": { "type": "string", "description": "A chat's id, to search inside that chat only." },
                },
                "required": ["query"],
            },
        },
        {
            "name": "recent",
            "description": "List this project's latest chats, newest first. Use it when the user points \
at a chat by time (\"my last chat\", \"the one from this morning\").",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "read",
            "description": format!(
                "Read a chat of this project (an earlier one, or this one when the app asks you to): what the user and the agent said, each tool call \
as one line (marked FAILED when it failed), its plans, and the names of the docs it wrote. {WHEN} Later \
turns override earlier ones, and a tool line is an attempt, not proof it worked or stayed. A long chat \
comes in parts, newest first; give `upto_turn` for earlier turns. Give `doc` with a listed doc's name to \
read that doc instead. A chat the user names as [@Title](<chat:ID>) is read by that ID."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The chat's id, as search and recent show it." },
                    "upto_turn": { "type": "integer", "description": "Read the turns up to this one. Leave out for the newest." },
                    "doc": { "type": "string", "description": "The name of a doc the chat wrote, to read it." },
                },
                "required": ["id"],
            },
        },
    ])
}

#[derive(Clone)]
pub struct Chats {
    store: Arc<Store>,
    /// Where each chat's folder is now, to find a doc saved under an old one.
    notes: Arc<Notes>,
}

impl Chats {
    pub fn new(store: Arc<Store>, notes: Arc<Notes>) -> Self {
        Self { store, notes }
    }

    /// Runs one of the group's tools for a chat (`asking`) of a project (its
    /// main folder, as the window names it).
    pub async fn call(&self, asking: &str, project: &str, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
        let (store, notes) = (self.store.clone(), self.notes.clone());
        let (asking, project) = (asking.to_string(), project.to_string());
        let (tool, args) = (tool.to_string(), args.clone());
        let text = tokio::task::spawn_blocking(move || run(&store, &notes, &asking, &project, &tool, &args))
            .await
            .map_err(|e| e.to_string())??;
        Ok(vec![json!({ "type": "text", "text": text })])
    }
}

fn run(store: &Store, notes: &Notes, asking: &str, project: &str, tool: &str, args: &Value) -> Result<String, String> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|s| !s.is_empty());
    let now = now_ms();
    match tool {
        "search" => {
            let words = search_words(text("query").unwrap_or_default());
            if words.is_empty() {
                return Err("Give a few words to search for.".into());
            }
            match text("chat") {
                Some(id) => {
                    let chat = store.find_chat(project, id, asking)?;
                    let hits = store.search_in_chat(&chat.id, &words)?;
                    if hits.is_empty() {
                        return Ok("Nothing in that chat matched. Try other words.".into());
                    }
                    let mut out = format!("# {}: matching turns, newest first\n", one_line(&chat.title, 200));
                    for hit in hits {
                        out.push_str(&format!("turn {} ({}): …{}…\n", hit.turn, hit.role, one_line(&hit.snippet, 400)));
                    }
                    Ok(out)
                }
                None => {
                    let (hits, more) = store.search_chats(project, asking, &words)?;
                    if hits.is_empty() {
                        return Ok("No chats matched. Try other words.".into());
                    }
                    let mut out = String::new();
                    for hit in hits {
                        out.push_str(&chat_line(&hit.session, hit.turns, now));
                        out.push('\n');
                        if !hit.snippet.is_empty() {
                            out.push_str(&format!(
                                "    turn {}: …{}…\n",
                                hit.snippet_turn,
                                one_line(&hit.snippet, 400)
                            ));
                        }
                    }
                    if more > 0 {
                        out.push_str(&format!("({more} more matches not shown)\n"));
                    }
                    Ok(out)
                }
            }
        }
        "recent" => {
            let chats = store.recent_chats(project, asking, RECENT)?;
            if chats.is_empty() {
                return Ok("This project has no other chats.".into());
            }
            let mut out = String::new();
            for chat in chats {
                let turns = store.turn_count(&chat.id)?;
                out.push_str(&chat_line(&chat, turns, now));
                out.push('\n');
            }
            Ok(out)
        }
        "read" => {
            // An ID copied whole from a `[@Title](<chat:ID>)` word is taken too.
            let id = text("id").ok_or("Give the chat's id.")?;
            let chat = store.find_chat(project, id.trim_start_matches("chat:"), asking)?;
            let events = store.events(&chat.id)?;
            match text("doc") {
                // A doc written in the chat's folder before chat folders moved
                // is in the chat's folder today.
                Some(name) => read_doc(&events, name, &|path| notes.current(&chat.id, path)),
                None => {
                    let upto = args["upto_turn"].as_u64().map(|n| n as usize);
                    Ok(render(&chat, &events, upto, READ_CHARS, now))
                }
            }
        }
        _ => Err(format!("No such tool: {tool}")),
    }
}

/// What a new chat is told with its first message: the project's latest
/// chats, so a passing mention of one is easy to spot. None when there are
/// no others.
pub fn titles_note(store: &Store, project: &str, asking: &str) -> Option<String> {
    let chats = store.recent_chats(project, asking, RECENT).ok()?;
    if chats.is_empty() {
        return None;
    }
    let now = now_ms();
    let list: Vec<String> = chats
        .iter()
        .map(|c| format!("{} · {} · {}", short_id(&c.id), age(now, c.last_used_at), one_line(&c.title, 80)))
        .collect();
    Some(format!(
        "Recent chats in this project (id, last used, title), readable with the {SERVER} tools: {}.",
        list.join("; ")
    ))
}

/// The start of a chat's ID, which is what agents are shown and give back.
fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// How long ago, in plain words.
fn age(now: i64, then: i64) -> String {
    let minutes = (now - then).max(0) / 60_000;
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match minutes {
        0 => "just now".into(),
        1..60 => plural(minutes, "minute"),
        60..1440 => plural(minutes / 60, "hour"),
        _ => plural(minutes / 1440, "day"),
    }
}

/// Text on one line, cut to `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let mut cut: String = line.chars().take(max).collect();
    cut.push('…');
    cut
}

fn agent_name(agent: &str) -> &str {
    if crate::agent::ALL.contains(&agent) { crate::agent::label(agent) } else { agent }
}

/// A chat's pull requests in a few words, like "PR #42 merged". None without any.
fn pr_words(chat: &SessionRow) -> Option<String> {
    let state = worktree::summary(&chat.prs)?;
    let pr = chat.prs.iter().rev().find(|pr| pr.state == state)?;
    Some(format!("PR #{} {state}", pr.number))
}

/// One chat as `search` and `recent` list it. The window reads these lines
/// too (`chatLines` in tools.ts): the ID, how long ago and the title are set
/// apart by two spaces, so the title is kept free of them.
fn chat_line(chat: &SessionRow, turns: usize, now: i64) -> String {
    let pr = pr_words(chat).map(|pr| format!(", {pr}")).unwrap_or_default();
    format!(
        "{}  {}  {}  ({}, {turns} turn{}{pr})",
        short_id(&chat.id),
        age(now, chat.last_used_at),
        one_line(&chat.title, 200),
        agent_name(&chat.agent),
        if turns == 1 { "" } else { "s" },
    )
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A tool call as one line: its name and the one thing that says what it was
/// for, like `Bash: Run the tests` or `Edit: store.rs`.
pub fn tool_line(name: &str, input: &Value) -> String {
    let text = |key: &str| input[key].as_str().map(str::trim).filter(|s| !s.is_empty());
    let first = || input.as_object()?.values().find_map(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
    let (label, what) = match name.strip_prefix("mcp__") {
        Some(rest) => {
            let (server, tool) = rest.split_once("__").unwrap_or((rest, ""));
            (format!("{server} {tool}").trim().to_string(), first())
        }
        None => {
            let what = match name {
                "Bash" => text("description").or(text("command")),
                "Read" | "Edit" | "Write" | "MultiEdit" | "Delete" | "Move" | "NotebookEdit" => {
                    text("file_path").or(text("notebook_path")).map(base_name)
                }
                "Grep" | "Glob" => text("pattern"),
                "WebFetch" => text("url"),
                "WebSearch" => text("query"),
                "Agent" | "Task" => text("description"),
                // The plan and the question follow in full; a to-do list says little.
                "ExitPlanMode" | "AskUserQuestion" | "TodoWrite" => None,
                _ => first(),
            };
            (name.to_string(), what)
        }
    };
    match what {
        Some(what) => one_line(&format!("{label}: {what}"), LINE_CHARS),
        None => label,
    }
}

/// Whether a tool call wrote a markdown file that counts as one of the
/// chat's docs: not its notes, not an agent's own plan file.
fn doc_path<'a>(name: &str, input: &'a Value) -> Option<&'a str> {
    if !matches!(name, "Write" | "Edit" | "MultiEdit") {
        return None;
    }
    let path = input["file_path"].as_str()?;
    let lower = path.to_lowercase();
    let markdown = lower.ends_with(".md") || lower.ends_with(".markdown");
    let own = base_name(path) != "notes.md" && !path.contains("/.claude/plans/");
    (markdown && own).then_some(path)
}

/// The docs a chat wrote, as paths, in the order they were first written. A
/// doc the user saved to the project is at its new place.
pub fn docs(events: &[AgentEvent]) -> Vec<String> {
    let failed = failed_tools(events);
    let latest = latest_inputs(events);
    let mut paths: Vec<String> = Vec::new();
    for event in events {
        match event {
            AgentEvent::AssistantBlock { block: Block::ToolCall { tool_id, name, input }, parent_tool_id: None, .. } => {
                let (name, input) = latest.get(tool_id.as_str()).copied().unwrap_or((name, input));
                if let Some(path) = doc_path(name, input) {
                    if !failed.contains(tool_id.as_str()) && !paths.iter().any(|p| p == path) {
                        paths.push(path.to_string());
                    }
                }
            }
            AgentEvent::DocSaved { from, to } => {
                paths.retain(|p| p != to);
                match paths.iter_mut().find(|p| *p == from) {
                    Some(path) => *path = to.clone(),
                    None => paths.push(to.clone()),
                }
            }
            _ => {}
        }
    }
    paths
}

/// Each tool call's name and input as last told: a call that started before
/// its input arrived (Cursor) is followed by a `ToolInput` with the real one.
fn latest_inputs(events: &[AgentEvent]) -> HashMap<&str, (&str, &Value)> {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolInput { tool_id, name, input, .. } => Some((tool_id.as_str(), (name.as_str(), input))),
            _ => None,
        })
        .collect()
}

fn failed_tools(events: &[AgentEvent]) -> HashSet<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolResult { tool_id, is_error: true, .. } => Some(tool_id.as_str()),
            _ => None,
        })
        .collect()
}

/// One of the chat's docs, by name. Only a doc the chat wrote can be read
/// this way, never another file. `now` gives a saved path as it is today.
fn read_doc(events: &[AgentEvent], name: &str, now: &dyn Fn(&str) -> String) -> Result<String, String> {
    let paths = docs(events);
    let Some(path) = paths.iter().rev().find(|p| *p == name || base_name(p) == name) else {
        let names: Vec<&str> = paths.iter().map(|p| base_name(p)).collect();
        return Err(match names.is_empty() {
            true => "That chat wrote no docs.".into(),
            false => format!("That chat wrote no doc with that name. Its docs: {}", names.join(", ")),
        });
    };
    let path = &now(path);
    let text = std::fs::read_to_string(path).map_err(|_| format!("That doc is no longer at {path}"))?;
    Ok(match cut_middle(&text, READ_CHARS) {
        Some(cut) => format!("# {} (cut: it is longer than one read)\n\n{cut}", base_name(path)),
        None => format!("# {}\n\n{text}", base_name(path)),
    })
}

/// A long text with its middle taken out: the first third and the last half
/// of `max` characters. None when it fits.
fn cut_middle(text: &str, max: usize) -> Option<String> {
    let count = text.chars().count();
    if count <= max {
        return None;
    }
    let head: String = text.chars().take(max / 3).collect();
    let tail: String = text.chars().skip(count - max / 2).collect();
    Some(format!("{head}\n\n[… cut …]\n\n{tail}"))
}

/// One turn: what the user sent, then what the agent said and did.
struct Turn {
    user: String,
    /// The chat moved to another agent just before this turn: who from and to.
    switched: Option<String>,
    /// The agent's text, and its tool calls with those in a row kept together.
    parts: Vec<String>,
    /// Whether the last part is a run of tool lines, so the next call joins it.
    in_tools: bool,
}

impl Turn {
    fn say(&mut self, text: String) {
        self.parts.push(text);
        self.in_tools = false;
    }

    fn tool(&mut self, line: String) {
        match self.parts.last_mut().filter(|_| self.in_tools) {
            Some(run) => {
                run.push('\n');
                run.push_str(&line);
            }
            None => self.parts.push(line),
        }
        self.in_tools = true;
    }
}

/// A chat as an agent reads it (see the module's opening). Turns are counted
/// from 1. The newest turns that fit in `max` characters are given, ending at
/// `upto_turn` when one is asked for; a single turn too long to fit has its
/// middle cut.
pub fn render(chat: &SessionRow, events: &[AgentEvent], upto_turn: Option<usize>, max: usize, now: i64) -> String {
    // What later events say about earlier ones.
    let failed = failed_tools(events);
    let latest = latest_inputs(events);
    let mut withdrawn: HashSet<&str> = HashSet::new();
    let mut edited_plans: HashMap<&str, &str> = HashMap::new();
    let mut answered: HashMap<&str, &AgentEvent> = HashMap::new();
    for event in events {
        match event {
            AgentEvent::MessageWithdrawn { id, ok: true } => {
                withdrawn.insert(id);
            }
            AgentEvent::PlanEdited { request_id, plan } => {
                edited_plans.insert(request_id, plan);
            }
            AgentEvent::ApprovalAnswered { request_id, .. } => {
                answered.insert(request_id, event);
            }
            _ => {}
        }
    }

    let mut turns: Vec<Turn> = Vec::new();
    // A switch is saved just before the message that went to the new agent.
    let mut switched = None;
    for event in events {
        if let AgentEvent::UserMessage { text, id, .. } = event {
            if !id.as_deref().is_some_and(|id| withdrawn.contains(id)) {
                turns.push(Turn { user: text.clone(), switched: switched.take(), parts: Vec::new(), in_tools: false });
            }
            continue;
        }
        if let AgentEvent::AgentSwitched { from, to, model } = event {
            let model = model.as_deref().map(|m| format!(" ({m})")).unwrap_or_default();
            switched = Some(format!("(the chat moved from {} to {}{model} here)", agent_name(from), agent_name(to)));
            continue;
        }
        let Some(turn) = turns.last_mut() else { continue };
        match event {
            // A subagent's own steps stay out; the call that started it shows.
            AgentEvent::AssistantBlock { parent_tool_id: Some(_), .. } => {}
            AgentEvent::AssistantBlock { block: Block::Text { text }, .. } => {
                if !text.trim().is_empty() {
                    turn.say(text.trim().to_string());
                }
            }
            AgentEvent::AssistantBlock { block: Block::ToolCall { tool_id, name, input }, .. } => {
                let (name, input) = latest.get(tool_id.as_str()).copied().unwrap_or((name, input));
                let mark = if failed.contains(tool_id.as_str()) { " [FAILED]" } else { "" };
                turn.tool(format!("  → {}{mark}", tool_line(name, input)));
            }
            AgentEvent::ApprovalRequest { request_id, approval: ApprovalKind::Plan, input, .. } => {
                let plan = edited_plans.get(request_id.as_str()).copied().or(input["plan"].as_str());
                if let Some(plan) = plan.map(str::trim).filter(|p| !p.is_empty()) {
                    let outcome = match answered.get(request_id.as_str()) {
                        Some(AgentEvent::ApprovalAnswered { decision: Decision::Deny, .. }) => "\n\n(plan turned down)",
                        Some(_) => "\n\n(plan approved)",
                        None => "",
                    };
                    turn.say(format!("Plan:\n\n{plan}{outcome}"));
                }
            }
            AgentEvent::ApprovalRequest { request_id, approval: ApprovalKind::Question, .. } => {
                if let Some(AgentEvent::ApprovalAnswered { answers: Some(answers), .. }) = answered.get(request_id.as_str()) {
                    let lines: Vec<String> = answers
                        .iter()
                        .map(|(question, answer)| format!("Asked the user: {question}\nThe user answered: {answer}"))
                        .collect();
                    if !lines.is_empty() {
                        turn.say(lines.join("\n"));
                    }
                }
            }
            AgentEvent::Interrupted => turn.say("(stopped by the user)".into()),
            AgentEvent::Error { message } => turn.say(format!("(error: {})", one_line(message, 300))),
            AgentEvent::WorktreeChanged { status, .. } if status == "merged" => turn.say("(branch merged)".into()),
            _ => {}
        }
    }

    let count = turns.len();
    let texts: Vec<String> = turns
        .iter()
        .enumerate()
        .map(|(i, turn)| {
            let agent = match turn.parts.is_empty() {
                true => "(no reply)".to_string(),
                false => turn.parts.join("\n\n"),
            };
            let switched = turn.switched.as_deref().map(|s| format!("{s}\n\n")).unwrap_or_default();
            format!("## Turn {} of {count}\n\n{switched}### User\n\n{}\n\n### Agent\n\n{agent}\n", i + 1, turn.user.trim())
        })
        .collect();

    // The newest turns that fit, ending at the one asked for.
    let last = upto_turn.unwrap_or(count).clamp(usize::from(count > 0), count);
    let mut first = last;
    let mut used = texts.get(last.wrapping_sub(1)).map_or(0, |t| t.chars().count());
    while first > 1 && used + texts[first - 2].chars().count() <= max {
        first -= 1;
        used += texts[first - 1].chars().count();
    }

    let pr = pr_words(chat).map(|pr| format!(" · {pr}")).unwrap_or_default();
    let mut out = format!(
        "# {} · {} · last used {} · {count} turn{}{pr}\n",
        one_line(&chat.title, 200),
        agent_name(&chat.agent),
        age(now, chat.last_used_at),
        if count == 1 { "" } else { "s" },
    );
    let written = docs(events);
    if !written.is_empty() {
        let names: Vec<&str> = written.iter().map(|p| base_name(p)).collect();
        out.push_str(&format!("Docs this chat wrote: {} (read one with doc)\n", names.join(", ")));
    }
    if count == 0 {
        out.push_str("\n(nothing was said in this chat)\n");
        return out;
    }
    if first > 1 || last < count {
        out.push_str(&format!("Showing turns {first}–{last} of {count}."));
        if first > 1 {
            out.push_str(&format!(
                " For earlier turns call read with upto_turn={}, or search inside this chat.",
                first - 1
            ));
        }
        out.push('\n');
    }
    for text in &texts[first - 1..last] {
        out.push('\n');
        match cut_middle(text, max) {
            Some(cut) => out.push_str(&cut),
            None => out.push_str(text),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn chat() -> SessionRow {
        SessionRow {
            id: "abcdef0123456789".into(),
            project_path: "/p".into(),
            title: "Tab  reordering".into(),
            mode: "default".into(),
            model: "default".into(),
            effort: "default".into(),
            agent_session_id: None,
            last_used_at: 0,
            work_path: None,
            branch: None,
            wt_status: None,
            prs: Vec::new(),
            trashed_at: None,
            agent: "claude-code".into(),
            former_paths: Vec::new(),
        }
    }

    fn user(text: &str, id: &str) -> AgentEvent {
        AgentEvent::UserMessage { text: text.into(), id: Some(id.into()), images: vec![] }
    }

    fn said(text: &str) -> AgentEvent {
        AgentEvent::AssistantBlock { message_id: "m".into(), block: Block::Text { text: text.into() }, parent_tool_id: None }
    }

    fn tool(id: &str, name: &str, input: Value) -> AgentEvent {
        AgentEvent::AssistantBlock {
            message_id: "m".into(),
            block: Block::ToolCall { tool_id: id.into(), name: name.into(), input },
            parent_tool_id: None,
        }
    }

    fn result(id: &str, is_error: bool) -> AgentEvent {
        AgentEvent::ToolResult { tool_id: id.into(), content: json!("out"), is_error, parent_tool_id: None }
    }

    #[test]
    fn three_tools_are_declared_and_say_when_to_look() {
        let tools = tools();
        let names: Vec<&str> = tools.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["search", "recent", "read"]);
        for tool in tools.as_array().unwrap() {
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
        assert!(tools[0]["description"].as_str().unwrap().contains("like we tried in"));
        assert!(tools[2]["description"].as_str().unwrap().contains("like we tried in"));
    }

    #[test]
    fn a_tool_call_is_one_line_that_says_what_it_was_for() {
        assert_eq!(tool_line("Bash", &json!({ "command": "cargo test", "description": "Run the\n tests" })), "Bash: Run the tests");
        assert_eq!(tool_line("Bash", &json!({ "command": "cargo  test" })), "Bash: cargo test");
        assert_eq!(tool_line("Edit", &json!({ "file_path": "/a/b/store.rs", "old_string": "x" })), "Edit: store.rs");
        assert_eq!(tool_line("Grep", &json!({ "pattern": "fn main", "path": "/a" })), "Grep: fn main");
        assert_eq!(tool_line("Grep", &json!({ "pattern": null, "path": null })), "Grep");
        assert_eq!(tool_line("Agent", &json!({ "description": "Look around", "prompt": "long" })), "Agent: Look around");
        assert_eq!(tool_line("ExitPlanMode", &json!({ "plan": "the plan" })), "ExitPlanMode");
        assert_eq!(
            tool_line("mcp__tilde_browser__open", &json!({ "url": "http://localhost:1420" })),
            "tilde_browser open: http://localhost:1420"
        );
        assert_eq!(tool_line("a tool", &Value::Null), "a tool");
        let long = tool_line("Bash", &json!({ "command": "x".repeat(300) }));
        assert_eq!(long.chars().count(), LINE_CHARS + 1);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn a_chat_reads_as_turns_with_tool_lines_plans_and_answers() {
        let mut answers = BTreeMap::new();
        answers.insert("Which row?".to_string(), "Within a row".to_string());
        let events = vec![
            AgentEvent::SessionStarted { agent_session_id: "a".into(), model: None, mode: None },
            user("make tabs draggable", "u1"),
            said("Looking."),
            tool("t1", "Read", json!({ "file_path": "/p/src/tabs.ts" })),
            result("t1", false),
            tool("t2", "Bash", json!({ "command": "npm test", "description": "Run the tests" })),
            result("t2", true),
            // A subagent's steps are left out.
            AgentEvent::AssistantBlock {
                message_id: "s".into(),
                block: Block::Text { text: "subagent talk".into() },
                parent_tool_id: Some("t9".into()),
            },
            AgentEvent::ApprovalRequest {
                request_id: "q1".into(),
                approval: ApprovalKind::Question,
                tool_name: "AskUserQuestion".into(),
                title: String::new(),
                input: json!({}),
                tool_id: None,
                always_label: None,
            },
            AgentEvent::ApprovalAnswered { request_id: "q1".into(), decision: Decision::Allow, answers: Some(answers) },
            AgentEvent::ApprovalRequest {
                request_id: "p1".into(),
                approval: ApprovalKind::Plan,
                tool_name: "ExitPlanMode".into(),
                title: String::new(),
                input: json!({ "plan": "first plan" }),
                tool_id: None,
                always_label: None,
            },
            AgentEvent::PlanEdited { request_id: "p1".into(), plan: "edited plan".into() },
            AgentEvent::ApprovalAnswered { request_id: "p1".into(), decision: Decision::Allow, answers: None },
            said("Done."),
            AgentEvent::TurnEnded { ok: true, duration_ms: None },
            user("taken back", "u2"),
            AgentEvent::MessageWithdrawn { id: "u2".into(), ok: true },
            AgentEvent::AgentSwitched { from: "claude-code".into(), to: "cursor".into(), model: Some("composer-2.5".into()) },
            user("now the terminal row", "u3"),
            AgentEvent::Interrupted,
        ];
        let text = render(&chat(), &events, None, READ_CHARS, 3 * 3_600_000);
        assert!(text.starts_with("# Tab reordering · Claude Code · last used 3 hours ago · 2 turns\n"), "{text}");
        assert!(text.contains("## Turn 1 of 2\n\n### User\n\nmake tabs draggable\n\n### Agent\n\nLooking.\n\n  → Read: tabs.ts\n  → Bash: Run the tests [FAILED]\n\n"));
        assert!(text.contains("Asked the user: Which row?\nThe user answered: Within a row"));
        assert!(text.contains("Plan:\n\nedited plan\n\n(plan approved)\n\nDone.\n"));
        assert!(text.contains("## Turn 2 of 2\n\n(the chat moved from Claude Code to Cursor (composer-2.5) here)\n\n### User\n\nnow the terminal row\n\n### Agent\n\n(stopped by the user)\n"));
        assert!(!text.contains("subagent talk") && !text.contains("taken back") && !text.contains("first plan"));
        assert!(!text.contains("Showing turns"));
    }

    #[test]
    fn a_tool_call_reads_by_the_input_that_arrived_after_it_started() {
        let events = vec![
            user("look for it", "u1"),
            tool("t1", "Grep", json!({ "pattern": null, "path": null })),
            AgentEvent::ToolInput {
                tool_id: "t1".into(),
                name: "Grep".into(),
                input: json!({ "pattern": "fn main", "path": null }),
                parent_tool_id: None,
            },
            result("t1", false),
            tool("t2", "a tool", Value::Null),
            AgentEvent::ToolInput {
                tool_id: "t2".into(),
                name: "Edit".into(),
                input: json!({ "file_path": "/x/notes/abc/report.md" }),
                parent_tool_id: None,
            },
            result("t2", false),
        ];
        let text = render(&chat(), &events, None, READ_CHARS, 0);
        assert!(text.contains("  → Grep: fn main\n  → Edit: report.md\n"), "{text}");
        assert_eq!(docs(&events), ["/x/notes/abc/report.md"]);
    }

    #[test]
    fn a_long_chat_comes_in_parts_newest_first() {
        let mut events = Vec::new();
        for n in 1..=5 {
            events.push(user(&format!("message {n}"), &format!("u{n}")));
            events.push(said(&"x".repeat(100)));
        }
        // Each turn is about 170 characters; 400 fits two.
        let text = render(&chat(), &events, None, 400, 0);
        assert!(text.contains("Showing turns 4–5 of 5. For earlier turns call read with upto_turn=3, or search inside this chat.\n"), "{text}");
        assert!(text.contains("message 4") && text.contains("message 5") && !text.contains("message 3"));

        let earlier = render(&chat(), &events, Some(3), 400, 0);
        assert!(earlier.contains("Showing turns 2–3 of 5. For earlier turns call read with upto_turn=1"), "{earlier}");
        assert!(earlier.contains("message 2") && earlier.contains("message 3") && !earlier.contains("message 4"));

        // The first turn alone: nothing earlier to point at.
        let start = render(&chat(), &events, Some(1), 400, 0);
        assert!(start.contains("Showing turns 1–1 of 5.\n"), "{start}");

        // One turn longer than a read keeps its start and its end.
        let big = vec![user("go", "u1"), said(&format!("START{}END", "y".repeat(5000)))];
        let cut = render(&chat(), &big, None, 600, 0);
        assert!(cut.contains("START") && cut.contains("END\n") && cut.contains("[… cut …]"));
        assert!(cut.chars().count() < 1200);

        // An empty chat and a turn past the end don't trip it.
        assert!(render(&chat(), &[], None, 400, 0).contains("(nothing was said in this chat)"));
        assert!(render(&chat(), &events, Some(99), 400, 0).contains("Showing turns 4–5 of 5."));
    }

    #[test]
    fn docs_a_chat_wrote_are_listed_and_only_they_can_be_read() {
        let dir = std::env::temp_dir().join(format!("tilde-chats-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let report = dir.join("report.md");
        std::fs::write(&report, "the findings").unwrap();
        let moved = dir.join("moved.md");
        std::fs::write(&moved, "now in the project").unwrap();
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
        let path_of_dir = |p: &std::path::Path| p.to_string_lossy().into_owned();
        let events = vec![
            user("write it up", "u1"),
            tool("w1", "Write", json!({ "file_path": path(&report), "content": "x" })),
            result("w1", false),
            tool("w2", "Edit", json!({ "file_path": path(&report) })),
            result("w2", false),
            tool("w3", "Write", json!({ "file_path": "/x/notes/abc/notes.md" })),
            result("w3", false),
            tool("w4", "Write", json!({ "file_path": "/Users/me/.claude/plans/a-plan.md" })),
            result("w4", false),
            tool("w5", "Write", json!({ "file_path": "/x/failed.md" })),
            result("w5", true),
            tool("w6", "Write", json!({ "file_path": "/x/code.rs" })),
            result("w6", false),
            tool("w7", "Write", json!({ "file_path": "/x/draft.md" })),
            result("w7", false),
            AgentEvent::DocSaved { from: "/x/draft.md".into(), to: path(&moved) },
        ];
        assert_eq!(docs(&events), [path(&report), path(&moved)]);
        let same = |path: &str| path.to_string();
        let text = render(&chat(), &events, None, READ_CHARS, 0);
        assert!(text.contains("Docs this chat wrote: report.md, moved.md (read one with doc)\n"), "{text}");
        assert_eq!(read_doc(&events, "report.md", &same).unwrap(), "# report.md\n\nthe findings");
        assert!(read_doc(&events, "moved.md", &same).unwrap().ends_with("now in the project"));
        assert!(read_doc(&events, "code.rs", &same).unwrap_err().contains("report.md, moved.md"));
        assert_eq!(read_doc(&[], "a.md", &same).unwrap_err(), "That chat wrote no docs.");
        // A doc saved under the chat's old folder is read from where it is now.
        let old_place = |path: &str| path.replace(&path_of_dir(&dir), "/gone/notes/abc");
        let was = vec![user("write", "u1"), tool("o1", "Write", json!({ "file_path": "/gone/notes/abc/report.md" })), result("o1", false)];
        let now = |path: &str| path.replace("/gone/notes/abc", &path_of_dir(&dir));
        assert_eq!(std::path::Path::new(&old_place(&path(&report))), std::path::Path::new("/gone/notes/abc/report.md"));
        assert_eq!(read_doc(&was, "report.md", &now).unwrap(), "# report.md\n\nthe findings");
        std::fs::remove_file(&report).unwrap();
        assert!(read_doc(&events, "report.md", &same).unwrap_err().starts_with("That doc is no longer at"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chats_are_listed_on_one_line_each() {
        let mut row = chat();
        row.last_used_at = 0;
        assert_eq!(chat_line(&row, 1, 30_000), "abcdef01  just now  Tab reordering  (Claude Code, 1 turn)");
        row.agent = "cursor".into();
        row.prs = vec![worktree::PrInfo {
            number: 42,
            state: "merged".into(),
            url: "u".into(),
            head: "h".into(),
            branch: "b".into(),
        }];
        assert_eq!(
            chat_line(&row, 5, 2 * 86_400_000),
            "abcdef01  2 days ago  Tab reordering  (Cursor, 5 turns, PR #42 merged)"
        );
        assert_eq!(age(90_000, 0), "1 minute ago");
        assert_eq!(age(59 * 60_000, 0), "59 minutes ago");
        assert_eq!(age(60 * 60_000, 0), "1 hour ago");
    }

    #[tokio::test]
    async fn the_tools_search_list_and_read_a_projects_other_chats() {
        let store = Arc::new(Store::in_memory());
        store.touch_session("aaaa1111-x", "/p", "Reordering side panel tabs", "default", "default").unwrap();
        store.append_event("aaaa1111-x", &user("let users drag tabs to reorder them", "u1")).unwrap();
        store.append_event("aaaa1111-x", &said("Tabs now reorder within a row.")).unwrap();
        store.touch_session("bbbb2222-x", "/p", "The asking chat", "default", "default").unwrap();
        store.append_event("bbbb2222-x", &user("same deal as the tabs", "u1")).unwrap();
        store.touch_session("cccc3333-x", "/other", "Tabs elsewhere", "default", "default").unwrap();
        let home = crate::home::Home::new(std::env::temp_dir().join(format!("tilde-chats-{}", uuid::Uuid::new_v4())));
        let chats = Chats::new(store.clone(), Arc::new(Notes::for_store(home, store.clone())));
        let text = |out: Vec<Value>| out[0]["text"].as_str().unwrap().to_string();
        let call = |tool: &'static str, args: Value| {
            let chats = chats.clone();
            async move { chats.call("bbbb2222-x", "/p", tool, &args).await }
        };

        let found = text(call("search", json!({ "query": "reordering tabs" })).await.unwrap());
        assert!(found.starts_with("aaaa1111  just now  Reordering side panel tabs  (Claude Code, 1 turn)\n    turn 1: …"), "{found}");
        assert!(!found.contains("bbbb2222") && !found.contains("cccc3333"));
        assert_eq!(text(call("search", json!({ "query": "zebra" })).await.unwrap()), "No chats matched. Try other words.");
        assert_eq!(call("search", json!({ "query": " . " })).await.unwrap_err(), "Give a few words to search for.");

        let inside = text(call("search", json!({ "query": "row", "chat": "aaaa1111" })).await.unwrap());
        assert!(inside.contains("turn 1 (agent): …"), "{inside}");

        let recent = text(call("recent", json!({})).await.unwrap());
        assert_eq!(recent, "aaaa1111  just now  Reordering side panel tabs  (Claude Code, 1 turn)\n");

        let read = text(call("read", json!({ "id": "aaaa1111" })).await.unwrap());
        assert!(read.contains("### User\n\nlet users drag tabs to reorder them"), "{read}");
        assert!(text(call("read", json!({ "id": "chat:aaaa1111-x" })).await.unwrap()).starts_with("# Reordering side panel tabs"));
        // This chat reads itself only when the app asks (an agent taking it over).
        assert!(text(call("read", json!({ "id": "bbbb" })).await.unwrap()).starts_with("# "));
        assert_eq!(call("read", json!({ "id": "cccc" })).await.unwrap_err(), "No chat with that id in this project.");
        assert_eq!(call("nope", json!({})).await.unwrap_err(), "No such tool: nope");

        // A new chat is told the project's other chats.
        let note = titles_note(&store, "/p", "bbbb2222-x").unwrap();
        assert!(note.contains("aaaa1111 · just now · Reordering side panel tabs"), "{note}");
        assert!(!note.contains("bbbb2222"));
        assert_eq!(titles_note(&store, "/other", "cccc3333-x"), None);
    }
}
