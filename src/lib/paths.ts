// How folder paths are shown: short, with the home folder as "~".

/** `path` with the home folder written as "~". */
export function tilde(path: string, home: string): string {
  const base = home.replace(/[\\/]$/, "");
  return base && path.startsWith(base) ? "~" + path.slice(base.length) : path;
}

/**
 * A path that fits in `max` characters: the whole thing if it can, else its
 * last two parts, else just the last. The end of a path says the most.
 */
export function shortPath(path: string, home: string, max = 40): string {
  const short = tilde(path, home);
  if (short.length <= max) return short;
  const parts = short.split(/[\\/]/).filter(Boolean);
  const lastTwo = "…/" + parts.slice(-2).join("/");
  return lastTwo.length <= max || parts.length < 2 ? lastTwo : "…/" + parts[parts.length - 1];
}

type Pr = { number: number; state: string };

/**
 * One state for a chat's whole set of pull requests: "open" while any is
 * open, else "draft" while any is a draft, else "merged" if any merged, else
 * "closed". Null for none. The core has the same rule (`summary` in
 * src-tauri/src/worktree.rs).
 */
export function prSummary(prs: Pr[]): string | null {
  if (!prs.length) return null;
  return ["open", "draft", "merged"].find((state) => prs.some((pr) => pr.state === state)) ?? "closed";
}

/**
 * A chat's pull requests in the fewest words, for the line under the message
 * box: "#12" for one, "2/3 merged" for several. Closed (abandoned) parts
 * aren't counted; when every part was closed it reads "closed".
 */
export function prShort(prs: Pr[]): string {
  if (prs.length === 1) return `#${prs[0].number}`;
  const kept = prs.filter((pr) => pr.state !== "closed");
  if (!kept.length) return "closed";
  return `${kept.filter((pr) => pr.state === "merged").length}/${kept.length} merged`;
}

/** A chat's pull requests, in a few words. */
export function prLabel(prs: Pr[]): string {
  if (!prs.length) return "no pull request yet";
  if (prs.length === 1) return `PR #${prs[0].number} ${prs[0].state}`;
  const counts = ["merged", "open", "draft", "closed"]
    .map((state) => [prs.filter((pr) => pr.state === state).length, state] as const)
    .filter(([count]) => count)
    .map(([count, state]) => `${count} ${state}`);
  return `${prs.length} pull requests · ${counts.join(", ")}`;
}

/**
 * The file a link in chat text points at, as an absolute path, or null if it
 * isn't a local file. Takes plain paths and file: links; a relative path is
 * read from `base`. Drops "#heading", "?query" and a ":12" line number.
 */
export function localPath(href: string, base: string, home: string): string | null {
  let path = href.trim();
  if (!path || path.startsWith("#")) return null;
  const drive = /^[a-z]:[\\/]/i;
  if (/^file:/i.test(path)) path = path.replace(/^file:(\/\/(localhost)?)?/i, "");
  else if (/^[a-z][a-z0-9+.-]*:/i.test(path) && !drive.test(path)) return null; // a web link or other scheme
  path = path.replace(/[?#].*$/, "");
  try {
    path = decodeURIComponent(path);
  } catch {
    // Not encoded after all; use it as written.
  }
  path = path.replace(/:\d+(:\d+)?$/, "");
  if (/^\/[a-z]:[\\/]/i.test(path)) path = path.slice(1); // file:///C:/...

  if (path === "~" || /^~[\\/]/.test(path)) {
    if (!home) return null;
    path = home.replace(/[\\/]$/, "") + path.slice(1);
  } else if (!path.startsWith("/") && !drive.test(path) && !path.startsWith("\\\\")) {
    if (!base) return null;
    path = base.replace(/[\\/]$/, "") + "/" + path;
  }
  return tidy(path);
}

/** `path` with "." and ".." worked out, in the separator it mostly uses. */
function tidy(path: string): string {
  const sep = /^[a-z]:[\\/]|^\\\\/i.test(path) ? "\\" : "/";
  const lead = path.match(/^([a-z]:[\\/]|\\\\|\/)/i)?.[0] ?? "";
  const parts: string[] = [];
  for (const part of path.slice(lead.length).split(/[\\/]/)) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return lead.replace(/[\\/]$/, sep) + parts.join(sep);
}

/** The folder a file is in. */
export function folderOf(path: string): string {
  return path.replace(/[\\/][^\\/]*$/, "");
}

/** A worktree branch's short name: "harness/idle-timeout-1a2b3c" → "idle-timeout". Other branches stay as they are. */
export function shortBranch(branch: string): string {
  if (!branch.startsWith("harness/")) return branch;
  return branch.slice("harness/".length).replace(/-[0-9a-f]{6}$/, "");
}
