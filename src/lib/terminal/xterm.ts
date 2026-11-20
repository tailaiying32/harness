// The terminal's renderer, xterm.js. Loaded on demand the first time a
// terminal tab shows, so it costs nothing until then. This is the only module
// that imports xterm.

import { FitAddon } from "@xterm/addon-fit";
import { Terminal, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { matchesIn, type Searcher } from "../find";
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

// A size mark in kept output: ESC ] 7717 ; <cols> x <rows> BEL. The core puts
// one in each time the shell's size changes (see terminal.rs).
const MARK = [0x1b, 0x5d, 0x37, 0x37, 0x31, 0x37, 0x3b];
const MARK_END = 0x07;

/** The next whole size mark at or after `from`. */
function nextMark(bytes: Uint8Array, from: number): { start: number; end: number; cols: number; rows: number } | null {
  for (let at = from; (at = bytes.indexOf(MARK[0], at)) >= 0; at++) {
    if (!MARK.every((b, i) => bytes[at + i] === b)) continue;
    const body = at + MARK.length;
    const stop = bytes.indexOf(MARK_END, body);
    if (stop < 0 || stop - body > 12) continue;
    const size = /^(\d+)x(\d+)$/.exec(String.fromCharCode(...bytes.subarray(body, stop)));
    if (size) return { start: at, end: stop + 1, cols: Number(size[1]), rows: Number(size[2]) };
  }
  return null;
}

/**
 * Draws a tab's kept output, each stretch at the size the shell had when it
 * wrote it. Output only reads right at that size: a progress display that
 * steps back up over its own lines lands on the wrong ones when they wrap.
 * The terminal is left at the last size; fitting it to the panel afterwards
 * rewraps the finished screen, as resizing any terminal does.
 */
export async function replay(term: Terminal, kept: Uint8Array): Promise<void> {
  const write = (part: Uint8Array) => (part.length ? new Promise<void>((done) => term.write(part, done)) : null);
  let at = 0;
  for (let mark = nextMark(kept, at); mark; mark = nextMark(kept, at)) {
    await write(kept.subarray(at, mark.start));
    if (mark.cols >= 2 && mark.rows >= 1) term.resize(mark.cols, mark.rows);
    at = mark.end;
  }
  await write(kept.subarray(at));
}

/** Takes the theme and text size on show. */
export function restyle(made: Made, theme: Theme): void {
  const { fontFamily, fontSize } = look();
  made.term.options.theme = xtermTheme(theme);
  if (made.term.options.fontFamily !== fontFamily) made.term.options.fontFamily = fontFamily;
  if (made.term.options.fontSize !== fontSize) made.term.options.fontSize = fontSize;
}

/**
 * Find (Cmd+F) across a terminal's active buffer (scrollback + screen).
 * Joins soft-wrapped lines before matching so matches across wraps are found,
 * then maps the shown match back to exact cell columns for `term.select`.
 */
export function terminalSearcher(term: Terminal): Searcher {
  type Match = { row: number; charOffset: number; charLen: number };
  let matches: Match[] = [];

  return {
    search(query) {
      matches = [];
      const q = query.toLowerCase();
      if (!q) {
        term.clearSelection();
        return 0;
      }
      const buf = term.buffer.active;
      let y = 0;
      while (y < buf.length) {
        const segs: { row: number; start: number }[] = [];
        let logical = "";
        let row = y;
        while (row < buf.length) {
          const line = buf.getLine(row);
          if (!line) break;
          if (row > y && !line.isWrapped) break;
          const continues = row + 1 < buf.length && !!buf.getLine(row + 1)?.isWrapped;
          segs.push({ row, start: logical.length });
          logical += line.translateToString(!continues);
          row++;
        }
        const lower = logical.toLowerCase();
        const same = lower.length === logical.length;
        const needle = same ? q : query;
        for (const pos of matchesIn(same ? lower : logical, needle)) {
          let seg = segs[0];
          for (let s = 1; s < segs.length && segs[s].start <= pos; s++) seg = segs[s];
          matches.push({ row: seg.row, charOffset: pos - seg.start, charLen: needle.length });
        }
        y = Math.max(row, y + 1);
      }
      if (!matches.length) term.clearSelection();
      return matches.length;
    },
    show(i) {
      const m = matches[i];
      if (!m) return;
      const buf = term.buffer.active;
      // Map UTF-16 offsets to terminal cell columns across any wrapped rows.
      let startCol = 0;
      let cellLen = 0;
      let seen = 0;
      let started = false;
      const endChar = m.charOffset + m.charLen;
      for (let r = m.row; r < buf.length && seen < endChar; r++) {
        const line = buf.getLine(r);
        if (!line || (r > m.row && !line.isWrapped)) break;
        const cell = line.getCell(0);
        for (let c = 0; c < line.length && seen < endChar; c++) {
          const got = line.getCell(c, cell);
          const width = got ? got.getWidth() : 1;
          if (width === 0) continue;
          const ch = got?.getChars() || " ";
          const next = seen + ch.length;
          if (!started && next > m.charOffset) {
            started = true;
            startCol = c;
          }
          if (started) cellLen += width;
          seen = next;
        }
      }
      term.scrollToLine(Math.max(0, m.row - Math.floor(term.rows / 2)));
      term.select(startCol, m.row, Math.max(1, cellLen));
    },
    clear() {
      matches = [];
      term.clearSelection();
    },
    watch(changed) {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const sub = term.onWriteParsed(() => {
        clearTimeout(timer);
        timer = setTimeout(changed, 200);
      });
      return () => {
        clearTimeout(timer);
        sub.dispose();
      };
    },
    finish() {
      term.focus();
      return true;
    },
  };
}
