// One chat's state: what's on screen, and how agent events change it.

import {
  answerApproval,
  browserCloseTab,
  browserNavigate,
  browserNewTab,
  browserPick,
  browserCurrent,
  closeSession,
  chooseWorktree,
  interrupt,
  readDoc,
  saveDocToProject,
  readNotes,
  sendMessage,
  sideEnd,
  sideInterrupt,
  sideSend,
  signIn,
  fileKind,
  openFile,
  setMode,
  setEffort,
  setModel,
  type AgentEvent,
  type ApprovalKind,
  type BrowserTab,
  type Decision,
  type ImageRef,
  type Mode,
  type PrInfo,
  type SessionRow,
  type SessionState,
  type WorktreeChoice,
  type WorktreeStatus,
  writeDoc,
  writeNotes,
  CLAUDE,
  CURSOR,
  type AgentId,
} from "../ipc";
import { modeFor } from "../models";
import { FILE_LINK, fileWord, linkable, toLinks, type FileRef } from "../chat/files";
import { docPath, planOf, showsOutput, textAfter, type ToolCall } from "../tools";
import { ChatInfo } from "./chatInfo.svelte";

/** What the side panel shows. */
export type PanelView = "docs" | "side" | "browser" | "terminal" | "info";

/** The browser tool that opens a page (see browser.rs). */
const BROWSER_OPEN = "mcp__harness_browser__open";

/** A string field of a tool's input, or "". */
function field(input: unknown, key: string): string {
  const value = input && typeof input === "object" ? (input as Record<string, unknown>)[key] : undefined;
  return typeof value === "string" ? value : "";
}

export type UserItem = {
  id: number;
  type: "user";
  text: string;
  /** Names the message to the core, which says when Claude has taken it in. */
  messageId?: string;
  /** Sent while Claude was working, and not taken in yet. */
  queued?: boolean;
  /** Images sent with it; `[image n]` in the text points at them. */
  images?: ImageRef[];
};
export type TextItem = { id: number; type: "text"; messageId: string; text: string; streaming: boolean };
export type ThinkingItem = { id: number; type: "thinking"; text: string };
export type ToolItem = { id: number; type: "tool" } & ToolCall;
export type ApprovalItem = {
  id: number;
  type: "approval";
  requestId: string;
  /** "worktree": Claude asked for a worktree; the app shows it as its own card. */
  approval: ApprovalKind | "worktree";
  toolName: string;
  title: string;
  input: unknown;
  /** The tool call this is about, when the agent says. */
  toolId: string | null;
  /** Button text for "always allow". Null when there is nothing to remember. */
  alwaysLabel: string | null;
  answered?: Decision;
  /** For a question: what was picked, by question text. */
  answers?: Record<string, string>;
  /** Withdrawn before it was answered, e.g. the turn was stopped. */
  cancelled?: boolean;
  /** For the worktree question: the repo, the agent's name for the work, and what was picked. */
  repoRoot?: string;
  workName?: string;
  choice?: WorktreeChoice;
};
export type ErrorItem = { id: number; type: "error"; text: string };
/** A faint one-line remark from the app, e.g. "stopped". */
export type NoteItem = { id: number; type: "note"; text: string };
/** Claude opened a page in the browser; the chat's line for it, with a way to look. */
export type PageItem = { id: number; type: "page"; url: string };
/** Claude Code isn't signed in. "done" once signed in, or once a later turn went through. */
export type SignInItem = {
  id: number;
  type: "signin";
  state: "ready" | "working" | "done" | "failed";
  error?: string;
};

/** A markdown file the chat wrote, as a tab in the side panel. */
export type DocTab = { path: string; text: string; /** Its tab was closed; the text still follows edits. */ closed?: boolean };

/** The plan's tab in the side panel. Documents use their file path. */
export const PLAN_TAB = "plan";
/** The chat's notes tab, always first in docs. Paths are absolute, so no clash. */
export const NOTES_TAB = "notes";

/** The `[image n]` words in a text, in order. */
export const IMAGE_WORD = /\[image (\d+)\]/g;

/**
 * The web addresses in a text, in order. Punctuation that ends a sentence is
 * left off, and so is a closing bracket the address didn't open, so
 * "(see https://a.com)." gives "https://a.com".
 */
