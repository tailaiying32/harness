// The page's theme. It is the app's own list (src/lib/themeData.ts), and the
// pick is kept for the next visit.

import { themes, themeVars, type Theme } from "../src/lib/themeData";

/** The visitor's pick, and the colours last shown (for the head's script to put up before the page draws). */
const PICK = "tilde.site.theme";
const LOOK = "tilde.look";

/** The picked theme, or Tilde Dark by default. */
function pickedTheme(): Theme {
  let id: string | null = null;
  try {
    id = localStorage.getItem(PICK);
  } catch {
    // Storage can be unavailable; fall back to Tilde Dark.
  }
  return (id ? themes.find((t) => t.id === id) : undefined) ?? themes.find((t) => t.id === "tilde-dark") ?? themes[0];
}

let shown = pickedTheme();
const watchers = new Set<(theme: Theme) => void>();

export function shownTheme(): Theme {
  return shown;
}

/** Calls `changed` whenever the page's theme changes. */
export function watchTheme(changed: (theme: Theme) => void): void {
  watchers.add(changed);
}

/** Draws the page in a theme. */
export function show(theme: Theme): void {
  shown = theme;
  const vars = themeVars(theme);
  const root = document.documentElement;
  for (const [name, value] of Object.entries(vars)) root.style.setProperty(name, value);
  const scheme = theme.dark ? "dark" : "light";
  root.style.colorScheme = scheme;
  try {
    localStorage.setItem(LOOK, JSON.stringify({ vars, scheme }));
  } catch {
    // Storage can be unavailable; the page then starts in the default colours.
  }
  for (const changed of watchers) changed(theme);
}

/** Sync the document immediately on load so cached variables match the active theme. */
show(shown);

/** The visitor's pick: kept, then shown. */
export function pick(id: string): void {
  const theme = themes.find((t) => t.id === id);
  if (!theme) return;
  try {
    localStorage.setItem(PICK, theme.id);
  } catch {
    // The pick then lasts for this visit only.
  }
  show(theme);
}
