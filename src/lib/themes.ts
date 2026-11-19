// Colour themes and sizes. A theme is data: the UI reads only the CSS custom
// properties set here, so adding a theme never touches a component.
// See PLAN.md, "Look" and "Appearance settings".

import { titleBar } from "./ipc";

export type Theme = {
  id: string;
  name: string;
  dark: boolean;
  /** Window background. */
  bg: string;
  /** Background of code blocks, diffs, the terminal and the user's messages. Must be clearly visible against `bg`. */
  panel: string;
  /** Body text. */
  fg: string;
  /** Emphasised text: the user's messages, headings, bold. */
  bright: string;
  /** Secondary text: folded tool lines, labels. */
  dim: string;
  /** Least important text: hints, placeholders, list markers. */
  faint: string;
  /** Hairline rules. Must show on both `bg` and `panel`. */
  line: string;
  /** Prompt marker, approval border, primary button. */
  accent: string;
  ok: string;
  bad: string;
  /** The palette's own blue: a chat finished since you last looked (sidebar dot).
   *  Gruvbox Dark and Everforest get a slightly bluer one: theirs read as grey-green next to `dim`. */
  blue: string;
  /** The palette's own yellow, darkened on light themes so it shows: a chat that needs you (sidebar dot). */
  yellow: string;
  /** The palette's own purple or magenta, darkened on light themes: a merged pull request (sidebar mark). */
  purple: string;
  /** Syntax colours inside code blocks. */
  syn: { kw: string; fn: string; str: string; type: string; num: string; comment: string };
};

