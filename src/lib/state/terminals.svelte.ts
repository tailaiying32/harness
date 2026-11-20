// The side panel's terminal tabs, which belong to a project rather than a chat:
// every chat of the project shows the same ones. Keyed by the project's main
// folder, as chats name it (`Chat.projectPath`).
//
// Each tab's xterm lives here, not in the panel, because the panel is made
// again for every chat: the terminal's element is moved into whichever panel
// shows it, and keeps its screen and scrollback.

import {
  terminalAttach,
  terminalClose,
  terminalMove,
  terminalNew,
  terminalRunning,
  terminalTabs,
  terminalWrite,
  type TerminalTab,
} from "../ipc";
import { currentTheme, LOOK_CHANGED } from "../themes";
import type { Searcher } from "../find";
import type { Made } from "../terminal/xterm";

type Lib = typeof import("../terminal/xterm");

/** A size to start a shell at before its tab has been measured. */
const START = { cols: 100, rows: 30 };

class Terminals {
  /** Each project's tabs, in order. */
  byProject = $state<Record<string, TerminalTab[]>>({});
  /** Each project's tab on show. */
  active = $state<Record<string, string>>({});
  /** A tab whose close is waiting on "close it?", with what runs in it. */
  confirming = $state<{ id: string; running: string } | null>(null);
  /**
   * Set by an action that should put the keys in the terminal (a click, Cmd+T);
   * the terminal view takes them and clears it. A flag, not a count, so the
   * view doesn't take the keys again when its tab changes (a command starting
   * or ending updates the tab) or when it is shown for another chat.
   */
  wantsKeys = $state(false);
  /** The last size a tab was fitted to, for starting new shells close to it. */
  size = { ...START };
  /** Counts changes of theme or text size, so the view fits its tab again (and tells the shell). */
  restyled = $state(0);

  private made = new Map<string, Made>();
  private making = new Map<string, Promise<Made>>();
  private lib: Promise<Lib> | null = null;
  private libMod: Lib | null = null;
  private loaded = new Set<string>();
  /** A tab an agent opened, to show once the core's list has it. */
  private revealing: Record<string, string> = {};
  /** App shortcuts the terminal leaves to the window (set by App). */
  keys: (e: KeyboardEvent) => boolean = () => false;

  constructor() {
    if (typeof window !== "undefined") {
      window.addEventListener(LOOK_CHANGED, () => {
        if (this.made.size === 0) return;
        void this.load().then((lib) => {
          for (const made of this.made.values()) lib.restyle(made, currentTheme());
          this.restyled++;
        });
      });
    }
  }

  tabs(project: string): TerminalTab[] {
    return this.byProject[project] ?? [];
  }

  /** The project's tab on show: the one picked, or else the last. */
  shown(project: string): TerminalTab | null {
    const tabs = this.tabs(project);
    return tabs.find((t) => t.id === this.active[project]) ?? tabs[tabs.length - 1] ?? null;
  }

  /** Asks the core for a project's tabs, once per run (events keep them current after). */
  async fetch(project: string): Promise<TerminalTab[]> {
    if (this.loaded.has(project)) return this.tabs(project);
    this.set(project, await terminalTabs(project));
    this.loaded.add(project);
    return this.tabs(project);
  }

  /** The core's list for a project; xterms of tabs no longer in it are let go. */
  set(project: string, tabs: TerminalTab[]): void {
    const before = this.tabs(project);
    this.byProject[project] = tabs;
    this.loaded.add(project);
    for (const old of before) {
      if (!tabs.some((t) => t.id === old.id)) this.forget(old.id);
    }
    const asked = this.revealing[project];
    if (asked && tabs.some((t) => t.id === asked)) {
      this.active[project] = asked;
      delete this.revealing[project];
    } else if (!tabs.some((t) => t.id === this.active[project]) && !asked) {
      const at = before.findIndex((t) => t.id === this.active[project]);
      const next = tabs[Math.min(Math.max(at, 0), tabs.length - 1)];
      if (next) this.active[project] = next.id;
      else delete this.active[project];
    }
    if (this.confirming && !tabs.some((t) => t.id === this.confirming?.id) && before.some((t) => t.id === this.confirming?.id)) {
      this.confirming = null;
    }
  }

