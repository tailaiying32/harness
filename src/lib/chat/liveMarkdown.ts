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
import { indent, newLine, outdent, renumbered, todo, type Edit } from "./editing";
import { renderMarkdown } from "../markdown";
import { EditorState, type Range, StateEffect, StateField } from "@codemirror/state";
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
 * The message box's list editing (editing.ts), so notes and plans behave the
 * same: Enter in a list starts the next item (an empty one steps back or ends
 * the list), Tab and Shift+Tab indent, and numbered lists stay in order.
 */
function listCommand(make: (text: string, start: number, end: number) => Edit | null) {
  return (view: EditorView): boolean => {
    const text = view.state.doc.toString();
    const { from, to } = view.state.selection.main;
    const edit = make(text, from, to);
    if (!edit) return false;
    const done = renumbered(text, edit);
    view.dispatch({
      changes: { from: done.from, to: done.to, insert: done.insert },
      selection: { anchor: done.selStart, head: done.selEnd },
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
          ...historyKeymap,
          ...defaultKeymap,
        ]),
        markdownSupport,
        EditorView.lineWrapping,
        placeholderText(options.placeholder ?? ""),
        livePreview,
        tablePreview,
        look,
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