export const themes: Theme[] = [
  {
    id: "harness-dark", name: "Harness Dark", dark: true,
    bg: "#111210", panel: "#1a1b17", fg: "#cfccc2", bright: "#f1eee4", dim: "#7d7a70", faint: "#4d4b43",
    line: "#24251f", accent: "#d9a35b", ok: "#8fae7e", bad: "#d47766", blue: "#7fa9d6", yellow: "#d8bd66", purple: "#b392c9",
    syn: { kw: "#d9a35b", fn: "#9db7c9", str: "#8fae7e", type: "#c9a8c4", num: "#d48a6a", comment: "#6b685f" },
  },
  {
    id: "harness-light", name: "Harness Light", dark: false,
    bg: "#fcfcfb", panel: "#f1f1ef", fg: "#3a3832", bright: "#191813", dim: "#858480", faint: "#b7b6b2",
    line: "#e6e6e3", accent: "#a8691c", ok: "#52733f", bad: "#b0463a", blue: "#2f6aa8", yellow: "#94740c", purple: "#7a4f9a",
    syn: { kw: "#a8691c", fn: "#2f6480", str: "#52733f", type: "#8a4f86", num: "#b0502a", comment: "#9a978b" },
  },
  {
    id: "solarized-dark", name: "Solarized Dark", dark: true,
    bg: "#002b36", panel: "#073642", fg: "#839496", bright: "#eee8d5", dim: "#657b83", faint: "#3f5a63",
    line: "#124652", accent: "#b58900", ok: "#859900", bad: "#dc322f", blue: "#268bd2", yellow: "#b58900", purple: "#6c71c4",
    syn: { kw: "#859900", fn: "#268bd2", str: "#2aa198", type: "#b58900", num: "#d33682", comment: "#586e75" },
  },
  {
    id: "solarized-light", name: "Solarized Light", dark: false,
    bg: "#fdf6e3", panel: "#eee8d5", fg: "#657b83", bright: "#073642", dim: "#93a1a1", faint: "#c3c7b8",
    line: "#e6dfc8", accent: "#b58900", ok: "#859900", bad: "#dc322f", blue: "#268bd2", yellow: "#b58900", purple: "#6c71c4",
    syn: { kw: "#859900", fn: "#268bd2", str: "#2aa198", type: "#b58900", num: "#d33682", comment: "#93a1a1" },
  },
  {
    id: "gruvbox-dark", name: "Gruvbox Dark", dark: true,
    bg: "#282828", panel: "#32302f", fg: "#d5c4a1", bright: "#fbf1c7", dim: "#928374", faint: "#665c54",
    line: "#3c3836", accent: "#fabd2f", ok: "#b8bb26", bad: "#fb4934", blue: "#80a9c9", yellow: "#fabd2f", purple: "#d3869b",
    syn: { kw: "#fb4934", fn: "#8ec07c", str: "#b8bb26", type: "#fabd2f", num: "#d3869b", comment: "#928374" },
  },
  {
    id: "gruvbox-light", name: "Gruvbox Light", dark: false,
    bg: "#fbf1c7", panel: "#f2e5bc", fg: "#504945", bright: "#282828", dim: "#928374", faint: "#bdae93",
    line: "#ebdbb2", accent: "#b57614", ok: "#79740e", bad: "#9d0006", blue: "#076678", yellow: "#b57614", purple: "#8f3f71",
    syn: { kw: "#9d0006", fn: "#427b58", str: "#79740e", type: "#b57614", num: "#8f3f71", comment: "#928374" },
  },
  {
    id: "dracula", name: "Dracula", dark: true,
    bg: "#282a36", panel: "#21222c", fg: "#e3e3dd", bright: "#f8f8f2", dim: "#7b88b8", faint: "#4f5372",
    line: "#343746", accent: "#bd93f9", ok: "#50fa7b", bad: "#ff5555", blue: "#8be9fd", yellow: "#f1fa8c", purple: "#bd93f9",
    syn: { kw: "#ff79c6", fn: "#50fa7b", str: "#f1fa8c", type: "#8be9fd", num: "#bd93f9", comment: "#6272a4" },
  },
  {
    id: "nord", name: "Nord", dark: true,
    // `line` is one step lighter than the palette's usual choice so rules show on the panel.
    bg: "#2e3440", panel: "#3b4252", fg: "#d8dee9", bright: "#eceff4", dim: "#7b88a1", faint: "#4c566a",
    line: "#434c5e", accent: "#88c0d0", ok: "#a3be8c", bad: "#bf616a", blue: "#88c0d0", yellow: "#ebcb8b", purple: "#b48ead",
    syn: { kw: "#81a1c1", fn: "#88c0d0", str: "#a3be8c", type: "#8fbcbb", num: "#b48ead", comment: "#616e88" },
  },
  {
    id: "tokyo-night", name: "Tokyo Night", dark: true,
    bg: "#1a1b26", panel: "#1f2335", fg: "#a9b1d6", bright: "#c0caf5", dim: "#565f89", faint: "#3b4261",
    line: "#292e42", accent: "#7aa2f7", ok: "#9ece6a", bad: "#f7768e", blue: "#7aa2f7", yellow: "#e0af68", purple: "#bb9af7",
    syn: { kw: "#bb9af7", fn: "#7aa2f7", str: "#9ece6a", type: "#2ac3de", num: "#ff9e64", comment: "#565f89" },
  },
  {
    id: "catppuccin-mocha", name: "Catppuccin Mocha", dark: true,
    bg: "#1e1e2e", panel: "#181825", fg: "#bac2de", bright: "#cdd6f4", dim: "#7f849c", faint: "#585b70",
    line: "#313244", accent: "#cba6f7", ok: "#a6e3a1", bad: "#f38ba8", blue: "#89b4fa", yellow: "#f9e2af", purple: "#cba6f7",
    syn: { kw: "#cba6f7", fn: "#89b4fa", str: "#a6e3a1", type: "#f9e2af", num: "#fab387", comment: "#6c7086" },
  },
  {
    id: "catppuccin-latte", name: "Catppuccin Latte", dark: false,
    bg: "#eff1f5", panel: "#e6e9ef", fg: "#4c4f69", bright: "#2c2f45", dim: "#8c8fa1", faint: "#bcc0cc",
    line: "#dce0e8", accent: "#8839ef", ok: "#40a02b", bad: "#d20f39", blue: "#1e66f5", yellow: "#b87512", purple: "#8839ef",
    syn: { kw: "#8839ef", fn: "#1e66f5", str: "#40a02b", type: "#df8e1d", num: "#fe640b", comment: "#9ca0b0" },
  },
  {
    id: "one-dark", name: "One Dark", dark: true,
    bg: "#282c34", panel: "#21252b", fg: "#abb2bf", bright: "#d7dae0", dim: "#7f848e", faint: "#4b5263",
    line: "#3a3f4b", accent: "#61afef", ok: "#98c379", bad: "#e06c75", blue: "#61afef", yellow: "#e5c07b", purple: "#c678dd",
    syn: { kw: "#c678dd", fn: "#61afef", str: "#98c379", type: "#e5c07b", num: "#d19a66", comment: "#5c6370" },
  },
  {
    id: "monokai", name: "Monokai", dark: true,
    bg: "#272822", panel: "#1e1f1c", fg: "#dedcd2", bright: "#f8f8f2", dim: "#8f908a", faint: "#5b5a4f",
    line: "#3e3d32", accent: "#e6db74", ok: "#a6e22e", bad: "#f92672", blue: "#66d9ef", yellow: "#e6db74", purple: "#ae81ff",
    syn: { kw: "#f92672", fn: "#a6e22e", str: "#e6db74", type: "#66d9ef", num: "#ae81ff", comment: "#75715e" },
  },
  {
    id: "github-dark", name: "GitHub Dark", dark: true,
    bg: "#0d1117", panel: "#161b22", fg: "#c9d1d9", bright: "#f0f6fc", dim: "#8b949e", faint: "#484f58",
    line: "#21262d", accent: "#58a6ff", ok: "#3fb950", bad: "#f85149", blue: "#58a6ff", yellow: "#d29922", purple: "#a371f7",
    syn: { kw: "#ff7b72", fn: "#d2a8ff", str: "#a5d6ff", type: "#ffa657", num: "#79c0ff", comment: "#8b949e" },
  },
  {
    id: "github-light", name: "GitHub Light", dark: false,
    // The palette's own subtle grey (#f6f8fa) is too close to white for a code panel.
    bg: "#ffffff", panel: "#eef1f4", fg: "#24292f", bright: "#0d1117", dim: "#57606a", faint: "#afb8c1",
    line: "#d8dee4", accent: "#0969da", ok: "#1a7f37", bad: "#cf222e", blue: "#0969da", yellow: "#9a6700", purple: "#8250df",
    syn: { kw: "#cf222e", fn: "#8250df", str: "#0a3069", type: "#953800", num: "#0550ae", comment: "#6e7781" },
  },
  {
    id: "rose-pine", name: "Rosé Pine", dark: true,
    bg: "#191724", panel: "#1f1d2e", fg: "#c9c6e0", bright: "#e0def4", dim: "#908caa", faint: "#524f67",
    line: "#26233a", accent: "#ebbcba", ok: "#9ccfd8", bad: "#eb6f92", blue: "#9ccfd8", yellow: "#f6c177", purple: "#c4a7e7",
    syn: { kw: "#31748f", fn: "#ebbcba", str: "#f6c177", type: "#9ccfd8", num: "#c4a7e7", comment: "#6e6a86" },
  },
  {
    id: "rose-pine-dawn", name: "Rosé Pine Dawn", dark: false,
    bg: "#faf4ed", panel: "#f2e9e1", fg: "#575279", bright: "#3b3761", dim: "#797593", faint: "#cecacd",
    line: "#dfdad9", accent: "#d7827e", ok: "#56949f", bad: "#b4637a", blue: "#286983", yellow: "#c0791c", purple: "#907aa9",
    syn: { kw: "#286983", fn: "#d7827e", str: "#ea9d34", type: "#56949f", num: "#907aa9", comment: "#9893a5" },
  },
  {
    id: "everforest", name: "Everforest", dark: true,
    bg: "#2d353b", panel: "#343f44", fg: "#d3c6aa", bright: "#e6dcc1", dim: "#859289", faint: "#5c6a72",
    line: "#3d484d", accent: "#dbbc7f", ok: "#a7c080", bad: "#e67e80", blue: "#82a9c8", yellow: "#dbbc7f", purple: "#d699b6",
    syn: { kw: "#e67e80", fn: "#a7c080", str: "#83c092", type: "#dbbc7f", num: "#d699b6", comment: "#859289" },
  },
  {
    id: "kanagawa", name: "Kanagawa", dark: true,
    bg: "#1f1f28", panel: "#16161d", fg: "#c8c093", bright: "#dcd7ba", dim: "#727169", faint: "#54546d",
    line: "#2a2a37", accent: "#7e9cd8", ok: "#98bb6c", bad: "#e46876", blue: "#7e9cd8", yellow: "#e6c384", purple: "#957fb8",
    syn: { kw: "#957fb8", fn: "#7e9cd8", str: "#98bb6c", type: "#7aa89f", num: "#d27e99", comment: "#727169" },
  },
  {
    id: "ayu-dark", name: "Ayu Dark", dark: true,
    bg: "#0b0e14", panel: "#131721", fg: "#bfbdb6", bright: "#e6e1cf", dim: "#6c7380", faint: "#3d424d",
    line: "#1e222b", accent: "#e6b450", ok: "#aad94c", bad: "#f07178", blue: "#59c2ff", yellow: "#e6b450", purple: "#d2a6ff",
    syn: { kw: "#ff8f40", fn: "#ffb454", str: "#aad94c", type: "#59c2ff", num: "#d2a6ff", comment: "#636a76" },
  },
];

