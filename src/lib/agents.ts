// The agents a chat can run on, in the order the app lists them everywhere:
// the most used first, and Cursor (the longest model list) last. Mirrors
// `agent::ALL` in the core. No imports, so any module can use it.

export type AgentId = "claude-code" | "codex" | "antigravity" | "cursor";

export const CLAUDE: AgentId = "claude-code";
export const CODEX: AgentId = "codex";
export const ANTIGRAVITY: AgentId = "antigravity";
export const CURSOR: AgentId = "cursor";

export const AGENTS: AgentId[] = [CLAUDE, CODEX, ANTIGRAVITY, CURSOR];

/** What the user sees each agent called, and the command that signs in to it. */
export const AGENT_INFO: Record<AgentId, { label: string; login: string }> = {
  "claude-code": { label: "Claude Code", login: "claude auth login" },
  codex: { label: "Codex", login: "codex login" },
  antigravity: { label: "Antigravity", login: "agy acp --login" },
  cursor: { label: "Cursor", login: "agent login" },
};

export const isAgent = (agent: unknown): agent is AgentId => AGENTS.includes(agent as AgentId);

export const agentLabel = (agent: string) => (isAgent(agent) ? AGENT_INFO[agent].label : "Claude Code");

/** Installed agents, in the app's order. */
export const inOrder = (agents: string[]): AgentId[] => AGENTS.filter((a) => agents.includes(a));
