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
  type SessionState,
} from "../ipc";
import type { ToolCall } from "../tools";

export type UserItem = { id: number; type: "user"; text: string };
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
  answered?: Decision;
};
export type ErrorItem = { id: number; type: "error"; text: string };

export type Item = UserItem | TextItem | ThinkingItem | ToolItem | ApprovalItem | ErrorItem;

type NewItem =
  | Omit<UserItem, "id">
  | Omit<TextItem, "id">
  | Omit<ThinkingItem, "id">
  | Omit<ToolItem, "id">
  | Omit<ApprovalItem, "id">
  | Omit<ErrorItem, "id">;

function resultText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => (part && typeof part === "object" && "text" in part ? String(part.text) : ""))
      .join("\n");
  }
  return content == null ? "" : JSON.stringify(content);
}

export class Chat {
  readonly sessionId = crypto.randomUUID();

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

  /** Called once each time a turn ends. Used by the self-test. */
  onTurnEnd: (() => void) | null = null;
  /** Answer every approval with "allow". Used by the self-test only. */
  autoAllow = false;

  private nextId = 0;
  // Live text arrives in many small pieces. They are collected here and applied
  // once per frame, so a fast stream costs one update per frame, not per piece.
  private pendingText = new Map<string, { messageId: string; text: string }>();
  private drafts = new Map<string, TextItem>();
  private frame = 0;

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
          tool.result = resultText(event.content);
          tool.isError = event.is_error;
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
        });
        if (this.autoAllow) void this.answer(item, "allow");
        break;
      }

      case "turn_ended":
        this.flushText();
        for (const item of this.items) {
          if (item.type === "text") item.streaming = false;
          // A turn that was stopped can leave a tool without a result.
          if (item.type === "tool" && item.result === undefined) item.result = "";
        }
        this.drafts.clear();
        this.turnStartedAt = null;
        this.onTurnEnd?.();
        break;

      case "error":
        this.push({ type: "error", text: event.message });
        break;

      case "user_message":
        break;
    }
  }

  async send(text: string): Promise<void> {
    const message = text.trim();
    if (!message) return;
    this.push({ type: "user", text: message });
    if (this.turnStartedAt === null) this.turnStartedAt = Date.now();
    try {
      await sendMessage(this.sessionId, this.projectPath.trim(), this.mode, this.modelChoice, message);
    } catch (e) {
      this.turnStartedAt = null;
      this.push({ type: "error", text: String(e) });
    }
  }

  async answer(item: ApprovalItem, decision: Decision, message?: string): Promise<void> {
    try {
      await answerApproval(this.sessionId, item.requestId, decision, message);
      item.answered = decision;
    } catch (e) {
      this.push({ type: "error", text: String(e) });
    }
  }

  async stop(): Promise<void> {
    try {
      await interrupt(this.sessionId);
    } catch (e) {
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
