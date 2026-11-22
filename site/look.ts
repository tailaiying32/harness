// The page's theme. It is the app's own list (src/lib/themeData.ts), and the
// pick is kept for the next visit.

import { themes, themeVars, type Theme } from "../src/lib/themeData";

/** The visitor's pick, and the colours last shown (for the head's script to put up before the page draws). */
const PICK = "tilde.site.theme";
const LOOK = "tilde.look";

const system = window.matchMedia("(prefers-color-scheme: dark)");

/** The manual pick id, or null when following the system light/dark mode. */
export function manualPick(): string | null {
  try {
    const id = localStorage.getItem(PICK);
    return themes.some((t) => t.id === id) ? id : null;
  } catch {
    return null;
  }
}

/** The picked theme, or Tilde Dark / Tilde Light matching the system's colour scheme. */
function pickedTheme(): Theme {
  const id = manualPick();
  return (id ? themes.find((t) => t.id === id) : undefined) ?? themes.find((t) => t.dark === system.matches)!;
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
    if (manualPick()) localStorage.setItem(LOOK, JSON.stringify({ vars, scheme }));
    else localStorage.removeItem(LOOK);
  } catch {
    // Storage can be unavailable; the page then starts in the default colours.
  }
  for (const changed of watchers) changed(theme);
}

/** Sync the document immediately on load so light/dark system changes apply without stale vars. */
show(shown);

/** The visitor's pick ("auto" clears the override and follows system light/dark). */
export function pick(id: string): void {
  if (id === "auto") {
    try {
      localStorage.removeItem(PICK);
      localStorage.removeItem(LOOK);
    } catch {
      // Ignore storage errors.
    }
    show(pickedTheme());
    return;
  }
  const theme = themes.find((t) => t.id === id);
  if (!theme) return;
  try {
    localStorage.setItem(PICK, theme.id);
  } catch {
    // The pick then lasts for this visit only.
  }
  show(theme);
}

// With no pick made, the page follows the system as it goes light or dark.
system.addEventListener("change", () => {
  if (!manualPick()) show(pickedTheme());
});
