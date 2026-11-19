// The slash commands offered in the message box. Each agent reports its own
// for a folder (see `list_commands` in the core); Harness adds its skills and
// `/model`, and hides the few that do nothing useful here or clash with the
// app. The last lists are kept so the menu is full from the first `/` next
// time; each is read again once per run.

import { agentLabel, listCommands, listSkills, type AgentId, type CommandInfo, type SkillInfo } from "./ipc";

/** A command in the menu. `harness` marks the app's own: `/model`, `/btw`, `/clear`, or one of its skills. */
export type Command = CommandInfo & { harness?: "model" | "btw" | "clear" | "skill" };

/** Commands each agent lists that aren't offered. */
const HIDDEN: Record<AgentId, Set<string>> = {
  "claude-code": new Set([
    "model", // Harness's own `/model` opens the model menu instead
    "effort", // the model menu sets effort
    "fast", // "not available in the Agent SDK"
    "clear", // starts a new session under the chat
    "rename", // renames Claude's session, not the chat
    "color", // colors Claude Code's own prompt bar
    "focus", // Claude Code's own view
    "heapdump", // a debugging dump of Claude Code's memory
    "agents", // listed as removed
    "extra-usage", // renamed to /usage-credits
    "workflow-launch-exec", // server-launched sessions only
  ]),
  cursor: new Set([
    "rename-chat", // Harness names the chat
    "copy-request-id", // copies to the clipboard of Cursor's own screen
    "statusline", // the status line of Cursor's own screen
  ]),
  codex: new Set(),
  antigravity: new Set(),
};

/**
 * The agent's own commands that only work in its own terminal screen: over
 * the app they would reach the model as words and spend a turn, so they are
 * caught and the chat says so instead. A command the agent lists (a skill or
 * custom command of that name) is never caught. Claude Code turns these
 * down itself, for free. Each may name what to use instead.
 */
const SCREEN_ONLY: Record<AgentId, Record<string, string>> = {
  "claude-code": {},
  // Codex's own screen runs these (codex-cli 0.169.1); see `Builtin` in codex.rs for those the app runs.
  codex: {
    status: "Settings shows the usage limits.",
    usage: "Settings shows the usage limits.",
    skills: "Type $ to pick a skill.",
    plan: "Pick Plan in the mode menu.",
    permissions: "Pick a mode in the mode menu.",
    approvals: "Pick a mode in the mode menu.",
    rename: "Rename the chat in the sidebar.",
    title: "Rename the chat in the sidebar.",
    fast: "",
    diff: "",
    fork: "",
    resume: "",
    undo: "",
    side: "",
    ps: "",
    stop: "",
    copy: "",
    memory: "",
    hooks: "",
    plugins: "",
    apps: "",
    agent: "",
    recap: "",
    theme: "",
    statusline: "",
    settings: "",
    experimental: "",
    feedback: "",
    archive: "",
    logout: "",
  },
  // Antigravity's own screen only: its print mode says so (agy 1.5.12).
  antigravity: {
    context: "The ring by the message box shows how full the context is.",
    rename: "Rename the chat in the sidebar.",
    diff: "",
    mcp: "",
    tasks: "",
    rewind: "",
    resume: "",
    fork: "",
    copy: "",
    "add-dir": "",
  },
  // Cursor's own screen only: Cursor's ACP runs none of them (agent 2026.11.19).
  cursor: {
    summarize: "Cursor can't compact over ACP.",
    compress: "Cursor can't compact over ACP.",
    compact: "Cursor can't compact over ACP.",
    usage: "",
    context: "The ring by the message box shows how full the context is.",
    ask: "Pick Ask in the mode menu.",
    plan: "Pick Plan in the mode menu.",
    rename: "Rename the chat in the sidebar.",
    resume: "",
    fork: "",
    rewind: "",
    mcp: "",
    commit: "",
    changes: "",
    about: "",
    "add-dir": "",
  },
};

/** Harness's `/clear` (or `/new`): a new chat in the same folder, as Cmd+N. */
const CLEAR: Command = {
  name: "clear",
  description: "Start a new chat in this folder",
  hint: null,
  aliases: ["new"],
  harness: "clear",
};

const MODEL: Command = { name: "model", description: "Model and effort", hint: null, aliases: [], harness: "model" };
/** Harness's side chat (see `send` in state/chat.svelte.ts), on the chat's own agent. */
const BTW: Command = {
  name: "btw",
  description: "Ask a side question in a side chat, without adding to this one",
  hint: "<question>",
  aliases: [],
  harness: "btw",
};

const STORE_KEY = "harness.commands";
/** Most lists kept between runs, newest kept. */
const KEPT = 40;

