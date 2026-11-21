//! Each chat's own folder: its notes (a markdown file the user writes in the
//! side panel) and the docs agents write for it (see `docs_rule`). It is
//! under the chat's project in the app's home folder (`home.rs`), never in
//! the repo. The agent is told where it is (see `SpawnOptions::notes`). The
//! folder goes to the trash with its chat and is deleted with it for good; a
//! doc saved to the project (`docs::save_to_project`) outlives the chat.
//!
//! A chat has no project until its first message makes it, so notes typed
//! before then wait in `unsent/<chat>` and move in when the chat is made
//! (`adopt`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::home::Home;
use crate::images::check_session;
use crate::store::Store;

/// Largest notes file the window may save.
const MAX_BYTES: usize = 1024 * 1024;
const FILE: &str = "notes.md";

/// A chat's project (its main folder), once the chat exists.
pub type ProjectOf = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub struct Notes {
    home: Home,
    project_of: ProjectOf,
}

impl Notes {
    pub fn new(home: Home, project_of: ProjectOf) -> Self {
        Self { home, project_of }
    }

    /// Chat folders for the chats in a store.
    pub fn for_store(home: Home, store: Arc<Store>) -> Self {
        Self::new(
            home,
            Arc::new(move |chat| store.session(chat).ok().flatten().map(|row| row.project_path)),
        )
    }

    /// The chat's own folder, which the agent is given access to.
    pub fn folder(&self, chat: &str) -> Result<PathBuf, String> {
        check_session(chat)?;
        Ok(match (self.project_of)(chat) {
            Some(project) => self.home.chat(&project, chat),
            None => self.home.unsent(chat),
        })
    }

    /// The docs folder of the chat's project, shared by all its chats. None
    /// for a chat that doesn't exist yet.
    pub fn project_docs(&self, chat: &str) -> Option<PathBuf> {
        (self.project_of)(chat).map(|project| self.home.project_docs(&project))
    }

    /// The memory folder of the chat's project, made and filled as
    /// `Home::ensure_memory` does. None for a chat that doesn't exist yet.
    pub fn project_memory(&self, chat: &str) -> Option<PathBuf> {
        (self.project_of)(chat).and_then(|project| self.home.ensure_memory(&project))
    }

    /// The user's working preferences, as `Home::preferences` gives them.
    pub fn preferences(&self) -> Option<crate::home::Preferences> {
        self.home.preferences()
    }

    /// The chat's notes file. It may not exist yet.
    pub fn path(&self, chat: &str) -> Result<PathBuf, String> {
        Ok(self.folder(chat)?.join(FILE))
    }

    /// Makes the chat's folder, so the agent can be pointed at it before the
    /// user has written anything.
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

    /// Brings in what was written before the chat existed. Called once the
    /// chat's first message has made it, before its agent starts, so the
    /// agent only ever sees the folder under the project.
    pub fn adopt(&self, chat: &str) {
        let (Ok(to), from) = (self.folder(chat), self.home.unsent(chat)) else {
            return;
        };
        if to != from && from.is_dir() {
            move_folder(&from, &to);
        }
    }

    /// Removes a chat's folder, when the chat is deleted for good.
    pub fn delete_session(&self, chat: &str, project_path: &str) {
        if check_session(chat).is_ok() {
            let _ = std::fs::remove_dir_all(self.home.chat(project_path, chat));
            let _ = std::fs::remove_dir_all(self.home.unsent(chat));
        }
    }

    /// Removes the folders of chats that don't exist: deleted ones, and new
    /// chats that were never sent (drafts aren't saved). A project's docs are
    /// never touched.
    pub fn sweep(&self, known: &HashSet<String>) {
        for (chat, folder) in self.home.chat_folders() {
            if !known.contains(&chat) {
                let _ = std::fs::remove_dir_all(folder);
            }
        }
    }

