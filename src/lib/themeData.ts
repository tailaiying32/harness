// The colour themes as plain data: the list, and the CSS custom properties a
// theme sets. Nothing here touches the window or the core, so the website can
// load it without the rest of the app (see site/).

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
    id: "tilde-dark", name: "Tilde Dark", dark: true,
    bg: "#111210", panel: "#1a1b17", fg: "#cfccc2", bright: "#f1eee4", dim: "#7d7a70", faint: "#4d4b43",
    line: "#24251f", accent: "#d9a35b", ok: "#8fae7e", bad: "#d47766", blue: "#7fa9d6", yellow: "#d8bd66", purple: "#b392c9",
    syn: { kw: "#d9a35b", fn: "#9db7c9", str: "#8fae7e", type: "#c9a8c4", num: "#d48a6a", comment: "#6b685f" },
  },
  {
    id: "tilde-light", name: "Tilde Light", dark: false,
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

/** The CSS custom properties a theme sets, by name. */
export function themeVars(t: Theme): Record<string, string> {
  return {
    "--bg": t.bg,
    "--panel": t.panel,
    "--fg": t.fg,
    "--bright": t.bright,
    "--dim": t.dim,
    "--faint": t.faint,
    "--line": t.line,
    "--accent": t.accent,
    "--ok": t.ok,
    "--bad": t.bad,
    "--blue": t.blue,
    "--yellow": t.yellow,
    "--purple": t.purple,
    "--syn-kw": t.syn.kw,
    "--syn-fn": t.syn.fn,
    "--syn-str": t.syn.str,
    "--syn-type": t.syn.type,
    "--syn-num": t.syn.num,
    "--syn-comment": t.syn.comment,
  };
}
