// A markdown editor that shows its text formatted while you write in it, like
// Obsidian's live preview: headings, bold, code and links look as they do when
// read, and their marks (`**`, `[`…`](…)`) show only on the bit the cursor
// is in or touching (a heading's `##`: anywhere on its line). The text itself is never rewritten, so the file holds exactly
// what was typed. Loaded on demand (see LiveMarkdown.svelte): CodeMirror is
// the biggest library in the window, and only notes and plans use it.

import { defaultKeymap, history, historyKeymap, insertNewline } from "@codemirror/commands";
import { defineLanguageFacet, Language, LanguageSupport, syntaxTree } from "@codemirror/language";
import { GFM, parser as markdownParser } from "@lezer/markdown";
import { indent, newLine, outdent, renumbered, type Edit } from "./editing";
import { EditorState, type Range } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  placeholder as placeholderText,
  ViewPlugin,
  type ViewUpdate,
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
          case "QuoteMark":
          case "TaskMarker":
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

const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.viewportChanged || u.focusChanged) {
        this.decorations = decorate(u.view);
      }
    }
  },
  {
    decorations: (plugin) => plugin.decorations,
    eventHandlers: {
      // A shown link is followed, not edited: CodeMirror leaves the press
      // alone, and the click goes on to the panel's own link handling.
      mousedown(event) {
        const target = event.target as HTMLElement;
        if (target.closest("a.cm-md-link")) {
          event.preventDefault();
          return true;
        }
        return false;
      },
    },
  },
);

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

/** Makes the editor in `host`. Escape leaves it (blurs), as clicking away does. */
export function createLiveMarkdown(host: HTMLElement, options: LiveOptions): EditorView {
  return new EditorView({
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
              return true;
            },
          },
          { key: "Enter", run: (view) => listCommand(newLine)(view) || insertNewline(view) },
          { key: "Tab", run: listCommand(indent) },
          { key: "Shift-Tab", run: (view) => listCommand(outdent)(view) || true },
          ...historyKeymap,
          ...defaultKeymap,
        ]),
        markdownSupport,
        EditorView.lineWrapping,
        placeholderText(options.placeholder ?? ""),
        livePreview,
        look,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) options.onchange(u.state.doc.toString());
          if (u.focusChanged && !u.view.hasFocus) options.onblur?.();
        }),
      ],
    }),
  });
}