    /// Chat folders used to be `<data folder>/notes/<chat>`. Moves each
    /// existing chat's into its project, and returns the ones moved as (chat,
    /// old folder, new folder) so their agents can be told. A folder that
    /// can't be moved stays, to be tried again at the next start. With
    /// nothing at `old_root` this does nothing, so it runs at every start.
    pub fn migrate(&self, old_root: &Path) -> Vec<(String, PathBuf, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(old_root) else {
            return Vec::new();
        };
        let mut moved = Vec::new();
        let mut left = false;
        for entry in entries.flatten() {
            let (from, chat) = (entry.path(), entry.file_name().to_string_lossy().into_owned());
            // A folder of no chat (deleted, or never sent) would be swept away.
            let known = from.is_dir() && check_session(&chat).is_ok() && (self.project_of)(&chat).is_some();
            let Some(to) = self.folder(&chat).ok().filter(|_| known) else {
                continue;
            };
            if move_folder(&from, &to) {
                moved.push((chat, from, to));
            } else {
                eprintln!("tilde: couldn't move {} to {}", from.display(), to.display());
                left = true;
            }
        }
        if !left {
            let _ = std::fs::remove_dir_all(old_root);
        }
        moved
    }

    /// A path saved in a chat's history, as it is now: one into the chat's
    /// old folder (`…/notes/<chat>/…`) is the same file in its folder today.
    pub fn current(&self, chat: &str, path: &str) -> String {
        let Ok(folder) = self.folder(chat) else {
            return path.to_string();
        };
        let old = [format!("/notes/{chat}/"), format!("\\notes\\{chat}\\")];
        match old.iter().find_map(|mark| path.split_once(mark.as_str())) {
            Some((_, rest)) if !Path::new(path).starts_with(&folder) => folder.join(rest).to_string_lossy().into_owned(),
            _ => path.to_string(),
        }
    }
}

