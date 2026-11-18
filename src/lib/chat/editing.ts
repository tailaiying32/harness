// Document-style editing for the message box and the docs editor: Tab
// indents, Shift+Tab pulls back, a new line inside a list starts the next
// item, Cmd+B, Cmd+I and Cmd+K format, and pasting an address over text
// links it. Each function works on plain text and a selection, and returns
// one replacement to make.

/** Replace `text[from, to)` with `insert`, then select `[selStart, selEnd)`. */
export type Edit = {
  from: number;
  to: number;
  insert: string;
  selStart: number;
  selEnd: number;
  /** The replaced lines changed level, so one that now starts a numbered list counts from 1. */
  relevel?: boolean;
  /** Closing marks the edit put in, by position after it: typing the same character there steps over it (docs editor). */
  closers?: number[];
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

/**
 * Cmd+L: makes every line the selection touches a to-do ("- [ ] "), keeping
 * its indent; a list item keeps its marker and gains the box. If they are all
 * to-dos already, they go back to plain lines. Blank lines are left alone,
 * unless the cursor is on one: it becomes an empty to-do.
 */
export function todo(text: string, start: number, end: number): Edit {
  const { from, to } = block(text, start, end);
  const lines = text.slice(from, to).split("\n");
  const single = lines.length === 1;
  const isTodo = (line: string) => !!ITEM.exec(line)?.[6];
  const undo = lines.every((line) => isTodo(line) || (!single && line.trim() === ""));

  // Each line's old and new start (indent, marker, box), to move the cursor with its text.
  const heads: { old: number; now: number }[] = [];
  const out = lines.map((line) => {
    const item = ITEM.exec(line);
    const lead = /^\s*/.exec(line)![0];
    let head: string;
    let cut: number;
    if (undo) {
      if (!item) return heads.push({ old: 0, now: 0 }), line;
      [head, cut] = [item[1], item[0].length];
    } else if (line.trim() === "" && !single) {
      return heads.push({ old: 0, now: 0 }), line;
    } else if (item?.[6]) {
      [head, cut] = [item[0], item[0].length];
    } else if (item) {
      [head, cut] = [`${item[0]}[ ] `, item[0].length];
    } else {
      [head, cut] = [`${lead}- [ ] `, lead.length];
    }
    heads.push({ old: cut, now: head.length });
    return head + line.slice(cut);
  });

  const moved = (p: number) => {
    let oldAt = from;
    let newAt = from;
    for (let i = 0; i < lines.length; i++) {
      const { old, now } = heads[i];
      if (p <= oldAt + lines[i].length) return newAt + Math.max(now, p - oldAt - old + now);
      oldAt += lines[i].length + 1;
      newAt += out[i].length + 1;
    }
    return p - oldAt + newAt;
  };
  return { from, to, insert: out.join("\n"), selStart: moved(start), selEnd: moved(end) };
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

/** A character of a word, for formatting the word at the cursor. */
const WORD = /[\p{L}\p{N}_'’-]/u;

/** What a format key acts on: the selection without spaces at its ends, or with none, the word at the cursor (empty between words). */
function target(text: string, start: number, end: number): { from: number; to: number } {
  if (start === end) {
    while (start > 0 && WORD.test(text[start - 1])) start--;
    while (end < text.length && WORD.test(text[end])) end++;
  } else {
    while (start < end && /\s/.test(text[start])) start++;
    while (end > start && /\s/.test(text[end - 1])) end--;
  }
  return { from: start, to: end };
}

/**
 * Cmd+B (`bit` 2, `**`) and Cmd+I (`bit` 1, `*`): turns bold or italic on or
 * off for the selection, or the word at the cursor. Marks just outside it, or
 * at the selection's own ends, count as on; `***` is both. With no word, an
 * empty pair goes in with the cursor between.
 */
export function emphasis(text: string, start: number, end: number, bit: 1 | 2): Edit {
  let { from, to } = target(text, start, end);
  while (from < to && text[from] === "*") from++;
  while (to > from && text[to - 1] === "*") to--;
  let left = 0;
  while (from - left > 0 && text[from - left - 1] === "*") left++;
  let right = 0;
  while (to + right < text.length && text[to + right] === "*") right++;
  const have = Math.min(left, right, 3);
  const want = have ^ bit;
  const marks = "*".repeat(want);
  const core = text.slice(from, to);
  const at = from - have + want;
  const keep = start === end;
  const shut = at + core.length;
  return {
    closers: Array.from({ length: want }, (_, i) => shut + i),
    from: from - have,
    to: to + have,
    insert: marks + core + marks,
    selStart: keep ? Math.min(Math.max(start - have + want, at), at + core.length) : at,
    selEnd: keep ? Math.min(Math.max(start - have + want, at), at + core.length) : at + core.length,
  };
}

const LINK = /\[([^\]\n]*)\]\(([^)\n]*)\)/g;

/**
 * Cmd+K: makes the selection, or the word at the cursor, a link's text with
 * the cursor where the address goes. A selected address goes in the address
 * with the cursor in the text. Inside a link already, it goes back to plain text.
 */
export function link(text: string, start: number, end: number): Edit {
  const ls = lineStart(text, start);
  for (const m of text.slice(ls, lineEnd(text, end)).matchAll(LINK)) {
    const a = ls + m.index;
    const b = a + m[0].length;
    if (start === end ? a < start && start < b : a <= start && end <= b) {
      const words = m[1];
      const pos = Math.min(Math.max(start - 1, a), a + words.length);
      return { from: a, to: b, insert: words, selStart: start === end ? pos : a, selEnd: start === end ? pos : a + words.length };
    }
  }
  let { from, to } = target(text, start, end);
  if (start === end) {
    // The cursor in an address takes all of it, not just the word.
    let a = start;
    let b = start;
    while (a > 0 && /\S/.test(text[a - 1])) a--;
    while (b < text.length && /\S/.test(text[b])) b++;
    if (URL_TEXT.test(text.slice(a, b))) [from, to] = [a, b];
  }
  const core = text.slice(from, to);
  if (URL_TEXT.test(core)) {
    return { from, to, insert: `[](${core})`, selStart: from + 1, selEnd: from + 1, closers: [from + 1] };
  }
  const at = core ? from + core.length + 3 : from + 1;
  const shut = from + core.length + 1;
  return { from, to, insert: `[${core}]()`, selStart: at, selEnd: at, closers: [shut, shut + 1, shut + 2] };
}

const URL_TEXT = /^(https?:\/\/|www\.)\S+$/;

/** Pasting a web address over selected text on one line makes that text a link to it; null for any other paste. */
export function pasteLink(text: string, start: number, end: number, pasted: string): Edit | null {
  const url = pasted.trim();
  if (!URL_TEXT.test(url) || start === end || text.slice(start, end).includes("\n")) return null;
  const { from, to } = target(text, start, end);
  if (from === to) return null;
  const insert = `[${text.slice(from, to)}](${url})`;
  return { from, to, insert, selStart: from + insert.length, selEnd: from + insert.length };
}

/** Puts the cursor in a box, at the end of its text. */
export function focusEnd(box: HTMLTextAreaElement): void {
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
}
