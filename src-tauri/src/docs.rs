//! Markdown files shown as side panel tabs: ones the chat wrote and ones a
//! link opened. The user edits them in the tab, saved as they type.

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
