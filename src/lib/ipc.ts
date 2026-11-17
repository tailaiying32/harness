// Typed wrappers over the core's commands and its one event channel.
// These types mirror src-tauri/src/events.rs.

import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

export type SessionState = "starting" | "working" | "waiting_approval" | "idle" | "exited";

export type Block =
  | { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "tool_call"; tool_id: string; name: string; input: unknown };

export type ApprovalKind = "tool" | "plan" | "question";
export type Decision = "allow" | "always_allow" | "deny";

/**
 * A chat's mode, as its agent names it. Claude Code: "default",
 * "acceptEdits", "plan", "auto" (it decides by itself which actions are safe
 * to run without asking). Cursor: "agent", "plan", "ask".
 */
export type Mode = string;

/** The agents a chat can run on. Mirrors `agent::CLAUDE` and `agent::CURSOR`. */
export type AgentId = "claude-code" | "cursor";
export const CLAUDE: AgentId = "claude-code";
export const CURSOR: AgentId = "cursor";
export const agentLabel = (agent: string) => (agent === CURSOR ? "Cursor" : "Claude Code");

/** A tab key from the Mac menu (or the keyboard elsewhere): Cmd+T, Cmd+W, Cmd+Shift+[ and ]. */
export type TabAction = "new" | "close" | "prev" | "next";

export type AgentEvent =
  | { kind: "session_started"; agent_session_id: string; model: string | null; mode: string | null }
  | { kind: "status"; state: SessionState }
  | { kind: "text_delta"; message_id: string; block_index: number; text: string }
  | { kind: "assistant_block"; message_id: string; block: Block; parent_tool_id: string | null }
  | { kind: "tool_result"; tool_id: string; content: unknown; is_error: boolean; parent_tool_id: string | null }
  | {
      kind: "approval_request";
      request_id: string;
      approval: ApprovalKind;
      tool_name: string;
      title: string;
      input: unknown;
      tool_id: string | null;
      /** Button text for "always allow". Null when there is nothing to remember. */
      always_label: string | null;
    }
  | { kind: "approval_cancelled"; request_id: string }
  | { kind: "approval_answered"; request_id: string; decision: Decision; answers: Record<string, string> | null }
  | { kind: "mode_changed"; mode: string }
  | { kind: "interrupted" }
  | { kind: "user_message"; text: string; id?: string | null; images?: ImageRef[] }
  | { kind: "message_seen"; id: string }
  | { kind: "turn_ended"; ok: boolean; duration_ms: number | null }
  | { kind: "usage_update"; window: string | null; utilization: number; resets_at: number | null }
  | { kind: "background_work"; running: number; agents: number; tasks?: BackgroundTask[] }
  | {
      kind: "task_started";
      task_id: string;
      /** The main chat's tool call that started it; its steps carry this as `parent_tool_id`. */
      tool_id: string | null;
      description: string;
      agent_type: string | null;
      background: boolean;
    }
  | {
      kind: "task_progress";
      task_id: string;
      tool_id: string | null;
      description: string | null;
      tool_uses: number | null;
      last_tool: string | null;
    }
  | {
      kind: "task_ended";
      task_id: string;
      tool_id: string | null;
      status: "completed" | "failed" | "stopped";
      summary: string | null;
      duration_ms?: number | null;
    }
  | { kind: "context_usage"; used: number; max: number; parts?: { name: string; tokens: number }[] }
  | { kind: "error"; message: string }
  | { kind: "compacting"; active: boolean }
  | { kind: "compacted"; auto: boolean }
  | { kind: "sign_in_needed"; message: string }
  | { kind: "title_changed"; title: string }
  | { kind: "worktree_ask"; request_id: string; repo_root: string; tool_id: string | null; name?: string | null }
  | { kind: "worktree_chosen"; choice: WorktreeChoice }
  | { kind: "worktree_changed"; status: WorktreeStatus; branch: string | null }
  | { kind: "prs_changed"; prs: PrInfo[] }
  | { kind: "plan_edited"; request_id: string; plan: string }
  | { kind: "browser_tabs"; tabs: BrowserTab[]; shown: string | null }
  | { kind: "terminal_tabs"; project: string; tabs: TerminalTab[] }
  | { kind: "tab_key"; action: TabAction };

