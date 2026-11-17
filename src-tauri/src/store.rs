//! The app's saved state: projects, chats, each chat's events, and settings.
//! One SQLite file. A chat's history comes only from here, never from the
//! agent's own transcript files.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::events::{AgentEvent, Block};
use crate::worktree::{PrInfo, Worktree};

/// A chat's title is its first message, cut to this many characters.
const TITLE_CHARS: usize = 60;

pub struct Store {
    conn: Mutex<Connection>,
}

/// A chat, as the sidebar and the chat view need it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionRow {
    pub id: String,
    pub project_path: String,
    pub title: String,
    /// Permission mode last used ("default", "acceptEdits", "plan").
    pub mode: String,
    /// Model last chosen: an alias, a full name, or "default".
    pub model: String,
    /// Effort level picked for this chat, or "default" for the agent's own default.
    pub effort: String,
    /// The agent's own ID for the chat, once it has started. Needed to resume.
    pub agent_session_id: Option<String>,
    /// Milliseconds since 1970.
    pub last_used_at: i64,
    /// Where the chat's agent works when it has a worktree (live or put away).
    pub work_path: Option<String>,
    /// The worktree's branch.
    pub branch: Option<String>,
    /// "live" or "put_away"; None without a worktree.
    pub wt_status: Option<String>,
    /// The chat's pull requests as last seen, oldest first.
    pub prs: Vec<PrInfo>,
    /// When the chat was moved to the trash (ms); None when it isn't there.
    pub trashed_at: Option<i64>,
    /// The agent the chat runs on ("claude-code", "cursor"). Set when the
    /// chat is made and never changed.
    pub agent: String,
    /// Where the project's folder used to be, oldest first, when it was moved
    /// or renamed: paths in the chat's history may still name these.
    pub former_paths: Vec<String>,
}

/// Everything about a chat's git state, for the session manager and the sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatGit {
    pub id: String,
    pub project_path: String,
    pub title: String,
    pub last_used_at: i64,
    /// None = not asked yet; "worktree" or "here".
    pub choice: Option<String>,
    /// "live" or "put_away".
    pub status: Option<String>,
    pub worktree: Option<Worktree>,
    pub put_away_at: Option<i64>,
    /// The chat's pull requests as last seen, oldest first.
    pub prs: Vec<PrInfo>,
    /// The main folder's branch and commit when the chat began.
    pub start_branch: Option<String>,
    pub start_commit: Option<String>,
    pub trashed_at: Option<i64>,
}

/// A terminal tab as saved at quit, to come back closed with its output.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedTerminal {
    pub id: String,
    /// The project's main folder, as the window names it.
    pub project: String,
    /// Where its shell started.
    pub folder: String,
    /// The chat worktree folder it was opened in, if any.
    pub worktree: Option<String>,
    /// Its name when nothing runs in it.
    pub label: String,
    /// Its place among the project's tabs.
    pub position: i64,
    /// The last of its output, raw.
    pub output: Vec<u8>,
    pub saved_at: i64,
}

/// A project folder and its chats, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectRow {
    pub id: String,
    pub path: String,
    pub sessions: Vec<SessionRow>,
    /// The folder is gone: moved, renamed or on a drive that isn't there.
    /// The store doesn't look; the command that lists projects fills it in.
    pub missing: bool,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn err(e: rusqlite::Error) -> String {
    format!("Saved data error: {e}")
}

/// A line with each dropped file, `[name](<path>)`, written as just `[name]`.
fn without_file_paths(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("](<") {
        let Some(end) = rest[at..].find(">)") else { break };
        out.push_str(&rest[..=at]);
        rest = &rest[at + end + 2..];
    }
    out + rest
}

/// The first line of a message, cut to a title's length.
fn title_from(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let line = without_file_paths(line);
    let line = line.as_str();
    let mut title: String = line.chars().take(TITLE_CHARS).collect();
    if line.chars().count() > TITLE_CHARS {
        title.push('…');
    }
    title
}

const SESSION_COLUMNS: &str = "s.id, p.path, s.title, s.mode, s.model, s.agent_session_id, \
     s.last_used_at, s.wt, s.wt_status, s.prs, s.trashed_at, s.effort, s.agent, p.former_paths";

/// A JSON column read leniently: unreadable means absent.
fn json_col<T: serde::de::DeserializeOwned>(row: &rusqlite::Row, i: usize) -> rusqlite::Result<Option<T>> {
    let text: Option<String> = row.get(i)?;
    Ok(text.and_then(|t| serde_json::from_str(&t).ok()))
}

fn session_from(row: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    let worktree: Option<Worktree> = json_col(row, 7)?;
    let wt_status: Option<String> = row.get(8)?;
    let worktree = worktree.filter(|_| wt_status.is_some());
    Ok(SessionRow {
        id: row.get(0)?,
        project_path: row.get(1)?,
        title: row.get(2)?,
        mode: row.get(3)?,
        model: row.get(4)?,
        agent_session_id: row.get(5)?,
        last_used_at: row.get(6)?,
        work_path: worktree.as_ref().map(|w| w.work_path().to_string_lossy().into_owned()),
        branch: worktree.map(|w| w.branch),
        wt_status,
        prs: json_col(row, 9)?.unwrap_or_default(),
        trashed_at: row.get(10)?,
        effort: row.get(11)?,
        agent: row.get(12)?,
        former_paths: json_col(row, 13)?.unwrap_or_default(),
    })
}

const GIT_COLUMNS: &str = "s.id, p.path, s.title, s.last_used_at, s.worktree_choice, s.wt_status, \
     s.wt, s.wt_put_away_at, s.prs, s.start_branch, s.start_commit, s.trashed_at";

fn git_from(row: &rusqlite::Row) -> rusqlite::Result<ChatGit> {
    Ok(ChatGit {
        id: row.get(0)?,
        project_path: row.get(1)?,
        title: row.get(2)?,
        last_used_at: row.get(3)?,
        choice: row.get(4)?,
        status: row.get(5)?,
        worktree: json_col(row, 6)?,
        put_away_at: row.get(7)?,
        prs: json_col(row, 8)?.unwrap_or_default(),
        start_branch: row.get(9)?,
        start_commit: row.get(10)?,
        trashed_at: row.get(11)?,
    })
}

/// Columns added after the first release, as (name, definition). Missing ones
/// are added on open, so an older store upgrades in place.
const ADDED_SESSION_COLUMNS: &[(&str, &str)] = &[
    // "auto" (cut from the first message), "generated" or "user".
    ("title_source", "TEXT NOT NULL DEFAULT 'auto'"),
    // The main folder's branch (None when detached) and commit when the chat began.
    ("start_branch", "TEXT"),
    ("start_commit", "TEXT"),
    // None = not asked yet; "worktree" or "here".
    ("worktree_choice", "TEXT"),
    // The chat's worktree as JSON (`worktree::Worktree`), kept while put away.
    ("wt", "TEXT"),
    // "live" or "put_away"; None = no worktree.
    ("wt_status", "TEXT"),
    ("wt_put_away_at", "INTEGER"),
    // One pull request as JSON. No longer used: `prs` replaced it.
    ("pr", "TEXT"),
    ("trashed_at", "INTEGER"),
    // Effort level picked for this chat; "default" = the agent's own default.
    ("effort", "TEXT NOT NULL DEFAULT 'default'"),
    // The chat's pull requests: a JSON list of `worktree::PrInfo`, oldest first.
    ("prs", "TEXT"),
    // Something the app has to tell the chat's agent with its next message
    // (its folder moved, say). Cleared once told.
    ("pending_note", "TEXT"),
];

fn migrate(conn: &Connection) -> Result<(), String> {
    let existing: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('sessions')")
        .map_err(err)?
        .query_map([], |row| row.get(0))
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)?;
    for (name, definition) in ADDED_SESSION_COLUMNS {
        if !existing.iter().any(|c| c == name) {
            conn.execute_batch(&format!("ALTER TABLE sessions ADD COLUMN {name} {definition}"))
                .map_err(err)?;
            // A chat used to keep one pull request; it becomes the list's first.
            if *name == "prs" {
                conn.execute_batch("UPDATE sessions SET prs = json_array(json(pr)) WHERE pr IS NOT NULL")
                    .map_err(err)?;
            }
        }
    }
    // Where a project's folder used to be: a JSON list of paths, oldest first.
    let has_former: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('projects') WHERE name = 'former_paths'")
        .map_err(err)?
        .exists([])
        .map_err(err)?;
    if !has_former {
        conn.execute_batch("ALTER TABLE projects ADD COLUMN former_paths TEXT")
            .map_err(err)?;
    }
    Ok(())
}

