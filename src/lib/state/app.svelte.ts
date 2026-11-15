// The whole window's state: the sidebar's projects and chats, which chat is
// open, and what each chat's process is doing.

import {
  deleteSession,
  emptyTrash,
  listTrash,
  loadSession,
  renameSession,
  restoreChat,
  sidebar,
  type Envelope,
  type Mode,
  type ProjectRow,
  type SessionRow,
  type SessionState,
} from "../ipc";
import { loadPrefs } from "../themes";
import { Chat } from "./chat.svelte";

/** A new chat starts in the mode the user last picked (auto until they pick one). */
function fresh(): Chat {
  const chat = new Chat();
  chat.mode = loadPrefs().mode as Mode;
  return chat;
}

/** What the sidebar shows next to a chat's title. */
export type Dot = "working" | "waiting" | "unseen" | "idle";

export class AppState {
  /** Every project that has chats, most recently used first. */
  projects = $state<ProjectRow[]>([]);
  /** Chats in the trash, most recently trashed first. */
  trash = $state<SessionRow[]>([]);
  /** The chat on screen. A new one isn't in the sidebar until its first message. */
  current = $state<Chat>(fresh());
  /** What each chat's process is doing, for chats used since the app started. */
  statuses = $state<Record<string, SessionState>>({});
  /** Chats that finished a turn, or need an answer, while another was on screen. */
  unseen = $state<Record<string, boolean>>({});
  /** The latest usage-limit reading from any chat. Limits are per account, so the newest one wins. */
  usage = $state<{ window: string | null; utilization: number } | null>(null);
  /** A problem opening or deleting a chat, shown in the sidebar. */
  problem = $state("");

  /** Goes up each time a new chat is asked for, so the message box takes the cursor. */
  focusBox = $state(0);

  /** The user's home folder; where a first chat starts. */
  home = $state("");
  /** The model new chats start on. */
  defaultModel = "default";

  // Chats opened since the app started stay in memory, so switching back is
  // instant and a chat that is still working keeps receiving its events.
  private chats = new Map<string, Chat>();

  constructor() {
    this.adopt(this.current);
  }

  private adopt(chat: Chat): void {
    this.chats.set(chat.sessionId, chat);
    chat.onSent = () => void this.refresh();
  }

  /** Every event from the core comes through here. */
  route(envelope: Envelope): void {
    const { session_id: id, event } = envelope;
    // A side chat's events belong to it alone: no sidebar mark, no usage reading.
    if (id.endsWith("#side")) {
      this.chats.get(id.slice(0, -"#side".length))?.side?.handle(event);
      return;
    }
    if (event.kind === "status") this.statuses[id] = event.state;
    if (event.kind === "usage_update") this.usage = { window: event.window, utilization: event.utilization };
    // These change the chat's row (title, worktree, pull request).
    if (
      event.kind === "title_changed" ||
      event.kind === "worktree_changed" ||
      event.kind === "worktree_chosen" ||
      event.kind === "prs_changed"
    ) {
      void this.refresh();
    }
    this.chats.get(id)?.handle(event);
    const needsYou = event.kind === "approval_request" || event.kind === "worktree_ask";
    if (id !== this.current.sessionId && (event.kind === "turn_ended" || needsYou)) {
      this.unseen[id] = true;
    }
  }

  dot(sessionId: string): Dot {
    const status = this.statuses[sessionId];
    if (status === "waiting_approval") return "waiting";
    if (status === "starting" || status === "working") return "working";
    return this.unseen[sessionId] ? "unseen" : "idle";
  }

  async refresh(): Promise<void> {
    try {
      [this.projects, this.trash] = await Promise.all([sidebar(), listTrash()]);
    } catch (e) {
      this.problem = String(e);
      return;
    }
    // Open chats follow their rows: a worktree made, put away or merged, a PR.
    for (const row of [...this.projects.flatMap((p) => p.sessions), ...this.trash]) {
      this.chats.get(row.id)?.syncRow(row);
    }
  }

  /** Starts an empty chat in `folder` (default: the open chat's folder). */
  newChat(folder?: string): void {
    this.focusBox++;
    // An untouched new chat is reused rather than piling up.
    if (!this.current.started) {
      if (folder) this.current.projectPath = folder;
      return;
    }
    const chat = fresh();
    chat.projectPath = folder ?? (this.current.projectPath || this.home);
    chat.modelChoice = this.defaultModel;
    this.adopt(chat);
    this.current = chat;
  }

  async open(sessionId: string): Promise<void> {
    if (sessionId === this.current.sessionId) return;
    this.problem = "";
    let chat = this.chats.get(sessionId);
    if (!chat) {
      try {
        const saved = await loadSession(sessionId);
        chat = Chat.fromSaved(saved.session, saved.events, saved.live);
        // Claude may have opened a page while the chat wasn't loaded here.
        void chat.loadBrowser();
      } catch (e) {
        this.problem = String(e);
        void this.refresh();
        return;
      }
      this.adopt(chat);
    }
    if (!this.current.started) this.chats.delete(this.current.sessionId);
    this.current = chat;
    delete this.unseen[sessionId];
  }

  /** Gives a chat a new name; an empty name changes nothing. */
  async rename(sessionId: string, title: string): Promise<void> {
    if (!title.trim()) return;
    this.problem = "";
    try {
      await renameSession(sessionId, title);
    } catch (e) {
      this.problem = String(e);
      return;
    }
    await this.refresh();
  }

  /** Takes a chat out of the trash. */
  async restore(sessionId: string): Promise<void> {
    this.problem = "";
    try {
      await restoreChat(sessionId);
    } catch (e) {
      this.problem = String(e);
    }
    await this.refresh();
  }

  /** Deletes every chat in the trash for good. */
  async emptyTrash(): Promise<void> {
    this.problem = "";
    try {
      await emptyTrash();
    } catch (e) {
      this.problem = String(e);
    }
    for (const row of this.trash) {
      if (row.id !== this.current.sessionId) this.chats.delete(row.id);
    }
    if (this.trash.some((row) => row.id === this.current.sessionId)) this.newChat();
    await this.refresh();
  }

  /** Moves a chat to the trash. */
  async remove(sessionId: string): Promise<void> {
    this.problem = "";
    try {
      await deleteSession(sessionId);
    } catch (e) {
      this.problem = String(e);
      return;
    }
    const folder = this.chats.get(sessionId)?.projectPath;
    this.chats.delete(sessionId);
    delete this.statuses[sessionId];
    delete this.unseen[sessionId];
    if (this.current.sessionId === sessionId) {
      const chat = fresh();
      chat.projectPath = folder || this.home;
      chat.modelChoice = this.defaultModel;
      this.adopt(chat);
      this.current = chat;
    }
    await this.refresh();
  }
}
