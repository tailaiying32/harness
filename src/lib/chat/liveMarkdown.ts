// A markdown editor that shows its text formatted while you write in it, like
// Obsidian's live preview: headings, bold, code and links look as they do when
// read, and their marks (`**`, `[`…`](…)`) show only on the bit the cursor
// is in or touching (a heading's `##`: anywhere on its line). A table shows
// as a table until the cursor is in it, then as its source. The text itself
// is never rewritten, so the file holds exactly what was typed. Loaded on demand (see LiveMarkdown.svelte): CodeMirror is
// the biggest library in the window, and only notes and plans use it.

import { defaultKeymap, history, historyKeymap, insertNewline } from "@codemirror/commands";
import { defineLanguageFacet, forceParsing, Language, LanguageSupport, syntaxTree } from "@codemirror/language";
import { GFM, parser as markdownParser } from "@lezer/markdown";
import { emphasis, indent, link, newLine, outdent, pasteLink, renumbered, todo, type Edit } from "./editing";
import { renderMarkdown } from "../markdown";
import { commentsExtension } from "./commentMarks";
import type { DocComment } from "./comments";
import { EditorState, MapMode, type Range, StateEffect, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  placeholder as placeholderText,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from "@codemirror/view";

export type LiveOptions = {
  text: string;
  placeholder?: string;
  onchange: (text: string) => void;
  onblur?: () => void;
  /** Comments on the text, for a doc or a waiting plan (see commentMarks.ts); none for the notes. */
  comments?: DocComment[];
  oncomments?: (list: DocComment[]) => void;
};

/** Markdown with GitHub's additions (tables, strikethrough, task lists). Made
 *  straight from the parser: CodeMirror's markdown package also brings HTML,
 *  CSS and JavaScript support, which tripled the editor's size. */
const markdownSupport = new LanguageSupport(
  new Language(defineLanguageFacet(), markdownParser.configure([GFM]), [], "markdown"),
);

const hide = Decoration.replace({});
const mark = (cls: string) => Decoration.mark({ class: cls });
const line = (cls: string) => Decoration.line({ class: cls });

/** Lines with the cursor or a selection on them, where a heading's marks show. None while unfocused. */
function activeLines(view: EditorView): Set<number> {
  const lines = new Set<number>();
  if (!view.hasFocus) return lines;
  const doc = view.state.doc;
  for (const range of view.state.selection.ranges) {
    const first = doc.lineAt(range.from).number;
    const last = doc.lineAt(range.to).number;
    for (let n = first; n <= last; n++) lines.add(n);
  }
  return lines;
}

/** The cursor or a selection touches [from, to], ends included. Never while unfocused. */
function touched(view: EditorView, from: number, to: number): boolean {
  if (!view.hasFocus) return false;
  return view.state.selection.ranges.some((r) => r.from <= to && r.to >= from);
}

function decorate(view: EditorView): DecorationSet {
  const out: Range<Decoration>[] = [];
  const doc = view.state.doc;
  const active = activeLines(view);

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from,
      to,
      enter: (node) => {
        const name = node.name;
        const heading = /^ATXHeading(\d)$/.exec(name);
        if (heading) {
          out.push(line(`cm-md-h cm-md-h${heading[1]}`).range(doc.lineAt(node.from).from));
          return;
        }
        switch (name) {
          case "HeaderMark": {
            if (active.has(doc.lineAt(node.from).number)) {
              out.push(mark("cm-md-mark").range(node.from, node.to));
            } else {
              // The mark and the space after it.
              const end = doc.sliceString(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
              out.push(hide.range(node.from, end));
            }
            return;
          }
          case "StrongEmphasis":
            out.push(mark("cm-md-strong").range(node.from, node.to));
            return;
          case "Emphasis":
            out.push(mark("cm-md-em").range(node.from, node.to));
            return;
          case "Strikethrough":
            out.push(mark("cm-md-strike").range(node.from, node.to));
            return;
          case "InlineCode":
            out.push(mark("cm-md-code").range(node.from, node.to));
            return;
          case "EmphasisMark":
          case "StrikethroughMark":
          case "CodeMark": {
            // A fenced block's ``` stay; they are the block's edges. Other
            // marks show when the cursor touches their bit (bold, code…), so
            // both ends show together and nested bits show all their marks.
            const bit = node.node.parent;
            if (bit?.name === "FencedCode") {
              out.push(mark("cm-md-mark").range(node.from, node.to));
            } else if (bit && touched(view, bit.from, bit.to)) {
              out.push(mark("cm-md-mark").range(node.from, node.to));
            } else {
              out.push(hide.range(node.from, node.to));
            }
            return;
          }
          case "Link": {
            const marks = node.node.getChildren("LinkMark");
            const url = node.node.getChild("URL");
            if (marks.length < 2) return;
            const labelFrom = marks[0].to;
            const labelTo = marks[1].from;
            if (touched(view, node.from, node.to)) {
              out.push(mark("cm-md-mark").range(node.from, labelFrom));
              if (labelTo > labelFrom) out.push(mark("cm-md-linktext").range(labelFrom, labelTo));
              out.push(mark("cm-md-mark").range(labelTo, node.to));
            } else {
              // Shown as a link: the panel's click handler follows it.
              const href = url ? doc.sliceString(url.from, url.to) : "";
              out.push(hide.range(node.from, labelFrom));
              if (labelTo > labelFrom) {
                out.push(
                  Decoration.mark({ tagName: "a", class: "cm-md-link", attributes: { href } }).range(labelFrom, labelTo),
                );
              }
              out.push(hide.range(labelTo, node.to));
            }
            return false;
          }
          case "ListMark": {
            // A wrapped list line lines up under its text, as when read.
            const l = doc.lineAt(node.from);
            if (node.node.parent?.name === "ListItem" && node.from - l.from < 40) {
              const indent = node.to - l.from + 1;
              out.push(
                Decoration.line({
                  attributes: { style: `padding-left: ${indent}ch !important; text-indent: -${indent}ch` },
                }).range(l.from),
              );
            }
            out.push(mark("cm-md-mark").range(node.from, node.to));
            return;
          }
          case "TaskMarker": {
            // The box toggles on click (see the mousedown handler); a done
            // item's text goes dim.
            const done = /x/i.test(doc.sliceString(node.from, node.to));
            out.push(mark(done ? "cm-md-task cm-md-task-done" : "cm-md-task").range(node.from, node.to));
            const task = node.node.parent;
            if (done && task && task.to > node.to) out.push(mark("cm-md-done").range(node.to, task.to));
            return;
          }
          case "QuoteMark":
          case "TableDelimiter":
          case "CodeInfo":
          case "HorizontalRule":
            out.push(mark("cm-md-mark").range(node.from, node.to));
            return;
          case "Blockquote": {
            for (let pos = node.from; pos <= node.to; ) {
              const l = doc.lineAt(pos);
              out.push(line("cm-md-quote").range(l.from));
              pos = l.to + 1;
            }
            return;
          }
          case "FencedCode": {
            for (let pos = node.from; pos <= node.to; ) {
              const l = doc.lineAt(pos);
              out.push(line("cm-md-codeblock").range(l.from));
              pos = l.to + 1;
            }
            return;
          }
        }
      },
    });
  }
  return Decoration.set(out, true);
}