/// `path` with the folder `from` at its start swapped for `to`: the same file
/// after its folder moved. None when it isn't in that folder.
pub fn swap_root(path: &str, from: &str, to: &str) -> Option<String> {
    let from = from.trim_end_matches(['/', '\\']);
    let rest = path.strip_prefix(from)?;
    (rest.is_empty() || rest.starts_with(['/', '\\'])).then(|| format!("{}{rest}", to.trim_end_matches(['/', '\\'])))
}

/// The search index over what was said in every chat (`chat_text`): one row
/// per user message, per piece of the agent's own text (a subagent's is left
/// out) and per chat title (`role` "user", "agent" or "title"; a title's
/// `event_id` is 0). Tool calls and their output are not in it.
///
/// Filled once from the saved history, the first time a build with the index
/// opens the store; `append_event` and the title writes keep it current.
fn build_chat_index(conn: &Connection) -> Result<(), String> {
    let built: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'chat_index'", [], |r| r.get(0))
        .optional()
        .map_err(err)?;
    if built.is_some() {
        return Ok(());
    }
    conn.execute_batch(
        "BEGIN;
         DELETE FROM chat_text;
         INSERT INTO chat_text (body, session_id, event_id, role)
             SELECT json_extract(payload, '$.text'), session_id, id, 'user' FROM events
             WHERE json_extract(payload, '$.kind') = 'user_message';
         INSERT INTO chat_text (body, session_id, event_id, role)
             SELECT json_extract(payload, '$.block.text'), session_id, id, 'agent' FROM events
             WHERE json_extract(payload, '$.kind') = 'assistant_block'
               AND json_extract(payload, '$.block.type') = 'text'
               AND json_extract(payload, '$.parent_tool_id') IS NULL;
         DELETE FROM chat_text WHERE role = 'user' AND event_id IN (
             SELECT e.id FROM events w JOIN events e ON e.session_id = w.session_id
             WHERE json_extract(w.payload, '$.kind') = 'message_withdrawn'
               AND json_extract(e.payload, '$.kind') = 'user_message'
               AND json_extract(e.payload, '$.id') = json_extract(w.payload, '$.id'));
         INSERT INTO chat_text (body, session_id, event_id, role)
             SELECT title, id, 0, 'title' FROM sessions;
         INSERT INTO settings (key, value) VALUES ('chat_index', '1');
         COMMIT;",
    )
    .map_err(err)
}

/// Puts a chat's title in the search index, in place of the one there.
fn index_title(conn: &Connection, session_id: &str, title: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM chat_text WHERE session_id = ?1 AND role = 'title'",
        [session_id],
    )?;
    conn.execute(
        "INSERT INTO chat_text (body, session_id, event_id, role) VALUES (?1, ?2, 0, 'title')",
        params![title, session_id],
    )?;
    Ok(())
}

/// The words of a search, as the index matches them: lowercase, letters and
/// digits only, no one-letter words, no repeats, twelve at most.
pub fn search_words(query: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for word in query.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if word.chars().count() >= 2 && !words.iter().any(|w| w == word) {
            words.push(word.to_string());
        }
    }
    words.truncate(12);
    words
}

/// A search for rows holding every word (`AND`) or any of them (`OR`).
fn match_text(words: &[String], joiner: &str) -> String {
    let quoted: Vec<String> = words.iter().map(|w| format!("\"{w}\"")).collect();
    quoted.join(joiner)
}

/// A chat a search found.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatHit {
    pub session: SessionRow,
    /// How many messages the user sent in it.
    pub turns: usize,
    /// A few words around the best match in what was said; empty when only
    /// the title matched.
    pub snippet: String,
    /// The turn that match is in, counted from 1.
    pub snippet_turn: usize,
}

/// One match inside a chat.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnHit {
    /// Counted from 1.
    pub turn: usize,
    /// "user" or "agent".
    pub role: String,
    pub snippet: String,
}

/// How many chats a search hands back; the rest are only counted.
const SEARCH_CHATS: usize = 8;

/// How many of a chat's messages the user sent up to an event (all of them
/// with `upto` None), not counting ones taken back. Counted from the search
/// index, which has one row for each: reading a long chat's saved events for
/// this took 30 times as long, and a search asks it for every chat it lists.
fn turns_upto(conn: &Connection, session_id: &str, upto: Option<i64>) -> rusqlite::Result<usize> {
    conn.query_row(
        "SELECT count(*) FROM chat_text WHERE session_id = ?1 AND role = 'user' AND event_id <= ?2",
        params![session_id, upto.unwrap_or(i64::MAX)],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n as usize)
}

