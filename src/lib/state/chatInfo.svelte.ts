// What one chat knows about itself for the "chat info" view: its subagents
// and their steps, Claude's task list, background commands and context use.
// Fed the same events as the chat (see Chat.handle); see PLAN.md, "Chat info view".

import type { AgentEvent, BackgroundTask } from "../ipc";
import { showsOutput, type ToolCall } from "../tools";

/** The tools that start a subagent: Claude Code's ("Task" before 2.1, "Agent" after) and Cursor's, renamed. */
const AGENT_TOOLS = new Set(["Agent", "Task"]);

/** How a subagent hands its report back to the chat. Not shown as a step. */
const HANDBACK = "SubagentHandback";

/** No new step for this long while running: the row says so in the accent colour. */
export const QUIET_MS = 3 * 60 * 1000;

export type AgentState = "running" | "done" | "failed" | "stopped";

/** One step of a subagent: a tool call, or text it wrote between them. */
export type AgentStep = ({ type: "tool" } & ToolCall) | { type: "text"; text: string };

export type Agent = {
  /** The main chat's tool call that started it. Its tab is named by this. */
  toolId: string;
  name: string;
  prompt: string;
  /** The kind of subagent, e.g. "general-purpose", when the agent says. */
  agentType: string;
  /** Runs on after its tool call returns; only its task's end says it's done. */
  background: boolean;
  state: AgentState;
  steps: AgentStep[];
  /** The answer it handed back. */
  answer: string;
  /** A short live line from the agent, e.g. "Reading src/one.txt". */
  live: string;
  /** Clock times, known only for what happened while the chat was open. */
  startedAt?: number;
  endedAt?: number;
  lastAt?: number;
  /** How long it ran, as the agent reported at the end (survives a restart). */
  durationMs?: number;
  /** The turn that started it: agents started in one turn are one batch. */
  batch: number;
  /** The user's message its batch answers. */
  ask: string;
  /** Started in a turn Claude Code began on its own (after a background agent ended), not by a message. */
  followUp: boolean;
};

/** Subagents started in one turn, and what they were for. `title`: a short written label, once it comes. */
export type Batch = { batch: number; ask: string; followUp: boolean; title?: string; agents: Agent[] };

/** How long a new batch waits for its siblings before asking for its label. */
const BATCH_SETTLE_MS = 1500;

/** A batch's label: its written title, else the first line of the message it answers. */
export function batchLabel(batch: Batch): string {
  const what = batch.title || batch.ask.trim().split("\n")[0] || "before any message";
  return batch.followUp ? `${what} · follow-up` : what;
}

export type Task = { id: string; text: string; status: "pending" | "in_progress" | "completed" };

export type Background = BackgroundTask & { since?: number };

export type ContextReading = { used: number; max: number; parts: { name: string; tokens: number }[] };

function field(input: unknown, key: string): string {
  const value = input && typeof input === "object" ? (input as Record<string, unknown>)[key] : undefined;
  return typeof value === "string" ? value : value == null ? "" : String(value);
}

function resultText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => (part && typeof part === "object" && "text" in part ? String(part.text) : ""))
      .join("\n");
  }
  return content == null ? "" : JSON.stringify(content);
}

/** Claude Code's "in_progress", or Cursor's "TODO_STATUS_IN_PROGRESS". */
function taskStatus(s: string): Task["status"] {
  const word = s.toLowerCase().replace(/^todo_status_/, "");
  return word === "completed" ? "completed" : word === "in_progress" ? "in_progress" : "pending";
}

export class ChatInfo {
  agents = $state<Agent[]>([]);
  tasks = $state<Task[]>([]);
  background = $state<Background[]>([]);
  context = $state<ContextReading | null>(null);

  /** The subagent on show, full size, by tool ID; null shows the overview page. */
  shown = $state<string | null>(null);
  /** The subagent picked last, kept while the overview shows. */
  picked = $state<string | null>(null);
  /** Written batch labels, by the batch's first subagent. */
  titles = $state<Record<string, string>>({});

  /** Asks for a batch's label (see BatchTitled); set by the chat. */
  onBatch: ((toolId: string, ask: string, agents: string[]) => void) | null = null;
  /** Live batches waiting for their siblings before their label is asked for, and ones asked. */
  private settling = new Map<number, ReturnType<typeof setTimeout>>();
  private asked = new Set<number>();

  /** Counts turns ended, so each agent knows its batch. */
  private turn = 0;
  /** The user's latest message, and how many turns ended since it. */
  private ask = "";
  private turnsSinceAsk = 0;

  /** The user pressed stop in this turn: agents cut short are "stopped", not "failed". */
  private stopped = false;

  /** The main chat's task-creating tool calls waiting for their number. */
  private pendingTasks = new Map<string, Task>();

  agent(toolId: string): Agent | undefined {
    return this.agents.find((a) => a.toolId === toolId);
  }

  get running(): number {
    return this.agents.filter((a) => a.state === "running").length;
  }

