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
     s.last_used_at, s.wt, s.wt_status, s.prs, s.trashed_at, s.effort, s.agent";

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
    Ok(())
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
             );",
        )
        .map_err(err)?;
        migrate(&conn)?;
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
    fn settings_round_trip() {
        let store = Store::in_memory();
        assert_eq!(store.setting("idle_seconds"), None);
        store.set_setting("idle_seconds", "120").unwrap();
        store.set_setting("idle_seconds", "300").unwrap();
        assert_eq!(store.setting("idle_seconds").as_deref(), Some("300"));
    }
}