/** One command or agent running in the background. `kind` is the agent's own word, e.g. "local_bash". */
export type BackgroundTask = { id: string; kind: string; description: string };

/** One saved image of a chat: what its `[image n]` word points at. */
export type ImageRef = { n: number; media_type: string };

/** The answer to Claude's request for a worktree. */
export type WorktreeChoice = "worktree" | "here";
export type WorktreeStatus = "created" | "restored" | "put_away" | "merged" | "gone";

/** One of a chat's pull requests. Mirrors `PrInfo` in src-tauri/src/worktree.rs. */
export type PrInfo = {
  number: number;
  state: "open" | "draft" | "merged" | "closed";
  url: string;
  head: string;
  /** The branch it was opened from. Empty for one saved before this was kept. */
  branch: string;
};

export type Envelope = { session_id: string; event: AgentEvent };

export type SetupInfo = {
  /** Claude Code's program. */
  binary: string | null;
  /** Cursor's agent program. */
  cursor: string | null;
  /** The agents found, Claude Code first. */
  agents: AgentId[];
  problem: string | null;
};

export async function subscribe(onEvent: (e: Envelope) => void): Promise<void> {
  const channel = new Channel<Envelope>();
  channel.onmessage = onEvent;
  await invoke("subscribe", { onEvent: channel });
}

export const checkSetup = () => invoke<SetupInfo>("check_setup");

/** A saved chat. Mirrors `SessionRow` in src-tauri/src/store.rs. */
export type SessionRow = {
  id: string;
  project_path: string;
  title: string;
  mode: string;
  model: string;
  /** Effort level picked for this chat, or "default" for Claude Code's own default. */
  effort: string;
  agent_session_id: string | null;
  last_used_at: number;
  /** Where the agent works when the chat has a worktree (live or put away). */
  work_path: string | null;
  /** The worktree's branch. */
  branch: string | null;
  wt_status: "live" | "put_away" | null;
  /** The chat's pull requests as last seen, oldest first. */
  prs: PrInfo[];
  /** When the chat went to the trash (ms); null when it isn't there. */
  trashed_at: number | null;
  /** The agent the chat runs on. */
  agent: AgentId;
};

/** A project folder and its chats, newest first. */
export type ProjectRow = { id: string; path: string; sessions: SessionRow[] };

/** Everything the sidebar shows. */
export const sidebar = () => invoke<ProjectRow[]>("sidebar");

/** A saved chat's details and history. `live` says whether its process is running. */
export const loadSession = (sessionId: string) =>
  invoke<{ session: SessionRow; events: AgentEvent[]; live: boolean }>("load_session", { sessionId });

export const renameSession = (sessionId: string, title: string) =>
  invoke<void>("rename_session", { sessionId, title });
/** Moves a chat to the trash. */
export const deleteSession = (sessionId: string) => invoke<void>("delete_session", { sessionId });
/** Chats in the trash, most recently trashed first. */
export const listTrash = () => invoke<SessionRow[]>("list_trash");
export const restoreChat = (sessionId: string) => invoke<void>("restore_chat", { sessionId });
/** Deletes every chat in the trash for good. */
export const emptyTrash = () => invoke<void>("empty_trash");

/** The answer to Claude's request for a worktree: a new one, or keep working here. */
export const chooseWorktree = (sessionId: string, requestId: string, choice: WorktreeChoice) =>
  invoke<void>("choose_worktree", { sessionId, requestId, choice });

/** Saves the chat on its first message. `mode` and `model` are saved with it, and
 *  are passed to the agent when this message has to start the chat's process. */
