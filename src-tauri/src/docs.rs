//! Markdown files shown as side panel tabs: ones the chat wrote and ones a
//! link opened. The user edits them in the tab, saved as they type. A doc an
//! agent wrote in the chat's own folder, and the chat's notes, can be saved
//! to the project, which moves the file into the project's docs folder in the
//! app's home folder (`home.rs`): outside the repo, shared by all the
//! project's chats.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
        let tmp = folder.join(format!(".{}.harness-save", name.to_string_lossy()));
        let saved = fs::write(&tmp, text)
            .and_then(|_| fs::set_permissions(&tmp, meta.permissions()))
            .and_then(|_| fs::rename(&tmp, &target));
        if saved.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        saved.map_err(|e| e.to_string())
    }
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
    move_into_project(&bytes, &source, docs, &stem, &ext)
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
    move_into_project(&bytes, notes, docs, if stem.is_empty() { "notes" } else { &stem }, "md")
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

/// Writes `bytes` to `<docs>/<stem>.<ext>`, or `-2`, `-3`... when that is
/// taken, then removes `source`. The folder is made if it isn't there. Copied
/// then removed, since a rename can't cross disks.
fn move_into_project(bytes: &[u8], source: &Path, docs: &Path, stem: &str, ext: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(docs).map_err(|e| e.to_string())?;
    for n in 1..1000 {
        let file = if n == 1 { format!("{stem}.{ext}") } else { format!("{stem}-{n}.{ext}") };
        let target = docs.join(file);
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
    Err("Too many docs with that name in the project.".into())
}

/// A markdown file's bytes. Only absolute paths ending in `.md` or
/// `.markdown`: the panel shows plans and notes, not whatever a link names.
fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let markdown = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"));
    if !markdown || !path.is_absolute() {
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
        let dir = std::env::temp_dir().join(format!("harness-docs-{name}-{}", std::process::id()));
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
        assert!(!dir.join(".a.md.harness-save").exists());

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
}