/** Ticks or clears the to-do box ("[ ]" or "[x]") starting at `at`. */
function toggleTask(view: EditorView, at: number): void {
  const box = view.state.doc.sliceString(at, at + 3);
  if (!/^\[[ xX]\]$/.test(box)) return;
  view.dispatch({ changes: { from: at + 1, to: at + 2, insert: box[1] === " " ? "x" : " " }, userEvent: "input" });
}

const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view);
    }
    update(u: ViewUpdate) {
      // A finished background parse changes only the tree: the text it
      // covers gets formatted then.
      const parsed = syntaxTree(u.state) !== syntaxTree(u.startState);
      if (u.docChanged || u.selectionSet || u.viewportChanged || u.focusChanged || parsed) {
        this.decorations = decorate(u.view);
      }
    }
  },
  {
    decorations: (plugin) => plugin.decorations,
    eventHandlers: {
      // A shown link is followed, not edited: CodeMirror leaves the press
      // alone, and the click goes on to the panel's own link handling.
      // A to-do's box is ticked or cleared, and the cursor stays put.
      mousedown(event, view) {
        const target = event.target as HTMLElement;
        if (target.closest("a.cm-md-link")) {
          event.preventDefault();
          return true;
        }
        const box = target.closest(".cm-md-task");
        if (box && event.button === 0) {
          event.preventDefault();
          toggleTask(view, view.posAtDOM(box));
          return true;
        }
        return false;
      },
    },
  },
);

