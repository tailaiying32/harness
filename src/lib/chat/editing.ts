// Document-style editing for the message box: Tab indents, Shift+Tab pulls
// back, and a new line inside a list starts the next item. Each function
// works on plain text and a selection, and returns one replacement to make.

/** Replace `text[from, to)` with `insert`, then select `[selStart, selEnd)`. */
export type Edit = {
  from: number;
  to: number;
  insert: string;
  selStart: number;
  selEnd: number;
  /** The replaced lines changed level, so one that now starts a numbered list counts from 1. */
  relevel?: boolean;
};

const INDENT = "  ";

/** A list item's start: indent, marker ("-", "*", "+", "1." or "1)"), optional checkbox. */
const ITEM = /^(\s*)([-*+]|(\d+)([.)]))(\s+)(\[[ xX]\]\s+)?/;

const lineStart = (text: string, at: number) => text.lastIndexOf("\n", at - 1) + 1;
const lineEnd = (text: string, at: number) => {
  const end = text.indexOf("\n", at);
  return end < 0 ? text.length : end;
};

/** The whole lines a selection touches. One that only reaches a line's start doesn't count that line. */
function block(text: string, start: number, end: number): { from: number; to: number } {
  const last = end > start && text[end - 1] === "\n" ? end - 1 : end;
  return { from: lineStart(text, start), to: lineEnd(text, last) };
}

/** Tab: with no selection, two spaces at the cursor's line start; with one, every line it touches. */
export function indent(text: string, start: number, end: number): Edit {
  const { from, to } = block(text, start, end);
  const lines = text.slice(from, to).split("\n");
  const insert = lines.map((line) => INDENT + line).join("\n");
  return {
    relevel: true,
    from,
    to,
    insert,
    selStart: start + INDENT.length,
    selEnd: start === end ? start + INDENT.length : to + INDENT.length * lines.length,
  };
}

/** Shift+Tab: up to two spaces (or one tab) off the front of every line the selection touches. */
export function outdent(text: string, start: number, end: number): Edit | null {
  const { from, to } = block(text, start, end);
  const lines = text.slice(from, to).split("\n");
  const cut = lines.map((line): number => (line.startsWith("\t") ? 1 : line.startsWith(INDENT) ? 2 : line.startsWith(" ") ? 1 : 0));
  const total = cut.reduce((sum, n) => sum + n, 0);
  if (total === 0) return null;
  const insert = lines.map((line, i) => line.slice(cut[i])).join("\n");
  // The cursor moves back with its line, but never past the line's start.
  const selStart = Math.max(from, start - cut[0]);
  return { relevel: true, from, to, insert, selStart, selEnd: start === end ? selStart : to - total };
}

/**
 * A new line (Shift+Enter) inside a list. After an item with text, the next
 * item starts: same indent, same marker, the next number. On an item that is
 * still empty, the item steps back one level, or goes away at the top level,
 * which ends the list. Anywhere else: null, for the box's ordinary new line.
 */
export function newLine(text: string, start: number, end: number): Edit | null {
  const from = lineStart(text, start);
  const to = lineEnd(text, start);
  const line = text.slice(from, to);
  const item = ITEM.exec(line);
  // Not in a list, or the cursor sits inside the marker itself.
  if (!item || start - from < item[0].length) return null;

  const [whole, lead, marker, number, delimiter, , checkbox] = item;
  if (line.length === whole.length && start === end) {
    const insert = lead.length > 0 ? line.slice(lead.startsWith("\t") ? 1 : Math.min(INDENT.length, lead.length)) : "";
    return { relevel: true, from, to, insert, selStart: from + insert.length, selEnd: from + insert.length };
  }

  const next = number ? `${Number(number) + 1}${delimiter}` : marker;
  const insert = `\n${lead}${next} ${checkbox ? "[ ] " : ""}`;
  return { from: start, to: end, insert, selStart: start + insert.length, selEnd: start + insert.length };
}

const width = (lead: string) => lead.replace(/\t/g, INDENT).length;

/**
 * Follows an edit with the renumbering it calls for, as one replacement (so
 * one undo step). Numbered lists are put back in order in the run of
 * non-blank lines around the edit: each item counts on from the one before
 * it at the same indent. A list's first item keeps its number, unless the
 * edit just moved it to that level, where it starts at 1. A bullet, or a
 * line that isn't indented under the list, ends it.
 */
export function renumbered(text: string, edit: Edit): Edit {
  const after = text.slice(0, edit.from) + edit.insert + text.slice(edit.to);
  const changedTo = edit.from + edit.insert.length;

  // The run of non-blank lines around the edit.
  const blank = (from: number, to: number) => after.slice(from, to).trim() === "";
  let top = lineStart(after, edit.from);
  while (top > 0 && !blank(lineStart(after, top - 1), top - 1)) top = lineStart(after, top - 1);
  let bottom = lineEnd(after, changedTo);
  while (bottom < after.length && !blank(bottom + 1, lineEnd(after, bottom + 1))) bottom = lineEnd(after, bottom + 1);

  // One entry per open list level: its indent, and its last number (null for bullets).
  const open: { indent: number; n: number | null; delimiter: string }[] = [];
  /** Where a number changed length: the position just past the old marker, and by how much. */
  const shifts: { at: number; by: number }[] = [];
  let out = "";
  let at = top;
  for (const line of after.slice(top, bottom).split("\n")) {
    const item = ITEM.exec(line);
    const indent = width(/^\s*/.exec(line)![0]);
    let written = line;
    if (!item) {
      // Text indented under an item belongs to it; anything else ends the lists it isn't under.
      while (open.length && open[open.length - 1].indent >= indent) open.pop();
    } else {
      while (open.length && open[open.length - 1].indent > indent) open.pop();
      const level = open[open.length - 1]?.indent === indent ? open.pop()! : null;
      const [, lead, , number, delimiter] = item;
      if (!number) {
        open.push({ indent, n: null, delimiter: "" });
      } else {
        const moved = !!edit.relevel && at < changedTo && at + line.length >= edit.from;
        const follows = level !== null && level.n !== null && level.delimiter === delimiter;
        const n = follows ? level.n! + 1 : moved ? 1 : Number(number);
        open.push({ indent, n, delimiter });
        if (String(n) !== number) {
          written = lead + n + line.slice(lead.length + number.length);
          shifts.push({ at: at + lead.length + number.length, by: String(n).length - number.length });
        }
      }
    }
    out += (out || at > top ? "\n" : "") + written;
    at += line.length + 1;
  }
  if (shifts.length === 0) return edit;

  const final = after.slice(0, top) + out + after.slice(bottom);
  const moved = (p: number) => p + shifts.reduce((sum, shift) => (p >= shift.at ? sum + shift.by : sum), 0);

  // The smallest single replacement that turns `text` into `final`.
  let from = 0;
  while (from < text.length && from < final.length && text[from] === final[from]) from++;
  let tail = 0;
  while (tail < text.length - from && tail < final.length - from && text[text.length - 1 - tail] === final[final.length - 1 - tail]) tail++;
  return {
    from,
    to: text.length - tail,
    insert: final.slice(from, final.length - tail),
    selStart: moved(edit.selStart),
    selEnd: moved(edit.selEnd),
  };
}
