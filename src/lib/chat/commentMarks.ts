// Comments in the live markdown editor (see comments.ts for what they are).
// The editor holds each comment's place and moves it with the text, whoever
// types; the chat holds the list (via `oncomments`) so a comment outlives the
// editor, which is rebuilt each time a tab shows.
//
// What shows, at most one floating window at a time, each below its text:
// - while writing a comment: the input, under the commented text
// - the cursor in a commented spot: that comment, with edit and ×
// - text selected: a small "+ comment" button under the selection
// Windows are CodeMirror tooltips, so they follow the text as it scrolls.

import { EditorSelection, type EditorState, type Extension, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, repositionTooltips, showTooltip, tooltips, type Tooltip, type TooltipView } from "@codemirror/view";
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

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** A button in a window that leaves the editor's focus and selection alone. */
function action(label: string, run: () => void): HTMLButtonElement {
  const button = el("button", "cm-comment-action", label);
  button.type = "button";
  button.addEventListener("mousedown", (e) => e.preventDefault());
  button.addEventListener("click", (e) => {
    e.preventDefault();
    run();
  });
  return button;
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

type CommentBox = {
  box: HTMLElement;
  /** Puts `text` in the box. */
  set: (text: string) => void;
  /** Puts the cursor in the box, at its end. */
  focus: () => void;
  mount: () => void;
  destroy: () => void;
};

/**
 * A comment's box: plain text edited in place, in an element that is as big
 * as its text (a textarea isn't: sized by measuring a copy of the text, it
 * wrapped the last letter on some keys). Enter saves (`done` with the text),
 * Escape gives up (`done` with null); both put the cursor back in the doc.
 * Clicking away saves without taking the cursor back. `resized` hears every
 * change to the text.
 */
function commentBox(
  dom: HTMLElement,
  text: string,
  done: (typed: string | null, back: boolean) => void,
  resized: () => void,
): CommentBox {
  const box = el("div", "cm-comment-box");
  box.contentEditable = "plaintext-only";
  // Engines without plain-text editing fall back to rich; only text is read.
  if (box.contentEditable !== "plaintext-only") box.contentEditable = "true";
  box.spellcheck = false;
  box.dataset.placeholder = "comment";
  box.textContent = text;
  const value = () => box.innerText.replace(/\n$/, "");
  /**
   * Text that wraps leaves its box at full width, with a ragged gap at the
   * right that one line never has. So a box of several lines is cut to its
   * widest line: laid out at its natural width first, then measured word by
   * word (spaces at a line's end hang past it and don't count).
   */
  const fit = () => {
    box.style.width = "";
    const left = box.getBoundingClientRect().left;
    const range = document.createRange();
    const lines = new Set<number>();
    let right = 0;
    const walker = document.createTreeWalker(box, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      for (const word of (node.textContent ?? "").matchAll(/\S+/g)) {
        range.setStart(node, word.index);
        range.setEnd(node, word.index + word[0].length);
        for (const rect of range.getClientRects()) {
          lines.add(Math.round(rect.top));
          right = Math.max(right, rect.right);
        }
      }
    }
    // One line is as wide as its text already.
    if (lines.size > 1) box.style.width = `${Math.ceil(right - left) + 1}px`;
  };
  const changed = () => {
    fit();
    resized();
  };
  box.addEventListener("input", () => {
    // Emptied, a stray line break can stay behind and hide the placeholder.
    if (!box.textContent) box.replaceChildren();
    changed();
  });
  box.addEventListener("keydown", (e) => {
    // The box's own keys: the editor's (and the app's) never see them.
    e.stopPropagation();
    if (e.isComposing) return;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      done(value(), true);
    } else if (e.key === "Enter") {
      e.preventDefault();
      document.execCommand("insertLineBreak");
    } else if (e.key === "Escape") {
      e.preventDefault();
      done(null, true);
    }
  });
  // Only a real leaving counts: the editor places a new window after making
  // it, which can take the focus from it once.
  let ready = false;
  /** When the user last pressed somewhere outside the window. */
  let pressedAway = 0;
  const onPress = (e: PointerEvent) => {
    if (!dom.contains(e.target as Node)) pressedAway = Date.now();
  };
  const focus = () => {
    box.focus();
    const range = document.createRange();
    range.selectNodeContents(box);
    range.collapse(false);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  };
  box.addEventListener("focus", () => (ready = true));
  box.addEventListener("blur", () => {
    if (!ready) return;
    requestAnimationFrame(() => {
      if (document.activeElement === box || !box.isConnected) return;
      // A click or tab elsewhere: done. Otherwise the window was only moved: take the cursor back.
      if (Date.now() - pressedAway < 500 || document.activeElement !== document.body) done(value(), false);
      else focus();
    });
  });
  return {
    box,
    set: (next) => {
      box.textContent = next;
      changed();
    },
    focus,
    mount() {
      window.addEventListener("pointerdown", onPress, true);
      changed();
    },
    destroy() {
      window.removeEventListener("pointerdown", onPress, true);
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
          if (back) effects.push(setQuiet.of(view.state.selection.main.head));
          view.dispatch({ effects });
          if (back) view.focus();
        },
        () => repositionTooltips(view),
      );
      parts.box.addEventListener("focus", () => view.dispatch({ effects: holdComment.of(id) }));
      dom.prepend(
        parts.box,
        action("×", () => view.dispatch({ effects: [removeComment.of(id), holdComment.of(null)] })),
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

const window_ = showTooltip.compute(["selection", commentField, inputField, focused, held, quiet], (state): Tooltip | null => {
  const input = state.field(inputField);
  if (input) return { pos: input.from, above: false, create: input.create };
  const kept = state.field(held);
  const keptComment = kept ? state.field(commentField).find((c) => c.id === kept && c.from >= 0) : undefined;
  if (keptComment) return { pos: keptComment.from, above: false, create: commentWindow(keptComment.id) };
  if (!state.field(focused)) return null;
  const sel = state.selection.main;
  if (!sel.empty) return { pos: sel.from, above: false, create: plusButton };
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

const look = EditorView.theme({
  ".cm-comment": {
    textDecoration: "underline",
    textDecorationColor: "var(--accent)",
    textUnderlineOffset: "0.25em",
  },
  ".cm-comment-sent": { textDecorationColor: "var(--faint)" },
  ".cm-comment-pending": { backgroundColor: "color-mix(in srgb, var(--accent) 22%, transparent)" },
  ".cm-tooltip.cm-comment-window, .cm-tooltip.cm-comment-plus": {
    backgroundColor: "var(--panel)",
    color: "var(--fg)",
    border: "1px solid var(--line)",
    borderRadius: "0",
    font: "inherit",
    fontFamily: "var(--font-family)",
    fontSize: "var(--font)",
    lineHeight: "var(--lh)",
    marginTop: "0.25em",
  },
  // As wide as the comment needs, up to a short paragraph's width. Padded
  // like the app's other popups (the model menu, / and @).
  // The button: a popup's colours, padded like the app's strong buttons
  // (Approve plan, Allow): none above or below, 1ch at the sides.
  ".cm-tooltip.cm-comment-plus": { display: "flex" },
  ".cm-comment-plus .cm-comment-action": { color: "var(--accent)", padding: "0 1ch", lineHeight: "inherit" },
  // Its width is its text's, whatever room is left to its right (the panel
  // is at the window's edge): the editor then moves it left to fit.
  ".cm-tooltip.cm-comment-window": {
    padding: "calc(var(--gap) * 0.4) 2ch",
    width: "max-content",
    maxWidth: "min(40ch, calc(100vw - 4ch))",
    boxSizing: "border-box",
  },
  ".cm-comment-input, .cm-comment-written": { display: "flex", gap: "1ch", alignItems: "flex-start" },
  ".cm-comment-written .cm-comment-box": { color: "var(--fg)" },
  ".cm-comment-written .cm-comment-box:focus": { color: "var(--bright)" },
  ".cm-comment-box": {
    flex: "0 1 auto",
    minWidth: "1px",
    outline: "0",
    color: "var(--bright)",
    caretColor: "var(--accent)",
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
    maxHeight: "30vh",
    overflowX: "hidden",
    overflowY: "auto",
    cursor: "text",
    // Its width is its text's; a character of room past that, taken back by
    // the margin, lets the cursor show after a space at a line's end.
    boxSizing: "content-box",
    paddingRight: "1ch",
    marginRight: "-1ch",
  },
  ".cm-comment-box:empty::before": { content: "attr(data-placeholder)", color: "var(--faint)", pointerEvents: "none" },
  ".cm-comment-action": {
    background: "none",
    border: "0",
    padding: "0",
    font: "inherit",
    color: "var(--faint)",
    cursor: "pointer",
  },
  ".cm-comment-action:hover": { color: "var(--bright)" },
});

/** Comments in the editor; `onchange` hears every change to the list. */
export function commentsExtension(initial: DocComment[], onchange?: (list: DocComment[]) => void): Extension {
  return [
    commentField.init(() => initial),
    inputField,
    focused,
    held,
    quiet,
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
    look,
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
