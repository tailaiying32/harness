// How tool calls are worded and summarised in the chat.
// Tool names and input fields here are Claude Code's; unknown tools fall back
// to their own name, so a new tool never breaks the view.

export type ToolCall = {
  toolId: string;
  name: string;
  input: unknown;
  /** Undefined while the tool is still running. */
  result?: string;
  isError?: boolean;
  /** The user denied it, or it was cancelled while waiting for an answer. */
  declined?: boolean;
};

export type DiffLine = { sign: "+" | "-"; text: string };

function field(input: unknown, key: string): string {
  if (input && typeof input === "object" && key in input) {
    const value = (input as Record<string, unknown>)[key];
    if (typeof value === "string") return value;
  }
  return "";
}

const fileName = (path: string) => path.split(/[\\/]/).pop() || path;
const clip = (text: string, max: number) => (text.length > max ? text.slice(0, max - 1) + "…" : text);
const firstLine = (text: string) => text.split("\n")[0];

function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** [past tense, present tense, "to ..." form], each followed by what it acted on. */
function wording(tool: ToolCall): [string, string, string] {
  const { name, input } = tool;
  const file = fileName(field(input, "file_path") || field(input, "notebook_path"));
  const pattern = clip(field(input, "pattern"), 30);
  const query = clip(field(input, "query"), 30);
  const site = host(field(input, "url"));
  switch (name) {
    case "Read":
      return [`read ${file}`, `Reading ${file}`, `read ${file}`];
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit":
      return [`edited ${file}`, `Editing ${file}`, `edit ${file}`];
    case "Write":
      return [`wrote ${file}`, `Writing ${file}`, `write ${file}`];
    case "Bash": {
      const command = clip(firstLine(field(input, "command")), 40);
      return [`ran ${command}`, `Running ${command}`, `run ${command}`];
    }
    case "Grep":
      return [`searched for ${pattern}`, `Searching for ${pattern}`, `search for ${pattern}`];
    case "Glob":
      return [`listed ${field(input, "pattern")}`, `Listing ${field(input, "pattern")}`, `list ${field(input, "pattern")}`];
    case "WebFetch":
      return [`fetched ${site}`, `Fetching ${site}`, `fetch ${site}`];
    case "WebSearch":
      return [`searched the web for ${query}`, `Searching the web`, `search the web for ${query}`];
    case "Task":
    case "Agent":
      return [`ran a subagent`, `Running a subagent`, `run a subagent`];
    case "TodoWrite":
      return [`updated its task list`, `Updating its task list`, `update its task list`];
    case "ExitPlanMode":
      return [`presented a plan`, `Presenting a plan`, `present a plan`];
    case "AskUserQuestion":
      return [`asked a question`, `Asking a question`, `ask a question`];
    case "ToolSearch":
      return [`looked up a tool`, `Looking up a tool`, `look up a tool`];
    default:
      return [`used ${name}`, `Using ${name}`, `use ${name}`];
  }
}

/** Tools whose output is shown when a folded run is opened. Errors always are. */
export const showsOutput = (tool: ToolCall) =>
  !tool.declined && (Boolean(tool.isError) || ["Bash", "Grep", "Glob", "WebSearch"].includes(tool.name));

/** Plans and questions have their own card, which says how they went. */
const hasOwnCard = (tool: ToolCall) => tool.name === "ExitPlanMode" || tool.name === "AskUserQuestion";

/** True when the call never ran because the user said no. */
export const skipped = (tool: ToolCall) => Boolean(tool.declined) && !hasOwnCard(tool);

/** "ran npm test" becomes "skipped npm test": the verb is swapped for "skipped". */
export function past(tool: ToolCall): string {
  const text = wording(tool)[0];
  return skipped(tool) ? text.replace(/^\S+/, "skipped") : text;
}
export const present = (tool: ToolCall) => wording(tool)[1];

/**
 * True while the call waits for the user to allow it. Plans and questions
 * don't count: showing their card is the call.
 */
