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
 * Claude Code's permission modes, as the app offers them. In "auto" Claude Code
 * decides by itself which actions are safe to run without asking.
 */
export type Mode = "default" | "acceptEdits" | "plan" | "auto";

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
  | { kind: "usage_update"; window: string | null; utilization: number }
  | { kind: "background_work"; running: number }
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
  | { kind: "browser_state"; url: string; title: string; loading: boolean }
  | { kind: "browser_closed" };

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
  binary: string | null;
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
  projectPath: string,
  mode: Mode,
  model: string,
  effort: string,
  text: string,
  messageId: string,
  /** Numbers of the chat's saved images to send with the text. */
  images: number[] = [],
) => invoke<void>("send_message", { sessionId, projectPath, mode, model, effort, text, images, messageId });

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

/** Ask a chat's side chat something. Starts it as a copy of the chat on the first question. */
export const sideSend = (
  chatId: string,
  projectPath: string,
  model: string,
  effort: string,
  text: string,
  messageId: string,
) => invoke<void>("side_send", { chatId, projectPath, model, effort, text, messageId });
export const sideInterrupt = (chatId: string) => invoke<void>("side_interrupt", { chatId });
/** End a chat's side chat and forget it. */
export const sideEnd = (chatId: string) => invoke<void>("side_end", { chatId });

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

/** Asks Claude Code for its current models and their effort levels. Takes about 7 seconds. */
export const listModels = () => invoke<ModelInfo[]>("list_models");

/** Runs Claude Code's own sign-in, which opens the browser. Resolves once the user is done. */
export const signIn = () => invoke<void>("sign_in");

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

/** A markdown file's text, for a link opened in the side panel. */
export const readDoc = (path: string) => invoke<string>("read_doc", { path });

// ---- The side panel's browser (one per chat, localhost pages only) ----

/** What a chat's browser shows. */
export type BrowserInfo = { url: string; title: string; loading: boolean };
/** Where the panel's body is, in CSS pixels from the window's top left. */
export type BrowserRect = { x: number; y: number; width: number; height: number };

/** Opens an address the user typed; makes the chat's browser if needed. */
export const browserNavigate = (chatId: string, url: string) => invoke<void>("browser_navigate", { chatId, url });
export const browserReload = (chatId: string) => invoke<void>("browser_reload", { chatId });
/** Puts the browser over the panel's body, or parks it out of sight. */
export const browserPlace = (chatId: string, rect: BrowserRect, visible: boolean) =>
  invoke<void>("browser_place", { chatId, rect, visible });
export const browserClose = (chatId: string) => invoke<void>("browser_close", { chatId });
export const browserCurrent = (chatId: string) => invoke<BrowserInfo | null>("browser_current", { chatId });
