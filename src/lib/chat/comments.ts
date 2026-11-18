// Comments on a doc or a waiting plan in the side panel: the user selects
// text, writes a comment, and sends them all to the agent together. They live
// only until sent (on the chat, never in the file), so they never have to
// survive the agent rewriting the text. What the agent gets is plain text:
// each comment with its line and the text it is about, the same for every
// agent.

export type DocComment = {
  id: string;
  /** Offsets in the tab's current text; both -1 once the commented text is gone. */
  from: number;
  to: number;
  /** The commented text when the comment was written, kept if the text goes. */
  quote: string;
  /** The comment itself. */
  text: string;
  /** The message it went with (docs): shown faint, not editable, dropped when that turn ends. */
  sentWith?: string;
};

/** Said with every set of comments: the agent asks rather than guesses. */
export const GUIDANCE =
  "Change the text where a comment asks for a change; answer questions in the chat. If a comment is unclear, ask in the chat instead of guessing.";

/** Said with comments that go along with an approved plan. */
export const BUILD_GUIDANCE =
  "Take these into account while building. If one is unclear, ask in the chat before acting on it.";

const MAX_QUOTE = 200;

function quoted(text: string): string {
  const one = text.replace(/\s+/g, " ").trim();
  return one.length > MAX_QUOTE ? `${one.slice(0, MAX_QUOTE - 1)}…` : one;
}

/** The 1-based line `at` falls on in `text`. */
export function lineAt(text: string, at: number): number {
  let line = 1;
  for (let i = 0; i < at && i < text.length; i++) if (text[i] === "\n") line++;
  return line;
}

/** The comments in the order they appear in the text; ones whose text is gone last. */
export function inOrder(comments: DocComment[]): DocComment[] {
  return [...comments].sort((a, b) => (a.from < 0 ? Infinity : a.from) - (b.from < 0 ? Infinity : b.from));
}

/** One numbered line per comment, with where it is and what it is about. */
export function commentList(comments: DocComment[], text: string): string {
  return inOrder(comments)
    .map((c, i) => {
      const on = c.from < 0 ? `On "${quoted(c.quote)}" (text since removed)` : `Line ${lineAt(text, c.from)}, on "${quoted(c.quote)}"`;
      return `${i + 1}. ${on}: ${c.text.trim()}`;
    })
    .join("\n");
}

/** The comments as a block for a message: "Comments on <where>:" and the list. */
export function formatComments(comments: DocComment[], text: string, where: string): string {
  return `Comments on ${where}:\n\n${commentList(comments, text)}`;
}

/** Two comment lists hold the same comments in the same places. */
export function sameComments(a: DocComment[], b: DocComment[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((c, i) => {
    const d = b[i];
    return c.id === d.id && c.from === d.from && c.to === d.to && c.text === d.text && c.sentWith === d.sentWith;
  });
}

/**
 * A comment on the agent's latest answer in the chat. Like a doc's, it lives
 * only until sent, then goes; nothing keeps it. `from` and `to` are offsets
 * in the answer piece's drawn text (its `.md` element's text), which never
 * changes once that piece is finished, the only kind that takes comments.
 */
export type ChatComment = {
  id: string;
  /** The text item (one piece of the answer) it is on. */
  itemId: number;
  from: number;
  to: number;
  quote: string;
  text: string;
};

/** Said with every message that carries chat comments. */
export const CHAT_GUIDANCE =
  "Each comment is about the quoted part of your last answer. Respond to each, acting on any that ask for a change; if one is unclear, ask instead of guessing.";

/** Chat comments in the order they appear in the answer: by piece, then place. */
export function chatInOrder(comments: ChatComment[]): ChatComment[] {
  return [...comments].sort((a, b) => a.itemId - b.itemId || a.from - b.from);
}

/** The message that carries chat comments: what was typed, then each quote with its comment. */
export function withChatComments(typed: string, comments: ChatComment[]): string {
  if (!comments.length) return typed;
  const list = chatInOrder(comments)
    .map((c, i) => `${i + 1}. On "${quoted(c.quote)}": ${c.text.trim()}`)
    .join("\n");
  const block = `Comments on your last answer:\n\n${list}`;
  return typed.trim() ? `${typed.trim()}\n\n${block}` : block;
}