// ---- Tables ----
// A table is drawn as a table, the way the chat draws one, until the cursor
// is in it; then its source shows for editing. It replaces whole lines, which
// CodeMirror only allows from a state field, not from the view plugin above,
// so the field keeps its own note of focus.

const setFocus = StateEffect.define<boolean>();
const hasFocus = StateField.define<boolean>({
  create: () => false,
  update: (value, tr) => tr.effects.reduce((v, e) => (e.is(setFocus) ? e.value : v), value),
});

/** Where a click on a table's cell puts the cursor: the start of that cell's text in the source. */
function cellPosition(state: EditorState, tableFrom: number, target: HTMLElement): number {
  const cell = target.closest("th, td") as HTMLTableCellElement | null;
  const row = cell?.parentElement as HTMLTableRowElement | null;
  if (!cell || !row) return tableFrom;
  // The header is the first line; the |---| line comes next, then the body rows.
  const offset = row.parentElement?.tagName === "THEAD" ? 0 : row.sectionRowIndex + 2;
  const first = state.doc.lineAt(tableFrom).number;
  if (first + offset > state.doc.lines) return tableFrom;
  const line = state.doc.line(first + offset);
  const text = line.text;
  // Step over the leading pipe, then past one unescaped pipe per earlier cell.
  let i = 0;
  while (text[i] === " ") i++;
  if (text[i] === "|") i++;
  for (let n = 0; n < cell.cellIndex && i < text.length; i++) {
    if (text[i] === "\\") i++;
    else if (text[i] === "|") n++;
  }
  while (text[i] === " ") i++;
  return line.from + Math.min(i, text.length);
}

class TableWidget extends WidgetType {
  constructor(readonly source: string) {
    super();
  }
  eq(other: TableWidget) {
    return other.source === this.source;
  }
  toDOM(view: EditorView) {
    const box = document.createElement("div");
    box.className = "md cm-md-table";
    // Safe: see the note at the top of markdown.ts.
    box.innerHTML = renderMarkdown(this.source);
    box.addEventListener("mousedown", (event) => {
      const target = event.target as HTMLElement;
      // A link is followed by the panel's click handling, not edited.
      if (target.closest("a")) return;
      event.preventDefault();
      view.dispatch({ selection: { anchor: cellPosition(view.state, view.posAtDOM(box), target) } });
      view.focus();
    });
    return box;
  }
  ignoreEvent() {
    return true;
  }
}

function tableDecorations(state: EditorState): DecorationSet {
  const out: Range<Decoration>[] = [];
  const focused = state.field(hasFocus);
  const doc = state.doc;
  syntaxTree(state).iterate({
    enter: (node) => {
      if (node.name !== "Table") return /^(Paragraph|FencedCode|CodeBlock|ATXHeading\d|HTMLBlock)$/.test(node.name) ? false : undefined;
      const editing = focused && state.selection.ranges.some((r) => r.from <= node.to && r.to >= node.from);
      // Only a table on whole lines: one in a quote or list stays as source.
      const whole = doc.lineAt(node.from).from === node.from && doc.lineAt(node.to).to === node.to;
      if (!editing && whole) {
        const widget = new TableWidget(doc.sliceString(node.from, node.to));
        out.push(Decoration.replace({ widget, block: true }).range(node.from, node.to));
      }
      return false;
    },
  });
  return Decoration.set(out);
}

