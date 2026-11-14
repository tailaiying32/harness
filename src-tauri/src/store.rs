//! The app's saved state: projects, chats, each chat's events, and settings.
//! One SQLite file. A chat's history comes only from here, never from the
//! agent's own transcript files.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::events::AgentEvent;
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
    /// The branch's pull request, as last seen.
    pub pr: Option<PrInfo>,
    /// When the chat was moved to the trash (ms); None when it isn't there.
    pub trashed_at: Option<i64>,
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
    pub pr: Option<PrInfo>,
    /// The main folder's branch and commit when the chat began.
    pub start_branch: Option<String>,
    pub start_commit: Option<String>,
    pub trashed_at: Option<i64>,
}

/// A project folder and its chats, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectRow {
    pub id: String,
    pub path: String,
    pub sessions: Vec<SessionRow>,
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

/// The first line of a message, cut to a title's length.
fn title_from(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let mut title: String = line.chars().take(TITLE_CHARS).collect();
    if line.chars().count() > TITLE_CHARS {
        title.push('…');
    }
    title
}

const SESSION_COLUMNS: &str = "s.id, p.path, s.title, s.mode, s.model, s.agent_session_id, \
     s.last_used_at, s.wt, s.wt_status, s.pr, s.trashed_at";

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
        pr: json_col(row, 9)?,
        trashed_at: row.get(10)?,
    })
}

const GIT_COLUMNS: &str = "s.id, p.path, s.title, s.last_used_at, s.worktree_choice, s.wt_status, \
     s.wt, s.wt_put_away_at, s.pr, s.start_branch, s.start_commit, s.trashed_at";

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
        pr: json_col(row, 8)?,
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
    // The branch's pull request as JSON (`worktree::PrInfo`).
    ("pr", "TEXT"),
    ("trashed_at", "INTEGER"),
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
        }
    }
    Ok(())
}

impl Store {
    /// Opens (or creates) the store in `dir`.
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("Can't create {}: {e}", dir.display()))?;
        Self::with(Connection::open(dir.join("harness.db")).map_err(err)?)
    }

    #[cfg(test)]
    fn in_memory() -> Self {
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
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(err)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
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
        tx.execute(
            "INSERT INTO sessions (id, project_id, title, mode, model, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET mode = ?4, model = ?5, last_used_at = ?6",
            params![session_id, project_id, title_from(first_message), mode, model, now],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(created)
    }

    /// Replaces the title cut from the first message with a written one. A chat
    /// the user renamed keeps its name. Returns whether the title changed.
    pub fn set_generated_title(&self, session_id: &str, title: &str) -> Result<bool, String> {
        let title: String = title.trim().chars().take(TITLE_CHARS).collect();
        if title.is_empty() {
            return Ok(false);
        }
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET title = ?2, title_source = 'generated'
                 WHERE id = ?1 AND title_source = 'auto'",
                params![session_id, title],
            )
            .map(|changed| changed > 0)
            .map_err(err)
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

    /// Every chat the cleanup sweep may act on: with a worktree, or in the trash.
    pub fn sweep_list(&self) -> Result<Vec<ChatGit>, String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .prepare(&format!(
                "SELECT {GIT_COLUMNS} FROM sessions s JOIN projects p ON p.id = s.project_id
                 WHERE s.wt_status IS NOT NULL OR s.trashed_at IS NOT NULL"
            ))
            .map_err(err)?
            .query_map([], git_from)
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err);
        rows
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
    pub fn set_worktree(&self, session_id: &str, worktree: &Worktree, status: &str) -> Result<(), String> {
        let json = serde_json::to_string(worktree).map_err(|e| e.to_string())?;
        let put_away_at = (status == "put_away").then(now_ms);
        // A chat starting a new worktree (none saved) drops the last one's pull
        // request: new branch, new pull request. Put away and back keeps it.
        self.exec(
            "UPDATE sessions SET wt = ?2, wt_status = ?3, wt_put_away_at = ?4,
                 pr = CASE WHEN wt IS NULL THEN NULL ELSE pr END WHERE id = ?1",
            params![session_id, json, status, put_away_at],
        )
    }

    /// The chat no longer has a worktree, and will be asked again at its next edit.
    /// Its pull request stays, so a finished chat keeps its mark (purple once
    /// merged). It is no longer checked on GitHub after this.
    pub fn clear_worktree(&self, session_id: &str) -> Result<(), String> {
        self.exec(
            "UPDATE sessions SET wt = NULL, wt_status = NULL, wt_put_away_at = NULL,
                 worktree_choice = NULL WHERE id = ?1",
            params![session_id],
        )
    }

    pub fn set_pr(&self, session_id: &str, pr: Option<&PrInfo>) -> Result<(), String> {
        let json = pr.map(|p| serde_json::to_string(p).unwrap_or_default());
        self.exec("UPDATE sessions SET pr = ?2 WHERE id = ?1", params![session_id, json])
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
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO events (session_id, payload, created_at)
                 SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM sessions WHERE id = ?1)",
                params![session_id, payload, now_ms()],
            )
            .map(|_| ())
            .map_err(err)
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

    /// Removes a chat and its history. A project left with no chats goes too.
    pub fn delete_session(&self, session_id: &str) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(err)?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
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
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET title = ?2, title_source = 'user' WHERE id = ?1",
                params![session_id, title],
            )
            .map(|_| ())
            .map_err(err)
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
        AgentEvent::UserMessage { text: text.into(), id: None }
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

        let pr = PrInfo { number: 4, state: "open".into(), url: "u".into(), head: "h".into() };
        store.set_pr("a", Some(&pr)).unwrap();
        store.set_worktree("a", &worktree(), "put_away").unwrap();
        let git = store.chat_git("a").unwrap().unwrap();
        assert_eq!(git.status.as_deref(), Some("put_away"));
        assert!(git.put_away_at.is_some());
        assert_eq!(git.pr, Some(pr.clone()));
        assert_eq!(git.worktree, Some(worktree()));
        assert_eq!((git.start_branch.as_deref(), git.start_commit.as_deref()), (Some("main"), Some("c1")));
        assert_eq!(store.sweep_list().unwrap().len(), 1);

        store.clear_worktree("a").unwrap();
        let git = store.chat_git("a").unwrap().unwrap();
        assert_eq!((git.choice, git.status, git.worktree), (None, None, None));
        assert!(store.session("a").unwrap().unwrap().work_path.is_none());
        assert!(store.sweep_list().unwrap().is_empty());
        // The pull request outlives the worktree, so the sidebar keeps its mark...
        assert_eq!(git.pr, Some(pr.clone()));
        assert_eq!(store.session("a").unwrap().unwrap().pr, Some(pr));
        // ...until the chat starts a new worktree, which gets its own.
        store.set_worktree("a", &worktree(), "live").unwrap();
        assert_eq!(store.chat_git("a").unwrap().unwrap().pr, None);
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
    fn settings_round_trip() {
        let store = Store::in_memory();
        assert_eq!(store.setting("idle_seconds"), None);
        store.set_setting("idle_seconds", "120").unwrap();
        store.set_setting("idle_seconds", "300").unwrap();
        assert_eq!(store.setting("idle_seconds").as_deref(), Some("300"));
    }
}
