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

    /// The preferences file exactly as it is, for the user to edit in
    /// Settings; "" when there is none yet.
    pub fn read_preferences(&self) -> Result<String, String> {
        match std::fs::read_to_string(self.preferences_file()) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Saves the user's edit to the preferences file, making it (and the
    /// home folder) when there is none. Written to a side file and moved
    /// into place, so a crash never leaves half a file; a link is followed
    /// so it stays a link. Chats started after this read the new text.
    pub fn save_preferences(&self, text: &str) -> Result<(), String> {
        let path = self.preferences_file();
        let target = std::fs::canonicalize(&path).unwrap_or(path);
        let folder = target.parent().ok_or("The preferences file has no folder.")?;
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let tmp = folder.join(".preferences.md.harness-save");
        let saved = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &target));
        if saved.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        saved.map_err(|e| e.to_string())
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

    /// A project's folder moved or was renamed: what is kept for it here
    /// goes to its new name. When that name already holds a project (the
    /// user had started chats in the new folder), the two are merged: chat
    /// folders move across, docs move across with `-2`, `-3`... on a name
    /// clash, and a memory that can't simply move in is kept whole as
    /// `memory/from-<old key>/` with one line in the index pointing at it.
    pub fn move_project(&self, from: &str, to: &str) {
        let (old, new) = (self.project(from), self.project(to));
        if old == new || !old.is_dir() {
            return;
        }
        if !new.exists() {
            if let Some(parent) = new.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::rename(&old, &new).is_ok() {
                return;
            }
        }
        let entries = |dir: PathBuf| -> Vec<PathBuf> {
            std::fs::read_dir(dir).map(|found| found.flatten().map(|e| e.path()).collect()).unwrap_or_default()
        };
        let _ = std::fs::create_dir_all(new.join("chats"));
        for chat in entries(old.join("chats")) {
            if let Some(name) = chat.file_name() {
                let _ = std::fs::rename(&chat, new.join("chats").join(name));
            }
        }
        let _ = std::fs::create_dir_all(new.join("docs"));
        for doc in entries(old.join("docs")) {
            let _ = std::fs::rename(&doc, free_name(&new.join("docs"), &doc));
        }
        let (theirs, ours) = (old.join("memory"), new.join("memory"));
        if theirs.is_dir() {
            if entries(ours.clone()).is_empty() {
                let _ = std::fs::remove_dir(&ours);
                let _ = std::fs::rename(&theirs, &ours);
            } else {
                let kept = format!("from{}", Self::key(from));
                if std::fs::rename(&theirs, ours.join(&kept)).is_ok() {
                    let line = format!(
                        "- [Memory from the old folder]({kept}/MEMORY.md) — notes saved before the project moved\n"
                    );
                    let mut index = std::fs::read_to_string(ours.join("MEMORY.md")).unwrap_or_default();
                    if !index.is_empty() && !index.ends_with('\n') {
                        index.push('\n');
                    }
                    let _ = std::fs::write(ours.join("MEMORY.md"), index + &line);
                }
            }
        }
        // Whatever is left is empty folders; anything else stays where it is.
        for folder in ["chats", "docs", "memory"] {
            let _ = std::fs::remove_dir(old.join(folder));
        }
        let _ = std::fs::remove_dir(&old);
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

/// Where `file` can go in `folder` without replacing anything: under its own
/// name, or with `-2`, `-3`... before the extension.
fn free_name(folder: &Path, file: &Path) -> PathBuf {
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = file.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|n| folder.join(if n == 1 { format!("{stem}{ext}") } else { format!("{stem}-{n}{ext}") }))
        .find(|path| !path.exists())
        .expect("an unused name")
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
    fn what_is_kept_for_a_project_follows_its_folder() {
        let dir = std::env::temp_dir().join(format!("harness-home-test-{}", uuid::Uuid::new_v4()));
        let home = Home::new(dir.clone());
        let put = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap();
        put(home.chat("/w/old", "c1").join("notes.md"), "c1 notes");
        put(home.project_docs("/w/old").join("plan.md"), "old plan");
        put(home.project_memory("/w/old").join("MEMORY.md"), "- [A](a.md) — old fact\n");

        // A plain move: the folder is renamed.
        home.move_project("/w/old", "/w/new");
        assert!(!home.project("/w/old").exists());
        assert_eq!(read(home.chat("/w/new", "c1").join("notes.md")), "c1 notes");
        assert_eq!(read(home.project_docs("/w/new").join("plan.md")), "old plan");
        assert_eq!(read(home.project_memory("/w/new").join("MEMORY.md")), "- [A](a.md) — old fact\n");

        // Onto a project that is already there: the two are merged.
        put(home.chat("/w/new", "c2").join("notes.md"), "c2 notes");
        put(home.project_docs("/w/merged").join("plan.md"), "their plan");
        put(home.project_memory("/w/merged").join("MEMORY.md"), "- [B](b.md) — their fact");
        put(home.chat("/w/merged", "c3").join("notes.md"), "c3 notes");
        home.move_project("/w/new", "/w/merged");
        assert!(!home.project("/w/new").exists());
        for (chat, notes) in [("c1", "c1 notes"), ("c2", "c2 notes"), ("c3", "c3 notes")] {
            assert_eq!(read(home.chat("/w/merged", chat).join("notes.md")), notes);
        }
        let docs = home.project_docs("/w/merged");
        assert_eq!((read(docs.join("plan.md")), read(docs.join("plan-2.md"))), ("their plan".into(), "old plan".into()));
        let memory = home.project_memory("/w/merged");
        assert_eq!(
            read(memory.join("MEMORY.md")),
            "- [B](b.md) — their fact\n- [Memory from the old folder](from-w-new/MEMORY.md) — notes saved before the project moved\n"
        );
        assert_eq!(read(memory.join("from-w-new/MEMORY.md")), "- [A](a.md) — old fact\n");

        // Nothing kept for the old folder: nothing to do.
        home.move_project("/w/never", "/w/merged");
        let _ = std::fs::remove_dir_all(dir);
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
    fn preferences_are_edited_as_the_whole_file() {
        let dir = std::env::temp_dir().join(format!("harness-home-test-{}", uuid::Uuid::new_v4()));
        let home = Home::new(dir.join("home"));
        // No file, and no home folder yet: nothing to show, and saving makes both.
        assert_eq!(home.read_preferences().unwrap(), "");
        home.save_preferences("- Be brief.\n").unwrap();
        assert_eq!(home.read_preferences().unwrap(), "- Be brief.\n");
        assert_eq!(home.preferences().unwrap().text, "- Be brief.");
        // Saved over, with no side file left behind.
        home.save_preferences("\n").unwrap();
        assert_eq!(home.read_preferences().unwrap(), "\n");
        assert_eq!(home.preferences(), None);
        assert_eq!(std::fs::read_dir(dir.join("home")).unwrap().count(), 1);
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