/// Moves a folder to a place that may already hold some of it: renamed whole
/// when the place is free, else entry by entry, never over a file that is
/// there. True when nothing is left behind.
fn move_folder(from: &Path, to: &Path) -> bool {
    if let Some(parent) = to.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if !to.exists() {
        return std::fs::rename(from, to).is_ok();
    }
    let Ok(entries) = std::fs::read_dir(from) else {
        return false;
    };
    let mut all = true;
    for entry in entries.flatten() {
        let target = to.join(entry.file_name());
        all &= !target.exists() && std::fs::rename(entry.path(), target).is_ok();
    }
    all && std::fs::remove_dir(from).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Notes in a scratch home, and the chats that exist (chat → project).
    struct Rig {
        notes: Notes,
        root: PathBuf,
        chats: Arc<Mutex<HashMap<String, String>>>,
    }

    impl Rig {
        fn made(&self, chat: &str, project: &str) {
            self.chats.lock().unwrap().insert(chat.into(), project.into());
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn rig() -> Rig {
        let root = std::env::temp_dir().join(format!("tilde-notes-test-{}", uuid::Uuid::new_v4()));
        let chats: Arc<Mutex<HashMap<String, String>>> = Arc::default();
        let project_of: ProjectOf = {
            let chats = chats.clone();
            Arc::new(move |chat| chats.lock().unwrap().get(chat).cloned())
        };
        Rig { notes: Notes::new(Home::new(root.join("home")), project_of), root, chats }
    }

    #[test]
    fn notes_are_saved_and_read_back() {
        let rig = rig();
        rig.made("chat-1", "/w/app");
        let notes = &rig.notes;
        assert_eq!(notes.read("chat-1").unwrap(), "");
        notes.write("chat-1", "# Ideas\n\n- one").unwrap();
        assert_eq!(notes.read("chat-1").unwrap(), "# Ideas\n\n- one");
        notes.write("chat-1", "changed").unwrap();
        assert_eq!(notes.read("chat-1").unwrap(), "changed");
        assert!(!notes.folder("chat-1").unwrap().join("notes.md.tmp").exists());
        assert_eq!(notes.path("chat-1").unwrap(), rig.root.join("home/projects/-w-app/chats/chat-1/notes.md"));
    }

    #[test]
    fn a_bad_chat_id_is_refused() {
        let rig = rig();
        assert!(rig.notes.write("../escape", "x").is_err());
        assert!(rig.notes.read("a/b").is_err());
        assert!(rig.notes.path("").is_err());
    }

    #[test]
    fn notes_typed_before_the_first_message_move_in_when_the_chat_is_made() {
        let rig = rig();
        let notes = &rig.notes;
        notes.write("new", "typed early").unwrap();
        assert_eq!(notes.path("new").unwrap(), rig.root.join("home/unsent/new/notes.md"));
        rig.made("new", "/w/app");
        // Not there until it is brought in.
        assert_eq!(notes.read("new").unwrap(), "");
        notes.adopt("new");
        assert_eq!(notes.read("new").unwrap(), "typed early");
        assert!(!rig.root.join("home/unsent/new").exists());
        // Nothing waiting: nothing happens.
        notes.adopt("new");
        notes.adopt("other");
        assert_eq!(notes.read("new").unwrap(), "typed early");
    }

    #[test]
    fn deleted_and_unknown_chats_lose_their_folders_but_a_projects_docs_stay() {
        let rig = rig();
        let notes = &rig.notes;
        for chat in ["keep", "gone", "trashed"] {
            rig.made(chat, "/w/app");
            notes.write(chat, chat).unwrap();
        }
        notes.write("draft", "never sent").unwrap();
        let docs = rig.root.join("home/projects/-w-app/docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("kept.md"), "x").unwrap();

        notes.sweep(&HashSet::from(["keep".to_string(), "trashed".to_string()]));
        assert_eq!(notes.read("keep").unwrap(), "keep");
        assert!(!notes.folder("gone").unwrap().exists());
        assert!(!rig.root.join("home/unsent/draft").exists());
        assert!(docs.join("kept.md").exists());

        notes.delete_session("trashed", "/w/app");
        assert!(!notes.folder("trashed").unwrap().exists());
        assert!(docs.join("kept.md").exists());
    }

    #[test]
    fn old_chat_folders_move_into_their_projects_once() {
        let rig = rig();
        let notes = &rig.notes;
        let old = rig.root.join("data/notes");
        for chat in ["a", "b", "nobody"] {
            std::fs::create_dir_all(old.join(chat)).unwrap();
            std::fs::write(old.join(chat).join("notes.md"), chat).unwrap();
        }
        std::fs::write(old.join("a/report.md"), "a doc").unwrap();
        rig.made("a", "/w/app");
        rig.made("b", "/w/other");

        let mut moved = notes.migrate(&old);
        moved.sort();
        assert_eq!(
            moved,
            [
                ("a".to_string(), old.join("a"), rig.root.join("home/projects/-w-app/chats/a")),
                ("b".to_string(), old.join("b"), rig.root.join("home/projects/-w-other/chats/b")),
            ]
        );
        assert_eq!(notes.read("a").unwrap(), "a");
        assert_eq!(std::fs::read_to_string(notes.folder("a").unwrap().join("report.md")).unwrap(), "a doc");
        assert_eq!(notes.read("b").unwrap(), "b");
        // The old place is gone, the folder of no chat with it.
        assert!(!old.exists());
        // A second start finds nothing to do.
        assert!(notes.migrate(&old).is_empty());
    }

    #[test]
    fn a_path_into_a_chats_old_folder_is_the_same_file_in_its_new_one() {
        let rig = rig();
        rig.made("c1", "/w/app");
        let notes = &rig.notes;
        let folder = notes.folder("c1").unwrap();
        let now = folder.join("sub/report.md").to_string_lossy().into_owned();
        assert_eq!(notes.current("c1", "/data/notes/c1/sub/report.md"), now);
        // Its own new paths, other chats' paths and other files are left alone.
        assert_eq!(notes.current("c1", &now), now);
        assert_eq!(notes.current("c1", "/data/notes/c2/report.md"), "/data/notes/c2/report.md");
        assert_eq!(notes.current("c1", "/w/app/PLAN.md"), "/w/app/PLAN.md");
    }

    #[test]
    fn very_long_notes_are_refused() {
        let rig = rig();
        assert!(rig.notes.write("chat-1", &"x".repeat(MAX_BYTES + 1)).is_err());
    }
}
