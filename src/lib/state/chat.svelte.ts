// One chat's state: what's on screen, and how agent events change it.

import {
  answerApproval,
  interrupt,
  sendMessage,
  setMode,
  setModel,
  type AgentEvent,
  type ApprovalKind,
  type Decision,
  type Mode,
  type SessionRow,
  type SessionState,
} from "../ipc";
import { docPath, planOf, showsOutput, textAfter, type ToolCall } from "../tools";

export type UserItem = {
  id: number;
  type: "user";
  text: string;
  /** Names the message to the core, which says when Claude has taken it in. */
  messageId?: string;
  /** Sent while Claude was working, and not taken in yet. */
  queued?: boolean;
};
export type TextItem = { id: number; type: "text"; messageId: string; text: string; streaming: boolean };
export type ThinkingItem = { id: number; type: "thinking"; text: string };
export type ToolItem = { id: number; type: "tool" } & ToolCall;
export type ApprovalItem = {
  id: number;
  type: "approval";
  requestId: string;
  approval: ApprovalKind;
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
};
export type ErrorItem = { id: number; type: "error"; text: string };
/** A faint one-line remark from the app, e.g. "stopped". */
export type NoteItem = { id: number; type: "note"; text: string };

/** A markdown file the chat wrote, as a tab in the side panel. */
export type DocTab = { path: string; text: string; /** Its tab was closed; the text still follows edits. */ closed?: boolean };

/** The plan's tab in the side panel. Documents use their file path. */
export const PLAN_TAB = "plan";

export type Item = UserItem | TextItem | ThinkingItem | ToolItem | ApprovalItem | ErrorItem | NoteItem;

type NewItem =
  | Omit<UserItem, "id">
  | Omit<TextItem, "id">
  | Omit<ThinkingItem, "id">
  | Omit<ToolItem, "id">
  | Omit<ApprovalItem, "id">
  | Omit<ErrorItem, "id">
  | Omit<NoteItem, "id">;

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

  /** A new chat gets a fresh ID; a saved one keeps its own. */
  constructor(sessionId: string = crypto.randomUUID()) {
    this.sessionId = sessionId;
  }

  /**
   * Rebuilds a saved chat by replaying its history through the same code that
   * handles live events, so a reloaded chat looks exactly as it did.
   */
  static fromSaved(row: SessionRow, events: AgentEvent[], live: boolean): Chat {
    const chat = new Chat(row.id);
    chat.projectPath = row.project_path;
    chat.replaying = true;
    for (const event of events) chat.handle(event);
    chat.replaying = false;
    chat.mode = row.mode as Mode;
    chat.modelChoice = row.model;
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
  items = $state<Item[]>([]);
  status = $state<SessionState | "none">("none");
  /** The model Claude Code reports it is using. Known after the first turn starts. */
  model = $state<string | null>(null);
  /** The model the user picked: an alias, a full name, or "default" for Claude Code's own default. */
  modelChoice = $state("default");
  mode = $state<Mode>("default");
  usage = $state<{ window: string | null; utilization: number } | null>(null);
  projectPath = $state("");
  /** When the current turn began, for the elapsed-time counter. */
  turnStartedAt = $state<number | null>(null);

  // ---- The side panel ----
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

  /** Opens the panel; on `tab` if given, bringing it back if it was closed. */
  openPanel(tab?: string): void {
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
          // Claude is waiting on the user, so the plan is put in front of them.
          if (!this.replaying) this.openPanel(PLAN_TAB);
        }
        if (this.autoAnswer) {
          // A question has no "always"; it is answered with its first options.
          const answers = event.approval === "question" ? firstOptions(event.input) : undefined;
          void this.answer(item, answers ? "allow" : this.autoAnswer, undefined, answers);
        }
        break;
      }

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

      // Live, the message is already on screen from when it was sent.
      case "user_message":
        if (this.replaying) {
          this.push({ type: "user", text: event.text, messageId: event.id ?? undefined, queued: !!event.id });
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

  async send(text: string): Promise<void> {
    const message = text.trim();
    if (!message) return;
    const messageId = crypto.randomUUID();
    this.push({ type: "user", text: message, messageId, queued: this.busy });
    this.stopping = false;
    if (this.turnStartedAt === null) this.turnStartedAt = Date.now();
    try {
      await sendMessage(this.sessionId, this.projectPath.trim(), this.mode, this.modelChoice, message, messageId);
      this.onSent?.();
    } catch (e) {
      this.turnStartedAt = null;
      this.push({ type: "error", text: String(e) });
    }
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

  private cancel(item: ApprovalItem): void {
    item.cancelled = true;
    this.markDeclined(item);
  }

  /** The tool call behind a denied or cancelled approval never ran; say so, don't call it a failure. */
  private markDeclined(item: ApprovalItem): void {
    const tool = this.items.find((i): i is ToolItem => i.type === "tool" && i.toolId === item.toolId);
    if (tool) tool.declined = true;
  }

  async stop(): Promise<void> {
    try {
      this.stopping = true;
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