  /** Opens a tab and shows it: in the main folder, or in a chat's worktree folder. */
  async open(project: string, worktree?: { path: string; label: string }): Promise<void> {
    const tab = await terminalNew(project, worktree?.path ?? null, worktree?.label ?? null, this.size.cols, this.size.rows);
    if (!this.tabs(project).some((t) => t.id === tab.id)) this.byProject[project] = [...this.tabs(project), tab];
    this.loaded.add(project);
    this.active[project] = tab.id;
    this.wantsKeys = true;
  }

  /**
   * Shows a tab an agent opened, leaving the keys where they are (the user
   * may be typing a message). The core's list may not have it yet.
   */
  reveal(project: string, id: string): void {
    this.active[project] = id;
    if (!this.tabs(project).some((t) => t.id === id)) this.revealing[project] = id;
  }

  pick(project: string, id: string): void {
    this.active[project] = id;
    this.wantsKeys = true;
  }

  /** A tab dragged to place `to` in the row. Moved here at once, so it doesn't jump back while the core answers. */
  move(project: string, id: string, to: number): void {
    const tabs = this.tabs(project);
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return;
    const rest = tabs.filter((t) => t !== tab);
    rest.splice(Math.min(to, rest.length), 0, tab);
    this.byProject[project] = rest;
    void terminalMove(id, to).catch(() => {});
  }

  /** The tab next to the one on show, wrapping round. */
  step(project: string, by: number): void {
    const tabs = this.tabs(project);
    const at = tabs.findIndex((t) => t.id === this.shown(project)?.id);
    if (tabs.length < 2 || at < 0) return;
    this.pick(project, tabs[(at + by + tabs.length) % tabs.length].id);
  }

  /** × or Cmd+W: closes at once, or first asks when something runs in it. */
  async askClose(id: string): Promise<void> {
    let running: string | null = null;
    try {
      running = await terminalRunning(id);
    } catch {
      // Gone already.
    }
    if (running) this.confirming = { id, running };
    else await this.close(id);
  }

  async close(id: string): Promise<void> {
    this.confirming = null;
    try {
      await terminalClose(id);
    } catch {
      // Gone already.
    }
  }

  /** Whether the keys are in a terminal right now. */
  focused(): boolean {
    return !!document.activeElement?.closest(".terminal-host");
  }

  /** Find (Cmd+F) in a tab's terminal buffer; null until its xterm is ready. */
  searcher(id: string): Searcher | null {
    const made = this.made.get(id);
    return made && this.libMod ? this.libMod.terminalSearcher(made.term) : null;
  }

  private load(): Promise<Lib> {
    this.lib ??= import("../terminal/xterm").then((mod) => {
      this.libMod = mod;
      return mod;
    });
    return this.lib;
  }

  /**
   * A tab's xterm, made the first time it is asked for and fed the tab's
   * output from then on. Ready once the output kept so far is drawn: fitting
   * it sooner would change its size under that drawing.
   */
  async xterm(id: string): Promise<Made> {
    const going = this.making.get(id);
    if (going) return going;
    const ready = this.made.get(id);
    if (ready) return ready;
    const making = (async () => {
      const lib = await this.load();
      const made = lib.makeTerminal(currentTheme(), (data) => void terminalWrite(id, data).catch(() => {}), (e) => this.keys(e));
      this.made.set(id, made);
      // New output waits its turn behind the kept output.
      let early: Uint8Array[] | null = [];
      const kept = await terminalAttach(id, (bytes) => (early ? early.push(bytes) : made.term.write(bytes)));
      await lib.replay(made.term, kept);
      for (const bytes of early) made.term.write(bytes);
      early = null;
      return made;
    })();
    this.making.set(id, making);
    const done = () => void this.making.delete(id);
    making.then(done, done);
    return making;
  }

  private forget(id: string): void {
    const made = this.made.get(id);
    this.made.delete(id);
    if (made) {
      made.host.remove();
      made.term.dispose();
    }
  }
}

export const terminals = new Terminals();
