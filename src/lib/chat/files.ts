// Files dropped or pasted into a message. Nothing is copied: the message
// carries the file's path and Claude opens it itself. In the message box a
// file is a short word, `[report.pdf]`; once sent it is `[report.pdf](</full/path>)`,
// which Claude reads as a link and the chat draws as the word again.
// See PLAN.md, "Files".

/** A file in the draft: its word as it stands in the text, and where the file is. */
export type FileRef = { word: string; path: string };

/** The sent form of a file: `[label](<path>)`. */
export const FILE_LINK = /\[([^\]\n]+)\]\(<([^>\n]+)>\)/g;

/** Image files that become `[image n]` instead, by extension, with their types. */
const IMAGE_TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  bmp: "image/bmp",
  avif: "image/avif",
  heic: "image/heic",
  heif: "image/heif",
  tif: "image/tiff",
  tiff: "image/tiff",
};

/** The image type of a path, going by its extension, or null if it isn't an image. */
export function imageType(path: string): string | null {
  const ext = path.match(/\.([a-z0-9]+)$/i)?.[1].toLowerCase();
  return (ext && IMAGE_TYPES[ext]) || null;
}

/** Whether a path can be written in the sent form at all. */
export const linkable = (path: string) => !/[>\n]/.test(path);

/**
 * The word for a file: its name in brackets, a folder's ending in "/". A path
 * already in `taken` keeps its word. When another file has the name, parent
 * folders are added in front until the word is its own.
 */
export function fileWord(path: string, taken: FileRef[], isDir = false): string {
  const same = taken.find((f) => f.path === path);
  if (same) return same.word;
  const parts = path.split(/[\\/]/).filter(Boolean);
  for (let n = 1; n <= parts.length; n++) {
    const label = parts.slice(-n).join("/").replace(/\[/g, "(").replace(/\]/g, ")") + (isDir ? "/" : "");
    const word = `[${label || path}]`;
    if (!taken.some((f) => f.word === word)) return word;
  }
  return `[${path}]`;
}

/** `text` with each file's word swapped for its sent form. */
export function toLinks(text: string, files: FileRef[]): string {
  let out = text;
  // Longest first, so a word that is part of a longer one can't be hit inside it.
  for (const file of [...files].sort((a, b) => b.word.length - a.word.length)) {
    out = out.split(file.word).join(`${file.word}(<${file.path}>)`);
  }
  return out;
}