const tables = StateField.define<DecorationSet>({
  create: tableDecorations,
  update(value, tr) {
    const redo =
      tr.docChanged ||
      tr.selection ||
      tr.effects.some((e) => e.is(setFocus)) ||
      syntaxTree(tr.state) !== syntaxTree(tr.startState);
    return redo ? tableDecorations(tr.state) : value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/**
 * Up or Down from next to a drawn table would jump the whole table, so it
 * could only be reached with the mouse. Land in it instead: on its first line
 * going down, its last going up, where its source then shows.
 */
function intoTable(forward: boolean) {
  return (view: EditorView): boolean => {
    const sel = view.state.selection.main;
    if (!sel.empty) return false;
    const next = view.moveVertically(sel, forward).head;
    let land = -1;
    view.state.field(tables).between(Math.min(sel.head, next), Math.max(sel.head, next), (from, to) => {
      if (forward && from > sel.head && from <= next) land = from;
      else if (!forward && to < sel.head && to >= next) land = view.state.doc.lineAt(to).from;
      else return;
      return false;
    });
    if (land < 0) return false;
    view.dispatch({ selection: { anchor: land }, scrollIntoView: true, userEvent: "select" });
    return true;
  };
}

const tablePreview = [hasFocus, tables, EditorView.focusChangeEffect.of((_state, focusing) => setFocus.of(focusing))];

/** Colours and spacing from the app's theme, matching formatted markdown (`.md` in app.css). */
const look = EditorView.theme({
  "&": { color: "var(--fg)", backgroundColor: "transparent" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "inherit", lineHeight: "inherit", overflow: "visible" },
  ".cm-content": { padding: "0", caretColor: "var(--accent)", minHeight: "12em" },
  ".cm-line": { padding: "0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)" },
  ".cm-placeholder": { color: "var(--faint)" },
  ".cm-md-mark": { color: "var(--faint)" },
  ".cm-md-task": { color: "var(--faint)", cursor: "pointer" },
  ".cm-md-task:hover": { color: "var(--bright)" },
  ".cm-md-task-done": { color: "var(--accent)" },
  ".cm-md-done": { color: "var(--dim)" },
  ".cm-md-h": { fontWeight: "700" },
  ".cm-md-h1": { color: "var(--accent)", borderBottom: "1px solid var(--line)", paddingBottom: "0.3em" },
  ".cm-md-h2": { color: "var(--bright)" },
  ".cm-md-strong": { fontWeight: "700", color: "var(--bright)" },
  ".cm-md-em": { fontStyle: "italic" },
  ".cm-md-strike": { textDecoration: "line-through" },
  ".cm-md-code": { backgroundColor: "var(--panel)", color: "var(--bright)" },
  ".cm-md-link, .cm-md-linktext": { color: "var(--accent)" },
  ".cm-md-link": { textDecoration: "underline", textUnderlineOffset: "2px", cursor: "pointer" },
  ".cm-md-quote": { color: "var(--dim)", borderLeft: "1px solid var(--dim)", paddingLeft: "2ch !important" },
  ".cm-md-codeblock": { backgroundColor: "var(--panel)", paddingLeft: "1ch !important" },
  // Words stay whole (the editor's wrapping would split them); a table too
  // wide for the panel scrolls on its own instead.
  ".cm-md-table": { whiteSpace: "normal", wordBreak: "normal", overflowWrap: "normal", overflowX: "auto", cursor: "text" },
});

/**
 * Closing marks the editor put in itself: the `)` after a typed `(`, the `**`
 * after Cmd+B. Typing the same character just before one steps over it, and
 * Backspace between it and its opening mark removes both. Only marks it put
 * in, so a `*` the user typed is never skipped; forgotten once the cursor
 * leaves their line.
 */
const addClosers = StateEffect.define<number[]>();
const dropCloser = StateEffect.define<number>();
const closers = StateField.define<number[]>({
  create: () => [],
  update(value, tr) {
    let out = value;
    if (tr.docChanged) {
      out = out.map((p) => tr.changes.mapPos(p, 1, MapMode.TrackAfter)).filter((p): p is number => p !== null);
    }
    for (const e of tr.effects) {
      if (e.is(addClosers)) out = [...out, ...e.value];
      else if (e.is(dropCloser)) out = out.filter((p) => p !== e.value);
    }
    if (out.length && (tr.selection || tr.docChanged)) {
      const line = tr.state.doc.lineAt(tr.state.selection.main.head);
      out = out.filter((p) => p >= line.from && p < line.to);
    }
    return out;
  },
});

/** What typing an opening mark adds after the cursor. Not quotes (apostrophes); `*` waits (`starAt`). */
const PAIRS: Record<string, string> = { "(": ")", "[": "]", "{": "}", "`": "`" };

/**
 * Where the cursor stands just after a `*` the user typed, else -1. A `*` is
 * a list item or bold/italic, so it isn't paired at once: the next key
 * decides. A space leaves it alone; a word's first character after `*`,
 * `**` or `***` puts the same stars in after that character. Any other
 * change or cursor move forgets it.
 */
const typedStar = StateEffect.define<number>();
const starAt = StateField.define<number>({
  create: () => -1,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(typedStar)) return e.value;
    return tr.docChanged || tr.selection ? -1 : value;
  },
});

/** Only before a space, the line's end or closing punctuation: before a word, the user is wrapping it by hand. */
const OPEN_BEFORE = /[\s)\]}.,;:!?*_`]/;

const pairTyping = EditorView.inputHandler.of((view, from, to, text) => {
  const { state } = view;
  const sel = state.selection.main;
  if (text.length !== 1 || state.selection.ranges.length > 1 || !sel.empty || from !== sel.from || to !== sel.to) return false;
  const next = state.doc.sliceString(from, from + 1);
  if (next === text && state.field(closers).includes(from)) {
    view.dispatch({ selection: { anchor: from + 1 }, effects: dropCloser.of(from), userEvent: "input.type" });
    return true;
  }
  if (text === "*") {
    view.dispatch({ changes: { from, insert: "*" }, selection: { anchor: from + 1 }, effects: typedStar.of(from + 1), userEvent: "input.type" });
    return true;
  }
  if (state.field(starAt) === from && !/[\s*)\]}.,;:!?]/.test(text) && (!next || OPEN_BEFORE.test(next))) {
    let run = 0;
    while (run < 3 && from - run > 0 && state.doc.sliceString(from - run - 1, from - run) === "*") run++;
    // Stars that open: at the line's start or after a space or opening
    // punctuation (not `2*3`), and not in code.
    const before = from - run > 0 ? state.doc.sliceString(from - run - 1, from - run) : "";
    const code = /Code/.test(syntaxTree(state).resolveInner(from, -1).name);
    if (run && (!before || /[\s([{"']/.test(before)) && !code) {
      const end = from + 1;
      view.dispatch({
        changes: { from, insert: text + "*".repeat(run) },
        selection: { anchor: end },
        effects: addClosers.of(Array.from({ length: run }, (_, i) => end + i)),
        userEvent: "input.type",
      });
      return true;
    }
  }
  const close = PAIRS[text];
  if (!close || (next && !OPEN_BEFORE.test(next))) return false;
  // A backtick after a word or a backtick closes or extends a code mark: no pair.
  if (text === "`" && /[\p{L}\p{N}_`]/u.test(state.doc.sliceString(from - 1, from))) return false;
  view.dispatch({
    changes: { from, insert: text + close },
    selection: { anchor: from + 1 },
    effects: addClosers.of([from + 1]),
    userEvent: "input.type",
  });
  return true;
});