  /** The subagent on show, if it is one this chat has. */
  get shownAgent(): Agent | undefined {
    return this.shown ? this.agent(this.shown) : undefined;
  }

  /** The subagent picked last, on show or not: the top bar's name for it, one click away. */
  get pickedAgent(): Agent | undefined {
    return this.picked ? this.agent(this.picked) : undefined;
  }

  /** The subagents by batch, newest batch first; each in the order started. */
  get batches(): Batch[] {
    const out: Batch[] = [];
    for (const agent of this.agents) {
      const last = out[out.length - 1];
      if (last?.batch === agent.batch) last.agents.push(agent);
      else {
        const title = this.titles[agent.toolId];
        out.push({ batch: agent.batch, ask: agent.ask, followUp: agent.followUp, title, agents: [agent] });
      }
    }
    return out.reverse();
  }

  /** Shows a subagent full size, or the overview (null). */
  show(toolId: string | null): void {
    this.shown = toolId;
    if (toolId) this.picked = toolId;
  }

  /** The overview, then each subagent in the overview's order: the one before or after, wrapping round. */
  step(by: number): void {
    const stops = [null, ...this.batches.flatMap((b) => b.agents.map((a) => a.toolId))];
    const at = Math.max(0, stops.indexOf(this.shownAgent ? this.shown : null));
    this.show(stops[(at + by + stops.length) % stops.length]);
  }