impl Store {
    /// Opens (or creates) the store in `dir`.
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("Can't create {}: {e}", dir.display()))?;
        Self::with(Connection::open(dir.join("harness.db")).map_err(err)?)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        Self::with(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn with(conn: Connection) -> Result<Self, String> {
        // WAL keeps writes cheap while events stream in; a lost last write after
        // a power cut is acceptable for chat history.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS projects (
                 id TEXT PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE,
                 created_at INTEGER NOT NULL,
                 last_used_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS sessions (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 agent TEXT NOT NULL DEFAULT 'claude-code',
                 agent_session_id TEXT,
                 title TEXT NOT NULL,
                 mode TEXT NOT NULL DEFAULT 'default',
                 model TEXT NOT NULL DEFAULT 'default',
                 created_at INTEGER NOT NULL,
                 last_used_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                 payload TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS events_by_session ON events (session_id, id);
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS terminals (
                 id TEXT PRIMARY KEY,
                 project TEXT NOT NULL,
                 folder TEXT NOT NULL,
                 worktree TEXT,
                 label TEXT NOT NULL,
                 position INTEGER NOT NULL,
                 output BLOB NOT NULL,
                 saved_at INTEGER NOT NULL
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS chat_text USING fts5(
                 body, session_id UNINDEXED, event_id UNINDEXED, role UNINDEXED,
                 tokenize = 'porter unicode61'
             );",
        )
        .map_err(err)?;
        migrate(&conn)?;
        build_chat_index(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Replaces the saved terminal tabs with these.
    pub fn save_terminals(&self, tabs: &[SavedTerminal]) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(err)?;
        tx.execute("DELETE FROM terminals", []).map_err(err)?;
        for t in tabs {
            tx.execute(
                "INSERT INTO terminals (id, project, folder, worktree, label, position, output, saved_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![t.id, t.project, t.folder, t.worktree, t.label, t.position, t.output, t.saved_at],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    /// The saved terminal tabs of projects that still exist, in order.
    pub fn load_terminals(&self) -> Result<Vec<SavedTerminal>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, project, folder, worktree, label, position, output, saved_at FROM terminals
                 WHERE project IN (SELECT path FROM projects) ORDER BY project, position",
            )
            .map_err(err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SavedTerminal {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    folder: row.get(2)?,
                    worktree: row.get(3)?,
                    label: row.get(4)?,
                    position: row.get(5)?,
                    output: row.get(6)?,
                    saved_at: row.get(7)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    /// Every project that has chats, most recently used first, each with its
    /// chats newest first.
    pub fn sidebar(&self) -> Result<Vec<ProjectRow>, String> {
        let conn = self.conn.lock().unwrap();
        let mut projects: Vec<ProjectRow> = conn
            .prepare("SELECT id, path FROM projects ORDER BY last_used_at DESC")
            .map_err(err)?
            .query_map([], |row| {
                Ok(ProjectRow {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    sessions: Vec::new(),
                    missing: false,
                })
            })
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;

        let mut by_project = conn
            .prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                 WHERE s.project_id = ?1 AND s.trashed_at IS NULL ORDER BY s.last_used_at DESC"
            ))
            .map_err(err)?;
        for project in &mut projects {
            project.sessions = by_project
                .query_map([&project.id], session_from)
                .map_err(err)?
                .collect::<Result<_, _>>()
                .map_err(err)?;
        }
        projects.retain(|p| !p.sessions.is_empty());
        Ok(projects)
    }

    pub fn session(&self, session_id: &str) -> Result<Option<SessionRow>, String> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                &format!(
                    "SELECT {SESSION_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                     WHERE s.id = ?1"
                ),
                [session_id],
                session_from,
            )
            .optional()
            .map_err(err)
    }

    /// Called for every message sent. Creates the project and the chat the first
    /// time (titled after `first_message`), and marks both as just used.
    /// Returns true when the chat was created by this call.
    pub fn touch_session(
        &self,
        session_id: &str,
        project_path: &str,
        first_message: &str,
        mode: &str,
        model: &str,
    ) -> Result<bool, String> {
        let now = now_ms();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO projects (id, path, created_at, last_used_at) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(path) DO UPDATE SET last_used_at = ?3",
            params![uuid::Uuid::new_v4().to_string(), project_path, now],
        )
        .map_err(err)?;
        let project_id: String = tx
            .query_row("SELECT id FROM projects WHERE path = ?1", [project_path], |r| r.get(0))
            .map_err(err)?;
        let created = tx
            .query_row("SELECT 1 FROM sessions WHERE id = ?1", [session_id], |_| Ok(()))
            .optional()
            .map_err(err)?
            .is_none();
        // A chat stays in the folder it started in; later sends don't move it.
        let title = title_from(first_message);
        tx.execute(
            "INSERT INTO sessions (id, project_id, title, mode, model, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET mode = ?4, model = ?5, last_used_at = ?6",
            params![session_id, project_id, title, mode, model, now],
        )
        .map_err(err)?;
        if created {
            index_title(&tx, session_id, &title).map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(created)
    }

    /// The agent a new chat runs on. Called once, when the chat is made.
    pub fn set_agent(&self, session_id: &str, agent: &str) -> Result<(), String> {
        self.conn
            .lock()
            .unwrap()
            .execute("UPDATE sessions SET agent = ?2 WHERE id = ?1", params![session_id, agent])
            .map_err(err)?;
        Ok(())
    }

    /// Replaces the title cut from the first message with a written one. A chat
    /// the user renamed keeps its name. Returns whether the title changed.
    pub fn set_generated_title(&self, session_id: &str, title: &str) -> Result<bool, String> {
        let title: String = title.trim().chars().take(TITLE_CHARS).collect();
        if title.is_empty() {
            return Ok(false);
        }
        let conn = self.conn.lock().unwrap();
        let changed = conn
            .execute(
                "UPDATE sessions SET title = ?2, title_source = 'generated'
                 WHERE id = ?1 AND title_source = 'auto'",
                params![session_id, title],
            )
            .map_err(err)?
            > 0;
        if changed {
            index_title(&conn, session_id, &title).map_err(err)?;
        }
        Ok(changed)
    }

    /// Chats in the trash, most recently trashed first.
    pub fn trash(&self) -> Result<Vec<SessionRow>, String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                 WHERE s.trashed_at IS NOT NULL ORDER BY s.trashed_at DESC"
            ))
            .map_err(err)?
            .query_map([], session_from)
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err);
        rows
    }

    /// A chat's git state. None if the chat doesn't exist.
    pub fn chat_git(&self, session_id: &str) -> Result<Option<ChatGit>, String> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                &format!(
                    "SELECT {GIT_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                     WHERE s.id = ?1"
                ),
                [session_id],
                git_from,
            )
            .optional()
            .map_err(err)
    }

    /// Every chat the cleanup sweep may act on: with a worktree, in the trash,
    /// or with a pull request still open (to keep its state fresh).
    pub fn sweep_list(&self) -> Result<Vec<ChatGit>, String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .prepare(&format!(
                "SELECT {GIT_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                 WHERE s.wt_status IS NOT NULL OR s.trashed_at IS NOT NULL
                    OR s.prs LIKE '%\"state\":\"open\"%' OR s.prs LIKE '%\"state\":\"draft\"%'"
            ))
            .map_err(err)?
            .query_map([], git_from)
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err);
        rows
    }

    /// Whether a folder is a project already.
    pub fn has_project(&self, path: &str) -> Result<bool, String> {
        self.conn
            .lock()
            .unwrap()
            .prepare("SELECT 1 FROM projects WHERE path = ?1")
            .map_err(err)?
            .exists([path])
            .map_err(err)
    }

    /// Every project's folder, with the commit its repo was on when the
    /// newest of its chats that noted one began. A folder holding that commit
    /// is the same repo: how a moved project is recognised.
    pub fn project_commits(&self) -> Result<Vec<(String, Option<String>)>, String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .prepare(
                "SELECT p.path, (SELECT s.start_commit FROM sessions s
                                 WHERE s.project_id = p.id AND s.start_commit IS NOT NULL
                                 ORDER BY s.last_used_at DESC LIMIT 1)
                 FROM projects p",
            )
            .map_err(err)?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err);
        rows
    }

    /// Re-points a project whose folder moved from `from` to `to`, and
    /// everything kept by that path: its chats (by joining the project
    /// already at `to`, if there is one), its saved terminal tabs, and each
    /// chat's worktree record (`repo`: the repo `to` is in and its place in
    /// it, when it is in one). `from` is added to the folders the project
    /// used to be at, and each chat's agent is left a note for its next
    /// message. Returns the moved chats' IDs.
    pub fn relocate_project(&self, from: &str, to: &str, repo: Option<(&Path, &str)>) -> Result<Vec<String>, String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(err)?;
        let found = |path: &str| -> Result<Option<(String, Vec<String>)>, String> {
            tx.query_row("SELECT id, former_paths FROM projects WHERE path = ?1", [path], |row| {
                Ok((row.get::<_, String>(0)?, json_col::<Vec<String>>(row, 1)?.unwrap_or_default()))
            })
            .optional()
            .map_err(err)
        };
        let (from_id, mut former) = found(from)?.ok_or("That folder isn't one of the projects.")?;
        former.push(from.to_string());
        let chats: Vec<(String, Option<Worktree>)> = tx
            .prepare("SELECT id, wt FROM sessions WHERE project_id = ?1")
            .map_err(err)?
            .query_map([&from_id], |row| Ok((row.get(0)?, json_col(row, 1)?)))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;

        // The project: renamed, or folded into the one already at the new place.
        let kept = match found(to)? {
            Some((to_id, mut theirs)) => {
                tx.execute("UPDATE sessions SET project_id = ?1 WHERE project_id = ?2", params![to_id, from_id])
                    .map_err(err)?;
                tx.execute("DELETE FROM projects WHERE id = ?1", [&from_id]).map_err(err)?;
                theirs.extend(former);
                former = theirs;
                to_id
            }
            None => {
                tx.execute("UPDATE projects SET path = ?1 WHERE id = ?2", params![to, from_id])
                    .map_err(err)?;
                from_id
            }
        };
        former.retain(|path| path != to);
        former.dedup();
        tx.execute(
            "UPDATE projects SET former_paths = ?1, last_used_at = ?2 WHERE id = ?3",
            params![serde_json::to_string(&former).map_err(|e| e.to_string())?, now_ms(), kept],
        )
        .map_err(err)?;

        // Its terminal tabs, after any the new place already has.
        let next: i64 = tx
            .query_row("SELECT coalesce(max(position) + 1, 0) FROM terminals WHERE project = ?1", [to], |r| r.get(0))
            .map_err(err)?;
        let tabs: Vec<(String, String)> = tx
            .prepare("SELECT id, folder FROM terminals WHERE project = ?1 ORDER BY position")
            .map_err(err)?
            .query_map([from], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        for (at, (id, folder)) in tabs.iter().enumerate() {
            let folder = swap_root(folder, from, to).unwrap_or_else(|| folder.clone());
            tx.execute(
                "UPDATE terminals SET project = ?1, folder = ?2, position = ?3 WHERE id = ?4",
                params![to, folder, next + at as i64, id],
            )
            .map_err(err)?;
        }

        // Each chat: its worktree's repo, and a word for its agent.
        let note = format!(
            "This project's folder moved from {from} to {to}. Paths under the old folder are now under the new one."
        );
        for (id, worktree) in &chats {
            if let (Some(mut worktree), Some((root, subpath))) = (worktree.clone(), repo) {
                worktree.repo_root = root.to_path_buf();
                worktree.subpath = subpath.to_string();
                let json = serde_json::to_string(&worktree).map_err(|e| e.to_string())?;
                tx.execute("UPDATE sessions SET wt = ?2 WHERE id = ?1", params![id, json])
                    .map_err(err)?;
            }
            tx.execute(
                "UPDATE sessions SET pending_note = CASE
                     WHEN pending_note IS NULL OR pending_note = '' THEN ?2
                     ELSE pending_note || char(10) || char(10) || ?2 END
                 WHERE id = ?1",
                params![id, note],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(chats.into_iter().map(|(id, _)| id).collect())
    }

    /// Leaves something for the chat's agent to be told with the chat's next
    /// message. Added after anything already waiting.
    pub fn add_pending_note(&self, session_id: &str, note: &str) -> Result<(), String> {
        self.exec(
            "UPDATE sessions SET pending_note = CASE
                 WHEN pending_note IS NULL OR pending_note = '' THEN ?2
                 ELSE pending_note || char(10) || char(10) || ?2 END
             WHERE id = ?1",
            params![session_id, note],
        )
    }

    /// What is waiting to be told to the chat's agent, which is then cleared:
    /// it is told once.
    pub fn take_pending_note(&self, session_id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().unwrap();
        let note: Option<String> = conn
            .query_row("SELECT pending_note FROM sessions WHERE id = ?1", [session_id], |r| r.get(0))
            .optional()
            .map_err(err)?
            .flatten();
        let note = note.filter(|n| !n.trim().is_empty());
        if note.is_some() {
            conn.execute("UPDATE sessions SET pending_note = NULL WHERE id = ?1", [session_id])
                .map_err(err)?;
        }
        Ok(note)
    }

    /// Where the main folder was when the chat began. Set once.
    pub fn set_start(&self, session_id: &str, branch: Option<&str>, commit: &str) -> Result<(), String> {
        self.exec(
            "UPDATE sessions SET start_branch = ?2, start_commit = ?3 WHERE id = ?1",
            params![session_id, branch, commit],
        )
    }

    pub fn set_choice(&self, session_id: &str, choice: Option<&str>) -> Result<(), String> {
        self.exec(
            "UPDATE sessions SET worktree_choice = ?2 WHERE id = ?1",
            params![session_id, choice],
        )
    }

    /// Saves a chat's worktree as live (its folder exists) or put away.
    ///
    /// A chat starting a new worktree (none saved) drops its finished pull
    /// requests: new work, new list. It keeps them while any is open or a
    /// draft, and through being put away and brought back.
    pub fn set_worktree(&self, session_id: &str, worktree: &Worktree, status: &str) -> Result<(), String> {
        let json = serde_json::to_string(worktree).map_err(|e| e.to_string())?;
        let put_away_at = (status == "put_away").then(now_ms);
        self.exec(
            "UPDATE sessions SET wt = ?2, wt_status = ?3, wt_put_away_at = ?4,
                 prs = CASE WHEN wt IS NULL AND prs NOT LIKE '%\"state\":\"open\"%'
                                 AND prs NOT LIKE '%\"state\":\"draft\"%'
                            THEN NULL ELSE prs END
             WHERE id = ?1",
            params![session_id, json, status, put_away_at],
        )
    }

    /// Notes that the worktree's folder is on another branch now. Changes
    /// nothing else about it.
    pub fn set_wt_branch(&self, session_id: &str, worktree: &Worktree) -> Result<(), String> {
        let json = serde_json::to_string(worktree).map_err(|e| e.to_string())?;
        self.exec(
            "UPDATE sessions SET wt = ?2 WHERE id = ?1 AND wt IS NOT NULL",
            params![session_id, json],
        )
    }

    /// The chat no longer has a worktree, and will be asked again at its next edit.
    /// Its pull requests stay, so a finished chat keeps its mark (purple once
    /// merged).
    pub fn clear_worktree(&self, session_id: &str) -> Result<(), String> {
        self.exec(
            "UPDATE sessions SET wt = NULL, wt_status = NULL, wt_put_away_at = NULL,
                 worktree_choice = NULL WHERE id = ?1",
            params![session_id],
        )
    }

    /// Adds a pull request to the chat's list, or replaces the one with the
    /// same link. Returns the list afterwards and whether it changed.
    ///
    /// The only way the list is written, and it reads and writes under one
    /// lock, so two updates at once (a refresh and a new pull request) can't
    /// undo each other.
    pub fn upsert_pr(&self, session_id: &str, pr: &PrInfo) -> Result<(Vec<PrInfo>, bool), String> {
        let conn = self.conn.lock().unwrap();
        let saved: Option<Option<String>> = conn
            .query_row("SELECT prs FROM sessions WHERE id = ?1", [session_id], |row| row.get(0))
            .optional()
            .map_err(err)?;
        let Some(saved) = saved else {
            return Ok((Vec::new(), false)); // the chat is gone
        };
        let mut prs: Vec<PrInfo> = saved.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        match prs.iter_mut().find(|p| p.url == pr.url) {
            Some(same) if same == pr => return Ok((prs, false)),
            Some(old) => *old = pr.clone(),
            None => prs.push(pr.clone()),
        }
        let json = serde_json::to_string(&prs).map_err(|e| e.to_string())?;
        conn.execute("UPDATE sessions SET prs = ?2 WHERE id = ?1", params![session_id, json])
            .map_err(err)?;
        Ok((prs, true))
    }

    /// Moves a chat to the trash (`Some(now)`) or back out (`None`).
    pub fn set_trashed(&self, session_id: &str, at: Option<i64>) -> Result<(), String> {
        self.exec(
            "UPDATE sessions SET trashed_at = ?2 WHERE id = ?1",
            params![session_id, at],
        )
    }

    fn exec(&self, sql: &str, params: impl rusqlite::Params) -> Result<(), String> {
        self.conn
            .lock()
            .unwrap()
            .execute(sql, params)
            .map(|_| ())
            .map_err(err)
    }

    pub fn set_agent_session_id(&self, session_id: &str, agent_session_id: &str) -> Result<(), String> {
        self.update("agent_session_id", session_id, agent_session_id)
    }

    pub fn set_mode(&self, session_id: &str, mode: &str) -> Result<(), String> {
        self.update("mode", session_id, mode)
    }

    pub fn set_model(&self, session_id: &str, model: &str) -> Result<(), String> {
        self.update("model", session_id, model)
    }

    pub fn set_effort(&self, session_id: &str, effort: &str) -> Result<(), String> {
        self.update("effort", session_id, effort)
    }

    /// `column` is always one of the fixed names above, never user input.
    fn update(&self, column: &str, session_id: &str, value: &str) -> Result<(), String> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                &format!("UPDATE sessions SET {column} = ?2 WHERE id = ?1"),
                params![session_id, value],
            )
            .map(|_| ())
            .map_err(err)
    }

    /// Adds one event to a chat's history. A chat that doesn't exist (deleted
    /// while its process was still talking) is skipped quietly.
    pub fn append_event(&self, session_id: &str, event: &AgentEvent) -> Result<(), String> {
        let payload = serde_json::to_string(event).map_err(|e| e.to_string())?;
        let conn = self.conn.lock().unwrap();
        let saved = conn
            .execute(
                "INSERT INTO events (session_id, payload, created_at)
                 SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM sessions WHERE id = ?1)",
                params![session_id, payload, now_ms()],
            )
            .map_err(err)?
            == 1;
        if !saved {
            return Ok(());
        }
        // What was said goes in the search index too (see `build_chat_index`).
        let said = match event {
            AgentEvent::UserMessage { text, .. } => Some(("user", text)),
            AgentEvent::AssistantBlock { block: Block::Text { text }, parent_tool_id: None, .. } => {
                Some(("agent", text))
            }
            _ => None,
        };
        if let Some((role, text)) = said {
            conn.execute(
                "INSERT INTO chat_text (body, session_id, event_id, role) VALUES (?1, ?2, ?3, ?4)",
                params![text, session_id, conn.last_insert_rowid(), role],
            )
            .map_err(err)?;
        }
        // A message taken back leaves the index, as it leaves the chat.
        if let AgentEvent::MessageWithdrawn { id, ok: true } = event {
            conn.execute(
                "DELETE FROM chat_text WHERE session_id = ?1 AND role = 'user' AND event_id IN (
                     SELECT id FROM events WHERE session_id = ?1
                       AND json_extract(payload, '$.kind') = 'user_message'
                       AND json_extract(payload, '$.id') = ?2)",
                params![session_id, id],
            )
            .map_err(err)?;
        }
        Ok(())
    }

    /// The one chat in a project whose ID starts with `id`, the trash and the
    /// asking chat (`asking`) left out. Agents are shown the first 8
    /// characters of an ID, so a start is enough.
    pub fn find_chat(&self, project_path: &str, id: &str, asking: &str) -> Result<SessionRow, String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("Give the chat's id.".into());
        }
        let conn = self.conn.lock().unwrap();
        let mut found: Vec<SessionRow> = conn
            .prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                 WHERE p.path = ?1 AND s.trashed_at IS NULL AND substr(s.id, 1, length(?2)) = ?2"
            ))
            .map_err(err)?
            .query_map(params![project_path, id], session_from)
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        if found.len() == 1 && found[0].id == asking {
            return Err("That is this chat.".into());
        }
        found.retain(|s| s.id != asking);
        match found.len() {
            0 => Err("No chat with that id in this project.".into()),
            1 => Ok(found.remove(0)),
            _ => Err("More than one chat's id starts with that. Give more of the id.".into()),
        }
    }

    /// A project's latest chats, newest first, the trash and the asking chat
    /// left out.
    pub fn recent_chats(&self, project_path: &str, asking: &str, limit: usize) -> Result<Vec<SessionRow>, String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                 WHERE p.path = ?1 AND s.trashed_at IS NULL AND s.id <> ?2
                 ORDER BY s.last_used_at DESC LIMIT ?3"
            ))
            .map_err(err)?
            .query_map(params![project_path, asking, limit as i64], session_from)
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err);
        rows
    }

    /// How many messages the user sent in a chat, not counting ones taken back.
    pub fn turn_count(&self, session_id: &str) -> Result<usize, String> {
        turns_upto(&self.conn.lock().unwrap(), session_id, None).map_err(err)
    }

    /// The project's chats that match the words, best first, and how many
    /// more matched than are handed back. The trash and the asking chat are
    /// left out.
    ///
    /// A chat ranks by how many of the words it holds, with a word in its
    /// title counting three times: that is what tells the chat about a thing
    /// from a long chat that happens to mention each word often. Chats that
    /// tie are ordered by their three best-matching messages, then by which
    /// was used last.
    pub fn search_chats(
        &self,
        project_path: &str,
        asking: &str,
        words: &[String],
    ) -> Result<(Vec<ChatHit>, usize), String> {
        if words.is_empty() {
            return Ok((Vec::new(), 0));
        }
        let conn = self.conn.lock().unwrap();

        #[derive(Default)]
        struct Found {
            /// How many of the words it holds, a title word counted three times.
            weight: usize,
            /// Its matching messages' ranks (bm25: negative, lower is better).
            ranks: Vec<f64>,
            /// The best matching message: its rank, event and snippet.
            best: Option<(f64, i64, String)>,
        }
        let mut found: std::collections::HashMap<String, Found> = std::collections::HashMap::new();

        // Which chats hold each word, and whether in the title.
        let mut holders = conn
            .prepare(
                "SELECT chat_text.session_id, max(chat_text.role = 'title') FROM chat_text
                 JOIN sessions s ON s.id = chat_text.session_id
                 JOIN projects p ON p.id = s.project_id
                 WHERE chat_text MATCH ?1 AND p.path = ?2 AND s.trashed_at IS NULL AND s.id <> ?3
                 GROUP BY chat_text.session_id",
            )
            .map_err(err)?;
        for word in words {
            let rows: Vec<(String, bool)> = holders
                .query_map(params![format!("\"{word}\""), project_path, asking], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(err)?
                .collect::<Result<_, _>>()
                .map_err(err)?;
            for (id, in_title) in rows {
                found.entry(id).or_default().weight += if in_title { 3 } else { 1 };
            }
        }

        // The messages that match, for the order among equals and the snippet.
        let any: Vec<(String, i64, f64, String)> = conn
            .prepare(
                "SELECT chat_text.session_id, chat_text.event_id, bm25(chat_text),
                        snippet(chat_text, 0, '', '', '…', 16)
                 FROM chat_text
                 JOIN sessions s ON s.id = chat_text.session_id
                 JOIN projects p ON p.id = s.project_id
                 WHERE chat_text MATCH ?1 AND chat_text.role <> 'title'
                   AND p.path = ?2 AND s.trashed_at IS NULL AND s.id <> ?3
                 ORDER BY bm25(chat_text) LIMIT 500",
            )
            .map_err(err)?
            .query_map(params![match_text(words, " OR "), project_path, asking], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        for (id, event_id, rank, snippet) in any {
            let chat = found.entry(id).or_default();
            // Best first, so the first three are the three best.
            if chat.ranks.len() < 3 {
                chat.ranks.push(rank);
            }
            if chat.best.is_none() {
                chat.best = Some((rank, event_id, snippet));
            }
        }

        let mut hits: Vec<(String, Found, SessionRow)> = Vec::new();
        for (id, chat) in found {
            let row = conn
                .query_row(
                    &format!(
                        "SELECT {SESSION_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                         WHERE s.id = ?1"
                    ),
                    [&id],
                    session_from,
                )
                .map_err(err)?;
            hits.push((id, chat, row));
        }
        let score = |chat: &Found| -> f64 { chat.ranks.iter().map(|rank| -rank).sum() };
        hits.sort_by(|(_, a, a_row), (_, b, b_row)| {
            b.weight
                .cmp(&a.weight)
                .then(score(b).partial_cmp(&score(a)).unwrap_or(std::cmp::Ordering::Equal))
                .then(b_row.last_used_at.cmp(&a_row.last_used_at))
        });
        let more = hits.len().saturating_sub(SEARCH_CHATS);
        hits.truncate(SEARCH_CHATS);
        let mut out = Vec::new();
        for (id, chat, session) in hits {
            let turns = turns_upto(&conn, &id, None).map_err(err)?;
            let (snippet, snippet_turn) = match chat.best {
                Some((_, event_id, snippet)) => (snippet, turns_upto(&conn, &id, Some(event_id)).map_err(err)?),
                None => (String::new(), 0),
            };
            out.push(ChatHit { session, turns, snippet, snippet_turn });
        }
        Ok((out, more))
    }

    /// The places in one chat that match any of the words: the ten best,
    /// newest first.
    pub fn search_in_chat(&self, session_id: &str, words: &[String]) -> Result<Vec<TurnHit>, String> {
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().unwrap();
        let mut rows: Vec<(String, i64, String)> = conn
            .prepare(
                "SELECT role, event_id, snippet(chat_text, 0, '', '', '…', 24) FROM chat_text
                 WHERE chat_text MATCH ?1 AND session_id = ?2 AND role <> 'title'
                 ORDER BY bm25(chat_text) LIMIT 10",
            )
            .map_err(err)?
            .query_map(params![match_text(words, " OR "), session_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        let mut out = Vec::new();
        for (role, event_id, snippet) in rows {
            let turn = turns_upto(&conn, session_id, Some(event_id)).map_err(err)?;
            out.push(TurnHit { turn, role, snippet });
        }
        Ok(out)
    }

    /// A chat's history in order. Entries this version can't read are skipped,
    /// so an older or newer build's data never breaks loading.
    pub fn events(&self, session_id: &str) -> Result<Vec<AgentEvent>, String> {
        let conn = self.conn.lock().unwrap();
        let payloads: Vec<String> = conn
            .prepare("SELECT payload FROM events WHERE session_id = ?1 ORDER BY id")
            .map_err(err)?
            .query_map([session_id], |row| row.get(0))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        Ok(payloads
            .iter()
            .filter_map(|p| serde_json::from_str(p).ok())
            .collect())
    }

    /// The ID of every chat, the trash included.
    pub fn session_ids(&self) -> Result<std::collections::HashSet<String>, String> {
        let conn = self.conn.lock().unwrap();
        let ids = conn
            .prepare("SELECT id FROM sessions")
            .map_err(err)?
            .query_map([], |row| row.get(0))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err);
        ids
    }

    /// Removes a chat and its history. A project left with no chats goes too.
    pub fn delete_session(&self, session_id: &str) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(err)?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
            .map_err(err)?;
        tx.execute("DELETE FROM chat_text WHERE session_id = ?1", [session_id])
            .map_err(err)?;
        tx.execute(
            "DELETE FROM projects WHERE id NOT IN (SELECT project_id FROM sessions)",
            [],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }

    /// Gives a chat a new title. An empty name is refused.
    pub fn rename_session(&self, session_id: &str, title: &str) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("A chat needs a name".into());
        }
        let conn = self.conn.lock().unwrap();
        let changed = conn
            .execute(
                "UPDATE sessions SET title = ?2, title_source = 'user' WHERE id = ?1",
                params![session_id, title],
            )
            .map_err(err)?;
        if changed > 0 {
            index_title(&conn, session_id, title).map_err(err)?;
        }
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    /// For the settings page, which isn't built yet.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![key, value],
            )
            .map(|_| ())
            .map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> AgentEvent {
        AgentEvent::UserMessage { text: text.into(), id: None, images: vec![] }
    }

    #[test]
    fn a_title_names_dropped_files_without_their_paths() {
        assert_eq!(title_from("look at [a b.pdf](</Users/me/a b.pdf>) and [c/](</x/c>) now"), "look at [a b.pdf] and [c/] now");
        assert_eq!(title_from("a [link](<half"), "a [link](<half");
    }

    #[test]
    fn first_message_creates_project_and_titled_chat() {
        let store = Store::in_memory();
        store
            .touch_session("s1", "/work/app", "\n  Fix the login bug\nmore detail", "plan", "opus")
            .unwrap();
        let sidebar = store.sidebar().unwrap();
        assert_eq!(sidebar.len(), 1);
        assert_eq!(sidebar[0].path, "/work/app");
        let chat = &sidebar[0].sessions[0];
        assert_eq!(chat.title, "Fix the login bug");
        assert_eq!((chat.mode.as_str(), chat.model.as_str()), ("plan", "opus"));
        assert_eq!(chat.agent_session_id, None);
    }

    #[test]
    fn later_messages_keep_the_title_and_update_mode_and_model() {
        let store = Store::in_memory();
        store.touch_session("s1", "/work/app", "First", "default", "default").unwrap();
        store.touch_session("s1", "/work/app", "Second", "acceptEdits", "haiku").unwrap();
        let chat = store.session("s1").unwrap().unwrap();
        assert_eq!(chat.title, "First");
        assert_eq!((chat.mode.as_str(), chat.model.as_str()), ("acceptEdits", "haiku"));
        assert_eq!(store.sidebar().unwrap()[0].sessions.len(), 1);
    }

    #[test]
    fn long_titles_are_cut() {
        let store = Store::in_memory();
        store.touch_session("s1", "/p", &"x".repeat(200), "default", "default").unwrap();
        let title = store.session("s1").unwrap().unwrap().title;
        assert_eq!(title.chars().count(), TITLE_CHARS + 1);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn chats_group_by_folder_and_resume_id_is_kept() {
        let store = Store::in_memory();
        store.touch_session("a", "/one", "A", "default", "default").unwrap();
        store.touch_session("b", "/two", "B", "default", "default").unwrap();
        store.touch_session("c", "/one", "C", "default", "default").unwrap();
        store.set_agent_session_id("a", "agent-1").unwrap();
        let sidebar = store.sidebar().unwrap();
        assert_eq!(sidebar.len(), 2);
        let one = sidebar.iter().find(|p| p.path == "/one").unwrap();
        assert_eq!(one.sessions.len(), 2);
        assert_eq!(
            store.session("a").unwrap().unwrap().agent_session_id.as_deref(),
            Some("agent-1")
        );
        assert_eq!(store.session("nope").unwrap(), None);
    }

    #[test]
    fn events_come_back_in_order_and_unreadable_ones_are_skipped() {
        let store = Store::in_memory();
        store.touch_session("s1", "/p", "hi", "default", "default").unwrap();
        store.append_event("s1", &user("one")).unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO events (session_id, payload, created_at) VALUES ('s1', '{\"kind\":\"from_the_future\"}', 0)",
                [],
            )
            .unwrap();
        store.append_event("s1", &user("two")).unwrap();
        assert_eq!(store.events("s1").unwrap(), vec![user("one"), user("two")]);
        // An event for a chat that isn't saved is dropped, not an error.
        store.append_event("ghost", &user("lost")).unwrap();
        assert!(store.events("ghost").unwrap().is_empty());
    }

    #[test]
    fn deleting_the_last_chat_removes_its_project_and_history() {
        let store = Store::in_memory();
        store.touch_session("a", "/one", "A", "default", "default").unwrap();
        store.touch_session("b", "/one", "B", "default", "default").unwrap();
        store.append_event("a", &user("x")).unwrap();
        store.delete_session("a").unwrap();
        assert!(store.events("a").unwrap().is_empty());
        assert_eq!(store.sidebar().unwrap()[0].sessions.len(), 1);
        store.delete_session("b").unwrap();
        assert!(store.sidebar().unwrap().is_empty());
    }

    #[test]
    fn a_written_title_replaces_the_cut_one_but_never_a_rename() {
        let store = Store::in_memory();
        assert!(store.touch_session("a", "/p", "fix the thing please", "default", "default").unwrap());
        assert!(!store.touch_session("a", "/p", "again", "default", "default").unwrap());
        assert!(store.set_generated_title("a", " Fix the thing ").unwrap());
        assert_eq!(store.session("a").unwrap().unwrap().title, "Fix the thing");
        // Only once: a second written title doesn't replace the first.
        assert!(!store.set_generated_title("a", "Other").unwrap());

        store.touch_session("b", "/p", "hello", "default", "default").unwrap();
        store.rename_session("b", "Mine").unwrap();
        assert!(!store.set_generated_title("b", "Written").unwrap());
        assert_eq!(store.session("b").unwrap().unwrap().title, "Mine");
        assert!(!store.set_generated_title("b", "  ").unwrap());
    }

    #[test]
    fn an_older_store_gains_the_new_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE,
                 created_at INTEGER NOT NULL, last_used_at INTEGER NOT NULL);
             CREATE TABLE sessions (id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
                 agent TEXT NOT NULL DEFAULT 'claude-code', agent_session_id TEXT,
                 title TEXT NOT NULL, mode TEXT NOT NULL DEFAULT 'default',
                 model TEXT NOT NULL DEFAULT 'default', created_at INTEGER NOT NULL,
                 last_used_at INTEGER NOT NULL);
             INSERT INTO projects VALUES ('p', '/old', 0, 0);
             INSERT INTO sessions (id, project_id, title, created_at, last_used_at)
                 VALUES ('s', 'p', 'Old chat', 0, 0);",
        )
        .unwrap();
        let store = Store::with(conn).unwrap();
        assert!(store.set_generated_title("s", "New name").unwrap());
        assert_eq!(store.session("s").unwrap().unwrap().title, "New name");
    }

    fn worktree() -> Worktree {
        Worktree {
            repo_root: "/one".into(),
            subpath: "web".into(),
            path: "/wts/one-abc".into(),
            branch: "harness/x-abc".into(),
            base_commit: "c1".into(),
            base_branch: Some("main".into()),
            seen: Vec::new(),
        }
    }

    #[test]
    fn a_worktree_is_saved_shown_and_cleared() {
        let store = Store::in_memory();
        store.touch_session("a", "/one/web", "A", "default", "default").unwrap();
        store.set_start("a", Some("main"), "c1").unwrap();
        store.set_choice("a", Some("worktree")).unwrap();
        store.set_worktree("a", &worktree(), "live").unwrap();
        let row = store.session("a").unwrap().unwrap();
        assert_eq!(row.work_path.as_deref(), Some("/wts/one-abc/web"));
        assert_eq!(row.branch.as_deref(), Some("harness/x-abc"));
        assert_eq!(row.project_path, "/one/web");

        let open = pr(4, "open");
        let pr = open.clone();
        store.upsert_pr("a", &pr).unwrap();
        store.set_worktree("a", &worktree(), "put_away").unwrap();
        let git = store.chat_git("a").unwrap().unwrap();
        assert_eq!(git.status.as_deref(), Some("put_away"));
        assert!(git.put_away_at.is_some());
        assert_eq!(git.prs, vec![pr.clone()]);
        assert_eq!(git.worktree, Some(worktree()));
        assert_eq!((git.start_branch.as_deref(), git.start_commit.as_deref()), (Some("main"), Some("c1")));
        assert_eq!(store.sweep_list().unwrap().len(), 1);

        // The folder moved to another branch: only that changes.
        let moved = Worktree { branch: "part-2".into(), ..worktree() };
        store.set_wt_branch("a", &moved).unwrap();
        let git = store.chat_git("a").unwrap().unwrap();
        assert_eq!((git.worktree, git.status.as_deref()), (Some(moved), Some("put_away")));
        assert_eq!(git.prs, vec![pr.clone()]);

        store.clear_worktree("a").unwrap();
        let git = store.chat_git("a").unwrap().unwrap();
        assert_eq!((git.choice, git.status, git.worktree), (None, None, None));
        assert!(store.session("a").unwrap().unwrap().work_path.is_none());
        // Still swept while its pull request is open, to keep the mark fresh.
        assert_eq!(store.sweep_list().unwrap().len(), 1);
        // The pull requests outlive the worktree, so the sidebar keeps its mark.
        assert_eq!(store.session("a").unwrap().unwrap().prs, vec![pr.clone()]);
        // A new worktree keeps unfinished ones...
        store.set_worktree("a", &worktree(), "live").unwrap();
        assert_eq!(store.chat_git("a").unwrap().unwrap().prs, vec![pr]);
        // ...and starts a fresh list once they are all finished.
        store.upsert_pr("a", &PrInfo { state: "merged".into(), ..open }).unwrap();
        store.clear_worktree("a").unwrap();
        assert!(store.sweep_list().unwrap().is_empty());
        store.set_worktree("a", &worktree(), "live").unwrap();
        assert_eq!(store.chat_git("a").unwrap().unwrap().prs, vec![]);
        store.set_wt_branch("missing", &worktree()).unwrap();
    }

    fn pr(number: i64, state: &str) -> PrInfo {
        PrInfo {
            number,
            state: state.into(),
            url: format!("https://github.com/o/r/pull/{number}"),
            head: "h".into(),
            branch: format!("b{number}"),
        }
    }

    #[test]
    fn pull_requests_are_added_and_replaced_by_link() {
        let store = Store::in_memory();
        store.touch_session("a", "/one", "A", "default", "default").unwrap();
        assert_eq!(store.upsert_pr("a", &pr(1, "open")).unwrap(), (vec![pr(1, "open")], true));
        assert_eq!(store.upsert_pr("a", &pr(2, "draft")).unwrap().0.len(), 2);
        // The same again changes nothing; a new state replaces in place.
        assert!(!store.upsert_pr("a", &pr(1, "open")).unwrap().1);
        let (prs, changed) = store.upsert_pr("a", &pr(1, "merged")).unwrap();
        assert!(changed);
        assert_eq!(prs, vec![pr(1, "merged"), pr(2, "draft")]);
        assert_eq!(store.session("a").unwrap().unwrap().prs, prs);
        // A chat that is gone takes nothing.
        assert_eq!(store.upsert_pr("gone", &pr(1, "open")).unwrap(), (vec![], false));
    }

    #[test]
    fn an_older_stores_single_pull_request_becomes_the_list() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE,
                 created_at INTEGER NOT NULL, last_used_at INTEGER NOT NULL);
             CREATE TABLE sessions (id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
                 agent TEXT NOT NULL DEFAULT 'claude-code', agent_session_id TEXT, title TEXT NOT NULL,
                 mode TEXT NOT NULL DEFAULT 'default', model TEXT NOT NULL DEFAULT 'default',
                 created_at INTEGER NOT NULL, last_used_at INTEGER NOT NULL, pr TEXT);
             INSERT INTO projects VALUES ('p', '/old', 0, 0);
             INSERT INTO sessions (id, project_id, title, created_at, last_used_at, pr) VALUES
                 ('with', 'p', 'A', 0, 0, '{\"number\":1,\"state\":\"merged\",\"url\":\"u\",\"head\":\"h\"}'),
                 ('without', 'p', 'B', 0, 0, NULL);",
        )
        .unwrap();
        let store = Store::with(conn).unwrap();
        let old = PrInfo { number: 1, state: "merged".into(), url: "u".into(), head: "h".into(), branch: String::new() };
        assert_eq!(store.session("with").unwrap().unwrap().prs, vec![old]);
        assert_eq!(store.session("without").unwrap().unwrap().prs, vec![]);
    }

    #[test]
    fn trashed_chats_leave_the_sidebar_and_can_come_back() {
        let store = Store::in_memory();
        store.touch_session("a", "/one", "A", "default", "default").unwrap();
        store.touch_session("b", "/two", "B", "default", "default").unwrap();
        store.set_trashed("b", Some(5)).unwrap();
        let sidebar = store.sidebar().unwrap();
        assert_eq!(sidebar.len(), 1, "a project with only trashed chats is hidden");
        assert_eq!(store.trash().unwrap()[0].id, "b");
        assert_eq!(store.sweep_list().unwrap().len(), 1);
        store.set_trashed("b", None).unwrap();
        assert_eq!(store.sidebar().unwrap().len(), 2);
        assert!(store.trash().unwrap().is_empty());
    }

    #[test]
    fn a_pending_note_is_handed_over_once() {
        let store = Store::in_memory();
        store.touch_session("s1", "/p", "hi", "default", "default").unwrap();
        assert_eq!(store.take_pending_note("s1").unwrap(), None);
        store.add_pending_note("s1", "The folder moved.").unwrap();
        store.add_pending_note("s1", "And again.").unwrap();
        assert_eq!(store.take_pending_note("s1").unwrap().as_deref(), Some("The folder moved.\n\nAnd again."));
        assert_eq!(store.take_pending_note("s1").unwrap(), None);
        assert_eq!(store.take_pending_note("nope").unwrap(), None);
    }

    #[test]
    fn a_projects_saved_terminal_tabs_follow_its_folder() {
        let store = Store::in_memory();
        store.touch_session("s1", "/old/app", "hi", "default", "default").unwrap();
        store.touch_session("s2", "/new/app", "hi", "default", "default").unwrap();
        let tab = |id: &str, project: &str, folder: &str, position: i64| SavedTerminal {
            id: id.into(),
            project: project.into(),
            folder: folder.into(),
            worktree: None,
            label: "app".into(),
            position,
            output: Vec::new(),
            saved_at: 0,
        };
        store
            .save_terminals(&[
                tab("t1", "/old/app", "/old/app", 0),
                tab("t2", "/old/app", "/old/app/web", 1),
                tab("t3", "/old/app", "/somewhere/else", 2),
                tab("t4", "/new/app", "/new/app", 0),
            ])
            .unwrap();
        assert_eq!(store.relocate_project("/old/app", "/new/app", None).unwrap(), ["s1"]);
        let tabs: Vec<(String, String, String, i64)> =
            store.load_terminals().unwrap().into_iter().map(|t| (t.id, t.project, t.folder, t.position)).collect();
        assert_eq!(
            tabs,
            [
                ("t4".into(), "/new/app".into(), "/new/app".into(), 0),
                ("t1".into(), "/new/app".into(), "/new/app".into(), 1),
                ("t2".into(), "/new/app".into(), "/new/app/web".into(), 2),
                ("t3".into(), "/new/app".into(), "/somewhere/else".into(), 3),
            ]
        );
        // Moved a second time, it remembers both places it has been.
        store.relocate_project("/new/app", "/newer/app", None).unwrap();
        assert_eq!(store.session("s1").unwrap().unwrap().former_paths, ["/old/app", "/new/app"]);
        assert_eq!(swap_root("/old/app/x.md", "/old/app/", "/new/app"), Some("/new/app/x.md".to_string()));
        assert_eq!(swap_root("/old/application/x.md", "/old/app", "/new/app"), None);
    }

    #[test]
    fn settings_round_trip() {
        let store = Store::in_memory();
        assert_eq!(store.setting("idle_seconds"), None);
        store.set_setting("idle_seconds", "120").unwrap();
        store.set_setting("idle_seconds", "300").unwrap();
        assert_eq!(store.setting("idle_seconds").as_deref(), Some("300"));
    }

    fn said(text: &str) -> AgentEvent {
        AgentEvent::AssistantBlock { message_id: "m".into(), block: Block::Text { text: text.into() }, parent_tool_id: None }
    }

    fn words(query: &str) -> Vec<String> {
        search_words(query)
    }

    fn found(store: &Store, asking: &str, query: &str) -> Vec<String> {
        let (hits, _) = store.search_chats("/p", asking, &words(query)).unwrap();
        hits.into_iter().map(|h| h.session.id).collect()
    }

    #[test]
    fn a_search_is_cut_into_plain_words() {
        assert_eq!(words("Tab-reordering, the TABS & a b!"), ["tab", "reordering", "the", "tabs"]);
        assert!(words(" . ").is_empty());
        assert_eq!(words(&"w1 w2 w3 w4 w5 w6 w7 w8 w9 w10 w11 w12 w13 w14").len(), 12);
    }

    #[test]
    fn chats_are_found_by_what_was_said_and_by_title() {
        let store = Store::in_memory();
        store.touch_session("title", "/p", "Reordering side panel tabs", "default", "default").unwrap();
        store.append_event("title", &user("let people drag them")).unwrap();
        store.touch_session("body", "/p", "Browser work", "default", "default").unwrap();
        store.append_event("body", &user("first message")).unwrap();
        store.append_event("body", &said("ok")).unwrap();
        store.append_event("body", &user("we reordered the tabs in the panel yesterday")).unwrap();
        store.touch_session("some", "/p", "Other things", "default", "default").unwrap();
        store.append_event("some", &said("The tabs are fine.")).unwrap();
        store.touch_session("none", "/p", "Unrelated", "default", "default").unwrap();
        store.append_event("none", &user("nothing here")).unwrap();
        store.touch_session("asking", "/p", "Reorder tabs again", "default", "default").unwrap();
        store.touch_session("gone", "/p", "Reorder tabs, trashed", "default", "default").unwrap();
        store.set_trashed("gone", Some(1)).unwrap();
        store.touch_session("far", "/elsewhere", "Reorder tabs elsewhere", "default", "default").unwrap();

        // A word matches its other forms. A word in the title counts most; a
        // chat with every word comes before one with only some. The asking
        // chat, the trash and other projects stay out.
        assert_eq!(found(&store, "asking", "reorder tabs"), ["title", "body", "some"]);

        let (hits, more) = store.search_chats("/p", "asking", &words("reorder tabs")).unwrap();
        assert_eq!(more, 0);
        assert_eq!((hits[0].turns, hits[0].snippet.as_str(), hits[0].snippet_turn), (1, "", 0));
        assert_eq!((hits[1].turns, hits[1].snippet_turn), (2, 2));
        assert!(hits[1].snippet.contains("reordered the tabs"), "{}", hits[1].snippet);

        assert!(found(&store, "asking", "zebra").is_empty());
        assert!(store.search_chats("/p", "asking", &[]).unwrap().0.is_empty());
    }

    #[test]
    fn the_chat_about_a_thing_beats_a_long_chat_that_mentions_it_often() {
        let store = Store::in_memory();
        store.touch_session("long", "/p", "Backslash commands", "default", "default").unwrap();
        for _ in 0..40 {
            store.append_event("long", &said("The side panel has tabs. The panel's rules for a tab in the side panel.")).unwrap();
        }
        store.touch_session("right", "/p", "Reordering side panel tabs", "default", "default").unwrap();
        store.append_event("right", &user("let people drag them")).unwrap();
        // Both hold most of the words; the one named for them comes first.
        assert_eq!(found(&store, "x", "reordering tabs in the side panel rules"), ["right", "long"]);
    }

    #[test]
    fn a_search_hands_back_the_best_few_and_counts_the_rest() {
        let store = Store::in_memory();
        for n in 0..11 {
            store.touch_session(&format!("c{n}"), "/p", "A chat", "default", "default").unwrap();
            store.append_event(&format!("c{n}"), &user("about zebras")).unwrap();
        }
        let (hits, more) = store.search_chats("/p", "c0", &words("zebra")).unwrap();
        assert_eq!((hits.len(), more), (SEARCH_CHATS, 2));
    }

    #[test]
    fn only_what_the_user_and_the_agent_said_is_searchable() {
        let store = Store::in_memory();
        store.touch_session("s1", "/p", "A chat", "default", "default").unwrap();
        store.append_event("s1", &user("about apples")).unwrap();
        store
            .append_event(
                "s1",
                &AgentEvent::AssistantBlock {
                    message_id: "m".into(),
                    block: Block::Text { text: "subagent bananas".into() },
                    parent_tool_id: Some("t1".into()),
                },
            )
            .unwrap();
        store
            .append_event(
                "s1",
                &AgentEvent::ToolResult {
                    tool_id: "t1".into(),
                    content: serde_json::json!("cherries"),
                    is_error: false,
                    parent_tool_id: None,
                },
            )
            .unwrap();
        store
            .append_event("s1", &AgentEvent::UserMessage { text: "about dates".into(), id: Some("u2".into()), images: vec![] })
            .unwrap();
        assert_eq!(found(&store, "x", "apples"), ["s1"]);
        assert_eq!(found(&store, "x", "dates"), ["s1"]);
        assert!(found(&store, "x", "bananas").is_empty());
        assert!(found(&store, "x", "cherries").is_empty());

        // A message taken back leaves the search, and isn't counted as a turn.
        store.append_event("s1", &AgentEvent::MessageWithdrawn { id: "u2".into(), ok: true }).unwrap();
        assert!(found(&store, "x", "dates").is_empty());
        assert_eq!(store.turn_count("s1").unwrap(), 1);

        // An event for a chat that isn't there is not indexed either.
        store.append_event("nope", &user("about elderberries")).unwrap();
        assert!(found(&store, "x", "elderberries").is_empty());
    }

    #[test]
    fn the_search_index_is_built_from_history_saved_before_it() {
        let store = Store::in_memory();
        store.touch_session("s1", "/p", "Old chat", "default", "default").unwrap();
        store
            .append_event("s1", &AgentEvent::UserMessage { text: "about figs".into(), id: Some("u1".into()), images: vec![] })
            .unwrap();
        store.append_event("s1", &said("Figs it is.")).unwrap();
        store
            .append_event("s1", &AgentEvent::UserMessage { text: "about grapes".into(), id: Some("u2".into()), images: vec![] })
            .unwrap();
        store.append_event("s1", &AgentEvent::MessageWithdrawn { id: "u2".into(), ok: true }).unwrap();
        {
            // As an older build left it: history, and no index.
            let conn = store.conn.lock().unwrap();
            conn.execute_batch("DELETE FROM chat_text; DELETE FROM settings WHERE key = 'chat_index';").unwrap();
            build_chat_index(&conn).unwrap();
            // A second open leaves it alone.
            build_chat_index(&conn).unwrap();
            let rows: i64 = conn.query_row("SELECT count(*) FROM chat_text", [], |r| r.get(0)).unwrap();
            assert_eq!(rows, 3);
        }
        assert_eq!(found(&store, "x", "figs"), ["s1"]);
        assert_eq!(found(&store, "x", "old"), ["s1"]);
        assert!(found(&store, "x", "grapes").is_empty());
    }

    #[test]
    fn a_new_title_replaces_the_old_one_in_the_search() {
        let store = Store::in_memory();
        store.touch_session("s1", "/p", "about kiwis", "default", "default").unwrap();
        assert_eq!(found(&store, "x", "kiwis"), ["s1"]);
        assert!(store.set_generated_title("s1", "Lemon Notes").unwrap());
        assert!(found(&store, "x", "kiwis").is_empty());
        assert_eq!(found(&store, "x", "lemon"), ["s1"]);
        store.rename_session("s1", "Mango plans").unwrap();
        assert!(found(&store, "x", "lemon").is_empty());
        assert_eq!(found(&store, "x", "mango"), ["s1"]);

        // A deleted chat leaves nothing behind.
        store.append_event("s1", &user("about nectarines")).unwrap();
        store.delete_session("s1").unwrap();
        let rows: i64 = store.conn.lock().unwrap().query_row("SELECT count(*) FROM chat_text", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn a_chat_is_found_by_the_start_of_its_id() {
        let store = Store::in_memory();
        store.touch_session("abcd-1", "/p", "One", "default", "default").unwrap();
        store.touch_session("abcd-2", "/p", "Two", "default", "default").unwrap();
        store.touch_session("ef01-3", "/p", "Three", "default", "default").unwrap();
        store.touch_session("9999-4", "/elsewhere", "Four", "default", "default").unwrap();
        store.touch_session("7777-5", "/p", "Five", "default", "default").unwrap();
        store.set_trashed("7777-5", Some(1)).unwrap();

        assert_eq!(store.find_chat("/p", "ef01", "x").unwrap().title, "Three");
        assert_eq!(store.find_chat("/p", " abcd-2 ", "x").unwrap().title, "Two");
        assert!(store.find_chat("/p", "abcd", "x").unwrap_err().contains("Give more of the id"));
        // With the asking chat set aside, the start names one chat.
        assert_eq!(store.find_chat("/p", "abcd", "abcd-1").unwrap().title, "Two");
        assert_eq!(store.find_chat("/p", "ef01", "ef01-3").unwrap_err(), "That is this chat.");
        for missing in ["9999", "7777", "zzzz", "%"] {
            assert_eq!(store.find_chat("/p", missing, "x").unwrap_err(), "No chat with that id in this project.");
        }
        assert_eq!(store.find_chat("/p", " ", "x").unwrap_err(), "Give the chat's id.");

        let recent: Vec<String> = store.recent_chats("/p", "abcd-1", 2).unwrap().into_iter().map(|c| c.title).collect();
        assert_eq!(recent.len(), 2);
        assert!(!recent.contains(&"One".to_string()) && !recent.contains(&"Five".to_string()));
    }

    #[test]
    fn one_chat_can_be_searched_for_its_matching_turns() {
        let store = Store::in_memory();
        store.touch_session("s1", "/p", "Plums", "default", "default").unwrap();
        store.append_event("s1", &user("first, about plums")).unwrap();
        store.append_event("s1", &said("Plums noted.")).unwrap();
        store.append_event("s1", &user("second, about something else")).unwrap();
        store.append_event("s1", &user("third, plums again")).unwrap();
        let hits = store.search_in_chat("s1", &words("plum")).unwrap();
        let places: Vec<(usize, &str)> = hits.iter().map(|h| (h.turn, h.role.as_str())).collect();
        // Newest first, and the title is not a turn.
        assert_eq!(places, [(3, "user"), (1, "agent"), (1, "user")]);
        assert!(store.search_in_chat("s1", &words("zebra")).unwrap().is_empty());
    }
}