export const sendMessage = (
  sessionId: string,
  /** The agent a new chat runs on; a saved chat keeps its own. */
  agent: AgentId,
  projectPath: string,
  mode: Mode,
  model: string,
  effort: string,
  text: string,
  messageId: string,
  /** Numbers of the chat's saved images to send with the text. */
  images: number[] = [],
  /** Things the app tells the agent with this message; not shown or saved. */
  appNotes: string[] = [],
) => invoke<void>("send_message", { sessionId, agent, projectPath, mode, model, effort, text, images, messageId, appNotes });

export const answerApproval = (
  sessionId: string,
  requestId: string,
  decision: Decision,
  message?: string,
  /** For a question: each question's text mapped to the chosen answer. */
  answers?: Record<string, string>,
  /** For a plan the user edited before approving: the agent works from this text. */
  plan?: string,
) => invoke<void>("answer_approval", { sessionId, requestId, decision, message, answers, plan });

/** The chat's notes, written in the side panel; "" if there are none yet. */
export const readNotes = (chatId: string) => invoke<string>("read_notes", { chatId });
export const writeNotes = (chatId: string, text: string) => invoke<void>("write_notes", { chatId, text });

export const interrupt = (sessionId: string) => invoke<void>("interrupt", { sessionId });

/**
 * Ask one of a chat's side chats something. `sideId` is `<chat id>#side-<tab>`.
 * Starts it as a copy of the chat on its first question.
 */
export const sideSend = (
  sideId: string,
  projectPath: string,
  model: string,
  effort: string,
  text: string,
  messageId: string,
) => invoke<void>("side_send", { sideId, projectPath, model, effort, text, messageId });
export const sideInterrupt = (sideId: string) => invoke<void>("side_interrupt", { sideId });
/** End a side chat and forget it. */
export const sideEnd = (sideId: string) => invoke<void>("side_end", { sideId });
/** A fresh window: end the side chats an earlier one left running. */
export const sideEndAll = () => invoke<void>("side_end_all");

export const setMode = (sessionId: string, mode: Mode) => invoke<void>("set_mode", { sessionId, mode });

/** One model the agent offers. Mirrors `ModelInfo` in src-tauri/src/events.rs. */
export type ModelInfo = {
  /** What to pass back to pick it: an alias, a full name, or "default". */
  value: string;
  /** What to show, e.g. "Opus 5.5" or "Default (Fable 5.2)". */
  name: string;
  detail: string | null;
  /** Effort levels it takes, lowest first. Empty = no effort setting. (Missing in lists cached by older builds.) */
  efforts?: string[];
  /** The level it uses when none is picked, if known. */
  default_effort?: string | null;
};

/**
 * Asks an agent for its current models and their effort levels. About 7
 * seconds for Claude Code. Cursor reads effort levels only for `effortsFor`
 * (all if left out), about a second each.
 */
export const listModels = (agent: AgentId, effortsFor?: string[]) =>
  invoke<ModelInfo[]>("list_models", { agent, effortsFor });

/** Runs an agent's own sign-in, which opens the browser. Resolves once the user is done. */
export const signIn = (agent: AgentId) => invoke<void>("sign_in", { agent });

/** `model` is an alias ("opus"), a full name, or "default". */
export const setModel = (sessionId: string, model: string) => invoke<void>("set_model", { sessionId, model });

/** `effort` is a level such as "high"; it stays for this chat only. */
export const setEffort = (sessionId: string, effort: string) => invoke<void>("set_effort", { sessionId, effort });

export const closeSession = (sessionId: string) => invoke<void>("close_session", { sessionId });

/** Native folder picker. Null if the user cancels. */
export async function pickFolder(startAt?: string): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, defaultPath: startAt });
  return typeof picked === "string" ? picked : null;
}

/** Native file picker for the given extensions. Null if the user cancels. */
export async function pickFile(name: string, extensions: string[]): Promise<string | null> {
  const picked = await open({ directory: false, multiple: false, filters: [{ name, extensions }] });
  return typeof picked === "string" ? picked : null;
}

