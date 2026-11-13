// Typed wrappers over the core's commands and its one event channel.
// These types mirror src-tauri/src/events.rs.

import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type SessionState = "starting" | "working" | "waiting_approval" | "idle" | "exited";

export type Block =
  | { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "tool_call"; tool_id: string; name: string; input: unknown };

export type ApprovalKind = "tool" | "plan" | "question";
export type Decision = "allow" | "always_allow" | "deny";

/** Claude Code's permission modes. */
export type Mode = "default" | "acceptEdits" | "plan";

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
  | { kind: "mode_changed"; mode: string }
  | { kind: "user_message"; text: string }
  | { kind: "turn_ended"; ok: boolean; duration_ms: number | null }
  | { kind: "usage_update"; window: string | null; utilization: number }
  | { kind: "error"; message: string };

export type Envelope = { session_id: string; event: AgentEvent };

export type SetupInfo = {
  binary: string | null;
  logged_in: boolean;
  subscription: string | null;
  problem: string | null;
};

export async function subscribe(onEvent: (e: Envelope) => void): Promise<void> {
  const channel = new Channel<Envelope>();
  channel.onmessage = onEvent;
  await invoke("subscribe", { onEvent: channel });
}

export const checkSetup = () => invoke<SetupInfo>("check_setup");

/** `mode` and `model` only matter when this message starts the chat's process. */
export const sendMessage = (sessionId: string, projectPath: string, mode: Mode, model: string, text: string) =>
  invoke<void>("send_message", { sessionId, projectPath, mode, model, text });

export const answerApproval = (
  sessionId: string,
  requestId: string,
  decision: Decision,
  message?: string,
  /** For a question: each question's text mapped to the chosen answer. */
  answers?: Record<string, string>,
) => invoke<void>("answer_approval", { sessionId, requestId, decision, message, answers });

export const interrupt = (sessionId: string) => invoke<void>("interrupt", { sessionId });

export const setMode = (sessionId: string, mode: Mode) => invoke<void>("set_mode", { sessionId, mode });

/** One model the agent offers. Mirrors `ModelInfo` in src-tauri/src/events.rs. */
export type ModelInfo = {
  /** What to pass back to pick it: an alias, a full name, or "default". */
  value: string;
  /** What to show, e.g. "Opus 5.5" or "Default (Fable 5.2)". */
  name: string;
  detail: string | null;
};

/** Asks Claude Code for its current models. Takes about a second. */
export const listModels = () => invoke<ModelInfo[]>("list_models");

/** `model` is an alias ("opus"), a full name, or "default". */
export const setModel = (sessionId: string, model: string) => invoke<void>("set_model", { sessionId, model });

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
export const autotestReport = (text: string) => invoke<void>("autotest_report", { text });
