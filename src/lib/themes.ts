// Colour themes and sizes. A theme is data: the UI reads only the CSS custom
// properties set here, so adding a theme never touches a component.
// See PLAN.md, "Look" and "Appearance settings".

import { isAgent } from "./agents";
import { iconUrl, type IconColours } from "./appIcon";
import { devBuild, setAppIcon, titleBar } from "./ipc";
import { themes, themeVars, type Theme } from "./themeData";

export { themes, type Theme };

export const sizes = ["compact", "regular", "roomy"] as const;
export type Size = (typeof sizes)[number];

export const DEFAULT_DARK_THEME = "tilde-dark";
export const DEFAULT_LIGHT_THEME = "tilde-light";

/** Which of the two picked themes shows: always one, or whichever the system is in. */
export const appearances = ["auto", "light", "dark"] as const;
export type Appearance = (typeof appearances)[number];
export const DEFAULT_APPEARANCE: Appearance = "auto";

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

export function systemIsDark(): boolean {
  return systemDark.matches;
}

/** Calls `changed` when the system goes light or dark. The browser sends one event per switch; nothing polls. */
export function watchSystem(changed: (dark: boolean) => void): void {
  systemDark.addEventListener("change", (e) => changed(e.matches));
}

/** The theme to show: the dark pick or the light pick, as the appearance says. */
export function shownTheme(appearance: Appearance, darkTheme: string, lightTheme: string, systemDark: boolean): string {
  const dark = appearance === "auto" ? systemDark : appearance === "dark";
  return dark ? darkTheme : lightTheme;
}

/** A palette's other half, for filling in the side that had no pick before there were two. */
const pairs: [dark: string, light: string][] = [
  ["tilde-dark", "tilde-light"],
  ["solarized-dark", "solarized-light"],
  ["gruvbox-dark", "gruvbox-light"],
  ["catppuccin-mocha", "catppuccin-latte"],
  ["github-dark", "github-light"],
  ["rose-pine", "rose-pine-dawn"],
];
export const DEFAULT_SIZE: Size = "regular";

let shown: Theme | null = null;

/** The theme on show. */
export function currentTheme(): Theme {
  return shown ?? themes[0];
}

/** Told when the theme or text size changes, for what isn't drawn with CSS (the terminal). */
export const LOOK_CHANGED = "tilde-look";

/** Switching a theme only sets custom properties; nothing re-renders. */
export function applyTheme(id: string): void {
  const t = themes.find((theme) => theme.id === id) ?? themes[0];
  shown = t;
  const root = document.documentElement;
  for (const [name, value] of Object.entries(themeVars(t))) root.style.setProperty(name, value);
  root.style.colorScheme = t.dark ? "dark" : "light";
  if (WINDOWS) void titleBar(t.bg, t.dim, t.dark).catch(() => {});
  window.dispatchEvent(new Event(LOOK_CHANGED));
}

/** Spacing: line height, gaps and margins. Defined in app.css, keyed on this attribute. */
export function applySize(size: Size): void {
  document.documentElement.dataset.size = size;
}

/** An icon on offer: its colours, and the side of the picker it sits on. */
export type AppIcon = IconColours & { id: string; name: string; dark: boolean };
/** The app's own two first, then plain white on black and black on white, then one in each other theme's colours. */
const own = (t: Theme) => t.id === "tilde-dark" || t.id === "tilde-light";
export const appIcons: AppIcon[] = [
  ...themes.filter(own),
  // Off-white and dark grey: pure white on black, or black on white, glares.
  { id: "black", name: "Black", dark: true, bg: "#000000", accent: "#d9d9d9", faint: "#6e6e6e" },
  { id: "white", name: "White", dark: false, bg: "#ffffff", accent: "#333333", faint: "#b4b4b4" },
  ...themes.filter((t) => !own(t)),
];
/** The icon the app ships with. Any other is put up while the app runs. */
export const DEFAULT_APP_ICON = "tilde-dark";
/** Picking an icon changes the Dock's, which only a Mac has so far. */
export const APP_ICONS = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);

let dev: Promise<boolean> | null = null;
let iconAsked = 0;

/**
 * Shows an icon in the Dock: the user's own pick, whatever theme is on
 * show. It lasts while the app runs, so it is set again at each start
 * (`starting`). A build run from source wears hazard stripes on whichever icon
 * is picked, to tell it from the real app.
 */
