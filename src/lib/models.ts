// The model lists shown in the picker, one per agent. They come from the
// agents themselves (see `list_models` in the core), so names and versions
// are always current: only the newest of each family. Claude Code's list has
// no "Default" and the recommended one first; Cursor's starts with Auto;
// Codex's and Antigravity's start with their default. The
// last lists are kept so the picker is right from the first paint next time.

import { AGENTS, ANTIGRAVITY, CLAUDE, CODEX, CURSOR, type AgentId, type ModelInfo } from "./ipc";

/** Each agent's models, in the order they are shown. */
export type AgentModels = Record<AgentId, ModelInfo[]>;

const KEYS: Record<AgentId, string> = {
  "claude-code": "harness.models",
  cursor: "harness.models.cursor",
  codex: "harness.models.codex",
  antigravity: "harness.models.antigravity",
};

/** Shown only until Claude Code has answered once on this machine. */
export const fallbackModels: ModelInfo[] = [
  { value: "opus", name: "Opus", detail: null },
  { value: "fable", name: "Fable", detail: null },
  { value: "sonnet", name: "Sonnet", detail: null },
  { value: "haiku", name: "Haiku", detail: null },
];

function loadOne(agent: AgentId): ModelInfo[] {
  try {
    const saved = JSON.parse(localStorage.getItem(KEYS[agent]) ?? "[]") as ModelInfo[];
    if (Array.isArray(saved) && saved.every((m) => m && typeof m.value === "string")) {
      if (agent !== CLAUDE) return saved;
      // Leave out "default"; pinned older versions stay so Settings can offer them.
      const kept = saved.filter((m) => m.value !== "default");
      if (kept.length > 0) return kept;
    }
  } catch {
    // Fall through to the built-in list.
  }
  // The other agents' lists are only shown once they have answered.
  return agent === CLAUDE ? fallbackModels : [];
}

export function loadCachedModels(): AgentModels {
  return Object.fromEntries(AGENTS.map((agent) => [agent, loadOne(agent)])) as AgentModels;
}

/**
 * The model a new chat starts on: the last one picked, or the agent's first
 * shown model (recommended for Claude Code, Auto for Cursor, default for
 * Codex and Antigravity) if none was picked.
 */
export function startingModel(saved: string, agent: AgentId, models: AgentModels): string {
  const firstVisible = visibleModels(agent, models[agent], loadModelPick(agent))[0]?.value;
  if (agent === CURSOR) return saved || firstVisible || "default";
  if (agent === CODEX || agent === ANTIGRAVITY) {
    return saved && saved !== "default" ? saved : (firstVisible ?? models[agent][0]?.value ?? "default");
  }
  return !saved || saved === "default" ? (firstVisible ?? models[CLAUDE][0]?.value ?? "default") : saved;
}

export function saveCachedModels(agent: AgentId, models: ModelInfo[]): void {
  try {
    localStorage.setItem(KEYS[agent], JSON.stringify(models));
  } catch {
    // Storage can be unavailable; the list is then fetched again next run.
  }
}

/** Each agent's modes, in the order the mode button steps through them. */
export const agentModes: Record<AgentId, { id: string; label: string }[]> = {
  "claude-code": [
    { id: "default", label: "ask before edits" },
    { id: "acceptEdits", label: "auto-accept edits" },
    { id: "plan", label: "plan" },
    { id: "auto", label: "auto" },
  ],
  cursor: [
    { id: "agent", label: "agent" },
    { id: "plan", label: "plan" },
    { id: "ask", label: "ask only" },
  ],
  // Codex's own: its default asks before leaving the project.
  codex: [
    { id: "default", label: "default" },
    { id: "plan", label: "plan" },
    { id: "read-only", label: "read only" },
    { id: "full-access", label: "full access" },
  ],
  // Antigravity's own, named as Claude Code's are.
  antigravity: [
    { id: "default", label: "ask before edits" },
    { id: "accept-edits", label: "auto-accept edits" },
    { id: "plan", label: "plan" },
    { id: "auto", label: "auto" },
  ],
};

/** The same mode under another agent's name for it. */
const SAME_MODE: Record<string, string> = { acceptEdits: "accept-edits", "accept-edits": "acceptEdits" };

/** The mode a chat moving to `agent` (before its first message) takes: plan stays plan. */
export function modeFor(agent: AgentId, mode: string, saved: string): string {
  if (agentModes[agent].some((m) => m.id === mode)) return mode;
  const same = SAME_MODE[mode];
  if (same && agentModes[agent].some((m) => m.id === same)) return same;
  if (mode === "plan") return "plan";
  if (agent === CURSOR) return "agent";
  if (agent === CODEX || agent === ANTIGRAVITY) return "default";
  return saved;
}

/**
 * The mode a chat that has started takes when it moves to `agent`: that
 * agent's own default, nothing carried over. For Claude Code that is the
 * user's usual mode (`saved`), never plan.
 */