function deletePair(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  if (state.selection.ranges.length > 1 || !sel.empty || !state.field(closers).includes(sel.head)) return false;
  const open = state.doc.sliceString(sel.head - 1, sel.head);
  const shut = state.doc.sliceString(sel.head, sel.head + 1);
  if (PAIRS[open] !== shut && !(open === "*" && shut === "*")) return false;
  view.dispatch({ changes: { from: sel.head - 1, to: sel.head + 1 }, userEvent: "delete.backward", scrollIntoView: true });
  return true;
}

/** Pasting a web address over selected text makes it a link (editing.ts). */
const pasteLinks = EditorView.domEventHandlers({
  paste(event, view) {
    const pasted = event.clipboardData?.getData("text/plain");
    const { from, to } = view.state.selection.main;
    const edit = pasted ? pasteLink(view.state.doc.toString(), from, to, pasted) : null;
    if (!edit) return false;
    event.preventDefault();
    view.dispatch({
      changes: { from: edit.from, to: edit.to, insert: edit.insert },
      selection: { anchor: edit.selStart },
      scrollIntoView: true,
      userEvent: "input.paste",
    });
    return true;
  },
});

/**
 * The message box's list editing (editing.ts), so notes and plans behave the
 * same: Enter in a list starts the next item (an empty one steps back or ends
 * the list), Tab and Shift+Tab indent, and numbered lists stay in order.
 */
