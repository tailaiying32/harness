// Comments in the live markdown editor (see comments.ts for what they are).
// The editor holds each comment's place and moves it with the text, whoever
// types; the chat holds the list (via `oncomments`) so a comment outlives the
// editor, which is rebuilt each time a tab shows.
//
// What shows, at most one floating window at a time, each below its text:
// - while writing a comment: the input, under the commented text
// - the pointer resting on a commented spot, or the cursor in one: that
//   comment, edited in place, with ×
// - text selected: a small "+ comment" button under the selection
// Windows are CodeMirror tooltips, so they follow the text as it scrolls.

import { EditorSelection, type EditorState, type Extension, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, repositionTooltips, showTooltip, tooltips, ViewPlugin, type Tooltip, type TooltipView } from "@codemirror/view";
import { action, commentBox, commentLook, el, HoverIntent, type CommentBox } from "./commentBox";
import type { DocComment } from "./comments";

/** Puts a list from outside in place of the editor's (cleared after a send, say). */
export const replaceComments = StateEffect.define<DocComment[]>();
const addComment = StateEffect.define<DocComment>();
const editComment = StateEffect.define<{ id: string; text: string }>();
const removeComment = StateEffect.define<string>();

/** The comments, their places moved through every change to the text. */
export const commentField = StateField.define<DocComment[]>({
  create: () => [],
  update(list, tr) {
    let next = list;
    if (tr.docChanged) {
      next = next.map((c) => {
        if (c.from < 0) return c;
        // Typing just before or just after the commented text stays outside it.
        const from = tr.changes.mapPos(c.from, 1);
        const to = tr.changes.mapPos(c.to, -1);
        if (to <= from) return { ...c, from: -1, to: -1 };
        return from === c.from && to === c.to ? c : { ...c, from, to };
      });
    }
    for (const e of tr.effects) {
      if (e.is(replaceComments)) next = e.value;
      else if (e.is(addComment)) next = [...next, e.value];
      else if (e.is(editComment)) next = next.map((c) => (c.id === e.value.id ? { ...c, text: e.value.text } : c));
      else if (e.is(removeComment)) next = next.filter((c) => c.id !== e.value);
    }
    return next;
  },
});

/** A comment being written (or edited), and its window's maker, kept so the window lasts while the text changes. */
type Input = { from: number; to: number; id?: string; text: string; create: (view: EditorView) => TooltipView };

const startInput = StateEffect.define<Omit<Input, "create">>();
const endInput = StateEffect.define<null>();

const inputField = StateField.define<Input | null>({
  create: () => null,
  update(input, tr) {
    for (const e of tr.effects) {
      if (e.is(startInput)) {
        // A maker of its own, so each comment written gets a fresh window.
        const create = (view: EditorView): TooltipView => inputWindow(view, create);
        return { ...e.value, create };
      }
      if (e.is(endInput)) return null;
    }
    if (input && tr.docChanged) {
      return { ...input, from: tr.changes.mapPos(input.from, 1), to: tr.changes.mapPos(input.to, -1) };
    }
    return input;
  },
});

const setFocused = StateEffect.define<boolean>();
const focused = StateField.define<boolean>({
  create: () => false,
  update: (value, tr) => tr.effects.reduce((v, e) => (e.is(setFocused) ? e.value : v), value),
});

/** A written comment whose box has the cursor: its window stays while the doc is unfocused. */
const holdComment = StateEffect.define<string | null>();
const held = StateField.define<string | null>({
  create: () => null,
  update: (value, tr) => tr.effects.reduce((v, e) => (e.is(holdComment) ? e.value : v), value),
});

/**
 * Where the cursor was left by closing a comment (Enter or Escape): no
 * window opens there until the cursor moves, so closing really closes.
 */
const setQuiet = StateEffect.define<number | null>();
const quiet = StateField.define<number | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setQuiet)) return e.value;
    return tr.selection || tr.docChanged ? null : value;
  },
});

/** The comment the pointer rests on (or on its window): shown, focused or not. */
const setHovered = StateEffect.define<string | null>();
const hovered = StateField.define<string | null>({
  create: () => null,
  update: (value, tr) => tr.effects.reduce((v, e) => (e.is(setHovered) ? e.value : v), value),
});

/**
 * Hover: resting on a commented spot opens its window (see HoverIntent).
 * It never moves the cursor or takes the focus. Commented text is for the
 * agent to change, so a window over it rarely gets in the way.
 */
class Hover extends HoverIntent {
  constructor(view: EditorView) {
    super(
      () => view.state.field(hovered),
      (id) => view.dispatch({ effects: setHovered.of(id) }),
    );
  }
}

