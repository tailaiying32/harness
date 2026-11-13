// Typed wrappers over the core's commands and its one event channel.
// These types mirror src-tauri/src/events.rs.

import { Channel, invoke } from "@tauri-apps/api/core";

export type SessionState = "starting" | "working" | "waiting_approval" | "idle" | "exited";

export type Block =
  | { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "tool_call"; tool_id: string; name: string; input: unknown };

export type ApprovalKind = "tool" | "plan" | "question";
export type Decision = "allow" | "always_allow" | "deny";

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
    }
  | { kind: "user_message"; text: string }
  | { kind: "turn_ended"; ok: boolean; duration_ms: number | null }
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

export const sendMessage = (sessionId: string, projectPath: string, text: string) =>
  invoke<void>("send_message", { sessionId, projectPath, text });

export const answerApproval = (
  sessionId: string,
  requestId: string,
  decision: Decision,
  message?: string,
) => invoke<void>("answer_approval", { sessionId, requestId, decision, message });

export const interrupt = (sessionId: string) => invoke<void>("interrupt", { sessionId });

// Self-test (see `autotest` in src-tauri/src/lib.rs). Null when not turned on.
export const autotest = () => invoke<{ folder: string; messages: string[] } | null>("autotest");
export const autotestReport = (text: string) => invoke<void>("autotest_report", { text });

export const closeSession =(sessionId: string) => invoke<void>("close_session", { sessionId });
