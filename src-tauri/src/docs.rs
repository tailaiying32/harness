//! Markdown files shown as side panel tabs: ones the chat wrote and ones a
//! link opened. The user edits them in the tab, saved as they type. A doc an
//! agent wrote in the chat's own folder, and the chat's notes, can be saved
//! to the project, which moves the file into the project's docs folder in the
//! app's home folder (`home.rs`): outside the repo, shared by all the
//! project's chats. The panel's docs menu lists both folders, and renames or
//! deletes (to the Trash) the docs in them.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use serde::Serialize;

/// Bigger than any plan or note; stops a stray link from loading a huge file.
const MAX_DOC: u64 = 2 * 1024 * 1024;

/// The panel's documents. Only a file the panel has read may be written, so
/// the window can't be used to write files it never showed.
#[derive(Default)]
pub struct Docs {
    readable: Mutex<HashSet<PathBuf>>,
}

impl Docs {
    /// The text of a markdown file, which may then be saved from the panel
    /// (unless it isn't valid UTF-8: saving the shown text would change it).
    pub fn read(&self, path: &Path) -> Result<String, String> {
        let bytes = read_bytes(path)?;
        match String::from_utf8(bytes) {
            Ok(text) => {
                self.readable.lock().unwrap().insert(path.to_path_buf());
                Ok(text)
            }
            Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
        }
    }

    /// Saves the user's edit to a file the panel showed. Only a file that
    /// still exists: one Claude deleted isn't made again by typing. Written
    /// to a side file and moved into place, so a crash never leaves half a
    /// file; the side file gets the file's permissions, and a link is
    /// followed so it stays a link.
    pub fn write(&self, path: &Path, text: &str) -> Result<(), String> {
        if !self.readable.lock().unwrap().contains(path) {
            return Err("This document can't be saved from the panel.".into());
        }
        if text.len() as u64 > MAX_DOC {
            return Err("This document is too long to save (over 2 MB).".into());
        }
        let target = fs::canonicalize(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "That file doesn't exist any more.".to_string(),
            _ => e.to_string(),
        })?;
        let meta = fs::metadata(&target).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("That isn't a file.".into());
        }
        let (Some(folder), Some(name)) = (target.parent(), target.file_name()) else {
            return Err("That isn't a file.".into());
        };
        let tmp = folder.join(format!(".{}.tilde-save", name.to_string_lossy()));
        let saved = fs::write(&tmp, text)
            .and_then(|_| fs::set_permissions(&tmp, meta.permissions()))
            .and_then(|_| fs::rename(&tmp, &target));
        if saved.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        saved.map_err(|e| e.to_string())
    }

    /// Stops a path being saved from the panel: the doc was renamed or deleted.
    pub fn forget(&self, path: &Path) {
        self.readable.lock().unwrap().remove(path);
    }
}

/// The two folders the docs menu shows, and the chat's notes, which it never
/// renames or deletes.
pub struct Places {
    /// The chat's own folder: its notes and the docs it wrote.
    pub chat: PathBuf,
    pub notes: PathBuf,
    /// The project's docs folder, shared by all its chats.
    pub project: PathBuf,
}

/// A doc in one of the menu's folders.
#[derive(Debug, Serialize)]
pub struct DocEntry {
    pub path: String,
    pub name: String,
    /// When it last changed, in milliseconds since 1970.
    pub modified: u64,
}

/// The docs menu's two groups.
#[derive(Debug, Serialize)]
pub struct DocLists {
    pub chat: Vec<DocEntry>,
    pub project: Vec<DocEntry>,
}

/// The chat's docs (not its notes, which the menu shows on their own) and
/// the project's, each newest first. Read fresh each time the menu opens.
pub fn lists(places: &Places) -> DocLists {
    DocLists { chat: list(&places.chat, Some(&places.notes)), project: list(&places.project, None) }
}

