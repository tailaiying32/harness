// The whole window's state: the sidebar's projects and chats, which chat is
// open, and what each chat's process is doing.

import {
  deleteSession,
  loadSession,
  renameSession,
  sidebar,
  type Envelope,
  type ProjectRow,
  type SessionState,
} from "../ipc";
import { Chat } from "./chat.svelte";

/** What the sidebar shows next to a chat's title. */
export type Dot = "working" | "waiting" | "unseen" | "idle";

export class AppState {
  /** Every project that has chats, most recently used first. */
  projects = $state<ProjectRow[]>([]);
  /** The chat on screen. A new one isn't in the sidebar until its first message. */
  current = $state<Chat>(new Chat());
  /** What each chat's process is doing, for chats used since the app started. */
  statuses = $state<Record<string, SessionState>>({});
  /** Chats that finished a turn, or need an answer, while another was on screen. */
  unseen = $state<Record<string, boolean>>({});
  /** A problem opening or deleting a chat, shown in the sidebar. */
  problem = $state("");

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
    if (event.kind === "status") this.statuses[id] = event.state;
    this.chats.get(id)?.handle(event);
    if (id !== this.current.sessionId && (event.kind === "turn_ended" || event.kind === "approval_request")) {
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
      this.projects = await sidebar();
    } catch (e) {
      this.problem = String(e);
    }
  }

  /** Starts an empty chat in `folder` (default: the open chat's folder). */
  newChat(folder?: string): void {
    // An untouched new chat is reused rather than piling up.
    if (!this.current.started) {
      if (folder) this.current.projectPath = folder;
      return;
    }
    const chat = new Chat();
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

  /** Deletes a chat and its history for good. */
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
      const chat = new Chat();
      chat.projectPath = folder || this.home;
      chat.modelChoice = this.defaultModel;
      this.adopt(chat);
      this.current = chat;
    }
    await this.refresh();
  }
}