const hover = ViewPlugin.fromClass(Hover, {
  eventHandlers: {
    mousemove(e, view) {
      const target = e.target as Element | null;
      const onMark = target?.closest?.(".cm-comment:not(.cm-comment-sent)");
      const pos = onMark ? view.posAtCoords({ x: e.clientX, y: e.clientY }, false) : null;
      this.over(pos === null ? null : (commentAt(view.state, pos)?.id ?? null));
    },
    mouseleave() {
      this.over(null);
    },
  },
});

/** The commented spot under the cursor: the latest one starting at or before it. */
function commentAt(state: EditorState, at: number): DocComment | null {
  let best: DocComment | null = null;
  for (const c of state.field(commentField)) {
    if (c.sentWith || c.from < 0 || at < c.from || at > c.to) continue;
    if (!best || c.from > best.from) best = c;
  }
  return best;
}

/**
 * Where a window `width` wide hangs: under the last screen line of the text.
 * While it is narrower than the text on that line, its right edge sits at
 * the text's end; wider, it starts where the text starts (on a wrapped
 * line: where that line's text starts, past any list marker) and grows
 * right. So the button, the box it turns into and the finished comment all
 * sit in one place, and a box grows left, then right, as it is typed in.
 * A window narrower than the button is placed as if it were the button's
 * width, so the box opens with its left edge exactly where the button's was.
 */
function under(view: EditorView, from: number, to: number, width: number) {
  const lineStart = view.moveToLineBoundary(EditorSelection.cursor(to, -1), false, true).head;
  const start = view.coordsAtPos(Math.max(from, lineStart), 1);
  const end = view.coordsAtPos(to, -1);
  if (!start || !end) return { left: 0, right: 0, top: -1e5, bottom: -1e5 };
  // The button: "+ comment", 1ch of padding each side, and its border.
  const button = view.defaultCharacterWidth * 11 + 2;
  const left = Math.max(start.left, end.right - Math.max(width, button));
  return { left, right: left, top: end.top, bottom: end.bottom };
}

function plusButton(view: EditorView): TooltipView {
  const dom = el("div", "cm-comment-plus");
  dom.append(
    action("+ comment", () => {
      const { from, to } = view.state.selection.main;
      if (from !== to) view.dispatch({ effects: startInput.of({ from, to, text: "" }) });
    }),
  );
  return {
    dom,
    getCoords() {
      const { from, to } = view.state.selection.main;
      return under(view, from, to, dom.offsetWidth);
    },
  };
}

/** Closes the new comment's input: adds the comment if something was typed. */
function finishInput(view: EditorView, session: Input["create"], typed: string | null, back: boolean): void {
  const input = view.state.field(inputField, false);
  if (!input || input.create !== session) return;
  const text = typed?.trim() ?? "";
  const effects: StateEffect<unknown>[] = [endInput.of(null)];
  if (text && input.to > input.from) {
    effects.push(
      addComment.of({
        id: crypto.randomUUID(),
        from: input.from,
        to: input.to,
        quote: view.state.doc.sliceString(input.from, input.to),
        text,
      }),
    );
  }
  if (back) effects.push(setQuiet.of(input.to));
  view.dispatch({ effects, selection: back ? { anchor: input.to } : undefined });
  if (back) view.focus();
}

function inputWindow(view: EditorView, session: Input["create"]): TooltipView {
  const input = view.state.field(inputField)!;
  const dom = el("div", "cm-comment-window cm-comment-input");
  const parts = commentBox(
    dom,
    input.text,
    (typed, back) => finishInput(view, session, typed, back),
    () => repositionTooltips(view),
  );
  dom.prepend(parts.box);
  return {
    dom,
    getCoords() {
      const now = view.state.field(inputField, false);
      return under(view, now?.from ?? input.from, now?.to ?? input.to, dom.offsetWidth);
    },
    mount() {
      parts.mount();
      // After the editor has placed the window.
      requestAnimationFrame(parts.focus);
    },
    destroy: parts.destroy,
  };
}

/** Windows for written comments, by id, so moving within one (or editing it) keeps its window. */
const shownWindows = new Map<string, (view: EditorView) => TooltipView>();

/**
 * A written comment's window: its text is the box, so clicking in and typing
 * edits it; × removes it. While its box has the cursor the window stays,
 * though the doc no longer has the focus.
 */
