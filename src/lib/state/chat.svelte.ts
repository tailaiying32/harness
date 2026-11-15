// One chat's state: what's on screen, and how agent events change it.

import {
  answerApproval,
  browserClose,
  browserCurrent,
  closeSession,
  chooseWorktree,
  interrupt,
  readDoc,
  sendMessage,
  sideEnd,
  sideInterrupt,
  sideSend,
  signIn,
  setMode,
  setEffort,
  setModel,
  type AgentEvent,
  type ApprovalKind,
  type BrowserInfo,
  type Decision,
  type ImageRef,
  type Mode,
  type PrInfo,
  type SessionRow,
  type SessionState,
  type WorktreeChoice,
  type WorktreeStatus,
} from "../ipc";
import { docPath, planOf, showsOutput, textAfter, type ToolCall } from "../tools";

/** What the side panel shows. */
export type PanelView = "docs" | "side" | "browser";

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

/** The `[image n]` words in a text, in order. */
export const IMAGE_WORD = /\[image (\d+)\]/g;

/** A text in pieces: plain text, and the words of the given images. */
export function withImages(text: string, images: ImageRef[] = []): { text: string; image?: ImageRef }[] {
  if (!images.length) return [{ text }];
  const out: { text: string; image?: ImageRef }[] = [];
  let at = 0;
  for (const match of text.matchAll(IMAGE_WORD)) {
    const image = images.find((i) => i.n === Number(match[1]));
    if (!image) continue;
    out.push({ text: text.slice(at, match.index) }, { text: match[0], image });
    at = match.index + match[0].length;
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

export type Item = UserItem | TextItem | ThinkingItem | ToolItem | ApprovalItem | ErrorItem | NoteItem | SignInItem;

type NewItem =
  | Omit<UserItem, "id">
  | Omit<TextItem, "id">
  | Omit<ThinkingItem, "id">
  | Omit<ToolItem, "id">
  | Omit<ApprovalItem, "id">
  | Omit<ErrorItem, "id">
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
    chat.mode = row.mode as Mode;
    chat.modelChoice = row.model;
    chat.effortChoice = row.effort ?? "default";
    chat.stopping = false;
    if (!live) {
      chat.settle();
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
  items = $state<Item[]>([]);
  status = $state<SessionState | "none">("none");
  /** The model Claude Code reports it is using. Known after the first turn starts. */
  model = $state<string | null>(null);
  /** The model the user picked: an alias, a full name, or "default" for Claude Code's own default. */
  modelChoice = $state("default");
  /** The effort level picked for this chat, or "default" for Claude Code's default for the model. */
  effortChoice = $state("default");
  mode = $state<Mode>("default");
  usage = $state<{ window: string | null; utilization: number } | null>(null);
  projectPath = $state("");
  /** Where the agent works when the chat has a worktree; null otherwise. */
  workPath = $state<string | null>(null);
  /** The worktree's branch, and its pull request as last seen. */
  branch = $state<string | null>(null);
  prs = $state<PrInfo[]>([]);
  /** The folder paths are shown relative to: the worktree if there is one. */
  get root(): string {
    return this.workPath ?? this.projectPath;
  }
  /** Takes the saved row's worktree details (they change outside the chat's events). */
  syncRow(row: SessionRow): void {
    this.workPath = row.work_path;
    this.branch = row.branch;
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
  /** The side chat (`/btw`), while one is open. It is never saved. */
  side = $state<Chat | null>(null);
  /** The panel has something to show. */
  get panelHasContent(): boolean {
    return this.tabCount > 0 || this.side !== null || this.browserOn;
  }

  // ---- The browser (a localhost page beside the chat; Claude can drive it) ----
  /** The browser view is open, with or without a page yet. */
  browserOn = $state(false);
  /** The page it shows, once there is one. Lives in the core, not saved. */
  browser = $state<BrowserInfo | null>(null);
  /** Claude opened or changed the page while the browser wasn't on show. */
  browserUnseen = $state(false);

  /** The browser is what the panel shows right now. */
  get browserShown(): boolean {
    return this.panelOpen && this.panelView === "browser";
  }

  /** Opens the browser view: the page if there is one, an empty address line if not. */
  openBrowser(): void {
    this.browserOn = true;
    this.browserUnseen = false;
    this.panelView = "browser";
    this.panelOpen = true;
  }

  /** Closes the browser and its page. The panel goes back to another view, or closes. */
  async closeBrowser(): Promise<void> {
    const hadPage = this.browser !== null;
    this.leaveBrowser();
    if (hadPage) {
      try {
        await browserClose(this.sessionId);
      } catch {
        // Already gone.
      }
    }
  }

  private leaveBrowser(): void {
    this.browserOn = false;
    this.browser = null;
    this.browserUnseen = false;
    if (this.panelView !== "browser") return;
    this.panelView = this.side ? "side" : "docs";
    this.panelOpen = this.panelOpen && this.panelHasContent;
  }

  /** A chat opened again after a reload of the window: pick up its live page. */
  async loadBrowser(): Promise<void> {
    try {
      const info = await browserCurrent(this.sessionId);
      if (info) {
        this.browser = info;
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

  get tabCount(): number {
    return (this.planTab ? 1 : 0) + this.openDocs.length;
  }

  /** The tab on show: the chosen one if it still exists, else the first. */
  get activeTab(): string {
    if (this.panelTab === PLAN_TAB && this.planTab) return PLAN_TAB;
    if (this.openDocs.some((doc) => doc.path === this.panelTab)) return this.panelTab;
    return this.planTab ? PLAN_TAB : (this.openDocs[0]?.path ?? "");
  }

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

  /** Closes one tab. A plan still waiting for an answer can't be closed. */
  closeTab(tab: string): void {
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
    const doc = this.docs.find((d) => d.path === path);
    const text = textAfter(tool, doc?.text);
    if (text === null) return;
    if (doc) doc.text = text;
    else this.docs.push({ path, text });
  }

  /** Called once each time a turn ends. Used by the self-test. */
  onTurnEnd: (() => void) | null = null;
  /** Called after a message has been handed to the core, which saves the chat. */
  onSent: (() => void) | null = null;
  /** Answer every approval this way. Used by the self-test only. */
  autoAnswer: Decision | null = null;
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
          void this.answer(item, answers ? "allow" : this.autoAnswer, undefined, answers);
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

      // Claude or the user opened or moved the page. The panel isn't switched
      // for Claude; the browser view gets a mark instead, like a plan in side chat.
      case "browser_state":
        this.browser = { url: event.url, title: event.title, loading: event.loading };
        this.browserOn = true;
        if (!this.browserShown) this.browserUnseen = true;
        break;

      case "browser_closed":
        this.leaveBrowser();
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
    const message = text.trim();
    if (!message) return;
    // `/btw` goes to the side chat, never to Claude Code (which can't run it headless).
    const btw = this.owner ? null : message.match(/^\/btw(?:\s+([\s\S]*))?$/);
    if (btw) {
      const question = (btw[1] ?? "").trim();
      if (question) await this.askSide(question);
      else this.openSide();
      return;
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
        await sideSend(owner.sessionId, owner.projectPath.trim(), owner.modelChoice, owner.effortChoice, message, messageId);
      } else {
        await sendMessage(
          this.sessionId,
          this.projectPath.trim(),
          this.mode,
          this.modelChoice,
          this.effortChoice,
          message,
          messageId,
          images.map((i) => i.n),
        );
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
      await signIn();
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
  ): Promise<void> {
    try {
      await answerApproval(this.sessionId, item.requestId, decision, message, answers);
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

  // ---- The side chat ----

  /** Opens the side chat in the panel, starting an empty one if there is none. */
  openSide(): Chat {
    if (!this.side) {
      const side = new Chat(`${this.sessionId}#side`, this);
      side.projectPath = this.projectPath;
      side.workPath = this.workPath;
      this.side = side;
    }
    this.panelView = "side";
    this.panelOpen = true;
    // Read back so the caller holds the reactive version.
    return this.side!;
  }

  /** Asks the side chat something, opening it first. */
  async askSide(question: string): Promise<void> {
    await this.openSide().send(question);
  }

  /** Ends the side chat; it is forgotten. The panel goes back to docs, or closes. */
  async endSide(): Promise<void> {
    this.side = null;
    this.panelView = "docs";
    this.panelOpen = this.tabCount > 0;
    try {
      await sideEnd(this.sessionId);
    } catch {
      // Nothing running to end.
    }
  }

  /** Shows a view in the panel, opening the panel if it was hidden. Hiding it is the panel toggle's job. */
  showView(view: PanelView): void {
    if (view === "side") this.openSide();
    else if (view === "browser") this.openBrowser();
    else this.openPanel();
  }

  async stop(): Promise<void> {
    try {
      this.stopping = true;
      if (this.owner) {
        await sideInterrupt(this.owner.sessionId);
        return;
      }
      await interrupt(this.sessionId);
    } catch (e) {
      this.stopping = false;
      this.push({ type: "error", text: String(e) });
    }
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