/// The markdown files right in `folder`, newest first, leaving out `skip`,
/// hidden files (like a save's side file) and links. A missing folder has none.
fn list(folder: &Path, skip: Option<&Path>) -> Vec<DocEntry> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let skip = skip.and_then(|p| p.file_name());
    let mut docs: Vec<DocEntry> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let path = entry.path();
            if name.to_string_lossy().starts_with('.') || !is_markdown(&path) || Some(name.as_os_str()) == skip {
                return None;
            }
            let meta = fs::symlink_metadata(&path).ok().filter(|m| m.is_file())?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            Some(DocEntry { path: path.display().to_string(), name: name.to_string_lossy().into_owned(), modified })
        })
        .collect();
    docs.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    docs
}

/// Renames a doc in the chat's folder or the project's, keeping it in its
/// folder. A name without `.md` keeps the doc's own ending. A name another
/// doc there has (in any case, as Mac disks ignore case) is refused, so
/// nothing is overwritten. Returns the new path, or `None` if the name is
/// the same.
pub fn rename(path: &Path, name: &str, places: &Places) -> Result<Option<PathBuf>, String> {
    let source = managed(path, places)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("Give it a name.".into());
    }
    if name.contains(['/', '\\', ':']) || name.starts_with('.') || name.chars().any(char::is_control) {
        return Err("A name can't have slashes or colons, or start with a dot.".into());
    }
    let file = if is_markdown(Path::new(name)) {
        name.to_string()
    } else {
        let ext = source.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "md".into());
        format!("{name}.{ext}")
    };
    let own = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if file == own {
        return Ok(None);
    }
    let folder = source.parent().ok_or("That isn't a file.")?;
    let wanted = file.to_lowercase();
    let taken = fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy() != own && entry.file_name().to_string_lossy().to_lowercase() == wanted);
    if taken {
        return Err(format!("There's already a doc named {file}."));
    }
    fs::rename(&source, folder.join(&file)).map_err(|e| e.to_string())?;
    // The path as the window knows it, not the resolved one.
    Ok(Some(path.with_file_name(&file)))
}

/// Moves a doc in the chat's folder or the project's to the Trash.
pub fn delete(path: &Path, places: &Places) -> Result<(), String> {
    delete_with(path, places, to_trash)
}

fn delete_with(path: &Path, places: &Places, remove: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    remove(&managed(path, places)?)
}

/// Finder's way (the default) plays a sound and asks to control Finder, so
/// the Mac uses NSFileManager: no prompt, though Finder's "Put Back" won't
/// know where the doc came from.
fn to_trash(file: &Path) -> Result<(), String> {
    #[allow(unused_mut)]
    let mut trash = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        trash.set_delete_method(DeleteMethod::NsFileManager);
    }
    trash.delete(file).map_err(|e| format!("Couldn't move it to the Trash: {e}"))
}

/// A doc the menu may rename or delete: a markdown file (not a link) right
/// inside the chat's folder or the project's docs folder, and not the
/// notes. Returns it with its folder resolved.
fn managed(path: &Path, places: &Places) -> Result<PathBuf, String> {
    if !path.is_absolute() || !is_markdown(path) {
        return Err("Only markdown docs can be changed here.".into());
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err("That isn't a file.".into());
    };
    let gone = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound => "That file doesn't exist.".to_string(),
        _ => e.to_string(),
    };
    let parent = fs::canonicalize(parent).map_err(gone)?;
    let inside = [&places.chat, &places.project].iter().filter_map(|f| fs::canonicalize(f).ok()).any(|f| f == parent);
    if !inside {
        return Err("Only this chat's docs and the project's docs can be changed here.".into());
    }
    let file = parent.join(name);
    if !fs::symlink_metadata(&file).map_err(gone)?.is_file() {
        return Err("That isn't a file.".into());
    }
    if fs::canonicalize(&places.notes).is_ok_and(|n| n == file) {
        return Err("The chat's notes can't be renamed or deleted.".into());
    }
    Ok(file)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

