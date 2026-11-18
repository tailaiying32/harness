// The parts of a comment's window shared by the docs editor
// (commentMarks.ts) and the chat (chatComments.ts): the box a comment is
// written and edited in, the window's buttons, and their look.

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** A button in a window that leaves the editor's focus and selection alone. */
export function action(label: string, run: () => void): HTMLButtonElement {
  const button = el("button", "cm-comment-action", label);
  button.type = "button";
  button.addEventListener("mousedown", (e) => e.preventDefault());
  button.addEventListener("click", (e) => {
    e.preventDefault();
    run();
  });
  return button;
}

export type CommentBox = {
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
export function commentBox(
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

/** The top and bottom of the screen a window can show in: its text's scrolling box, or the window. */
export function roomFor(node: Element): { top: number; bottom: number } {
  for (let n = node.parentElement; n; n = n.parentElement) {
    const overflow = getComputedStyle(n).overflowY;
    if ((overflow === "auto" || overflow === "scroll") && n.scrollHeight > n.clientHeight) {
      const r = n.getBoundingClientRect();
      return { top: Math.max(0, r.top), bottom: Math.min(window.innerHeight, r.bottom) };
    }
  }
  return { top: 0, bottom: window.innerHeight };
}

/**
 * For the "+ comment" button: the height of the one-line box it opens
 * into, measured from a hidden copy beside it (0 for any other window).
 */
function boxHeight(dom: HTMLElement): number {
  if (!dom.classList.contains("cm-comment-plus") || !dom.parentElement) return 0;
  const probe = el("div", "cm-tooltip cm-comment-window cm-comment-input");
  probe.style.cssText = "position: fixed; visibility: hidden; top: 0; left: 0";
  probe.append(el("div", "cm-comment-box", "x"));
  dom.parentElement.append(probe);
  const height = probe.offsetHeight;
  probe.remove();
  return height;
}

/**
 * Which side of its text a window hangs on: under the text's last line,
 * unless it would run past the bottom of `room` and fits over the first
 * line instead. The button decides by the box it opens into, so the two
 * open on the same side. `top` is where to put the window's top edge (its CSS top
 * margin still applies, so over the text it is taken back twice: once to
 * undo it, once for the same gap above the line).
 */
export function hangSide(
  dom: HTMLElement,
  firstTop: number,
  lastBottom: number,
  room: { top: number; bottom: number },
): { above: boolean; top: number } {
  const margin = parseFloat(getComputedStyle(dom).marginTop) || 0;
  const height = Math.max(dom.offsetHeight, boxHeight(dom)) + margin;
  if (lastBottom + height <= room.bottom || firstTop - height < room.top) return { above: false, top: lastBottom };
  return { above: true, top: firstTop - dom.offsetHeight - 2 * margin };
}

/** How long the pointer rests on a comment before it opens, and is away before it closes. */
const HOVER_OPEN_MS = 150;
const HOVER_CLOSE_MS = 250;

/**
 * When to show a comment the pointer rests on. `over` hears which comment
 * the pointer is on (its text or its window), or null for neither. A comment
 * opens after a short rest; leaving both its text and its window closes it a
 * moment later, so the pointer can cross the gap between them.
 */
export class HoverIntent {
  private openTimer = 0;
  private closeTimer = 0;
  private waiting: string | null = null;

  constructor(
    private shown: () => string | null,
    private show: (id: string | null) => void,
  ) {}

  over(id: string | null): void {
    const shown = this.shown();
    if (id && id === shown) {
      this.cancelOpen();
      this.cancelClose();
      return;
    }
    if (id) {
      this.cancelClose();
      if (this.waiting === id) return;
      this.cancelOpen();
      this.waiting = id;
      this.openTimer = window.setTimeout(() => {
        this.waiting = null;
        this.show(id);
      }, HOVER_OPEN_MS);
      return;
    }
    this.cancelOpen();
    if (shown && !this.closeTimer) {
      this.closeTimer = window.setTimeout(() => {
        this.closeTimer = 0;
        this.show(null);
      }, HOVER_CLOSE_MS);
    }
  }

  private cancelOpen() {
    clearTimeout(this.openTimer);
    this.waiting = null;
  }

  private cancelClose() {
    clearTimeout(this.closeTimer);
    this.closeTimer = 0;
  }

  destroy(): void {
    this.cancelOpen();
    this.cancelClose();
  }
}

/** How comments and their windows look, by selector. */
export const commentLook: Record<string, Record<string, string>> = {
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
  // As wide as the comment needs, up to a short paragraph's width. 1ch at
  // the sides, and 1ch between the comment and its ×: even all round.
  // The button: a popup's colours, padded like the app's strong buttons
  // (Approve plan, Allow): none above or below, 1ch at the sides.
  ".cm-tooltip.cm-comment-plus": { display: "flex" },
  ".cm-comment-plus .cm-comment-action": { color: "var(--accent)", padding: "0 1ch", lineHeight: "inherit" },
  // Its width is its text's, whatever room is left to its right (the panel
  // is at the window's edge): the editor then moves it left to fit.
  ".cm-tooltip.cm-comment-window": {
    padding: "calc(var(--gap) * 0.4) 1ch",
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
};

/**
 * The look as plain CSS for windows and marks outside an editor (the chat),
 * each rule under `scope`. The docs editor takes the same rules as its theme
 * (`EditorView.theme(commentLook)`), so both look the same.
 */
export function mountLook(scope: string): void {
  const id = "comment-look";
  if (document.getElementById(id)) return;
  const kebab = (prop: string) => prop.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
  const css = Object.entries(commentLook)
    .map(([selector, props]) => {
      const scoped = selector
        .split(",")
        .map((s) => `${scope} ${s.trim()}`)
        .join(", ");
      const body = Object.entries(props)
        .map(([prop, value]) => `${kebab(prop)}: ${value};`)
        .join(" ");
      return `${scoped} { ${body} }`;
    })
    .join("\n");
  const style = document.createElement("style");
  style.id = id;
  style.textContent = css;
  document.head.append(style);
}