export async function applyAppIcon(id: string, starting = false): Promise<void> {
  if (!APP_ICONS) return;
  const asked = ++iconAsked;
  try {
    const stripes = await (dev ??= devBuild());
    // At the start the Dock already shows the icon the app ships with.
    if (starting && id === DEFAULT_APP_ICON && !stripes) return;
    const picture = new Image();
    picture.src = iconUrl(appIcons.find((icon) => icon.id === id) ?? themes[0], { dev: stripes });
    await picture.decode();
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1024;
    canvas.getContext("2d")!.drawImage(picture, 0, 0, 1024, 1024);
    // A later pick has been made meanwhile: that one shows.
    if (asked !== iconAsked) return;
    await setAppIcon(canvas.toDataURL("image/png").split(",")[1]);
  } catch {
    // The Dock keeps the icon it has.
  }
}

// Windows draws text on whole pixels (ClearType hinting), and its displays
// mostly run at 100-150%, where a half-pixel size lands between pixels and
// blurs; a Mac's Retina screen doubles every size, so halves are sharp there.
const WINDOWS = typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);

/** Text sizes on offer, in px. */
export const fontSizes: readonly number[] = WINDOWS
  ? [11, 12, 13, 14, 15, 16]
  : [11, 11.5, 12, 12.5, 13, 13.5, 14, 15, 16];
export const DEFAULT_FONT_SIZE = WINDOWS ? 13 : 12.5;
/** The text size each spacing used to bring with it, before the two were separate settings. */
const fontOfSize: Record<Size, number> = WINDOWS
  ? { compact: 12, regular: 13, roomy: 14 }
  : { compact: 11.5, regular: 12.5, roomy: 13.5 };

/** A saved size, as offered here: on Windows a half size goes up to the whole one. */
function offeredSize(px: unknown): number | null {
  if (typeof px !== "number") return null;
  const size = WINDOWS ? Math.ceil(px) : px;
  return fontSizes.includes(size) ? size : null;
}

/** The text size. Spacing (`applySize`) is separate and doesn't change it. */
export function applyFontSize(px: number): void {
  document.documentElement.style.setProperty("--font", `${px}px`);
  window.dispatchEvent(new Event(LOOK_CHANGED));
}

/** `size` is the spacing: line height, gaps and margins. */
type Prefs = {
  /** The theme shown when the app is dark, and the one shown when it is light. */
  darkTheme: string;
  lightTheme: string;
  appearance: Appearance;
  size: Size;
  model: string;
  /** The agent of the model new chats start on (one of `AGENTS`). */
  agent: string;
  fontSize: number;
  /** The permission mode new chats start in: the last one picked, plan aside. */
  mode: string;
  /** The side panel's share of the window's width, 0 to 1. */
  panelWidth: number;
  /** Project folders folded in the sidebar, by path. */
  foldedProjects: string[];
  /** The sidebar is put away. */
  sidebarHidden: boolean;
  /** Project folders in the order set by dragging, by path. */
  projectOrder: string[];
  /** The model last picked in each project, by path: new chats there start on it. */
  projectModels: Record<string, { agent: string; model: string }>;
  /** When overnight keep awake ends, "HH:MM". */
  overnightUntil: string;
  /** The side panel's list of every comment is kept open (see SidePanel). */
  commentListOpen: boolean;
  /** The "+ comment" button under selected text; Cmd+Enter works either way. */
  commentButton: boolean;
  /** The app's icon, one of `appIcons`. Picked by hand: it doesn't follow the theme on show. */
  appIcon: string;
};
export const DEFAULT_PANEL_WIDTH = 0.46;

/** The side panel's width, as a share of the area beside the sidebar. Set by dragging its left edge. */
export function applyPanelWidth(share: number): void {
  document.documentElement.style.setProperty("--panel-w", `${(share * 100).toFixed(2)}%`);
}
/** Shows or hides the "+ comment" button under selected text, in the chat and in docs. */
export function applyCommentButton(on: boolean): void {
  document.documentElement.classList.toggle("no-comment-button", !on);
}
export const DEFAULT_MODE = "auto";
/** The times overnight keep awake can end at, picked in Settings. */
export const overnightTimes = ["06:00", "07:00", "08:00", "09:00", "10:00"];
const DEFAULT_OVERNIGHT = "08:00";
const modes = ["default", "acceptEdits", "auto"];