/** One of the app's own skills (see src-tauri/src/skills.rs). */
export type SkillInfo = { name: string; description: string };

export const listSkills = () => invoke<SkillInfo[]>("list_skills");

/** A skill folder, a .zip of one, or a .md file. Replaces a skill of the same name. */
export const importSkill = (path: string) => invoke<SkillInfo>("import_skill", { path });

export const removeSkill = (name: string) => invoke<void>("remove_skill", { name });

// Self-test (see `autotest` in src-tauri/src/lib.rs). Null when not turned on.
export const autotest = () =>
  invoke<{
    folder: string;
    messages: string[];
    theme: string | null;
    size: string | null;
    model: string | null;
  } | null>("autotest");
/** A pause timed by the core; a hidden window's own timers run late. */
export const autotestWait = (ms: number) => invoke<void>("autotest_wait", { ms });
export const autotestReport = (text: string) => invoke<void>("autotest_report", { text });

/** Keeps a pasted or dropped image for a chat. `data` is base64. `original`
 *  is the full-size image when this one was shrunk; it is kept for looking at. */
export const saveImage = (
  sessionId: string,
  mediaType: string,
  data: string,
  original?: { media_type: string; data: string },
) => invoke<ImageRef>("save_image", { sessionId, mediaType, data, original });
/** A chat's image as raw bytes, for looking at: the original of a shrunk paste. */
export const readImage = (sessionId: string, n: number) => invoke<ArrayBuffer>("read_image", { sessionId, n });

// ---- Files ----
// The window takes file drops itself (the only way to learn a dropped file's
// path) and hands the paths to whoever listens: the message box.

const dropHandlers = new Set<(paths: string[]) => void>();
let dropsWired = false;

/** Hands dropped files' paths to the listeners. The window calls it; so does the browser preview. */
export function dropFiles(paths: string[]): void {
  if (paths.length) for (const handler of dropHandlers) handler(paths);
}

/** Calls `handler` with the paths of files dropped on the window. Returns how to stop. */
export function onFileDrop(handler: (paths: string[]) => void): () => void {
  dropHandlers.add(handler);
  if (!dropsWired) {
    dropsWired = true;
    try {
      void getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type === "drop") dropFiles(e.payload.paths);
        })
        .catch(() => {});
    } catch {
      // Not in the app's window (the browser preview): nothing to listen to.
    }
  }
  return () => dropHandlers.delete(handler);
}

/** An image file's bytes, for a dropped image that becomes `[image n]`. */
export const readImageFile = (path: string) => invoke<ArrayBuffer>("read_image_file", { path });
/** Whether a path is a file or a folder. */
export const fileKind = (path: string) => invoke<"file" | "folder">("file_kind", { path });
/** Opens a dropped file in its own app. Folders, apps and runnable files are shown in their folder instead. */
export const openFile = (path: string) => invoke<void>("open_file", { path });
/** Paths of the files on the system clipboard (copied in Finder or Explorer); none when it holds something else. */
export const clipboardFiles = () => invoke<string[]>("clipboard_files");

/** A markdown file's text, for a tab in the side panel. */
export const readDoc = (path: string) => invoke<string>("read_doc", { path });
/** Saves the user's edit to a markdown file the panel has read. */
export const writeDoc = (path: string, text: string) => invoke<void>("write_doc", { path, text });

/** Moves a doc from the chat's folder into `<project>/docs/`; the new path. */
export const saveDocToProject = (chatId: string, path: string, project: string) =>
  invoke<string>("save_doc_to_project", { chatId, path, project });

// ---- The side panel's browser (tabs per chat, localhost pages only) ----

/** One tab of a chat's browser. `url` is "" for a new tab with no page yet. */
export type BrowserTab = { id: string; url: string; title: string; loading: boolean };
/** Where the panel's body is, in CSS pixels from the window's top left. */
export type BrowserRect = { x: number; y: number; width: number; height: number };