/// Moves a doc from the chat's folder into the project's docs folder (`docs`),
/// keeping its name, or adding `-2`, `-3`... when that is taken: nothing is
/// overwritten. Copied then removed, since a rename can't cross disks. Only a
/// markdown file inside the chat's folder, and never the chat's notes by this
/// way. Returns the new path.
pub fn save_to_project(chat_folder: &Path, notes: &Path, from: &Path, docs: &Path) -> Result<PathBuf, String> {
    let bytes = read_bytes(from)?;
    let folder = fs::canonicalize(chat_folder).map_err(|e| e.to_string())?;
    let source = fs::canonicalize(from).map_err(|e| e.to_string())?;
    if !source.starts_with(&folder) || source == folder {
        return Err("Only a doc from this chat's folder can be saved to the project.".into());
    }
    if fs::canonicalize(notes).is_ok_and(|n| n == source) {
        return Err("The notes stay with the chat.".into());
    }
    let name = source.file_name().ok_or("That isn't a file.")?;
    let as_path = Path::new(name);
    let stem = as_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = as_path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    move_into(&bytes, &source, docs, &stem, &ext, None)
}

/// Moves the chat's notes into the project's docs folder (`docs`), named
/// after the chat (`notes.md` if the title leaves nothing), so the notes
/// start empty again. The same rules as a doc: nothing is overwritten.
/// Returns the new path.
pub fn save_notes_to_project(notes: &Path, title: &str, docs: &Path) -> Result<PathBuf, String> {
    let bytes = read_bytes(notes)?;
    if String::from_utf8_lossy(&bytes).trim().is_empty() {
        return Err("The notes are empty.".into());
    }
    let stem = file_stem_for(title);
    move_into(&bytes, notes, docs, if stem.is_empty() { "notes" } else { &stem }, "md", None)
}

/// A title as a file name: lower case words joined by dashes, at most 60 characters.
fn file_stem_for(title: &str) -> String {
    let mut stem = String::new();
    for word in title.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        let word = word.to_lowercase();
        if stem.len() + word.len() + 1 > 60 {
            break;
        }
        if !stem.is_empty() {
            stem.push('-');
        }
        stem.push_str(&word);
    }
    stem
}

/// Moves a project doc into the chat's folder, the way back from
/// `save_to_project`: same name, or `-2`, `-3`... when taken, and never the
/// notes' name, so it can't become the chat's notes. Returns the new path.
pub fn move_to_chat(path: &Path, places: &Places) -> Result<PathBuf, String> {
    let source = managed(path, places)?;
    let project = fs::canonicalize(&places.project).map_err(|e| e.to_string())?;
    if source.parent() != Some(project.as_path()) {
        return Err("Only a project doc can be moved into the chat.".into());
    }
    let bytes = read_bytes(&source)?;
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = source.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    move_into(&bytes, &source, &places.chat, &stem, &ext, places.notes.file_name())
}