export const awaitsAnswer = (tool: ToolCall) => tool.result === undefined && !hasOwnCard(tool);

/** "waiting to run npm test": for a call that hasn't been allowed yet. */
export const pending = (tool: ToolCall) => `waiting to ${wording(tool)[2]}`;

/** A path as seen from the project folder, when it is inside it. */
function relative(path: string, root: string): string {
  const base = root.replace(/[\\/]$/, "");
  if (base && path.startsWith(base) && /[\\/]/.test(path.charAt(base.length))) {
    return path.slice(base.length + 1);
  }
  return path;
}

/** One line naming the call in full, for an opened run. `root` is the project folder. */
export function title(tool: ToolCall, root = ""): { verb: string; target: string } {
  const { name, input } = tool;
  const target =
    relative(field(input, "file_path") || field(input, "notebook_path"), root) ||
    firstLine(field(input, "command")) ||
    field(input, "pattern") ||
    field(input, "url") ||
    field(input, "query") ||
    field(input, "description");
  return { verb: name, target };
}

/** Removed and added lines of an edit, or null if this call isn't one. */
export function diffLines(tool: ToolCall): DiffLine[] | null {
  const { name, input } = tool;
  // A final newline is not an extra line.
  const lines = (text: string, sign: "+" | "-"): DiffLine[] =>
    text === "" ? [] : text.replace(/\n$/, "").split("\n").map((line) => ({ sign, text: line }));

  if (name === "Edit") {
    return [...lines(field(input, "old_string"), "-"), ...lines(field(input, "new_string"), "+")];
  }
  if (name === "MultiEdit" && input && typeof input === "object" && "edits" in input) {
    const edits = (input as { edits: unknown }).edits;
    if (!Array.isArray(edits)) return null;
    return edits.flatMap((edit) => [
      ...lines(field(edit, "old_string"), "-"),
      ...lines(field(edit, "new_string"), "+"),
    ]);
  }
  if (name === "Write") return lines(field(input, "content"), "+");
  return null;
}

/** Lines added and removed across a run of calls. */
export function editCounts(tools: ToolCall[]): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const tool of tools) {
    if (tool.declined) continue; // nothing was written
    for (const line of diffLines(tool) ?? []) {
      if (line.sign === "+") added++;
      else removed++;
    }
  }
  return { added, removed };
}

// ---- The side panel: plans and the markdown documents a chat wrote ----

/** The markdown file a call writes or edits, or "" if it isn't one. */
export function docPath(tool: ToolCall): string {
  if (!["Write", "Edit", "MultiEdit"].includes(tool.name)) return "";
  const path = field(tool.input, "file_path");
  return /\.(md|markdown)$/i.test(path) ? path : "";
}

/**
 * A document's text after a call that wrote or edited it. `before` is what it
 * held, when the chat has seen it written. Null means the text can't be told:
 * an edit to a file this chat never wrote in full.
 */
export function textAfter(tool: ToolCall, before: string | undefined): string | null {
  const { name, input } = tool;
  if (name === "Write") return field(input, "content");
  if (before === undefined) return null;
  // Not String.replace with a string: "$" in the new text would be read as a pattern.
  const apply = (text: string, edit: unknown): string => {
    const from = field(edit, "old_string");
    const to = field(edit, "new_string");
    if (!from) return text;
    if ((edit as { replace_all?: unknown })?.replace_all === true) return text.split(from).join(to);
    const at = text.indexOf(from);
    return at < 0 ? text : text.slice(0, at) + to + text.slice(at + from.length);
  };
  if (name === "Edit") return apply(before, input);
  if (name === "MultiEdit") {
    const edits = (input as { edits?: unknown })?.edits;
    return Array.isArray(edits) ? edits.reduce(apply, before) : before;
  }
  return null;
}

/** A plan request's text, and the file Claude Code keeps the plan in. */
export function planOf(input: unknown): { text: string; path: string } {
  return { text: field(input, "plan"), path: field(input, "planFilePath") };
}

/** A file's name without its folders. */
export const baseName = fileName;
