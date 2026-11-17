//! Where the app keeps what the user might read: one folder in their home
//! directory, `~/.harness`, split by project. A project's docs are shared by
//! all its chats and worktrees, and each chat's notes and own docs sit under
//! its project. Nothing here is inside a repo, so git never sees it and every
//! worktree of a project sees the same files.
//!
//! ```text
//! ~/.harness/
//!   projects/<key>/docs/              the project's docs
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

#[derive(Clone, Debug)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder to use: `HARNESS_HOME` when set; else, when the data folder
    /// was set by `HARNESS_DATA_DIR`, a folder inside it, so a test or a test
    /// copy of the app never touches the real one; else `~/.harness`.
    pub fn resolve(data_dir: &Path) -> Self {
        if let Some(root) = std::env::var_os("HARNESS_HOME") {
            return Self::new(PathBuf::from(root));
        }
        let own_data = std::env::var_os("HARNESS_DATA_DIR").is_some();
        match user_home().filter(|_| !own_data) {
            Some(home) => Self::new(home.join(".harness")),
            None => Self::new(data_dir.join("home")),
        }
    }

    /// A project's folder name: its main folder's path with every character
    /// that isn't a letter or digit turned into `-`.
    pub fn key(project_path: &str) -> String {
        let path = project_path.trim().trim_end_matches(['/', '\\']);
        path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
    }

    /// Everything kept for a project.
    pub fn project(&self, project_path: &str) -> PathBuf {
        self.root.join("projects").join(Self::key(project_path))
    }

    /// The project's docs, shared by all its chats.
    pub fn project_docs(&self, project_path: &str) -> PathBuf {
        self.project(project_path).join("docs")
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
    }
}