/** Opens an address the user typed: in `tab`, else the tab on show, else a new tab. */
export const browserNavigate = (chatId: string, tab: string | null, url: string) =>
  invoke<void>("browser_navigate", { chatId, tab, url });
export const browserReload = (chatId: string, tab: string) => invoke<void>("browser_reload", { chatId, tab });
/** A new empty tab, shown. Returns its id. */
export const browserNewTab = (chatId: string) => invoke<string>("browser_new_tab", { chatId });
export const browserPick = (chatId: string, tab: string) => invoke<void>("browser_pick", { chatId, tab });
export const browserCloseTab = (chatId: string, tab: string) => invoke<void>("browser_close_tab", { chatId, tab });
/** A tab dragged to place `to` in its row. */
export const browserMove = (chatId: string, tab: string, to: number) => invoke<void>("browser_move", { chatId, tab, to });
/** Puts the browser over the panel's body, or parks it out of sight. */
export const browserPlace = (chatId: string, rect: BrowserRect, visible: boolean) =>
  invoke<void>("browser_place", { chatId, rect, visible });
/** Closes all of a chat's tabs. */
export const browserClose = (chatId: string) => invoke<void>("browser_close", { chatId });
export const browserCurrent = (chatId: string) =>
  invoke<{ tabs: BrowserTab[]; shown: string | null }>("browser_current", { chatId });

// ---- The side panel's terminal (tabs belong to a project) ----

/** A terminal tab: named for what runs in it, or its folder when nothing does. */
export type TerminalTab = {
  id: string;
  title: string;
  /** The command line running besides the shell, if any. */
  running: string | null;
  /** Its shell has ended; typing starts a new one. */
  closed: boolean;
  /** The chat worktree folder it was opened in, if any. */
  worktree: string | null;
};

export const terminalTabs = (project: string) => invoke<TerminalTab[]>("terminal_tabs", { project });
/** Opens a tab in the project's main folder, or in a chat's worktree folder (named `label`). */
export const terminalNew = (project: string, worktree: string | null, label: string | null, cols: number, rows: number) =>
  invoke<TerminalTab>("terminal_new", { project, worktree, label, cols, rows });
/** Sends the tab's output to `onData`: everything kept so far, then what comes. */
export async function terminalAttach(tab: string, onData: (data: Uint8Array) => void): Promise<void> {
  const channel = new Channel<ArrayBuffer | number[] | string>();
  channel.onmessage = (message) => {
    if (message instanceof ArrayBuffer) onData(new Uint8Array(message));
    else if (Array.isArray(message)) onData(Uint8Array.from(message));
    else onData(new TextEncoder().encode(String(message)));
  };
  await invoke("terminal_attach", { tab, onData: channel });
}
export const terminalWrite = (tab: string, data: string) => invoke<void>("terminal_write", { tab, data });
export const terminalResize = (tab: string, cols: number, rows: number) =>
  invoke<void>("terminal_resize", { tab, cols, rows });
/** Which tab is on screen, if any. */
export const terminalPlace = (tab: string | null, visible: boolean) => invoke<void>("terminal_place", { tab, visible });
/** What runs in a tab besides its shell, checked now. */
export const terminalRunning = (tab: string) => invoke<string | null>("terminal_running", { tab });
export const terminalClose = (tab: string) => invoke<void>("terminal_close", { tab });
/** A tab dragged to place `to` among its project's tabs. */
export const terminalMove = (tab: string, to: number) => invoke<void>("terminal_move", { tab, to });
/** A tab's output as plain text, as Claude reads it (for the self-test). */
export const terminalText = (tab: string, lines: number) => invoke<string>("terminal_text", { tab, lines });
/** Closes the window, as Cmd+W does outside a terminal. */
export const closeWindow = () => invoke<void>("close_window");