function load(): Record<string, CommandInfo[]> {
  try {
    const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? "{}");
    return saved && typeof saved === "object" && !Array.isArray(saved) ? saved : {};
  } catch {
    return {};
  }
}

const store = $state<{ lists: Record<string, CommandInfo[]>; skills: SkillInfo[] }>({ lists: load(), skills: [] });
/** Lists asked for in this run. */
const asked = new Set<string>();
let skillsAsked = false;

const keyOf = (agent: AgentId, folder: string) => `${agent}\n${folder}`;

function save() {
  try {
    const keys = Object.keys(store.lists);
    const kept = Object.fromEntries(keys.slice(-KEPT).map((k) => [k, store.lists[k]]));
    localStorage.setItem(STORE_KEY, JSON.stringify(kept));
  } catch {
    // Storage can be unavailable; the lists are then read again next run.
  }
}

/** Reads Harness's skills again: after one is imported or removed. */
export async function refreshSkills(): Promise<void> {
  skillsAsked = true;
  try {
    store.skills = await listSkills();
  } catch {
    // Keep the last list.
  }
}

/**
 * Asks the agent for its commands in the folder, once per run. A failed ask
 * (say the agent isn't found yet, at startup) is tried again next time.
 */
export function fetchCommands(agent: AgentId, folder: string): void {
  if (!skillsAsked) void refreshSkills();
  if (!folder) return;
  const key = keyOf(agent, folder);
  if (asked.has(key)) return;
  asked.add(key);
  listCommands(agent, folder)
    .then((list) => {
      // Every agent lists some; none means it wasn't ready, so ask again later.
      if (!list.length) {
        asked.delete(key);
        return;
      }
      // Newest last, so the oldest go first when trimming.
      delete store.lists[key];
      store.lists[key] = list;
      save();
    })
    .catch(() => {
      // Keep the saved list meanwhile.
      asked.delete(key);
    });
}

/** What the `/` menu offers: Harness's own commands and skills, then the agent's, by name. */
export function commandsFor(agent: AgentId, folder: string): Command[] {
  const own = [MODEL, BTW, CLEAR];
  const out: Command[] = [...own];
  const taken = new Set(own.map((c) => c.name));
  for (const skill of store.skills) {
    if (taken.has(skill.name)) continue;
    taken.add(skill.name);
    out.push({ name: skill.name, description: skill.description, hint: null, aliases: [], harness: "skill" });
  }
  for (const command of store.lists[keyOf(agent, folder)] ?? []) {
    // A Harness skill of the same name is the one attached (see `Skills::expand`).
    if (command.skill || taken.has(command.name) || HIDDEN[agent].has(command.name) || command.name.startsWith("__")) continue;
    taken.add(command.name);
    out.push(command);
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

/** The agent's skills picked with `$` (Codex's), by name; none for the others. */
export function skillsFor(agent: AgentId, folder: string): Command[] {
  return (store.lists[keyOf(agent, folder)] ?? []).filter((c) => c.skill).sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * What the chat says instead of sending a message that is one of the agent's
 * screen-only commands (see `SCREEN_ONLY`), or null to send it.
 */
export function screenOnly(agent: AgentId, folder: string, text: string): string | null {
  const name = text.trim().match(/^\/([^\s/]+)/)?.[1];
  if (!name || !Object.hasOwn(SCREEN_ONLY[agent], name)) return null;
  if (findCommand(commandsFor(agent, folder), name)) return null;
  const instead = SCREEN_ONLY[agent][name];
  return `/${name} only works in ${agentLabel(agent)}'s own terminal screen.${instead ? ` ${instead}` : ""}`;
}

/**
 * The commands for what is typed after `/`, best first: names (or other
 * names) that start with it, then names that hold it, then descriptions that
 * do. Each group stays in name order.
 */
export function matchCommands(list: Command[], typed: string): Command[] {
  const t = typed.toLowerCase();
  if (!t) return list;
  const starts: Command[] = [];
  const holds: Command[] = [];
  const describes: Command[] = [];
  for (const c of list) {
    const name = c.name.toLowerCase();
    if (name.startsWith(t) || c.aliases.some((a) => a.toLowerCase().startsWith(t))) starts.push(c);
    else if (name.includes(t)) holds.push(c);
    else if (c.description.toLowerCase().includes(t)) describes.push(c);
  }
  return [...starts, ...holds, ...describes];
}

/** The command a name (or one of its other names) stands for, if any. */
export function findCommand(list: Command[], name: string): Command | undefined {
  return list.find((c) => c.name === name || c.aliases.includes(name));
}
