//! The app's saved state: projects, chats, each chat's events, and settings.
//! One SQLite file. A chat's history comes only from here, never from the
//! agent's own transcript files.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::events::AgentEvent;

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
}

/// A project folder and its chats, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectRow {
    pub id: String,
    pub path: String,
    pub sessions: Vec<SessionRow>,
}

fn now_ms() -> i64 {
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

const SESSION_COLUMNS: &str =
    "s.id, p.path, s.title, s.mode, s.model, s.agent_session_id, s.last_used_at";

fn session_from(row: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        project_path: row.get(1)?,
        title: row.get(2)?,
        mode: row.get(3)?,
        model: row.get(4)?,
        agent_session_id: row.get(5)?,
        last_used_at: row.get(6)?,
    })
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
                 WHERE s.project_id = ?1 ORDER BY s.last_used_at DESC"
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
    pub fn touch_session(
        &self,
        session_id: &str,
        project_path: &str,
        first_message: &str,
        mode: &str,
        model: &str,
    ) -> Result<(), String> {
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
        // A chat stays in the folder it started in; later sends don't move it.
        tx.execute(
            "INSERT INTO sessions (id, project_id, title, mode, model, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET mode = ?4, model = ?5, last_used_at = ?6",
            params![session_id, project_id, title_from(first_message), mode, model, now],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
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
            .execute("UPDATE sessions SET title = ?2 WHERE id = ?1", params![session_id, title])
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
    fn settings_round_trip() {
        let store = Store::in_memory();
        assert_eq!(store.setting("idle_seconds"), None);
        store.set_setting("idle_seconds", "120").unwrap();
        store.set_setting("idle_seconds", "300").unwrap();
        assert_eq!(store.setting("idle_seconds").as_deref(), Some("300"));
    }
}