export function webLinks(text: string): { at: number; text: string }[] {
  const out: { at: number; text: string }[] = [];
  for (const match of text.matchAll(/https?:\/\/[^\s<>"'`]+/g)) {
    let url = match[0];
    for (let before = ""; before !== url; ) {
      before = url;
      url = url.replace(/[.,;:!?*]+$/, "");
      for (const [open, close] of ["()", "[]", "{}"]) {
        if (url.endsWith(close) && url.split(open).length < url.split(close).length) url = url.slice(0, -1);
      }
    }
    if (/^https?:\/\/[^/?#]/.test(url)) out.push({ at: match.index, text: url });
  }
  return out;
}

/** One piece of a message's text: plain text, an image's word, a file's word (with its path), or a web address. */
export type Piece = { text: string; image?: ImageRef; file?: string; url?: string };

/** A sent text in pieces: plain text, the words of the given images, the files it links, and web addresses. */
export function withWords(text: string, images: ImageRef[] = []): Piece[] {
  const found: { at: number; length: number; piece: Piece }[] = [];
  for (const match of text.matchAll(IMAGE_WORD)) {
    const image = images.find((i) => i.n === Number(match[1]));
    if (image) found.push({ at: match.index, length: match[0].length, piece: { text: match[0], image } });
  }
  for (const match of text.matchAll(FILE_LINK)) {
    found.push({ at: match.index, length: match[0].length, piece: { text: `[${match[1]}]`, file: match[2] } });
  }
  for (const link of webLinks(text)) {
    found.push({ at: link.at, length: link.text.length, piece: { text: link.text, url: link.text } });
  }
  if (!found.length) return [{ text }];
  found.sort((a, b) => a.at - b.at);
  const out: Piece[] = [];
  let at = 0;
  for (const word of found) {
    if (word.at < at) continue; // inside the word before it
    out.push({ text: text.slice(at, word.at) }, word.piece);
    at = word.at + word.length;
  }
  out.push({ text: text.slice(at) });
  return out;
}

/** The image refs a tool result carries in place of its pictures. */
function imagesIn(content: unknown): ImageRef[] {
  if (!Array.isArray(content)) return [];
  return content
    .filter((part) => part?.type === "image_ref" && typeof part.n === "number")
    .map((part) => ({ n: part.n, media_type: String(part.media_type) }));
}

export type Item = UserItem | TextItem | ThinkingItem | ToolItem | ApprovalItem | ErrorItem | NoteItem | SignInItem | PageItem;

type NewItem =
  | Omit<UserItem, "id">
  | Omit<TextItem, "id">
  | Omit<ThinkingItem, "id">
  | Omit<ToolItem, "id">
  | Omit<ApprovalItem, "id">
  | Omit<ErrorItem, "id">
  | Omit<PageItem, "id">
  | Omit<NoteItem, "id">
  | Omit<SignInItem, "id">;

/** What the chat says when its worktree changes. */
function worktreeNote(status: WorktreeStatus, branch: string | null): string {
  switch (status) {
    case "created":
      return `working in a new worktree · ${branch ?? ""}`;
    case "restored":
      return "worktree brought back";
    case "put_away":
      return "worktree put away while idle · edits kept; your next message brings it back";
    case "merged":
      return "branch merged · worktree and branch removed";
    case "gone":
      return "put-away worktree deleted";
  }
}

/** How an answered plan went, in a few words. */
export function planOutcome(item: ApprovalItem): string {
  if (item.cancelled) return "cancelled";
  if (item.answered === "allow") return "approved";
  if (item.answered === "always_allow") return "approved · auto-accepting edits";
  return "kept planning";
}

function resultText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => (part && typeof part === "object" && "text" in part ? String(part.text) : ""))
      .join("\n");
  }
  return content == null ? "" : JSON.stringify(content);
}

/** For the self-test: answer each question with its first option. */
function firstOptions(input: unknown): Record<string, string> | undefined {
  const questions = (input as { questions?: { question: string; options?: { label: string }[] }[] })?.questions;
  if (!Array.isArray(questions)) return undefined;
  return Object.fromEntries(questions.map((q) => [q.question, q.options?.[0]?.label ?? ""]));
}

export class Chat {
  readonly sessionId: string;
  /** For a side chat: the chat it belongs to. Null for a chat of its own. */
  readonly owner: Chat | null;

  /** A new chat gets a fresh ID; a saved one keeps its own. */
  constructor(sessionId: string = crypto.randomUUID(), owner: Chat | null = null) {
    this.sessionId = sessionId;
    this.owner = owner;
  }

  /**
   * Rebuilds a saved chat by replaying its history through the same code that
   * handles live events, so a reloaded chat looks exactly as it did.
   */
  static fromSaved(row: SessionRow, events: AgentEvent[], live: boolean): Chat {
    const chat = new Chat(row.id);
    chat.projectPath = row.project_path;
    chat.syncRow(row);
    chat.replaying = true;
    for (const event of events) chat.handle(event);
    chat.replaying = false;
    // Docs in the chat's folder are read as they are now; one that was saved
    // to the project (or deleted) since loses its tab.
    for (const doc of chat.docs) if (chat.isChatDoc(doc.path)) void chat.refreshDoc(doc.path);
    chat.mode = row.mode as Mode;
    chat.modelChoice = row.model;
    chat.effortChoice = row.effort ?? "default";
    chat.stopping = false;
    if (!live) {
      chat.settle();
      chat.info.processGone();
      // Nothing is running, so nothing is waiting to be read.
      for (const item of chat.items) if (item.type === "user") item.queued = false;
    }
    // A plan still waiting for an answer is shown, as it would be live.
    if (chat.planPending) chat.openPanel(PLAN_TAB);
    return chat;
  }

  /** What is typed in the message box but not sent yet. */
  draft = $state("");
  /** Images pasted into the draft. One goes with the message only if its word is still in the text. */
  draftImages = $state<ImageRef[]>([]);
  /** Files dropped into the draft. One goes with the message only if its word is still in the text. */
  draftFiles = $state<FileRef[]>([]);

  /**
   * Takes a dropped or pasted file into the draft. Returns what to put in the
   * text: the file's word, or the bare path when it can't be written as a word.
   */
  async addDraftFile(path: string): Promise<string> {
    if (!linkable(path)) return path;
    const isDir = (await fileKind(path).catch(() => "file")) === "folder";
    const word = fileWord(path, this.draftFiles, isDir);
    if (!this.draftFiles.some((f) => f.path === path)) this.draftFiles.push({ word, path });
    return word;
  }
  items = $state<Item[]>([]);
  status = $state<SessionState | "none">("none");
  /** The agent this chat runs on. Picked with the model before the first message, fixed after. */
  agent = $state<AgentId>(CLAUDE);
  /** The model Claude Code reports it is using. Known after the first turn starts. */
  model = $state<string | null>(null);
  /** The model the user picked: an alias, a full name, or "default" for Claude Code's own default. */
  modelChoice = $state("default");
  /** A model was picked in this chat, so moving it to another folder keeps it. */
  modelPicked = false;
  /** The effort level picked for this chat, or "default" for Claude Code's default for the model. */
  effortChoice = $state("default");
  mode = $state<Mode>("default");
  usage = $state<{ window: string | null; utilization: number } | null>(null);
  projectPath = $state("");
  /** Where the agent works when the chat has a worktree; null otherwise. */
  workPath = $state<string | null>(null);
  /** The worktree's branch, and its pull request as last seen. */
  branch = $state<string | null>(null);
  /** "live" or "put_away" while the chat has a worktree. */
  wtStatus = $state<string | null>(null);
  prs = $state<PrInfo[]>([]);
  /** The folder paths are shown relative to: the worktree if there is one. */
  get root(): string {
    return this.workPath ?? this.projectPath;
  }
  /** Takes the saved row's worktree details (they change outside the chat's events). */
  syncRow(row: SessionRow): void {
    this.agent = row.agent ?? CLAUDE;
    this.workPath = row.work_path;
    this.branch = row.branch;
    this.wtStatus = row.wt_status;
    this.prs = row.prs;
  }
  /** When the current turn began, for the elapsed-time counter. */
  turnStartedAt = $state<number | null>(null);
  /** Claude Code is compacting the conversation. */
  compacting = $state(false);
  /** A side chat's name, written from its first question. A chat's own title lives in the sidebar's row. */
  title = $state("");

  // ---- The side panel ----
  /** The panel's views: the plan and documents, the side chat, or the browser. */
  panelView = $state<PanelView>("docs");
  /** The side chats (`/btw`, or the side view's +), one per tab. Never saved. */
  sides = $state<Chat[]>([]);
  /** The side chat tab picked; the last one if it is gone. */
  sideShown = $state<string | null>(null);
  /** The side chat on show, if there are any. */
  get side(): Chat | null {
    return this.sides.find((s) => s.sessionId === this.sideShown) ?? this.sides.at(-1) ?? null;
  }
  /**
   * Something besides the notes: a plan, a document, the side chat or the
   * browser. Closing one of those closes the panel when nothing else is left,
   * rather than leaving it on the notes.
   */
  get hasPanelWork(): boolean {
    return this.planTab || this.openDocs.length > 0 || this.side !== null || this.browserOn || this.terminalOn;
  }

  /** The panel has something to show. */
  get panelHasContent(): boolean {
    return this.tabCount > 0 || this.side !== null || this.browserOn || this.terminalOn;
  }

  // ---- Chat info: context, tasks, subagents, background work ----
  /** What the chat knows about itself; its subagents' steps live here, not in `items`. */
  readonly info = new ChatInfo();

  /** Shows chat info: given a subagent's tool call, that subagent full size; else whatever it showed last. */
  openInfo(toolId?: string): void {
    if (this.owner) return this.owner.openInfo(toolId);
    if (toolId) this.info.show(toolId);
    this.panelView = "info";
    this.panelOpen = true;
  }

  // ---- The terminal (its tabs belong to the project; see terminals.svelte.ts) ----
  /** The terminal view is open in this chat's panel. */
  terminalOn = $state(false);

  openTerminal(): void {
    if (this.owner) return this.owner.openTerminal();
    this.terminalOn = true;
    this.panelView = "terminal";
    this.panelOpen = true;
  }

  /** The project's last tab closed: the panel goes back to another view, or closes. */
  leaveTerminal(): void {
    this.terminalOn = false;
    if (this.panelView !== "terminal") return;
    this.panelView = this.side ? "side" : this.browserOn ? "browser" : "docs";
    this.panelOpen = this.panelOpen && this.hasPanelWork;
  }

  // ---- The browser (localhost pages in tabs beside the chat; Claude can drive it) ----
  /** The browser view is open, with or without a tab yet. */
  browserOn = $state(false);
  /** Its tabs, as the core last said. They live in the core, not saved. */
  browserTabs = $state<BrowserTab[]>([]);
  /** The tab on show. */
  browserShownTab = $state<string | null>(null);
  /** Why the address last typed couldn't be opened; cleared by the next one. */
  browserProblem = $state("");
  /** Counts up when the address box should take the cursor (a new empty tab). */
  focusAddress = $state(0);
  /** The tab on show, if there are any. */
  get browser(): BrowserTab | null {
    return this.browserTabs.find((t) => t.id === this.browserShownTab) ?? this.browserTabs.at(-1) ?? null;
  }
  /** The browser is what the panel shows right now. */
  get browserShown(): boolean {
    return this.panelOpen && this.panelView === "browser";
  }

  /** Opens the browser view: the tab on show if there is one, an empty address line if not. */
  openBrowser(): void {
    this.browserOn = true;
    this.panelView = "browser";
    this.panelOpen = true;
  }

  /** + or Cmd+T: a new empty tab, with the cursor in the address box. */
  async newBrowserTab(): Promise<void> {
    this.openBrowser();
    try {
      this.browserShownTab = await browserNewTab(this.sessionId);
      this.focusAddress++;
    } catch (e) {
      this.browserProblem = String(e);
    }
  }

  pickBrowserTab(id: string): void {
    this.browserShownTab = id;
    void browserPick(this.sessionId, id).catch(() => {});
  }

  /** The tab before or after the one on show, wrapping round. */
  stepBrowserTab(by: number): void {
    const n = this.browserTabs.length;
    if (n < 2) return;
    const at = this.browserTabs.findIndex((t) => t.id === this.browser?.id);
    this.pickBrowserTab(this.browserTabs[(at + by + n) % n].id);
  }

  /**
   * Closes a tab and its page (the one on show if none is named). After the
   * last, the panel goes back to another view, or closes; with no tabs at
   * all, the browser view itself goes.
   */
  async closeBrowserTab(id: string | undefined = this.browser?.id): Promise<void> {
    if (!id) {
      this.leaveBrowser();
      return;
    }
    try {
      await browserCloseTab(this.sessionId, id);
    } catch {
      // Already gone.
    }
  }

  /** From a page line in the chat: shows the tab on that address, or opens it in a new tab. */
  async showPage(url: string): Promise<void> {
    if (this.owner) return this.owner.showPage(url);
    this.openBrowser();
    const same = (a: string) => {
      try {
        return new URL(a).href === new URL(url).href;
      } catch {
        return a === url;
      }
    };
    const open = this.browserTabs.find((t) => same(t.url));
    if (open) {
      this.pickBrowserTab(open.id);
      return;
    }
    try {
      const tab = await browserNewTab(this.sessionId);
      this.browserShownTab = tab;
      await browserNavigate(this.sessionId, tab, url);
    } catch (e) {
      this.showError(String(e));
    }
  }

  private leaveBrowser(): void {
    this.browserOn = false;
    this.browserTabs = [];
    this.browserShownTab = null;
    this.browserProblem = "";
    if (this.panelView !== "browser") return;
    this.panelView = this.side ? "side" : this.terminalOn ? "terminal" : "docs";
    this.panelOpen = this.panelOpen && this.hasPanelWork;
  }

  /** A chat opened again after a reload of the window: pick up its live tabs. */
  async loadBrowser(): Promise<void> {
    try {
      const { tabs, shown } = await browserCurrent(this.sessionId);
      if (tabs.length) {
        this.browserTabs = tabs;
        this.browserShownTab = shown;
        this.browserOn = true;
      }
    } catch {
      // No core (the preview) or nothing open.
    }
  }
  /** The latest plan Claude presented. A revised plan replaces it. */
  plan = $state<ApprovalItem | null>(null);
  /** Markdown files this chat wrote, kept in step with its later edits to them. */
  docs = $state<DocTab[]>([]);
  panelOpen = $state(false);
  /** The open panel covers the chat (not the sidebar). Ends when the panel hides. */
  panelWide = $state(false);
  /** The chosen tab: PLAN_TAB or a document's path. */
  panelTab = $state(PLAN_TAB);
  /** Where Claude Code keeps the plan. It is the plan's tab, not a document. */
  private planPath = "";

  get planPending(): boolean {
    return this.plan !== null && !this.plan.answered && !this.plan.cancelled;
  }

  /** The plan's tab was closed. A new plan, or "open plan" in the chat, brings it back. */
  planClosed = $state(false);

  /** The plan has a tab: there is one and its tab wasn't closed. */
  get planTab(): boolean {
    return this.plan !== null && !this.planClosed;
  }

  /** Documents whose tab is showing. */
  get openDocs(): DocTab[] {
    return this.docs.filter((doc) => !doc.closed);
  }

  /** The notes tab is always there, so docs is never empty. */
  get tabCount(): number {
    return 1 + (this.planTab ? 1 : 0) + this.openDocs.length;
  }

  /**
   * The tab on show: the chosen one if it still exists, else the plan, else the
   * notes. The notes tab sits first in the row, but a waiting plan comes first.
   */
  get activeTab(): string {
    if (this.panelTab === NOTES_TAB) return NOTES_TAB;
    if (this.panelTab === PLAN_TAB && this.planTab) return PLAN_TAB;
    if (this.openDocs.some((doc) => doc.path === this.panelTab)) return this.panelTab;
    return this.planTab ? PLAN_TAB : NOTES_TAB;
  }

  // ---- Notes ----
  /** The chat's notes; null until first read. Saved in the core, outside the repo. */
  notes = $state<string | null>(null);
  private notesLoading: Promise<void> | null = null;

  /** Reads the notes once, the first time they are shown. */
  loadNotes(): Promise<void> {
    if (this.owner) return this.owner.loadNotes();
    this.notesLoading ??= readNotes(this.sessionId)
      .then((text) => {
        // Typing may have started before the read came back.
        if (this.notes === null) this.notes = text;
      })
      .catch(() => {
        if (this.notes === null) this.notes = "";
      });
    return this.notesLoading;
  }

  /**
   * Reads the notes again after Claude may have written them, if they are
   * already showing (otherwise they are read when first shown).
   */
  async refreshNotes(): Promise<void> {
    if (this.owner) return this.owner.refreshNotes();
    const before = this.notes;
    if (before === null) return;
    try {
      const text = await readNotes(this.sessionId);
      // Typed into while reading: the typing is newer than what was read.
      if (this.notes === before && text !== before) this.notes = text;
    } catch {
      // Left as shown; the next write or turn end tries again.
    }
  }

  /** The chat's notes file, which Claude may write. Kept by the core in `notes/<chat id>/`. */
  private isNotes(path: string): boolean {
    return path.replace(/\\/g, "/").endsWith(`/notes/${this.sessionId}/notes.md`);
  }

  /**
   * A doc in the chat's own folder, where agents write docs (next to the
   * notes, kept by the core in `notes/<chat id>/`). Its tab follows the file
   * on disk, whatever the agent's tool said, and it can be saved to the project.
   */
  isChatDoc(path: string): boolean {
    const p = path.replace(/\\/g, "/");
    return p.includes(`/notes/${this.sessionId}/`) && /\.(md|markdown)$/i.test(p) && !this.isNotes(path);
  }

  /** Said to the agent with the next message the user sends, then dropped. */
  private appNotes: string[] = [];

  /**
   * Moves a doc from the chat's folder into the project's docs/ folder (the
   * worktree's, if the chat has one). Its tab follows it, and the agent is
   * told with the next message, so it edits the new file.
   */
  async saveToProject(path: string): Promise<void> {
    if (this.owner) return this.owner.saveToProject(path);
    let saved: string;
    try {
      saved = await saveDocToProject(this.sessionId, path, this.root);
    } catch (e) {
      this.showError(`Couldn't save it to the project: ${e}`);
      return;
    }
    const doc = this.docs.find((d) => d.path === path);
    if (doc) doc.path = saved;
    if (this.panelTab === path) this.panelTab = saved;
    this.appNotes.push(`The user saved the doc ${path} to the project; it is now ${saved}, so edit it there.`);
    void this.refreshDoc(saved);
  }

  /** Keeps the notes as typed and saves them. */
  async saveNotes(text: string): Promise<void> {
    this.notes = text;
    try {
      await writeNotes(this.sessionId, text);
    } catch (e) {
      this.showError(`Couldn't save the notes: ${e}`);
    }
  }

  /** The waiting plan as the user edited it; null while unedited. Sent with Approve. */
  planDraft = $state<string | null>(null);
  /** What is typed under the plan ("what should change?"), not sent yet. Kept here so switching chats keeps it. */
  planNote = $state("");

  /** Opens the panel on docs; on `tab` if given, bringing it back if it was closed. */
  openPanel(tab?: string): void {
    // A link or "open plan" in a side chat opens it in the chat's own panel.
    if (this.owner) return this.owner.openPanel(tab);
    this.panelView = "docs";
    if (tab === PLAN_TAB) this.planClosed = false;
    else if (tab) {
      const doc = this.docs.find((d) => d.path === tab);
      if (doc) doc.closed = false;
    }
    if (tab) this.panelTab = tab;
    this.panelOpen = this.tabCount > 0;
  }

  /** Closes one tab. A plan still waiting for an answer can't be closed, nor can the notes. */
  closeTab(tab: string): void {
    if (tab === NOTES_TAB) return;
    if (tab === PLAN_TAB) {
      if (!this.planPending) this.planClosed = true;
      return;
    }
    const doc = this.docs.find((d) => d.path === tab);
    if (doc) doc.closed = true;
  }

  /** Shows a problem in the chat, e.g. an image that couldn't be attached. */
  showError(text: string): void {
    this.push({ type: "error", text });
  }

  /**
   * Opens a dropped file: a markdown file as a tab in the side panel, like a
   * link to one; anything else in its own app. Says so in the chat when it can't.
   */
  openDropped(path: string): void {
    if (/\.(md|markdown)$/i.test(path)) void this.openFile(path);
    else openFile(path).catch((e) => this.showError(String(e)));
  }

  // ---- Looking at images ----
  /** The image drawn over the chat and panel, if any. */
  viewing = $state<ImageRef | null>(null);

  openImage(image: ImageRef): void {
    this.viewing = image;
  }

  /** Every image in the chat, the draft's included, by number: what ← and → step through. */
  get allImages(): ImageRef[] {
    const found = new Map<number, ImageRef>();
    for (const item of this.items) {
      const images = item.type === "user" || item.type === "tool" ? item.images : undefined;
      for (const image of images ?? []) found.set(image.n, image);
    }
    for (const image of this.draftImages) found.set(image.n, image);
    return [...found.values()].sort((a, b) => a.n - b.n);
  }

  /**
   * A markdown file on disk, from a link, shown as a tab. Read fresh each time,
   * so the tab shows the file as it is now. A link to the plan's own file
   * opens the plan tab.
   */
  async openFile(path: string): Promise<void> {
    if (this.owner) return this.owner.openFile(path);
    if (path === this.planPath && this.plan) {
      this.openPanel(PLAN_TAB);
      return;
    }
    if (this.isNotes(path)) {
      this.openPanel(NOTES_TAB);
      return;
    }
    let text: string;
    try {
      text = await readDoc(path);
    } catch (e) {
      this.push({ type: "error", text: `Couldn't open ${path}: ${e}` });
      return;
    }
    const doc = this.docs.find((d) => d.path === path);
    if (doc) doc.text = text;
    else this.docs.push({ path, text });
    this.openPanel(path);
  }

  /** A call that wrote or edited a markdown file adds its tab, or updates it. */
  private trackDocument(tool: ToolItem): void {
    const path = docPath(tool);
    if (!path || path === this.planPath) return;
    // The notes have their own tab, read back from disk.
    if (this.isNotes(path)) {
      void this.refreshNotes();
      return;
    }
    const doc = this.docs.find((d) => d.path === path);
    const text = textAfter(tool, doc?.text);
    if (text === null) {
      // The edit doesn't say what the file holds now (Cursor's tools never
      // do), but a doc in the chat's folder is read from disk.
      if (!this.isChatDoc(path)) return;
      if (!doc) this.docs.push({ path, text: "" });
      if (!this.replaying) void this.refreshDoc(path);
      this.showNewChatDoc(path, !doc);
      return;
    }
    if (doc) doc.text = text;
    else this.docs.push({ path, text });
    // Then the file itself: the user may have edited it since Claude read it.
    if (!this.replaying) void this.refreshDoc(path);
    this.showNewChatDoc(path, !doc);
  }

  /**
   * A doc the agent just made in the chat's folder was written for the user
   * to read, so its tab opens at once, as a plan does. Other markdown files
   * (a repo's README, say) only get a tab; later edits don't switch to it.
   */
  private showNewChatDoc(path: string, isNew: boolean): void {
    if (isNew && !this.replaying && this.isChatDoc(path)) this.openPanel(path);
  }

  /** Reads a document's tab again from its file. Left as shown if it can't be read. */
  async refreshDoc(path: string): Promise<void> {
    if (this.owner) return this.owner.refreshDoc(path);
    const before = this.docs.find((d) => d.path === path)?.text;
    let text: string;
    try {
      text = await readDoc(path);
    } catch (e) {
      // A doc in the chat's folder that is gone was moved or deleted: its tab goes.
      if (this.isChatDoc(path) && String(e).includes("doesn't exist")) {
        this.docs = this.docs.filter((d) => d.path !== path);
      }
      return;
    }
    const doc = this.docs.find((d) => d.path === path);
    // Typed into while reading: the typing is newer than what was read.
    if (doc && doc.text === before && doc.text !== text) doc.text = text;
  }

  /** Keeps a document as the user typed it and saves it to its file. */
  async saveDoc(path: string, text: string): Promise<void> {
    const doc = this.docs.find((d) => d.path === path);
    if (doc) doc.text = text;
    try {
      await writeDoc(path, text);
    } catch (e) {
      this.showError(`Couldn't save ${path}: ${e}`);
    }
  }

  /** Called once each time a turn ends. Used by the self-test. */
  onTurnEnd: (() => void) | null = null;
  /** Called after a message has been handed to the core, which saves the chat. */
  onSent: (() => void) | null = null;
  /** Answer every approval this way. Used by the self-test only. */
  autoAnswer: Decision | null = null;
  /** Self-test only: a line added to the next plan before it is auto-approved. */
  autoPlanEdit: string | null = null;
  /** Answer the worktree question this way. Used by the self-test only. */
  autoWorktree: WorktreeChoice | null = null;

  private nextId = 0;
  // Live text arrives in many small pieces. They are collected here and applied
  // once per frame, so a fast stream costs one update per frame, not per piece.
  private pendingText = new Map<string, { messageId: string; text: string }>();
  private drafts = new Map<string, TextItem>();
  private frame = 0;
  /** The user pressed stop and the turn hasn't ended yet. */
  private stopping = false;
  /** History is being replayed: events are the record, not news. */
  private replaying = false;

  /** Nothing is running any more: close whatever was left open. */
  private settle(): void {
    for (const item of this.items) {
      if (item.type === "text") item.streaming = false;
      if (item.type === "tool" && item.result === undefined) item.result = "";
      if (item.type === "approval" && !item.answered && !item.cancelled) this.cancel(item);
    }
    this.drafts.clear();
    this.turnStartedAt = null;
  }

  get busy(): boolean {
    return this.status === "starting" || this.status === "working" || this.status === "waiting_approval";
  }

  get started(): boolean {
    return this.items.length > 0;
  }

  /** Something is typed, pasted or dropped in the message box. */
  get hasDraft(): boolean {
    return this.draft.trim() !== "" || this.draftImages.length > 0 || this.draftFiles.length > 0;
  }

  private push<T extends Item>(item: NewItem): T {
    this.items.push({ ...item, id: this.nextId++ } as Item);
    // Read it back so the caller holds the reactive version.
    return this.items[this.items.length - 1] as T;
  }

  private flushText = () => {
    this.frame = 0;
    for (const [key, piece] of this.pendingText) {
      let draft = this.drafts.get(key);
      if (!draft) {
        draft = this.push<TextItem>({ type: "text", messageId: piece.messageId, text: "", streaming: true });
        this.drafts.set(key, draft);
      }
      draft.text += piece.text;
    }
    this.pendingText.clear();
  };

  handle(event: AgentEvent): void {
    // A subagent's steps go to chat info only.
    if (this.info.handle(event, this.replaying)) return;
    switch (event.kind) {
      case "session_started":
        this.model = event.model;
        if (event.mode) this.mode = event.mode as Mode;
        break;

      // Claude Code can change mode on its own: approving a plan leaves plan
      // mode, and "allow all edits" switches to auto-accept.
      case "mode_changed":
        this.mode = event.mode as Mode;
        break;

      case "status":
        this.status = event.state;
        if (event.state === "exited") this.compacting = false;
        break;

      case "usage_update":
        this.usage = { window: event.window, utilization: event.utilization };
        break;

      case "text_delta": {
        const key = `${event.message_id}:${event.block_index}`;
        const piece = this.pendingText.get(key);
        if (piece) piece.text += event.text;
        else this.pendingText.set(key, { messageId: event.message_id, text: event.text });
        if (!this.frame) this.frame = requestAnimationFrame(this.flushText);
        break;
      }

      case "assistant_block": {
        if (event.parent_tool_id) break; // subagent traffic: not shown yet
        this.flushText();
        const block = event.block;
        if (block.type === "text") {
          // The finished block is the final truth; it replaces the live draft.
          const live = this.items.find(
            (i): i is TextItem => i.type === "text" && i.streaming && i.messageId === event.message_id,
          );
          if (live) {
            live.text = block.text;
            live.streaming = false;
          } else {
            this.push({ type: "text", messageId: event.message_id, text: block.text, streaming: false });
          }
        } else if (block.type === "thinking") {
          this.push({ type: "thinking", text: block.text });
        } else {
          this.push({ type: "tool", toolId: block.tool_id, name: block.name, input: block.input });
        }
        break;
      }

      case "tool_result": {
        if (event.parent_tool_id) break;
        const tool = this.items.find((i): i is ToolItem => i.type === "tool" && i.toolId === event.tool_id);
        if (tool) {
          tool.isError = event.is_error;
          // Output that is never shown (a file that was read, say) isn't kept.
          tool.result = showsOutput(tool) ? resultText(event.content) : "";
          const images = imagesIn(event.content);
          if (images.length) tool.images = images;
          if (!event.is_error) this.trackDocument(tool);
          // A page Claude opened gets its own line, like a plan, with a way to look at it.
          if (!event.is_error && tool.name === BROWSER_OPEN) {
            const url = field(tool.input, "url").trim();
            this.push({ type: "page", url: /^https?:\/\//.test(url) ? url : `http://${url.replace(/^:/, "localhost:")}` });
          }
        }
        break;
      }

      case "approval_request": {
        const item = this.push<ApprovalItem>({
          type: "approval",
          requestId: event.request_id,
          approval: event.approval,
          toolName: event.tool_name,
          title: event.title,
          input: event.input,
          toolId: event.tool_id,
          alwaysLabel: event.always_label,
        });
        if (event.approval === "plan") {
          this.plan = item;
          this.planClosed = false;
          this.planDraft = null;
          this.planNote = "";
          // Claude Code drafts the plan in a file of its own; that file is this tab.
          this.planPath = planOf(event.input).path;
          this.docs = this.docs.filter((doc) => doc.path !== this.planPath);
          // Claude is waiting on the user, so the plan is put in front of them;
          // in a side chat it only marks docs, and the side chat stays.
          if (this.replaying) {
            // Nothing to show yet.
          } else if (this.panelOpen && this.panelView === "side") {
            this.panelTab = PLAN_TAB;
          } else {
            this.openPanel(PLAN_TAB);
          }
        }
        if (this.autoAnswer) {
          // A question has no "always"; it is answered with its first options.
          const answers = event.approval === "question" ? firstOptions(event.input) : undefined;
          let plan: string | undefined;
          if (event.approval === "plan" && this.autoPlanEdit) {
            plan = `${planOf(event.input).text}\n\n${this.autoPlanEdit}`;
            this.autoPlanEdit = null;
          }
          void this.answer(item, answers ? "allow" : this.autoAnswer, undefined, answers, plan);
        }
        break;
      }

      case "worktree_ask": {
        const item = this.push<ApprovalItem>({
          type: "approval",
          requestId: event.request_id,
          approval: "worktree",
          toolName: "",
          title: "",
          input: null,
          toolId: event.tool_id,
          alwaysLabel: null,
          repoRoot: event.repo_root,
          workName: event.name ?? undefined,
        });
        if (this.autoWorktree) void this.chooseWorktree(item, this.autoWorktree);
        break;
      }

      // The core's record of the pick. Live, the card already shows it.
      case "worktree_chosen": {
        const item = [...this.items]
          .reverse()
          .find((i): i is ApprovalItem => i.type === "approval" && i.approval === "worktree");
        if (item && !item.answered) this.markChosen(item, event.choice);
        break;
      }

      case "worktree_changed":
        this.push({ type: "note", text: worktreeNote(event.status, event.branch) });
        break;

      case "prs_changed":
        this.prs = event.prs;
        break;

      case "approval_cancelled": {
        const item = this.items.find(
          (i): i is ApprovalItem => i.type === "approval" && i.requestId === event.request_id,
        );
        if (item && !item.answered) this.cancel(item);
        break;
      }

      // The core's record of an answer. Live, the card already shows it.
      // The user edited a plan before approving it; it shows as they wrote it.
      case "plan_edited": {
        const item = this.items.find(
          (i): i is ApprovalItem => i.type === "approval" && i.requestId === event.request_id,
        );
        if (item && item.input && typeof item.input === "object") {
          item.input = { ...(item.input as Record<string, unknown>), plan: event.plan };
        }
        break;
      }

      case "approval_answered": {
        const item = this.items.find(
          (i): i is ApprovalItem => i.type === "approval" && i.requestId === event.request_id,
        );
        if (item && !item.answered) {
          item.answered = event.decision;
          item.answers = event.answers ?? undefined;
          if (event.decision === "deny") this.markDeclined(item);
        }
        break;
      }

      case "interrupted":
        this.stopping = true;
        break;

      case "turn_ended":
        this.flushText();
        this.compacting = false;
        // A turn that went through means the login works now.
        if (event.ok) {
          for (const item of this.items) {
            if (item.type === "signin" && item.state !== "done") item.state = "done";
          }
        }
        // A stopped turn can leave a tool without a result, and nothing can be
        // waiting for an answer once the turn is over.
        this.settle();
        if (this.stopping) this.push({ type: "note", text: "stopped" });
        this.stopping = false;
        // Claude may have written the notes or a document some other way (a
        // shell command, Cursor's tools).
        void this.refreshNotes();
        for (const doc of this.docs) if (!doc.closed || this.isChatDoc(doc.path)) void this.refreshDoc(doc.path);
        this.onTurnEnd?.();
        break;

      case "error":
        this.push({ type: "error", text: event.message });
        break;

      case "compacting":
        this.compacting = event.active;
        break;

      case "title_changed":
        this.title = event.title;
        break;

      // Claude or the user opened, moved or closed a page. The panel isn't
      // switched for Claude; the chat's page line (see tool_result) is how to look.
      case "browser_tabs":
        if (!event.tabs.length) {
          this.leaveBrowser();
          break;
        }
        this.browserTabs = event.tabs;
        this.browserShownTab = event.shown;
        this.browserOn = true;
        break;

      // Earlier items stay on screen; Claude now sees a summary of them.
      case "compacted":
        this.push({ type: "note", text: event.auto ? "compacted automatically" : "compacted" });
        break;

      case "sign_in_needed":
        this.push({ type: "signin", state: "ready" });
        break;

      // Live, the message is already on screen from when it was sent.
      case "user_message":
        if (this.replaying) {
          this.push({
            type: "user",
            text: event.text,
            messageId: event.id ?? undefined,
            queued: !!event.id,
            images: event.images?.length ? event.images : undefined,
          });
        }
        break;

      // Claude took the message in. One sent mid-turn was waiting at the end
      // of the chat; it now belongs after whatever Claude did before reading it.
      case "message_seen": {
        const at = this.items.findIndex((i) => i.type === "user" && i.messageId === event.id);
        if (at < 0) break;
        (this.items[at] as UserItem).queued = false;
        if (at !== this.items.length - 1) this.items.push(...this.items.splice(at, 1));
        break;
      }
    }
  }

  /** Sends a message, with the draft's images whose `[image n]` word is in it. */
  /** `from` is where the images come from: the draft's, or a sent message's when it is sent again. */
  async send(text: string, from: ImageRef[] = this.draftImages): Promise<void> {
    let message = text.trim();
    if (!message) return;
    // `/btw` goes to the side chat, never to Claude Code (which can't run it headless).
    const btw = this.owner ? null : message.match(/^\/btw(?:\s+([\s\S]*))?$/);
    if (btw && this.agent === CURSOR) {
      this.push({ type: "error", text: "Side chats work in Claude Code chats only." });
      return;
    }
    if (btw) {
      const question = (btw[1] ?? "").trim();
      if (question) await this.askSide(question);
      else this.openSide();
      return;
    }
    // A fresh send: the draft's file words become name-and-path links.
    if (from === this.draftImages) {
      message = toLinks(message, this.draftFiles);
      this.draftFiles = [];
    }
    const messageId = crypto.randomUUID();
    const images: ImageRef[] = [];
    for (const [, n] of message.matchAll(IMAGE_WORD)) {
      const image = from.find((i) => i.n === Number(n));
      if (image && !images.includes(image)) images.push(image);
    }
    if (from === this.draftImages) this.draftImages = [];
    this.push({ type: "user", text: message, messageId, queued: this.busy, images: images.length ? images : undefined });
    this.stopping = false;
    if (this.turnStartedAt === null) this.turnStartedAt = Date.now();
    try {
      const owner = this.owner;
      if (owner) {
        // Pictures are for the main chat; a side chat gets the words only.
        await sideSend(this.sessionId, owner.projectPath.trim(), owner.modelChoice, owner.effortChoice, message, messageId);
      } else {
        await sendMessage(
          this.sessionId,
          this.agent,
          this.projectPath.trim(),
          this.mode,
          this.modelChoice,
          this.effortChoice,
          message,
          messageId,
          images.map((i) => i.n),
          this.appNotes,
        );
        this.appNotes = [];
        this.onSent?.();
      }
    } catch (e) {
      this.turnStartedAt = null;
      this.push({ type: "error", text: String(e) });
    }
  }

  /**
   * Runs Claude Code's sign-in, then sends the last message again. The turn
   * that failed never reached the model, so nothing is repeated.
   */
  async signIn(item: SignInItem): Promise<void> {
    item.state = "working";
    item.error = undefined;
    try {
      await signIn(this.agent);
    } catch (e) {
      item.state = "failed";
      item.error = String(e);
      return;
    }
    item.state = "done";
    // The running process read the login when it started and won't see the
    // new one. Asked to close, it is replaced by a fresh one on the next send.
    await closeSession(this.sessionId);
    const last = [...this.items].reverse().find((i): i is UserItem => i.type === "user");
    if (last) await this.send(last.text, last.images ?? []);
  }

  async answer(
    item: ApprovalItem,
    decision: Decision,
    message?: string,
    answers?: Record<string, string>,
    plan?: string,
  ): Promise<void> {
    try {
      await answerApproval(this.sessionId, item.requestId, decision, message, answers, plan);
      if (plan !== undefined && item.input && typeof item.input === "object") {
        item.input = { ...(item.input as Record<string, unknown>), plan };
      }
      if (item === this.plan) this.planDraft = null;
      item.answered = decision;
      item.answers = answers;
      if (decision === "deny") this.markDeclined(item);
    } catch (e) {
      this.push({ type: "error", text: String(e) });
    }
  }

  /** The answer to the worktree question. */
  async chooseWorktree(item: ApprovalItem, choice: WorktreeChoice): Promise<void> {
    try {
      await chooseWorktree(this.sessionId, item.requestId, choice);
      if (!item.answered) this.markChosen(item, choice);
    } catch (e) {
      this.push({ type: "error", text: String(e) });
    }
  }

  private markChosen(item: ApprovalItem, choice: WorktreeChoice): void {
    item.answered = "allow";
    item.choice = choice;
    // Either way Claude Code's own worktree tool doesn't run.
    this.markDeclined(item);
  }

  private cancel(item: ApprovalItem): void {
    item.cancelled = true;
    this.markDeclined(item);
  }

  /** The tool call behind a denied or cancelled approval never ran; say so, don't call it a failure. */
  private markDeclined(item: ApprovalItem): void {
    const tool = this.items.find((i): i is ToolItem => i.type === "tool" && i.toolId === item.toolId);
    if (tool) tool.declined = true;
  }

  // ---- Side chats (one per tab) ----

  /** Starts an empty side chat in a new tab and shows it. It copies this chat on its first question. */
  newSide(): Chat {
    // Random, not counted: a reloaded window must not reuse an id the core may still know.
    const side = new Chat(`${this.sessionId}#side-${crypto.randomUUID().slice(0, 8)}`, this);
    side.projectPath = this.projectPath;
    side.workPath = this.workPath;
    this.sides.push(side);
    this.sideShown = side.sessionId;
    this.panelView = "side";
    this.panelOpen = true;
    // Read back so the caller holds the reactive version.
    return this.sides.at(-1)!;
  }

  /** Opens the side chat view on the tab on show, starting one if there are none. */
  openSide(): Chat {
    const side = this.side;
    if (!side) return this.newSide();
    this.sideShown = side.sessionId;
    this.panelView = "side";
    this.panelOpen = true;
    return side;
  }

  pickSide(id: string): void {
    this.sideShown = id;
  }

  /** The tab before or after the one on show, wrapping round. */
  stepSide(by: number): void {
    const n = this.sides.length;
    if (n < 2) return;
    const at = this.sides.findIndex((s) => s.sessionId === this.side?.sessionId);
    this.sideShown = this.sides[(at + by + n) % n].sessionId;
  }

  /** Asks the side chat on show something, starting one if there are none. */
  async askSide(question: string): Promise<void> {
    await this.openSide().send(question);
  }

  /**
   * Ends a side chat (the one on show if none is named); it is forgotten. The
   * tab beside it takes its place; after the last, the panel goes to another
   * view, or closes.
   */
  async endSide(id: string | undefined = this.side?.sessionId): Promise<void> {
    const at = this.sides.findIndex((s) => s.sessionId === id);
    if (at < 0 || !id) return;
    const wasShown = this.side?.sessionId === id;
    this.sides.splice(at, 1);
    if (wasShown) this.sideShown = this.sides[Math.min(at, this.sides.length - 1)]?.sessionId ?? null;
    if (!this.sides.length && this.panelView === "side") {
      this.panelView = this.browserOn ? "browser" : this.terminalOn ? "terminal" : "docs";
      this.panelOpen = this.panelOpen && this.hasPanelWork;
    }
    try {
      await sideEnd(id);
    } catch {
      // Nothing running to end.
    }
  }

  /** Ends every side chat, when this chat is dropped from the window. */
  endAllSides(): void {
    for (const side of this.sides) void sideEnd(side.sessionId).catch(() => {});
    this.sides = [];
    this.sideShown = null;
  }

  /** Shows a view in the panel, opening the panel if it was hidden. Hiding it is the panel toggle's job. */
  showView(view: PanelView): void {
    if (view === "side") this.openSide();
    else if (view === "browser") this.openBrowser();
    else if (view === "terminal") this.openTerminal();
    else if (view === "info") this.openInfo();
    else this.openPanel();
  }

  async stop(): Promise<void> {
    try {
      this.stopping = true;
      if (this.owner) {
        await sideInterrupt(this.sessionId);
        return;
      }
      await interrupt(this.sessionId);
    } catch (e) {
      this.stopping = false;
      this.push({ type: "error", text: String(e) });
    }
  }

  /** Moves a chat that hasn't started yet to another agent. Its mode follows (plan stays plan). */
  setAgent(agent: AgentId, savedMode: string): void {
    if (this.started || agent === this.agent) return;
    this.agent = agent;
    this.mode = modeFor(agent, this.mode, savedMode);
    this.effortChoice = "default";
  }

  /** Switch model. A running process is told now; otherwise it applies when one starts. */
  async changeModel(choice: string): Promise<void> {
    this.modelChoice = choice;
    if (this.status === "none" || this.status === "exited") return;
    try {
      await setModel(this.sessionId, choice);
    } catch {
      // No live process: the model is passed along with the next message instead.
    }
  }

  /** Pick an effort level for this chat only. A running process is told now; otherwise it applies when one starts. */
  async changeEffort(effort: string): Promise<void> {
    this.effortChoice = effort;
    if (this.status === "none") return;
    try {
      // Saved even with no live process, so a resumed chat keeps it.
      await setEffort(this.sessionId, effort);
    } catch {
      // No live process: the level is passed along with the next message instead.
    }
  }

  /** Switch mode. A running process is told now; otherwise it applies when one starts. */
  async changeMode(mode: Mode): Promise<void> {
    this.mode = mode;
    if (this.status === "none" || this.status === "exited") return;
    try {
      await setMode(this.sessionId, mode);
    } catch {
      // No live process: the mode is passed along with the next message instead.
    }
  }
}
