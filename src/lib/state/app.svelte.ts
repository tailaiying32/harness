// The whole window's state: the sidebar's projects and chats, which chat is
// open, and what each chat's process is doing.

import { terminals } from "./terminals.svelte";
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
import { loadCachedModels, modeFor, startingModel } from "../models";
import { loadPrefs, savePrefs } from "../themes";
import { Chat } from "./chat.svelte";
import { CLAUDE, type AgentId } from "../ipc";

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
  /** False until the first sidebar load; before it, `projects` is empty, not "no projects". */
  loaded = $state(false);
  /** Chats in the trash, most recently trashed first. */
  trash = $state<SessionRow[]>([]);
  /** The chat on screen. A new one isn't in the sidebar until its first message. */
  current = $state<Chat>(fresh());
  /** What each chat's process is doing, for chats used since the app started. */
  statuses = $state<Record<string, SessionState>>({});
  /** Chats with agents running in the background, which count as working. */
  helpers = $state<Record<string, boolean>>({});
  /** Chats that finished a turn, or need an answer, while another was on screen. */
  unseen = $state<Record<string, boolean>>({});
  /** The newest reading of each usage limit, by agent and then limit name.
   *  Limits are per account, so any chat's reading counts. Saved, because an
   *  agent reports a limit only after a reply from a model it applies to. */
  limits = $state<Limits>(loadLimits());
  /** A problem opening or deleting a chat, shown in the sidebar. */
  problem = $state("");

  /** Goes up each time a new chat is asked for, so the message box takes the cursor. */
  focusBox = $state(0);

  /** The user's home folder; where a first chat starts. */
  home = $state("");
  /** The model new chats start on in a project with no pick of its own, and its agent. */
  defaultModel = "default";
  defaultAgent: AgentId = "claude-code";
  /** The agents installed here; empty until checked. */
  agents: AgentId[] = [];

  /** A new chat's agent, model and mode: the last ones picked (the model in its project). */
  startChat(chat: Chat): void {
    const pick = this.pickFor(chat.projectPath);
    chat.agent = pick.agent;
    chat.modelChoice = pick.model;
    chat.mode = modeFor(chat.agent, loadPrefs().mode, loadPrefs().mode);
  }

  /** The model last picked in `folder`, if its agent is installed; else the last one picked anywhere. */
  private pickFor(folder: string): { agent: AgentId; model: string } {
    const saved = loadPrefs().projectModels[folder.trim()];
    const agent = saved?.agent as AgentId | undefined;
    if (saved && agent && (!this.agents.length || this.agents.includes(agent))) {
      return { agent, model: startingModel(saved.model, agent, loadCachedModels()) };
    }
    return { agent: this.defaultAgent, model: this.defaultModel };
  }

  /** A new chat moved to another folder takes that project's model, unless one was picked in it. */
  followProject(chat: Chat): void {
    if (chat.started || chat.modelPicked) return;
    const pick = this.pickFor(chat.projectPath);
    chat.setAgent(pick.agent, loadPrefs().mode);
    chat.modelChoice = pick.model;
  }

  /** A model picked in a chat: kept for its project, and for projects with no pick yet. */
  rememberModel(chat: Chat, agent: AgentId, model: string): void {
    chat.modelPicked = true;
    this.defaultAgent = agent;
    this.defaultModel = model;
    const folder = chat.projectPath.trim();
    const projectModels = { ...loadPrefs().projectModels };
    if (folder) projectModels[folder] = { agent, model };
    savePrefs({ model, agent, projectModels });
  }

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
    // The terminal's tabs belong to a project, not a chat.
    if (event.kind === "terminal_tabs") {
      terminals.set(event.project, event.tabs);
      return;
    }
    if (event.kind === "close_key") {
      terminals.closeKey(this.current.projectPath.trim());
      return;
    }
    // A side chat's events belong to it alone: no sidebar mark, no usage reading.
    if (id.endsWith("#side")) {
      this.chats.get(id.slice(0, -"#side".length))?.side?.handle(event);
      return;
    }
    if (event.kind === "status") this.statuses[id] = event.state;
    if (event.kind === "background_work") this.helpers[id] = event.agents > 0;
    // A process that ended took its background agents with it.
    if (event.kind === "status" && event.state === "exited") delete this.helpers[id];
    if (event.kind === "usage_update" && event.window) {
      const agent = this.chats.get(id)?.agent ?? CLAUDE;
      const reading = { utilization: event.utilization, resetsAt: event.resets_at };
      this.limits = { ...this.limits, [agent]: { ...this.limits[agent], [event.window]: reading } };
      localStorage.setItem(LIMITS_KEY, JSON.stringify(this.limits));
    }
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
    if (status === "starting" || status === "working" || this.helpers[sessionId]) return "working";
    return this.unseen[sessionId] ? "unseen" : "idle";
  }

  async refresh(): Promise<void> {
    try {
      [this.projects, this.trash] = await Promise.all([sidebar(), listTrash()]);
      this.loaded = true;
    } catch (e) {
      this.problem = String(e);
      return;
    }
    // Open chats follow their rows: a worktree made, put away or merged, a PR.
    for (const row of [...this.projects.flatMap((p) => p.sessions), ...this.trash]) {
      this.chats.get(row.id)?.syncRow(row);
    }
  }

  /**
   * Starts a new chat in `folder` (default: the open chat's folder). A new chat
   * left there with something typed in it comes back instead of a blank one.
   */
  newChat(folder?: string): void {
    this.focusBox++;
    const left = this.current;
    if (!left.started && (!folder || sameFolder(left.projectPath, folder))) return;
    const target = folder ?? (left.projectPath || this.home);
    const unsent = this.unsentIn(target);
    // An untouched new chat moves rather than piling up.
    if (!left.started && !left.hasDraft && !unsent) {
      left.projectPath = target;
      return;
    }
    this.current = unsent ?? this.blankIn(target);
    this.leave(left);
  }

  /** A new chat in `folder`, not sent yet, with something typed in it. */
  private unsentIn(folder: string): Chat | undefined {
    for (const chat of this.chats.values()) {
      if (chat === this.current || chat.started || !chat.hasDraft) continue;
      if (sameFolder(chat.projectPath, folder)) return chat;
    }
    return undefined;
  }

  private blankIn(folder: string): Chat {
    const chat = fresh();
    chat.projectPath = folder;
    this.startChat(chat);
    this.adopt(chat);
    return chat;
  }

  /** Forgets a chat just switched away from if it holds nothing: never sent, nothing typed. */
  private leave(chat: Chat): void {
    if (!chat.started && !chat.hasDraft) this.chats.delete(chat.sessionId);
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
    const left = this.current;
    this.current = chat;
    this.leave(left);
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
    delete this.helpers[sessionId];
    delete this.unseen[sessionId];
    if (this.current.sessionId === sessionId) {
      const target = folder || this.home;
      this.current = this.unsentIn(target) ?? this.blankIn(target);
    }
    await this.refresh();
  }
}

function sameFolder(a: string, b: string): boolean {
  return a.trim() === b.trim();
}

/** One usage limit: how much is spent (0 to 1) and when it starts over (seconds since 1970). */
export type Limit = { utilization: number; resetsAt: number | null };
export type Limits = Partial<Record<AgentId, Record<string, Limit>>>;

const LIMITS_KEY = "harness.limits";

function loadLimits(): Limits {
  try {
    return JSON.parse(localStorage.getItem(LIMITS_KEY) ?? "{}") as Limits;
  } catch {
    return {};
  }
}
