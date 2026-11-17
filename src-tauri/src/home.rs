//! Where the app keeps what the user might read: one folder in their home
//! directory, `~/.harness`, split by project. A project's docs are shared by
//! all its chats and worktrees, and each chat's notes and own docs sit under
//! its project. Nothing here is inside a repo, so git never sees it and every
//! worktree of a project sees the same files.
//!
//! ```text
//! ~/.harness/
//!   preferences.md                    the user's working preferences, for every agent
//!   projects/<key>/docs/              the project's docs
//!   projects/<key>/memory/            the project's memory, for every agent
//!   projects/<key>/chats/<chat id>/   that chat's notes and its own docs
//!   unsent/<chat id>/                 notes typed before a chat's first message
//! ```
//!
//! `<key>` is the project's main folder path with every character that isn't
//! a letter or digit turned into `-`, the way Claude Code names its own
//! project folders. The database, images, skills and worktrees are not for
//! reading and stay in the app's data folder.

use std::path::{Path, PathBuf};

/// The user's home folder.
pub fn user_home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
}

/// The most of the preferences file an agent is given, in characters.
const PREFERENCES_CHARS: usize = 20_000;

/// The user's working preferences: standing instructions for every agent,
/// whatever the project. `path` is the file they are kept in.
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Home {
    root: PathBuf,
    /// Claude Code's own project folders (`~/.claude/projects`), which hold
    /// the memory it kept before the app gave it a place here. None in tests.
    claude_projects: Option<PathBuf>,
}

impl Home {
    pub fn new(root: PathBuf) -> Self {
        Self { root, claude_projects: None }
    }

    /// With Claude Code's own project folders at `dir`.
    pub fn with_claude_projects(mut self, dir: PathBuf) -> Self {
        self.claude_projects = Some(dir);
        self
    }

    /// The folder to use: `HARNESS_HOME` when set; else, when the data folder
    /// was set by `HARNESS_DATA_DIR`, a folder inside it, so a test or a test
    /// copy of the app never touches the real one; else `~/.harness`.
    pub fn resolve(data_dir: &Path) -> Self {
        let own_data = std::env::var_os("HARNESS_DATA_DIR").is_some();
        let home = match (std::env::var_os("HARNESS_HOME"), user_home().filter(|_| !own_data)) {
            (Some(root), _) => Self::new(PathBuf::from(root)),
            (None, Some(home)) => Self::new(home.join(".harness")),
            (None, None) => Self::new(data_dir.join("home")),
        };
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| user_home().map(|home| home.join(".claude")));
        match claude {
            Some(claude) => home.with_claude_projects(claude.join("projects")),
            None => home,
        }
    }

    /// A project's folder name: its main folder's path with every character
    /// that isn't a letter or digit turned into `-`.
    pub fn key(project_path: &str) -> String {
        let path = project_path.trim().trim_end_matches(['/', '\\']);
        path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
    }

    /// The file the user's working preferences are kept in.
    pub fn preferences_file(&self) -> PathBuf {
        self.root.join("preferences.md")
    }

    /// The user's working preferences as they are now; None when the file is
    /// missing or blank. Read when a chat's process starts.
    pub fn preferences(&self) -> Option<Preferences> {
        let path = self.preferences_file();
        let text = std::fs::read_to_string(&path).ok()?;
        let text: String = text.trim().chars().take(PREFERENCES_CHARS).collect();
        (!text.is_empty()).then_some(Preferences { path, text })
    }

    /// Everything kept for a project.
    pub fn project(&self, project_path: &str) -> PathBuf {
        self.root.join("projects").join(Self::key(project_path))
    }

    /// The project's docs, shared by all its chats.
    pub fn project_docs(&self, project_path: &str) -> PathBuf {
        self.project(project_path).join("docs")
    }

    /// The project's memory: notes every agent that works on it keeps and
    /// reads, one markdown file each, with `MEMORY.md` as the index.
    pub fn project_memory(&self, project_path: &str) -> PathBuf {
        self.project(project_path).join("memory")
    }

    /// Makes the project's memory folder and, the first time, fills it with
    /// what Claude Code remembered for the project in its own folder
    /// (`~/.claude/projects/<key>/memory`, the same key). Copied, never
    /// moved, so Claude Code run outside the app keeps its own. Skipped once
    /// the folder here has an index, so it is safe at every start of a chat.
    pub fn ensure_memory(&self, project_path: &str) -> Option<PathBuf> {
        let memory = self.project_memory(project_path);
        std::fs::create_dir_all(&memory).ok()?;
        if !memory.join("MEMORY.md").exists() {
            let from = self.claude_projects.as_ref().map(|dir| dir.join(Self::key(project_path)).join("memory"));
            if let Some(from) = from.filter(|from| from.join("MEMORY.md").is_file()) {
                if let Err(e) = copy_folder(&from, &memory) {
                    eprintln!("harness: couldn't copy the memory in {}: {e}", from.display());
                }
            }
        }
        Some(memory)
    }

    /// A chat's own folder: its notes and the docs written for it.
    pub fn chat(&self, project_path: &str, chat: &str) -> PathBuf {
        self.project(project_path).join("chats").join(chat)
    }

    /// Where a new chat's notes wait until its first message makes the chat.
    pub fn unsent(&self, chat: &str) -> PathBuf {
        self.root.join("unsent").join(chat)
    }

    /// Every chat folder there is, as (chat id, folder): those under a
    /// project and those of chats not sent yet.
    pub fn chat_folders(&self) -> Vec<(String, PathBuf)> {
        let folders_in = |dir: PathBuf| -> Vec<PathBuf> {
            std::fs::read_dir(dir)
                .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
                .unwrap_or_default()
        };
        let mut found = folders_in(self.root.join("unsent"));
        for project in folders_in(self.root.join("projects")) {
            found.extend(folders_in(project.join("chats")));
        }
        found
            .into_iter()
            .filter_map(|path| Some((path.file_name()?.to_string_lossy().into_owned(), path)))
            .collect()
    }
}

