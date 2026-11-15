//! Images in chats: the ones the user pastes, and the ones tools hand the
//! agent. Each is a file in `<data dir>/images/<chat id>/<n>.<ext>`, numbered
//! from 1 per chat. Saved history holds only the number, never the picture.
//! A pasted image that had to be shrunk for the agent also keeps its original,
//! as `<n>.full.<ext>`, for looking at. See PLAN.md, "Images".

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use serde::{Deserialize, Serialize};

/// The image types the agent accepts, with the file extension each is kept under.
const TYPES: [(&str, &str); 4] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/gif", "gif"),
    ("image/webp", "webp"),
];

/// Names one saved image of a chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageRef {
    pub n: u32,
    pub media_type: String,
}

pub struct Images {
    root: PathBuf,
    /// Numbering a new image and writing it happen as one step.
    lock: Mutex<()>,
}

pub fn decode_base64(data: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|e| format!("Not a valid image ({e})"))
}

pub fn encode_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn extension(media_type: &str) -> Option<&'static str> {
    TYPES.iter().find(|(t, _)| *t == media_type).map(|(_, ext)| *ext)
}

/// A chat ID that can't reach outside the images folder.
fn check_session(session: &str) -> Result<(), String> {
    let ok = !session.is_empty()
        && session.len() <= 64
        && session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err("Not a chat ID".to_string())
    }
}

/// The number in a file name such as `3.png`.
fn number_of(path: &Path) -> Option<u32> {
    path.file_stem()?.to_str()?.parse().ok()
}

impl Images {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            lock: Mutex::new(()),
        }
    }

    fn folder(&self, session: &str) -> Result<PathBuf, String> {
        check_session(session)?;
        Ok(self.root.join(session))
    }

    /// Keeps an image under the chat's next number.
    pub fn save(&self, session: &str, media_type: &str, bytes: &[u8]) -> Result<ImageRef, String> {
        self.save_with_original(session, media_type, bytes, None)
    }

    /// Keeps an image, and the original it was shrunk from if there is one.
    /// The original is only looked at; the agent is sent the shrunk one.
    pub fn save_with_original(
        &self,
        session: &str,
        media_type: &str,
        bytes: &[u8],
        original: Option<(&str, &[u8])>,
    ) -> Result<ImageRef, String> {
        let ext = extension(media_type).ok_or_else(|| format!("Images of type {media_type} aren't supported"))?;
        let original = match original {
            Some((media_type, bytes)) => Some((
                extension(media_type).ok_or_else(|| format!("Images of type {media_type} aren't supported"))?,
                bytes,
            )),
            None => None,
        };
        let folder = self.folder(session)?;
        let _numbering = self.lock.lock().unwrap();
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let last = std::fs::read_dir(&folder)
            .map_err(|e| e.to_string())?
            .filter_map(|entry| number_of(&entry.ok()?.path()))
            .max()
            .unwrap_or(0);
        let n = last + 1;
        // The original first: `n` is only taken once its main file exists.
        if let Some((full_ext, full)) = original {
            std::fs::write(folder.join(format!("{n}.full.{full_ext}")), full).map_err(|e| e.to_string())?;
        }
        std::fs::write(folder.join(format!("{n}.{ext}")), bytes).map_err(|e| e.to_string())?;
        Ok(ImageRef {
            n,
            media_type: media_type.to_string(),
        })
    }

    /// An image's bytes and type, as the agent gets it.
    pub fn read(&self, session: &str, n: u32) -> Result<(Vec<u8>, String), String> {
        self.read_named(session, &n.to_string())
            .ok_or_else(|| format!("Image {n} is gone"))?
    }

    /// An image for looking at: its original if it was shrunk, else itself.
    pub fn read_largest(&self, session: &str, n: u32) -> Result<(Vec<u8>, String), String> {
        match self.read_named(session, &format!("{n}.full")) {
            Some(found) => found,
            None => self.read(session, n),
        }
    }

    /// The file `<name>.<ext>` for whichever extension exists. None if none does.
    fn read_named(&self, session: &str, name: &str) -> Option<Result<(Vec<u8>, String), String>> {
        let folder = match self.folder(session) {
            Ok(folder) => folder,
            Err(e) => return Some(Err(e)),
        };
        for (media_type, ext) in TYPES {
            match std::fs::read(folder.join(format!("{name}.{ext}"))) {
                Ok(bytes) => return Some(Ok((bytes, media_type.to_string()))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Some(Err(e.to_string())),
            }
        }
        None
    }

    /// Removes a chat's images, when the chat is deleted for good.
    pub fn delete_session(&self, session: &str) {
        if let Ok(folder) = self.folder(session) {
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// Removes the images of chats that don't exist: deleted ones, and pastes
    /// into a new chat that was never sent (drafts aren't saved).
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

    fn temp() -> (Images, PathBuf) {
        let dir = std::env::temp_dir().join(format!("harness-images-{}", uuid::Uuid::new_v4()));
        (Images::new(dir.clone()), dir)
    }

    #[test]
    fn numbers_from_one_per_chat() {
        let (images, dir) = temp();
        assert_eq!(images.save("a", "image/png", b"1").unwrap().n, 1);
        assert_eq!(images.save("a", "image/jpeg", b"2").unwrap().n, 2);
        assert_eq!(images.save("b", "image/png", b"x").unwrap().n, 1);
        assert_eq!(images.save("a", "image/webp", b"3").unwrap().n, 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reads_back_bytes_and_type() {
        let (images, dir) = temp();
        images.save("a", "image/png", b"one").unwrap();
        images.save("a", "image/jpeg", b"two").unwrap();
        assert_eq!(images.read("a", 2).unwrap(), (b"two".to_vec(), "image/jpeg".to_string()));
        assert!(images.read("a", 3).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_original_is_kept_for_looking_but_not_counted() {
        let (images, dir) = temp();
        let first = images
            .save_with_original("a", "image/jpeg", b"small", Some(("image/png", b"big")))
            .unwrap();
        assert_eq!(first.n, 1);
        assert_eq!(images.save("a", "image/png", b"next").unwrap().n, 2);
        assert_eq!(images.read("a", 1).unwrap(), (b"small".to_vec(), "image/jpeg".to_string()));
        assert_eq!(images.read_largest("a", 1).unwrap(), (b"big".to_vec(), "image/png".to_string()));
        // Without an original, the image itself.
        assert_eq!(images.read_largest("a", 2).unwrap().0, b"next".to_vec());
        assert!(images.read_largest("a", 3).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_other_types_and_bad_ids() {
        let (images, dir) = temp();
        assert!(images.save("a", "image/tiff", b"x").is_err());
        assert!(images.save("../a", "image/png", b"x").is_err());
        assert!(images.save("a/b", "image/png", b"x").is_err());
        assert!(images.save("", "image/png", b"x").is_err());
        assert!(images.read("..", 1).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sweep_keeps_only_known_chats() {
        let (images, dir) = temp();
        images.save("kept", "image/png", b"x").unwrap();
        images.save("gone", "image/png", b"x").unwrap();
        images.sweep(&HashSet::from(["kept".to_string()]));
        assert!(images.read("kept", 1).is_ok());
        assert!(images.read("gone", 1).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn deleting_a_chat_without_images_is_fine() {
        let (images, dir) = temp();
        images.delete_session("never");
        images.save("a", "image/png", b"x").unwrap();
        images.delete_session("a");
        assert!(images.read("a", 1).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