export const sizes = ["compact", "regular", "roomy"] as const;
export type Size = (typeof sizes)[number];

export const DEFAULT_DARK_THEME = "harness-dark";
export const DEFAULT_LIGHT_THEME = "harness-light";

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
  ["harness-dark", "harness-light"],
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
export const LOOK_CHANGED = "harness-look";

/** Switching a theme only sets custom properties; nothing re-renders. */
export function applyTheme(id: string): void {
  const t = themes.find((theme) => theme.id === id) ?? themes[0];
  shown = t;
  const root = document.documentElement;
  const set = (name: string, value: string) => root.style.setProperty(name, value);
  set("--bg", t.bg);
  set("--panel", t.panel);
  set("--fg", t.fg);
  set("--bright", t.bright);
  set("--dim", t.dim);
  set("--faint", t.faint);
  set("--line", t.line);
  set("--accent", t.accent);
  set("--ok", t.ok);
  set("--bad", t.bad);
  set("--blue", t.blue);
  set("--yellow", t.yellow);
  set("--purple", t.purple);
  set("--syn-kw", t.syn.kw);
  set("--syn-fn", t.syn.fn);
  set("--syn-str", t.syn.str);
  set("--syn-type", t.syn.type);
  set("--syn-num", t.syn.num);
  set("--syn-comment", t.syn.comment);
  root.style.colorScheme = t.dark ? "dark" : "light";
  if (WINDOWS) void titleBar(t.bg, t.dim, t.dark).catch(() => {});
  window.dispatchEvent(new Event(LOOK_CHANGED));
}

