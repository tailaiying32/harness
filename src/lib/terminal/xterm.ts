// The terminal's renderer, xterm.js. Loaded on demand the first time a
// terminal tab shows, so it costs nothing until then. This is the only module
// that imports xterm.

import { FitAddon } from "@xterm/addon-fit";
import { Terminal, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { Theme } from "../themes";

export type Made = { term: Terminal; fit: FitAddon; host: HTMLDivElement };

/** A colour with some see-through, for the scroll bar. */
function soft(colour: string): string {
  return /^#[0-9a-f]{6}$/i.test(colour) ? `${colour}55` : colour;
}

/** The theme's roles as terminal colours. ANSI black and white swap on light themes so text stays readable. */
export function xtermTheme(t: Theme): ITheme {
  return {
    background: t.bg,
    foreground: t.fg,
    cursor: t.accent,
    cursorAccent: t.bg,
    // Same as selected text elsewhere in the app: solid accent, background colour on top.
    selectionBackground: t.accent,
    selectionForeground: t.bg,
    scrollbarSliderBackground: soft(t.line),
    scrollbarSliderHoverBackground: soft(t.faint),
    scrollbarSliderActiveBackground: soft(t.dim),
    black: t.dark ? t.faint : t.bright,
    brightBlack: t.dim,
    red: t.bad,
    brightRed: t.bad,
    green: t.ok,
    brightGreen: t.ok,
    yellow: t.yellow,
    brightYellow: t.yellow,
    blue: t.blue,
    brightBlue: t.blue,
    magenta: t.purple,
    brightMagenta: t.purple,
    cyan: t.syn.fn,
    brightCyan: t.syn.type,
    white: t.dark ? t.fg : t.faint,
    brightWhite: t.dark ? t.bright : t.faint,
  };
}

/** The window's text: the same font and size as the chat. */
export function look(): { fontFamily: string; fontSize: number } {
  const css = getComputedStyle(document.documentElement);
  const fontFamily = css.getPropertyValue("--font-family").trim() || "ui-monospace, monospace";
  const fontSize = parseFloat(css.getPropertyValue("--font")) || 12.5;
  return { fontFamily, fontSize };
}

const WINDOWS = /Windows/.test(navigator.userAgent);

// Windows' ConPTY: xterm needs to know it, and whether its build reflows
// wrapped lines itself (21376 and later; every Windows 11 does). The browser
// tells Windows 11 (platform version 13+) from 10 but not the exact build.
let windowsBuild = 19041;
if (WINDOWS) {
  type UAData = { getHighEntropyValues(h: string[]): Promise<{ platformVersion?: string }> };
  const data = (navigator as Navigator & { userAgentData?: UAData }).userAgentData;
  void data
    ?.getHighEntropyValues(["platformVersion"])
    .then((v) => {
      if (parseInt(v.platformVersion ?? "0") >= 13) windowsBuild = 22000;
    })
    .catch(() => {});
}

/**
 * A terminal drawn into its own element, which the panel moves into view.
 * `keys` sees app shortcuts first: when it returns true the terminal ignores
 * the key, so it reaches the window's own handler.
 */
export function makeTerminal(theme: Theme, onData: (data: string) => void, keys: (e: KeyboardEvent) => boolean): Made {
  const host = document.createElement("div");
  host.className = "terminal-host";
  const term = new Terminal({
    ...look(),
    theme: xtermTheme(theme),
    scrollback: 5000,
    // On Windows a blinking bar, as Windows Terminal draws it.
    cursorBlink: WINDOWS,
    cursorStyle: WINDOWS ? "bar" : "block",
    allowProposedApi: false,
    macOptionClickForcesSelection: true,
    ...(WINDOWS ? { windowsPty: { backend: "conpty" as const, buildNumber: windowsBuild } } : {}),
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(host);
  term.onData(onData);
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== "keydown") return true;
    // Cmd+K clears the screen and its scrollback, as on a Mac terminal.
    if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key.toLowerCase() === "k") {
      term.clear();
      return false;
    }
    return !keys(e);
  });
  return { term, fit, host };
}

/** Takes the theme and text size on show. */
export function restyle(made: Made, theme: Theme): void {
  const { fontFamily, fontSize } = look();
  made.term.options.theme = xtermTheme(theme);
  if (made.term.options.fontFamily !== fontFamily) made.term.options.fontFamily = fontFamily;
  if (made.term.options.fontSize !== fontSize) made.term.options.fontSize = fontSize;
}
