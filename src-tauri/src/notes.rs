//! Each chat's notes: a markdown file the user writes in the side panel, kept
//! next to the chat's images in the app's data folder (never in the repo). The
//! agent is told where it is and may read it (see `SpawnOptions::notes`); it
//! goes to the trash with its chat and is deleted with it for good. The same
//! folder holds the docs agents write for the chat (see `docs_rule`), which
//! go with it too unless saved to the project (`docs::save_to_project`).

use std::collections::HashSet;
use std::path::PathBuf;

use crate::images::check_session;

/// Largest notes file the window may save.
const MAX_BYTES: usize = 1024 * 1024;
const FILE: &str = "notes.md";

pub struct Notes {
    root: PathBuf,
}

impl Notes {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The chat's notes folder, which the agent is given read access to.
    pub fn folder(&self, chat: &str) -> Result<PathBuf, String> {
        check_session(chat)?;
        Ok(self.root.join(chat))
    }

    /// The chat's notes file. It may not exist yet.
    pub fn path(&self, chat: &str) -> Result<PathBuf, String> {
        Ok(self.folder(chat)?.join(FILE))
    }

    /// Makes the chat's notes folder, so the agent can be pointed at it before
    /// the user has written anything.
    pub fn ensure_folder(&self, chat: &str) -> Result<PathBuf, String> {
        let folder = self.folder(chat)?;
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        Ok(folder)
    }

    /// The chat's notes; empty if there are none yet.
    pub fn read(&self, chat: &str) -> Result<String, String> {
        match std::fs::read_to_string(self.path(chat)?) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Saves the chat's notes. Written to a side file first and moved into
    /// place, so a crash mid-save never leaves half a file.
    pub fn write(&self, chat: &str, text: &str) -> Result<(), String> {
        if text.len() > MAX_BYTES {
            return Err("These notes are too long to save (over 1 MB).".to_string());
        }
        let folder = self.ensure_folder(chat)?;
        let tmp = folder.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, folder.join(FILE)).map_err(|e| e.to_string())
    }

    /// Removes a chat's notes, when the chat is deleted for good.
    pub fn delete_session(&self, chat: &str) {
        if let Ok(folder) = self.folder(chat) {
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// Removes the notes of chats that don't exist: deleted ones, and new
    /// chats that were never sent (drafts aren't saved).
    pub fn sweep(&self, known: &HashSet<String>) {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() && !known.contains(&name) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes() -> (Notes, PathBuf) {
        let root = std::env::temp_dir().join(format!("harness-notes-test-{}", uuid::Uuid::new_v4()));
        (Notes::new(root.clone()), root)
    }

    #[test]
    fn notes_are_saved_and_read_back() {
        let (notes, root) = notes();
        assert_eq!(notes.read("chat-1").unwrap(), "");
        notes.write("chat-1", "# Ideas\n\n- one").unwrap();
        assert_eq!(notes.read("chat-1").unwrap(), "# Ideas\n\n- one");
        notes.write("chat-1", "changed").unwrap();
        assert_eq!(notes.read("chat-1").unwrap(), "changed");
        assert!(!notes.folder("chat-1").unwrap().join("notes.md.tmp").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_bad_chat_id_is_refused() {
        let (notes, _) = notes();
        assert!(notes.write("../escape", "x").is_err());
        assert!(notes.read("a/b").is_err());
        assert!(notes.path("").is_err());
    }

    #[test]
    fn deleted_and_unknown_chats_lose_their_notes() {
        let (notes, root) = notes();
        notes.write("keep", "a").unwrap();
        notes.write("gone", "b").unwrap();
        notes.write("trashed", "c").unwrap();
        notes.sweep(&HashSet::from(["keep".to_string(), "trashed".to_string()]));
        assert_eq!(notes.read("keep").unwrap(), "a");
        assert_eq!(notes.read("gone").unwrap(), "");
        notes.delete_session("trashed");
        assert!(!notes.folder("trashed").unwrap().exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn very_long_notes_are_refused() {
        let (notes, _) = notes();
        assert!(notes.write("chat-1", &"x".repeat(MAX_BYTES + 1)).is_err());
    }
}