/** Spacing: line height, gaps and margins. Defined in app.css, keyed on this attribute. */
export function applySize(size: Size): void {
  document.documentElement.dataset.size = size;
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
  /** The agent of the model new chats start on ("claude-code", "cursor", "codex"). */
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
    const saved = JSON.parse(localStorage.getItem("harness.prefs") ?? "{}") as Partial<Prefs> & { theme?: string };
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
      agent: saved.agent === "cursor" || saved.agent === "codex" ? saved.agent : "claude-code",
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
          ([, pick]) => pick && typeof pick.model === "string" && pick.model && (pick.agent === "cursor" || pick.agent === "codex" || pick.agent === "claude-code"),
        ),
      ),
      overnightUntil: overnightTimes.includes(saved.overnightUntil as string) ? saved.overnightUntil! : DEFAULT_OVERNIGHT,
      commentListOpen: saved.commentListOpen === true,
      commentButton: saved.commentButton !== false,
    };
  } catch {
    return { darkTheme: DEFAULT_DARK_THEME, lightTheme: DEFAULT_LIGHT_THEME, appearance: DEFAULT_APPEARANCE, size: DEFAULT_SIZE, model: "default", agent: "claude-code", fontSize: DEFAULT_FONT_SIZE, mode: DEFAULT_MODE, panelWidth: DEFAULT_PANEL_WIDTH, foldedProjects: [], sidebarHidden: false, projectOrder: [], projectModels: {}, overnightUntil: DEFAULT_OVERNIGHT, commentListOpen: false, commentButton: true };
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
    localStorage.setItem("harness.prefs", JSON.stringify({ ...loadPrefs(), ...changed }));
  } catch {
    // Storage can be unavailable; the choice then lasts for this run only.
  }
}
