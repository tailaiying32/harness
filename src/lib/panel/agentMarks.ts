// The mark beside a subagent, in its tab and its row in chat info.
import type { AgentState } from "../state/chatInfo.svelte";

export const MARKS: Record<AgentState, string> = { running: "●", done: "✓", failed: "✗", stopped: "–" };
