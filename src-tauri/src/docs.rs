//! Markdown files opened from a link in the chat, for the side panel.

use std::fs;
use std::io::Read;
use std::path::Path;

/// Bigger than any plan or note; stops a stray link from loading a huge file.
const MAX_DOC: u64 = 2 * 1024 * 1024;

/// The text of a markdown file. Only absolute paths ending in `.md` or
/// `.markdown`: the panel shows plans and notes, not whatever a link names.
pub fn read_doc(path: &Path) -> Result<String, String> {
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
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_markdown_only() {
        let dir = std::env::temp_dir().join(format!("harness-docs-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("Plan.MD");
        fs::write(&doc, "# Plan\n").unwrap();
        let other = dir.join("notes.txt");
        fs::write(&other, "hi").unwrap();

        assert_eq!(read_doc(&doc).unwrap(), "# Plan\n");
        assert!(read_doc(&other).is_err());
        assert!(read_doc(&dir.join("missing.md")).is_err());
        assert!(read_doc(Path::new("Plan.md")).is_err(), "relative paths are refused");

        let big = dir.join("big.md");
        fs::write(&big, vec![b'a'; (MAX_DOC + 1) as usize]).unwrap();
        assert!(read_doc(&big).is_err());

        fs::remove_dir_all(&dir).unwrap();
    }
}
