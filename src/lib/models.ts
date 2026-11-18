// The model lists shown in the picker, one per agent. They come from the
// agents themselves (see `list_models` in the core), so names and versions
// are always current: only the newest of each family. Claude Code's list has
// no "Default" and the recommended one first; Cursor's starts with Auto;
// Codex's starts with its default. The
// last lists are kept so the picker is right from the first paint next time.

import { CLAUDE, CODEX, CURSOR, type AgentId, type ModelInfo } from "./ipc";

/** Each agent's models, in the order they are shown. */
export type AgentModels = Record<AgentId, ModelInfo[]>;

const KEYS: Record<AgentId, string> = {
  "claude-code": "harness.models",
  cursor: "harness.models.cursor",
  codex: "harness.models.codex",
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
      // A list saved before the picker was trimmed: the same trim (see
      // `parse_models` in the core).
      const kept = saved.filter((m) => m.value !== "default" && !m.value.includes("-"));
      if (kept.length > 0) return kept;
    }
  } catch {
    // Fall through to the built-in list.
  }
  // Cursor's and Codex's lists are only shown once they have answered.
  return agent === CLAUDE ? fallbackModels : [];
}

export function loadCachedModels(): AgentModels {
  return { "claude-code": loadOne(CLAUDE), cursor: loadOne(CURSOR), codex: loadOne(CODEX) };
}

/**
 * The model a new chat starts on: the last one picked, or for Claude Code the
 * recommended one (listed first) if that was its "default", which the picker
 * no longer offers.
 */
export function startingModel(saved: string, agent: AgentId, models: AgentModels): string {
  if (agent === CURSOR) return saved || "default";
  // Codex lists its default first.
  if (agent === CODEX) return saved && saved !== "default" ? saved : (models[CODEX][0]?.value ?? "default");
  return !saved || saved === "default" ? (models[CLAUDE][0]?.value ?? "default") : saved;
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
};

/** The mode a chat moving to `agent` (before its first message) takes: plan stays plan. */
export function modeFor(agent: AgentId, mode: string, saved: string): string {
  if (agentModes[agent].some((m) => m.id === mode)) return mode;
  if (mode === "plan") return "plan";
  if (agent === CURSOR) return "agent";
  if (agent === CODEX) return "default";
  return saved;
}

/**
 * The mode a chat that has started takes when it moves to `agent`: that
 * agent's own default, nothing carried over. For Claude Code that is the
 * user's usual mode (`saved`), never plan.
 */
export function defaultMode(agent: AgentId, saved: string): string {
  if (agent === CURSOR) return "agent";
  if (agent === CODEX) return "default";
  return saved && saved !== "plan" && agentModes[CLAUDE].some((m) => m.id === saved) ? saved : "default";
}

/** A model's name as the agent lists it, for the line that marks a switch. */
export function modelName(agent: AgentId, value: string): string {
  const found = loadCachedModels()[agent]?.find((m) => m.value === value);
  if (found) return found.name;
  if (value === "default") return agent === CURSOR ? "Auto" : "default";
  return value;
}

// Which of Cursor's models the menu shows: the user's pick, made in Settings.
// Cursor offers dozens, and its names don't say which are current. The first
// pick is Auto alone; a model Cursor adds later is shown as it appears.

const PICK_KEY = "harness.models.cursor.picked";

/** `shown`: in the menu. `known`: every model seen so far, so a new one can be told apart. */
export type CursorPick = { shown: string[]; known: string[] };

export function loadCursorPick(): CursorPick | null {
  try {
    const saved = JSON.parse(localStorage.getItem(PICK_KEY) ?? "null") as CursorPick | null;
    if (saved && Array.isArray(saved.shown) && Array.isArray(saved.known)) return saved;
  } catch {
    // Treated as never picked.
  }
  return null;
}

export function saveCursorPick(pick: CursorPick): void {
  try {
    localStorage.setItem(PICK_KEY, JSON.stringify(pick));
  } catch {
    // Storage can be unavailable; the pick is then lost at quit.
  }
}

/**
 * The pick after Cursor listed `models`: at first only Auto; after that, any
 * model not seen before is added to the menu.
 */
export function followList(pick: CursorPick | null, models: ModelInfo[]): CursorPick {
  const values = models.map((m) => m.value);
  if (!pick) return { shown: values.filter((v) => v === "default"), known: values };
  const fresh = values.filter((v) => !pick.known.includes(v));
  return { shown: [...pick.shown, ...fresh], known: [...pick.known, ...fresh] };
}

/** Effort levels from an earlier list, kept where a new list didn't read them. */
export function keepEfforts(list: ModelInfo[], before: ModelInfo[]): ModelInfo[] {
  return list.map((m) => {
    if (m.efforts?.length) return m;
    const old = before.find((b) => b.value === m.value);
    return old?.efforts?.length ? { ...m, efforts: old.efforts, default_effort: old.default_effort } : m;
  });
}