// Stored in the window's own storage until there is a settings page.
export function loadPrefs(): Prefs {
  try {
    const saved = JSON.parse(localStorage.getItem("tilde.prefs") ?? "{}") as Partial<Prefs> & { theme?: string };
    // Before there were two picks there was one `theme`: it becomes the pick
    // for its side, and its other half (or the default) the pick for the other.
    const old = themes.find((t) => t.id === saved.theme);
    const pick = (dark: boolean, savedId: string | undefined, fallback: string): string => {
      if (themes.some((t) => t.id === savedId && t.dark === dark)) return savedId!;
      if (!old) return fallback;
      if (old.dark === dark) return old.id;
      const pair = pairs.find((p) => p.includes(old.id));
      return pair ? pair[dark ? 0 : 1] : fallback;
    };
    const size = sizes.includes(saved.size as Size) ? (saved.size as Size) : DEFAULT_SIZE;
    return {
      darkTheme: pick(true, saved.darkTheme, DEFAULT_DARK_THEME),
      lightTheme: pick(false, saved.lightTheme, DEFAULT_LIGHT_THEME),
      appearance: appearances.includes(saved.appearance as Appearance) ? saved.appearance! : DEFAULT_APPEARANCE,
      size,
      // Any saved name is kept; if Claude Code no longer knows it, the chat says so.
      model: typeof saved.model === "string" && saved.model ? saved.model : "default",
      agent: isAgent(saved.agent) ? saved.agent : "claude-code",
      // With none saved, keep the text size the saved spacing used to give.
      fontSize: offeredSize(saved.fontSize) ?? fontOfSize[size],
      mode: modes.includes(saved.mode as string) ? saved.mode! : DEFAULT_MODE,
      panelWidth:
        typeof saved.panelWidth === "number" && saved.panelWidth > 0.1 && saved.panelWidth < 0.9
          ? saved.panelWidth
          : DEFAULT_PANEL_WIDTH,
      foldedProjects: Array.isArray(saved.foldedProjects) ? saved.foldedProjects.filter((p) => typeof p === "string") : [],
      sidebarHidden: saved.sidebarHidden === true,
      projectOrder: Array.isArray(saved.projectOrder) ? saved.projectOrder.filter((p) => typeof p === "string") : [],
      projectModels: Object.fromEntries(
        Object.entries(saved.projectModels ?? {}).filter(
          ([, pick]) => pick && typeof pick.model === "string" && pick.model && isAgent(pick.agent),
        ),
      ),
      overnightUntil: overnightTimes.includes(saved.overnightUntil as string) ? saved.overnightUntil! : DEFAULT_OVERNIGHT,
      commentListOpen: saved.commentListOpen === true,
      commentButton: saved.commentButton !== false,
      appIcon: appIcons.some((icon) => icon.id === saved.appIcon) ? saved.appIcon! : DEFAULT_APP_ICON,
    };
  } catch {
    return { darkTheme: DEFAULT_DARK_THEME, lightTheme: DEFAULT_LIGHT_THEME, appearance: DEFAULT_APPEARANCE, size: DEFAULT_SIZE, model: "default", agent: "claude-code", fontSize: DEFAULT_FONT_SIZE, mode: DEFAULT_MODE, panelWidth: DEFAULT_PANEL_WIDTH, foldedProjects: [], sidebarHidden: false, projectOrder: [], projectModels: {}, overnightUntil: DEFAULT_OVERNIGHT, commentListOpen: false, commentButton: true, appIcon: DEFAULT_APP_ICON };
  }
}

/**
 * A project's folder moved: what is kept by its path goes to the new one
 * (its place in the sidebar, whether it is folded, the model last picked in
 * it). Where the new path has its own already, that stays.
 */
export function moveProjectPrefs(from: string, to: string): void {
  const prefs = loadPrefs();
  const swap = (paths: string[]) => (paths.includes(to) ? paths.filter((p) => p !== from) : paths.map((p) => (p === from ? to : p)));
  const { [from]: model, ...models } = prefs.projectModels;
  savePrefs({
    foldedProjects: swap(prefs.foldedProjects),
    projectOrder: swap(prefs.projectOrder),
    projectModels: model && !models[to] ? { ...models, [to]: model } : models,
  });
}

/** Saves the given choices, leaving the others as they were. */
export function savePrefs(changed: Partial<Prefs>): void {
  try {
    localStorage.setItem("tilde.prefs", JSON.stringify({ ...loadPrefs(), ...changed }));
  } catch {
    // Storage can be unavailable; the choice then lasts for this run only.
  }
}