function commentWindow(id: string): (view: EditorView) => TooltipView {
  let create = shownWindows.get(id);
  if (!create) {
    create = (view) => {
      const find = () => view.state.field(commentField).find((x) => x.id === id);
      const dom = el("div", "cm-comment-window cm-comment-written");
      const parts: CommentBox = commentBox(
        dom,
        find()?.text ?? "",
        (typed, back) => {
          const c = find();
          const effects: StateEffect<unknown>[] = [holdComment.of(null)];
          if (c && typed !== null) effects.push(typed.trim() ? editComment.of({ id, text: typed.trim() }) : removeComment.of(id));
          // Given up: the box shows the comment as it was.
          if (c && typed === null) parts.set(c.text);
          if (back) effects.push(setQuiet.of(view.state.selection.main.head), setHovered.of(null));
          view.dispatch({ effects });
          if (back) view.focus();
        },
        () => repositionTooltips(view),
      );
      parts.box.addEventListener("focus", () => view.dispatch({ effects: holdComment.of(id) }));
      dom.addEventListener("mouseenter", () => view.plugin(hover)?.over(id));
      dom.addEventListener("mouseleave", () => view.plugin(hover)?.over(null));
      dom.prepend(
        parts.box,
        action("×", () => view.dispatch({ effects: [removeComment.of(id), holdComment.of(null), setHovered.of(null)] })),
      );
      return {
        dom,
        getCoords() {
          const c = find();
          return under(view, c?.from ?? 0, c?.to ?? 0, dom.offsetWidth);
        },
        mount: parts.mount,
        destroy: parts.destroy,
      };
    };
    if (shownWindows.size > 50) shownWindows.clear();
    shownWindows.set(id, create);
  }
  return create;
}

const window_ = showTooltip.compute(["selection", commentField, inputField, focused, held, quiet, hovered], (state): Tooltip | null => {
  const input = state.field(inputField);
  if (input) return { pos: input.from, above: false, create: input.create };
  const shownComment = (id: string | null) => (id ? state.field(commentField).find((c) => c.id === id && c.from >= 0) : undefined);
  const keptComment = shownComment(state.field(held));
  if (keptComment) return { pos: keptComment.from, above: false, create: commentWindow(keptComment.id) };
  const sel = state.selection.main;
  // Text selected: the button wins over a comment the pointer rests on.
  if (state.field(focused) && !sel.empty) return { pos: sel.from, above: false, create: plusButton };
  const hoveredComment = shownComment(state.field(hovered));
  if (hoveredComment) return { pos: hoveredComment.from, above: false, create: commentWindow(hoveredComment.id) };
  if (!state.field(focused)) return null;
  if (state.field(quiet) === sel.head) return null;
  const comment = commentAt(state, sel.head);
  return comment ? { pos: comment.from, above: false, create: commentWindow(comment.id) } : null;
});

/** The underlines: accent for comments waiting to go, faint once sent; the text being commented on, lit. */
const marks = EditorView.decorations.compute([commentField, inputField], (state) => {
  const ranges = state
    .field(commentField)
    .filter((c) => c.from >= 0 && c.to > c.from)
    .map((c) => Decoration.mark({ class: c.sentWith ? "cm-comment cm-comment-sent" : "cm-comment" }).range(c.from, c.to));
  const input = state.field(inputField);
  if (input && !input.id && input.to > input.from) {
    ranges.push(Decoration.mark({ class: "cm-comment-pending" }).range(input.from, input.to));
  }
  return Decoration.set(ranges, true);
});

/** Comments in the editor; `onchange` hears every change to the list. */
export function commentsExtension(initial: DocComment[], onchange?: (list: DocComment[]) => void): Extension {
  return [
    commentField.init(() => initial),
    inputField,
    focused,
    held,
    quiet,
    hovered,
    hover,
    EditorView.focusChangeEffect.of((_state, on) => setFocused.of(on)),
    marks,
    window_,
    // In the window's body, so the panel's scrolling box doesn't cut a window
    // off. Kept within the text's column side to side, so one near the right
    // edge stops at the text's edge as one at the left does, not the window's.
    tooltips({
      parent: document.body,
      tooltipSpace: (view) => {
        const text = view.contentDOM.getBoundingClientRect();
        return { left: text.left, right: text.right, top: 0, bottom: window.innerHeight };
      },
    }),
    EditorView.theme(commentLook),
    EditorView.updateListener.of((u) => {
      const list = u.state.field(commentField);
      if (list !== u.startState.field(commentField)) onchange?.(list);
    }),
  ];
}

/** Puts the cursor in a comment's text and shows it, scrolled into view. */
export function focusComment(view: EditorView, id: string): void {
  const c = view.state.field(commentField).find((x) => x.id === id);
  if (!c || c.from < 0) return;
  view.focus();
  view.dispatch({ selection: { anchor: c.from }, scrollIntoView: true, effects: setFocused.of(true) });
}