/// Copies a folder's files and folders into another, which exists. A file
/// already there is left as it is.
fn copy_folder(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_folder(&entry.path(), &target)?;
        } else if !target.exists() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_is_named_from_its_path() {
        assert_eq!(Home::key("/Users/me/tailai/harness"), "-Users-me-tailai-harness");
        assert_eq!(Home::key(" /Users/me/my app_v2.0/ "), "-Users-me-my-app-v2-0");
        assert_eq!(Home::key(r"C:\code\app"), "C--code-app");
    }

    #[test]
    fn everything_sits_under_the_project() {
        let home = Home::new(PathBuf::from("/h"));
        assert_eq!(home.project_docs("/w/app"), PathBuf::from("/h/projects/-w-app/docs"));
        assert_eq!(home.chat("/w/app", "c1"), PathBuf::from("/h/projects/-w-app/chats/c1"));
        assert_eq!(home.unsent("c1"), PathBuf::from("/h/unsent/c1"));
        assert_eq!(home.project_memory("/w/app"), PathBuf::from("/h/projects/-w-app/memory"));
    }

    #[test]
    fn preferences_are_the_files_text_or_nothing() {
        let dir = std::env::temp_dir().join(format!("harness-home-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let home = Home::new(dir.clone());
        assert_eq!(home.preferences(), None);
        std::fs::write(home.preferences_file(), " \n\n").unwrap();
        assert_eq!(home.preferences(), None, "a blank file is no preferences");
        std::fs::write(home.preferences_file(), "\n# Working preferences\n- Be brief.\n").unwrap();
        let found = home.preferences().unwrap();
        assert_eq!((found.path, found.text.as_str()), (dir.join("preferences.md"), "# Working preferences\n- Be brief."));
        std::fs::write(home.preferences_file(), "x".repeat(PREFERENCES_CHARS + 50)).unwrap();
        assert_eq!(home.preferences().unwrap().text.chars().count(), PREFERENCES_CHARS);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_projects_memory_starts_as_a_copy_of_what_claude_code_kept() {
        let dir = std::env::temp_dir().join(format!("harness-home-test-{}", uuid::Uuid::new_v4()));
        let theirs = dir.join("claude/projects/-w-app/memory");
        std::fs::create_dir_all(theirs.join("more")).unwrap();
        std::fs::write(theirs.join("MEMORY.md"), "- [A](a.md) — a fact").unwrap();
        std::fs::write(theirs.join("a.md"), "the fact").unwrap();
        std::fs::write(theirs.join("more/b.md"), "deeper").unwrap();
        let home = Home::new(dir.join("home")).with_claude_projects(dir.join("claude/projects"));

        let ours = home.ensure_memory("/w/app").unwrap();
        assert_eq!(ours, dir.join("home/projects/-w-app/memory"));
        assert_eq!(std::fs::read_to_string(ours.join("a.md")).unwrap(), "the fact");
        assert_eq!(std::fs::read_to_string(ours.join("more/b.md")).unwrap(), "deeper");
        // Copied, not moved.
        assert!(theirs.join("a.md").is_file());

        // Once there is an index here, theirs is not looked at again.
        std::fs::write(ours.join("MEMORY.md"), "- [Ours](ours.md) — changed here").unwrap();
        std::fs::write(theirs.join("later.md"), "written outside the app").unwrap();
        home.ensure_memory("/w/app").unwrap();
        assert_eq!(std::fs::read_to_string(ours.join("MEMORY.md")).unwrap(), "- [Ours](ours.md) — changed here");
        assert!(!ours.join("later.md").exists());

        // A project Claude Code kept nothing for gets an empty folder.
        let empty = home.ensure_memory("/w/other").unwrap();
        assert!(empty.is_dir() && std::fs::read_dir(&empty).unwrap().next().is_none());
        // And with nowhere to look, the same.
        assert!(Home::new(dir.join("bare")).ensure_memory("/w/app").unwrap().is_dir());
        let _ = std::fs::remove_dir_all(dir);
    }
}