/// Writes `bytes` to `<folder>/<stem>.<ext>`, or `-2`, `-3`... when that is
/// taken (or is `avoid`), then removes `source`. The folder is made if it
/// isn't there. Copied then removed, since a rename can't cross disks.
fn move_into(
    bytes: &[u8],
    source: &Path,
    folder: &Path,
    stem: &str,
    ext: &str,
    avoid: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    for n in 1..1000 {
        let file = if n == 1 { format!("{stem}.{ext}") } else { format!("{stem}-{n}.{ext}") };
        if avoid.is_some_and(|a| a.to_string_lossy().eq_ignore_ascii_case(&file)) {
            continue;
        }
        let target = folder.join(file);
        // `create_new` fails if the file is there, so a race can't overwrite.
        match fs::OpenOptions::new().write(true).create_new(true).open(&target) {
            Ok(mut out) => {
                use std::io::Write;
                if let Err(e) = out.write_all(bytes) {
                    let _ = fs::remove_file(&target);
                    return Err(e.to_string());
                }
                fs::remove_file(source).map_err(|e| e.to_string())?;
                return Ok(target);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Too many docs with that name there.".into())
}

/// A markdown file's bytes. Only absolute paths ending in `.md` or
/// `.markdown`: the panel shows plans and notes, not whatever a link names.
pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    if !is_markdown(path) || !path.is_absolute() {
        return Err("Only markdown files open in the panel.".into());
    }
    let file = fs::File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "That file doesn't exist.".to_string(),
        _ => e.to_string(),
    })?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("That isn't a file.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_DOC + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOC {
        return Err("That file is too big to show.".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tilde-docs-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_markdown_only() {
        let docs = Docs::default();
        let dir = folder("read");
        let doc = dir.join("Plan.MD");
        fs::write(&doc, "# Plan\n").unwrap();
        let other = dir.join("notes.txt");
        fs::write(&other, "hi").unwrap();

        assert_eq!(docs.read(&doc).unwrap(), "# Plan\n");
        assert!(docs.read(&other).is_err());
        assert!(docs.read(&dir.join("missing.md")).is_err());
        assert!(docs.read(Path::new("Plan.md")).is_err(), "relative paths are refused");

        let big = dir.join("big.md");
        fs::write(&big, vec![b'a'; (MAX_DOC + 1) as usize]).unwrap();
        assert!(docs.read(&big).is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_a_shown_document_is_saved() {
        let docs = Docs::default();
        let dir = folder("write");
        let doc = dir.join("a.md");
        fs::write(&doc, "one").unwrap();
        assert!(docs.write(&doc, "x").is_err(), "not read yet");
        assert_eq!(fs::read_to_string(&doc).unwrap(), "one");

        docs.read(&doc).unwrap();
        docs.write(&doc, "two").unwrap();
        assert_eq!(fs::read_to_string(&doc).unwrap(), "two");
        assert!(!dir.join(".a.md.tilde-save").exists());

        fs::remove_file(&doc).unwrap();
        assert!(docs.write(&doc, "three").is_err(), "a deleted file isn't made again");
        assert!(!doc.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saving_keeps_permissions_and_links() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let docs = Docs::default();
        let dir = folder("keep");
        let real = dir.join("real.md");
        fs::write(&real, "one").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.join("link.md");
        symlink(&real, &link).unwrap();

        docs.read(&link).unwrap();
        docs.write(&link, "two").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "two");
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_chat_doc_moves_into_the_project() {
        let chat = folder("chat");
        let project = folder("project");
        let notes = chat.join("notes.md");
        fs::write(&notes, "mine").unwrap();
        let doc = chat.join("ideas.md");
        fs::write(&doc, "# Ideas").unwrap();
        fs::create_dir_all(project.join("docs")).unwrap();
        fs::write(project.join("docs/ideas.md"), "older").unwrap();

        let saved = save_to_project(&chat, &notes, &doc, &project.join("docs")).unwrap();
        assert_eq!(saved, project.join("docs/ideas-2.md"), "an existing doc isn't overwritten");
        assert_eq!(fs::read_to_string(&saved).unwrap(), "# Ideas");
        assert_eq!(fs::read_to_string(project.join("docs/ideas.md")).unwrap(), "older");
        assert!(!doc.exists());

        assert!(save_to_project(&chat, &notes, &notes, &project.join("docs")).is_err(), "notes stay");
        let txt = chat.join("a.txt");
        fs::write(&txt, "x").unwrap();
        assert!(save_to_project(&chat, &notes, &txt, &project.join("docs")).is_err(), "markdown only");
        let outside = project.join("docs/ideas.md");
        assert!(save_to_project(&chat, &notes, &outside, &project.join("docs")).is_err(), "only from the chat folder");

        fs::remove_dir_all(&chat).unwrap();
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn the_notes_move_into_the_project_named_after_the_chat() {
        let chat = folder("notes-chat");
        let project = folder("notes-project");
        let notes = chat.join("notes.md");
        fs::write(&notes, "  \n").unwrap();
        assert!(save_notes_to_project(&notes, "Plan", &project.join("docs")).is_err(), "empty notes stay");

        fs::write(&notes, "- [ ] ship it").unwrap();
        let saved = save_notes_to_project(&notes, "Fix: the sidebar's (scroll) bug!", &project.join("docs")).unwrap();
        assert_eq!(saved, project.join("docs/fix-the-sidebar-s-scroll-bug.md"));
        assert_eq!(fs::read_to_string(&saved).unwrap(), "- [ ] ship it");
        assert!(!notes.exists(), "the notes start empty again");

        fs::write(&notes, "more").unwrap();
        let saved = save_notes_to_project(&notes, "?!", &project.join("docs")).unwrap();
        assert_eq!(saved, project.join("docs/notes.md"), "a title with no words names it notes");

        fs::remove_dir_all(&chat).unwrap();
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn text_that_isnt_utf8_is_shown_but_not_saved() {
        let docs = Docs::default();
        let dir = folder("bytes");
        let doc = dir.join("b.md");
        fs::write(&doc, [b'a', 0xff, b'b']).unwrap();
        assert_eq!(docs.read(&doc).unwrap(), "a\u{fffd}b");
        assert!(docs.write(&doc, "ab").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A chat folder with notes and a project docs folder, under one root.
    fn places(name: &str) -> (PathBuf, Places) {
        let root = folder(name);
        let chat = root.join("chat");
        let project = root.join("docs");
        fs::create_dir_all(&chat).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(chat.join("notes.md"), "mine").unwrap();
        let notes = chat.join("notes.md");
        (root, Places { chat, notes, project })
    }

    fn names(docs: &[DocEntry]) -> Vec<&str> {
        docs.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn the_menu_lists_markdown_newest_first_without_the_notes() {
        let (root, places) = places("list");
        let old = places.chat.join("old.md");
        fs::write(&old, "a").unwrap();
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        fs::File::options().write(true).open(&old).unwrap().set_modified(past).unwrap();
        fs::write(places.chat.join("new.markdown"), "b").unwrap();
        fs::write(places.chat.join("a.txt"), "c").unwrap();
        fs::write(places.chat.join(".new.md.tilde-save"), "d").unwrap();
        fs::write(places.chat.join(".hidden.md"), "d").unwrap();
        fs::create_dir_all(places.chat.join("folder.md")).unwrap();
        fs::write(places.project.join("TODO.md"), "e").unwrap();

        let lists = lists(&places);
        assert_eq!(names(&lists.chat), ["new.markdown", "old.md"]);
        assert_eq!(names(&lists.project), ["TODO.md"]);
        assert_eq!(lists.project[0].path, places.project.join("TODO.md").display().to_string());

        let missing = Places { chat: root.join("nope"), notes: root.join("nope/notes.md"), project: root.join("none") };
        let empty = super::lists(&missing);
        assert!(empty.chat.is_empty() && empty.project.is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_doc_is_renamed_in_its_folder() {
        let (root, places) = places("rename");
        let doc = places.chat.join("ideas.md");
        fs::write(&doc, "x").unwrap();
        let renamed = rename(&doc, " plan ", &places).unwrap().unwrap();
        assert_eq!(renamed, places.chat.join("plan.md"), "keeps its ending");
        assert_eq!(fs::read_to_string(&renamed).unwrap(), "x");
        assert!(!doc.exists());

        let todo = places.project.join("TODO.md");
        fs::write(&todo, "y").unwrap();
        assert_eq!(rename(&todo, "later.markdown", &places).unwrap().unwrap(), places.project.join("later.markdown"));
        assert_eq!(rename(&renamed, "plan", &places).unwrap(), None, "the same name changes nothing");
        assert_eq!(rename(&renamed, "Plan", &places).unwrap().unwrap(), places.chat.join("Plan.md"), "a change of case");
        assert!(places.chat.join("Plan.md").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_rename_never_overwrites_or_leaves_its_folder() {
        let (root, places) = places("rename-refused");
        let doc = places.chat.join("a.md");
        fs::write(&doc, "a").unwrap();
        fs::write(places.chat.join("B.md"), "b").unwrap();

        assert!(rename(&doc, "b", &places).unwrap_err().contains("already"), "taken in another case");
        assert!(rename(&doc, "notes", &places).is_err(), "not over the notes");
        for bad in ["", "  ", "../x", "x/y", "x\\y", ".x", "a:b"] {
            assert!(rename(&doc, bad, &places).is_err(), "{bad:?}");
        }
        assert_eq!(fs::read_to_string(&doc).unwrap(), "a");
        assert_eq!(fs::read_to_string(places.chat.join("B.md")).unwrap(), "b");

        assert!(rename(&places.notes, "kept", &places).is_err(), "the notes stay");
        let outside = root.join("outside.md");
        fs::write(&outside, "o").unwrap();
        assert!(rename(&outside, "moved", &places).is_err());
        let nested = places.project.join("sub");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("deep.md"), "d").unwrap();
        assert!(rename(&nested.join("deep.md"), "up", &places).is_err(), "only right inside the folders");
        assert!(rename(&places.project.join("sub/../../outside.md"), "x", &places).is_err());
        assert!(rename(&Path::new("a.md"), "x", &places).is_err(), "relative paths are refused");
        assert_eq!(rename(&places.chat.join("gone.md"), "x", &places).unwrap_err(), "That file doesn't exist.");
        assert!(outside.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folders_is_left_alone() {
        let (root, places) = places("rename-link");
        let outside = root.join("outside.md");
        fs::write(&outside, "o").unwrap();
        let link = places.project.join("link.md");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(rename(&link, "x", &places).is_err());
        assert!(delete_with(&link, &places, |_| Ok(())).is_err());
        assert!(lists(&places).project.is_empty(), "links aren't listed");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_the_menus_docs_are_deleted() {
        let (root, places) = places("delete");
        let doc = places.project.join("TODO.md");
        fs::write(&doc, "t").unwrap();
        let outside = root.join("outside.md");
        fs::write(&outside, "o").unwrap();
        let mut removed = Vec::new();
        delete_with(&doc, &places, |file| {
            removed.push(file.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert_eq!(removed, [fs::canonicalize(&doc).unwrap()]);
        assert!(delete_with(&places.notes, &places, |_| Ok(())).is_err());
        assert!(delete_with(&outside, &places, |_| Ok(())).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_forgotten_doc_cant_be_saved() {
        let docs = Docs::default();
        let (root, places) = places("forget");
        let doc = places.chat.join("a.md");
        fs::write(&doc, "one").unwrap();
        docs.read(&doc).unwrap();
        let renamed = rename(&doc, "b", &places).unwrap().unwrap();
        docs.forget(&doc);
        docs.read(&renamed).unwrap();
        fs::write(&doc, "back").unwrap();
        assert!(docs.write(&doc, "x").is_err(), "the old path is forgotten");
        docs.write(&renamed, "two").unwrap();
        assert_eq!(fs::read_to_string(&renamed).unwrap(), "two");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_project_doc_moves_back_into_the_chat() {
        let (root, places) = places("to-chat");
        let todo = places.project.join("TODO.md");
        fs::write(&todo, "t").unwrap();
        fs::write(places.chat.join("TODO.md"), "older").unwrap();
        let moved = move_to_chat(&todo, &places).unwrap();
        assert_eq!(moved, places.chat.join("TODO-2.md"), "nothing is overwritten");
        assert_eq!(fs::read_to_string(&moved).unwrap(), "t");
        assert!(!todo.exists());

        let named_notes = places.project.join("notes.md");
        fs::write(&named_notes, "n").unwrap();
        assert_eq!(move_to_chat(&named_notes, &places).unwrap(), places.chat.join("notes-2.md"), "never becomes the notes");
        assert_eq!(fs::read_to_string(&places.notes).unwrap(), "mine");

        let chat_doc = places.chat.join("a.md");
        fs::write(&chat_doc, "a").unwrap();
        assert!(move_to_chat(&chat_doc, &places).is_err(), "only from the project");
        fs::remove_dir_all(&root).unwrap();
    }
}