  /**
   * Takes one event. Returns true when it belongs to a subagent, so the chat
   * leaves it out. `replaying`: history, so no clock times are taken.
   */
  handle(event: AgentEvent, replaying: boolean): boolean {
    const now = replaying ? undefined : Date.now();
    switch (event.kind) {
      case "assistant_block": {
        const block = event.block;
        if (event.parent_tool_id) {
          const agent = this.agent(event.parent_tool_id);
          if (!agent) return true;
          if (block.type === "tool_call" && block.name === HANDBACK) {
            // Its report, handed back as a tool call (CLI 2.1.325); the
            // task's own summary then only points here.
            agent.answer = field(block.input, "message");
          } else if (block.type === "tool_call") {
            agent.steps.push({ type: "tool", toolId: block.tool_id, name: block.name, input: block.input });
          } else if (block.type === "text" && block.text.trim()) {
            agent.steps.push({ type: "text", text: block.text });
          }
          if (now) agent.lastAt = now;
          return true;
        }
        if (block.type !== "tool_call") return false;
        if (AGENT_TOOLS.has(block.name)) {
          this.agents.push({
            toolId: block.tool_id,
            name: field(block.input, "description") || "subagent",
            prompt: field(block.input, "prompt"),
            agentType: field(block.input, "subagent_type"),
            background: false,
            state: "running",
            steps: [],
            answer: "",
            live: "",
            startedAt: now,
            lastAt: now,
            batch: this.turn,
            ask: this.ask,
            followUp: this.turnsSinceAsk > 0,
          });
          if (!replaying) this.settle(this.turn);
        } else if (block.name === "TodoWrite") {
          // Claude Code before 2.1.3xx sends the whole list each time. Cursor
          // sends only the items that changed ("updateTodos", or `merge`).
          const input = block.input as {
            todos?: { content?: string; id?: string; status?: string }[];
            merge?: boolean;
            _toolName?: string;
          };
          if (Array.isArray(input?.todos)) {
            const sent = input.todos.map((t, i) => ({
              id: t.id ?? String(i + 1),
              text: t.content ?? "",
              status: taskStatus(t.status ?? ""),
            }));
            if (input.merge || input._toolName === "updateTodos") {
              for (const task of sent) {
                const kept = this.tasks.find((t) => t.id === task.id);
                if (kept) Object.assign(kept, task.text ? task : { status: task.status });
                else this.tasks.push(task);
              }
            } else {
              this.tasks = sent;
            }
          }
        } else if (block.name === "TaskCreate") {
          // Numbered by Claude Code in its result: "Task #3 created successfully".
          const task: Task = {
            id: String(this.tasks.length + this.pendingTasks.size + 1),
            text: field(block.input, "subject") || field(block.input, "description"),
            status: "pending",
          };
          this.pendingTasks.set(block.tool_id, task);
        } else if (block.name === "TaskUpdate") {
          const id = field(block.input, "taskId");
          const status = field(block.input, "status");
          if (status === "deleted") {
            this.tasks = this.tasks.filter((t) => t.id !== id);
          } else {
            const task = this.tasks.find((t) => t.id === id);
            if (task) {
              if (status) task.status = taskStatus(status);
              const subject = field(block.input, "subject");
              if (subject) task.text = subject;
            }
          }
        }
        return false;
      }

      case "tool_result": {
        if (event.parent_tool_id) {
          const agent = this.agent(event.parent_tool_id);
          const step = agent?.steps.find(
            (s): s is Extract<AgentStep, { type: "tool" }> => s.type === "tool" && s.toolId === event.tool_id,
          );
          if (step) {
            step.isError = event.is_error;
            step.result = showsOutput(step) ? resultText(event.content) : "";
          }
          if (agent && now) agent.lastAt = now;
          return true;
        }
        const pending = this.pendingTasks.get(event.tool_id);
        if (pending) {
          this.pendingTasks.delete(event.tool_id);
          if (!event.is_error) {
            const n = resultText(event.content).match(/#(\d+)/);
            if (n) pending.id = n[1];
            this.tasks.push(pending);
          }
          return false;
        }
        const agent = this.agent(event.tool_id);
        if (!agent) return false;
        const text = resultText(event.content);
        // A background subagent's call returns at once; its task's end says when it's done.
        if (agent.background || /^Async agent launched/.test(text)) {
          agent.background = true;
          return false;
        }
        if (event.is_error && this.stopped) {
          this.end(agent, "stopped", now);
          return false;
        }
        // A report handed back on its own (see HANDBACK) beats the pointer to it here.
        if (!agent.answer || event.is_error) agent.answer = text;
        agent.state = event.is_error ? "failed" : "done";
        agent.endedAt = now;
        return false;
      }

      case "task_started": {
        const agent = event.tool_id ? this.agent(event.tool_id) : undefined;
        if (agent) agent.background = event.background;
        return false;
      }

      case "task_progress": {
        const agent = event.tool_id ? this.agent(event.tool_id) : undefined;
        if (agent) {
          if (event.description) agent.live = event.description;
          if (now) agent.lastAt = now;
        }
        return false;
      }

      case "task_ended": {
        const agent = event.tool_id ? this.agent(event.tool_id) : undefined;
        if (!agent || agent.state !== "running") return false;
        agent.state = event.status === "completed" ? "done" : event.status === "failed" ? "failed" : "stopped";
        // A stopped task's summary is only its description again.
        if (event.summary && !agent.answer && event.status !== "stopped") agent.answer = event.summary;
        if (event.duration_ms) agent.durationMs = event.duration_ms;
        agent.endedAt = now;
        agent.live = "";
        return false;
      }

      case "background_work": {
        const seen = new Map(this.background.map((b) => [b.id, b.since]));
        this.background = (event.tasks ?? [])
          .filter((t) => !t.kind.includes("agent"))
          .map((t) => ({ ...t, since: seen.get(t.id) ?? now }));
        return false;
      }

      case "batch_titled":
        this.titles[event.tool_id] = event.title;
        return false;

      case "context_usage":
        this.context = { used: event.used, max: event.max, parts: event.parts ?? [] };
        return false;

      case "interrupted":
        this.stopped = true;
        return false;

      // A stopped turn ends the agents it was waiting on; background ones go on.
      case "turn_ended":
        for (const agent of this.agents) {
          if (agent.state === "running" && !agent.background) this.end(agent, "stopped", now);
        }
        this.stopped = false;
        this.turn++;
        this.turnsSinceAsk++;
        return false;

      case "user_message":
        this.ask = event.text;
        this.turnsSinceAsk = 0;
        return false;

      // The process is gone: nothing of it runs any more.
      case "status":
        if (event.state === "exited") this.processGone();
        return false;
    }
    return false;
  }

  /**
   * A live batch got a subagent: once none has joined for a moment, ask for
   * its label, once. Parallel subagents arrive together, so this catches them.
   */
  private settle(batch: number): void {
    if (this.asked.has(batch)) return;
    clearTimeout(this.settling.get(batch));
    const timer = setTimeout(() => {
      this.settling.delete(batch);
      this.asked.add(batch);
      const agents = this.agents.filter((a) => a.batch === batch);
      if (agents.length && !this.titles[agents[0].toolId]) {
        this.onBatch?.(agents[0].toolId, agents[0].ask, agents.map((a) => a.name));
      }
    }, BATCH_SETTLE_MS);
    this.settling.set(batch, timer);
  }

  /** Nothing runs any more (the process exited, or a saved chat was opened). */
  processGone(): void {
    for (const agent of this.agents) if (agent.state === "running") this.end(agent, "stopped");
    this.background = [];
  }

  private end(agent: Agent, state: AgentState, now?: number): void {
    agent.state = state;
    agent.endedAt = now;
    agent.live = "";
  }
}

/** "41s", "4m 10s", "1h 5m": like the chat's working timer. */
export function span(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

/** "now", "52s ago", "3m ago". */
export function ago(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 5) return "now";
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  return `${Math.floor(m / 60)}h ago`;
}

/** How long an agent has run, or ran; "" when not known. */
export function runTime(agent: Agent, now: number): string {
  if (agent.state === "running") return agent.startedAt ? span(now - agent.startedAt) : "";
  if (agent.startedAt && agent.endedAt) return span(agent.endedAt - agent.startedAt);
  return agent.durationMs ? span(agent.durationMs) : "";
}

/** "92k", "1.2M", "640". */
export function tokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n >= 10_000_000 ? 0 : 1).replace(/\.0$/, "")}M`;
  if (n >= 1000) return `${Math.round(n / 1000)}k`;
  return String(n);
}