export function defaultMode(agent: AgentId, saved: string): string {
  if (agent === CURSOR) return "agent";
  if (agent === CODEX || agent === ANTIGRAVITY) return "default";
  return saved && saved !== "plan" && agentModes[CLAUDE].some((m) => m.id === saved) ? saved : "default";
}

/** A model's name as the agent lists it, for the line that marks a switch. */
export function modelName(agent: AgentId, value: string): string {
  const found = loadCachedModels()[agent]?.find((m) => m.value === value);
  if (found) return found.name;
  if (value === "default") return agent === CURSOR ? "Auto" : "default";
  return value;
}

// Which models each agent shows in the menu: the user's pick, made in Settings.
// Cursor starts with Auto alone; Claude Code starts with its short-name
// families (pinned older versions stay unchecked until turned on); Codex and
// Antigravity start with all models on. Any new model an agent adds later
// joins the menu automatically (except Claude Code's pinned older versions).

const PICK_KEYS: Record<AgentId, string> = {
  "claude-code": "harness.models.claude-code.picked",
  codex: "harness.models.codex.picked",
  antigravity: "harness.models.antigravity.picked",
  cursor: "harness.models.cursor.picked",
};

/** `shown`: in the menu. `known`: every model seen so far, so a new one can be told apart. */
export type ModelPick = { shown: string[]; known: string[] };
export type AgentPicks = Record<AgentId, ModelPick | null>;

export function loadModelPick(agent: AgentId): ModelPick | null {
  try {
    const saved = JSON.parse(localStorage.getItem(PICK_KEYS[agent]) ?? "null") as ModelPick | null;
    if (saved && Array.isArray(saved.shown) && Array.isArray(saved.known)) return saved;
  } catch {
    // Treated as never picked.
  }
  return null;
}

export function loadModelPicks(): AgentPicks {
  return Object.fromEntries(AGENTS.map((agent) => [agent, loadModelPick(agent)])) as AgentPicks;
}

export function saveModelPick(agent: AgentId, pick: ModelPick): void {
  try {
    localStorage.setItem(PICK_KEYS[agent], JSON.stringify(pick));
  } catch {
    // Storage can be unavailable; the pick is then lost at quit.
  }
}

/** Which models of `values` are shown before the user has picked any. */
export function defaultShown(agent: AgentId, values: string[]): string[] {
  if (agent === CURSOR) {
    const auto = values.filter((v) => v === "default");
    return auto.length ? auto : values.slice(0, 1);
  }
  if (agent === CLAUDE) {
    const short = values.filter((v) => !v.includes("-"));
    return short.length ? short : [...values];
  }
  return [...values];
}

/** The model values currently shown for `agent`, before or after a first pick. */
export function shownFor(agent: AgentId, pick: ModelPick | null, list: ModelInfo[]): string[] {
  if (pick) return pick.shown;
  if (agent === CURSOR && list.length === 0) return ["default"];
  return defaultShown(agent, list.map((m) => m.value));
}

/** The subset of `list` shown in the model menu, in the agent's own order. */
export function visibleModels(agent: AgentId, list: ModelInfo[], pick: ModelPick | null): ModelInfo[] {
  const shown = shownFor(agent, pick, list);
  const kept = list.filter((m) => shown.includes(m.value));
  if (kept.length > 0 || list.length === 0) return kept;
  const fallback = defaultShown(agent, list.map((m) => m.value));
  return list.filter((m) => fallback.includes(m.value));
}

/**
 * The pick after `agent` listed `models`: defaults on the first run; after
 * that, newly seen models join the menu (except Claude Code's pinned older
 * versions, which stay unchecked until turned on).
 */
export function followList(agent: AgentId, pick: ModelPick | null, models: ModelInfo[]): ModelPick {
  const values = models.map((m) => m.value);
  if (!pick) return { shown: defaultShown(agent, values), known: values };
  const fresh = values.filter((v) => !pick.known.includes(v));
  const auto = agent === CLAUDE ? fresh.filter((v) => !v.includes("-")) : fresh;
  const shown = [...pick.shown, ...auto];
  const hasLive = values.some((v) => shown.includes(v));
  const nextShown = hasLive
    ? shown
    : [...shown, ...defaultShown(agent, values).filter((v) => !shown.includes(v))];
  return { shown: nextShown, known: [...pick.known, ...fresh] };
}

/** Effort levels and defaults from an earlier list, kept where a new list didn't read them. */
export function keepEfforts(list: ModelInfo[], before: ModelInfo[]): ModelInfo[] {
  return list.map((m) => {
    const old = before.find((b) => b.value === m.value);
    if (!old) return m;
    const efforts = m.efforts?.length ? m.efforts : old.efforts;
    const default_effort = m.default_effort ?? (efforts?.length ? old.default_effort : null);
    return { ...m, efforts, default_effort };
  });
}
