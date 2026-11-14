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

/** A worktree chat's pull request, in a few words. */
export function prLabel(pr: { number: number; state: string } | null, branch?: string | null): string {
  if (!pr) return branch ? `${branch} · no pull request yet` : "no pull request yet";
  return `PR #${pr.number} ${pr.state}`;
}