function listCommand(make: (text: string, start: number, end: number) => Edit | null, renumber = true) {
  return (view: EditorView): boolean => {
    const text = view.state.doc.toString();
    const { from, to } = view.state.selection.main;
    const edit = make(text, from, to);
    if (!edit) return false;
    const done = renumber ? renumbered(text, edit) : edit;
    view.dispatch({
      changes: { from: done.from, to: done.to, insert: done.insert },
      selection: { anchor: done.selStart, head: done.selEnd },
      effects: done.closers?.length ? addClosers.of(done.closers) : [],
      scrollIntoView: true,
      userEvent: "input",
    });
    return true;
  };
}

/** Makes the editor in `host`. Escape leaves it (blurs), as clicking away does,
    and goes on to the window, so the same press also ends a full screen panel. */
export function createLiveMarkdown(host: HTMLElement, options: LiveOptions): EditorView {
  const view = new EditorView({
    parent: host,
    state: EditorState.create({
      doc: options.text,
      extensions: [
        history(),
        keymap.of([
          {
            key: "Escape",
            run: (view) => {
              view.contentDOM.blur();
              return false;
            },
          },
          { key: "Enter", run: (view) => listCommand(newLine)(view) || insertNewline(view) },
          { key: "Tab", run: listCommand(indent) },
          { key: "ArrowDown", run: intoTable(true) },
          { key: "ArrowUp", run: intoTable(false) },
          { key: "Shift-Tab", run: (view) => listCommand(outdent)(view) || true },
          // Cmd+L: to-do boxes on the lines touched, or off if they all have them.
          { key: "Mod-l", run: listCommand(todo), preventDefault: true },
          // Bold, italic and link, on or off. Cmd+I replaces CodeMirror's
          // rarely used select-parent.
          { key: "Mod-b", run: listCommand((t, s, e) => emphasis(t, s, e, 2), false), preventDefault: true },
          { key: "Mod-i", run: listCommand((t, s, e) => emphasis(t, s, e, 1), false), preventDefault: true },
          { key: "Mod-k", run: listCommand(link, false), preventDefault: true },
          { key: "Backspace", run: deletePair },
          ...historyKeymap,
          ...defaultKeymap,
        ]),
        markdownSupport,
        EditorView.lineWrapping,
        placeholderText(options.placeholder ?? ""),
        livePreview,
        tablePreview,
        closers,
        starAt,
        pairTyping,
        pasteLinks,
        look,
        options.comments ? commentsExtension(options.comments, options.oncomments) : [],
        EditorView.updateListener.of((u) => {
          if (u.docChanged) options.onchange(u.state.doc.toString());
          if (u.focusChanged && !u.view.hasFocus) options.onblur?.();
        }),
      ],
    }),
  });
  // CodeMirror parses only the first 3000 characters up front and the rest
  // in the background, which in this webview (no requestIdleCallback) waits
  // half a second: the doc showed as plain text until then. A doc parses in
  // a few milliseconds, so do it now; a huge one stops at the time limit and
  // finishes in the background as before.
  forceParsing(view, view.state.doc.length, 50);
  return view;
}
