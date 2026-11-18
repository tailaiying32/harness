// Comments on the agent's latest answer in the chat (see comments.ts for
// what they are). Works like the docs editor's (commentMarks.ts), on the
// chat's drawn text instead of an editor:
//
// - selecting text in a finished piece of the latest answer shows a small
//   "+ comment" button under it, which turns into the comment's box
// - commented text is underlined; resting the pointer on it (or clicking it)
//   opens its window, where the comment is edited in place, with ×
// - at most one window at a time, in the page's body, under its text
//
// A piece of an answer is finished once text after it has started (or the
// answer is done): drawn block by block (Markdown.svelte), it is never
// redrawn after that, so a comment's offsets in it stay right. The marks are
// spans wrapped around the commented text; if the chat is drawn again they
// are put back from the offsets.

import type { Chat } from "../state/chat.svelte";
import { action, commentBox, el, HoverIntent, mountLook, type CommentBox } from "./commentBox";
import type { ChatComment } from "./comments";

/** Where marks and windows take their look from (mountLook). */
const SCOPE = ":is(.chat-comment-layer, .chat-comments)";

type Spot = { itemId: number; from: number; to: number; quote: string };

type Shown = {
  key: string;
  dom: HTMLElement;
  place: () => void;
  destroy: () => void;
};

/** The answer piece's drawn text a node is in, if any. */
function mdOf(node: Node | null): HTMLElement | null {
  const element = node instanceof Element ? node : node?.parentElement;
  const md = element?.closest(".md");
  return md instanceof HTMLElement && md.parentElement?.dataset.item ? md : null;
}

/** Characters from the start of `md`'s text to a point in it. */
function offsetIn(md: HTMLElement, node: Node, offset: number): number {
  const range = document.createRange();
  range.setStart(md, 0);
  range.setEnd(node, offset);
  return range.toString().length;
}

/** The text nodes under `md`, in order. */
function textNodes(md: HTMLElement): Text[] {
  const out: Text[] = [];
  const walker = document.createTreeWalker(md, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) out.push(node as Text);
  return out;
}

/** A range over characters `from` to `to` of `md`'s text. */
function rangeAt(md: HTMLElement, from: number, to: number): Range | null {
  const range = document.createRange();
  let at = 0;
  let started = false;
  for (const node of textNodes(md)) {
    const end = at + node.length;
    if (!started && from < end) {
      range.setStart(node, from - at);
      started = true;
    }
    if (started && to <= end) {
      range.setEnd(node, to - at);
      return range;
    }
    at = end;
  }
  return null;
}

/** Wraps characters `from` to `to` of `md`'s text in spans made by `make`. */
function wrap(md: HTMLElement, from: number, to: number, make: () => HTMLElement): void {
  const parts: [Text, number, number][] = [];
  let at = 0;
  for (const node of textNodes(md)) {
    const end = at + node.length;
    // Text between blocks belongs to the page's drawing of them: left alone.
    if (node.parentNode !== md && from < end && to > at) parts.push([node, Math.max(from, at) - at, Math.min(to, end) - at]);
    at = end;
  }
  for (const [node, start, stop] of parts) {
    let piece = node;
    if (start > 0) piece = piece.splitText(start);
    if (stop - start < piece.length) piece.splitText(stop - start);
    const span = make();
    piece.replaceWith(span);
    span.append(piece);
  }
}

const MAC = /Mac/.test(navigator.userAgent);

export class ChatComments {
  private layer: HTMLElement;
  private shown: Shown | null = null;
  /** The text selected, while it can take a comment. */
  private plus: Spot | null = null;
  /** The comment being written. */
  private input: (Spot & { n: number }) | null = null;
  private inputs = 0;
  /** A written comment whose box has the cursor: its window stays. */
  private held: string | null = null;
  /** The comment the pointer rests on, or clicked, or jumped to. */
  private hovered: string | null = null;
  private hover: HoverIntent;
  private marked = "";
  private buttonWidth = 0;
  private frame = 0;

  constructor(
    private chat: Chat,
    private scroller: HTMLElement,
  ) {
    mountLook(SCOPE);
    this.layer = el("div", "chat-comment-layer");
    document.body.append(this.layer);
    this.hover = new HoverIntent(
      () => this.hovered,
      (id) => {
        this.hovered = id;
        this.render();
      },
    );
    document.addEventListener("selectionchange", this.onSelection);
    window.addEventListener("keydown", this.onKey, true);
    scroller.addEventListener("mouseover", this.onOver);
    scroller.addEventListener("mouseout", this.onOut);
    scroller.addEventListener("click", this.onClick);
    scroller.addEventListener("scroll", this.place);
    window.addEventListener("resize", this.place);
  }

  destroy(): void {
    document.removeEventListener("selectionchange", this.onSelection);
    window.removeEventListener("keydown", this.onKey, true);
    this.scroller.removeEventListener("mouseover", this.onOver);
    this.scroller.removeEventListener("mouseout", this.onOut);
    this.scroller.removeEventListener("click", this.onClick);
    this.scroller.removeEventListener("scroll", this.place);
    window.removeEventListener("resize", this.place);
    cancelAnimationFrame(this.frame);
    this.hover.destroy();
    this.shown?.destroy();
    this.layer.remove();
  }

  /** After the chat is drawn again: marks back where they belong, windows placed. */
  update(): void {
    const comments = this.chat.chatComments;
    // Nothing to keep up (the usual case, on every streamed word).
    if (!comments.length && !this.input && !this.shown && !this.marked) return;
    if (this.held && !comments.some((c) => c.id === this.held)) this.held = null;
    if (this.hovered && !comments.some((c) => c.id === this.hovered)) this.hovered = null;
    this.mark();
    this.render();
  }

  /** Shows a comment from the list: its text scrolled into view, its window open. */
  jump(id: string): void {
    const span = this.scroller.querySelector(`[data-comment="${CSS.escape(id)}"]`);
    span?.scrollIntoView({ block: "nearest" });
    this.hovered = id;
    this.render();
  }

  private mdFor(itemId: number): HTMLElement | null {
    const md = this.scroller.querySelector(`[data-item="${itemId}"] > .md`);
    return md instanceof HTMLElement ? md : null;
  }

  // ---- Marks ----

  /** Puts the marks on the text, if they aren't there as they should be. */
  private mark(): void {
    const comments = this.chat.chatComments;
    const want = [
      ...comments.map((c) => `${c.id}:${c.itemId}:${c.from}:${c.to}`),
      this.input ? `input:${this.input.itemId}:${this.input.from}:${this.input.to}` : "",
    ].join("|");
    const spans = [...this.scroller.querySelectorAll<HTMLElement>("span.cm-comment[data-comment], span.cm-comment-pending")];
    const present = comments.every((c) => !this.mdFor(c.itemId) || spans.some((s) => s.dataset.comment === c.id));
    if (want === this.marked && present) return;
    this.marked = want;
    // Off with the old ones, then on with all.
    const parents = new Set<Node>();
    for (const span of spans) {
      const parent = span.parentNode;
      if (!parent) continue;
      span.replaceWith(...span.childNodes);
      parents.add(parent);
    }
    for (const parent of parents) parent.normalize();
    for (const c of comments) {
      const md = this.mdFor(c.itemId);
      if (!md) continue;
      wrap(md, c.from, c.to, () => {
        const span = el("span", "cm-comment");
        span.dataset.comment = c.id;
        return span;
      });
    }
    if (this.input) {
      const md = this.mdFor(this.input.itemId);
      if (md) wrap(md, this.input.from, this.input.to, () => el("span", "cm-comment-pending"));
    }
  }

  // ---- What the user does ----

  /** The selection, if it can take a comment: in one finished piece of the latest answer. */
  private selected(): Spot | null {
    if (this.chat.owner) return null;
    const selection = document.getSelection();
    if (!selection || selection.isCollapsed || !selection.rangeCount) return null;
    const range = selection.getRangeAt(0);
    const md = mdOf(range.startContainer);
    if (!md || md !== mdOf(range.endContainer) || !this.scroller.contains(md)) return null;
    const itemId = Number(md.parentElement!.dataset.item);
    if (!this.chat.latestAnswerIds.has(itemId)) return null;
    const item = this.chat.items.find((i) => i.id === itemId);
    if (!item || item.type !== "text") return null;
    // The block still being written isn't finished: it is drawn again on every word.
    const writing = item.streaming ? md.lastElementChild : null;
    if (writing && (writing.contains(range.startContainer) || writing.contains(range.endContainer))) return null;
    const quote = selection.toString();
    if (!quote.trim()) return null;
    const from = offsetIn(md, range.startContainer, range.startOffset);
    const to = offsetIn(md, range.endContainer, range.endOffset);
    return to > from ? { itemId, from, to, quote } : null;
  }

  private onSelection = () => {
    if (this.input || this.held) return;
    const spot = this.selected();
    const same = spot && this.plus && spot.itemId === this.plus.itemId && spot.from === this.plus.from && spot.to === this.plus.to;
    if (same) return;
    this.plus = spot;
    this.render();
  };

  /** Cmd+Enter on selected text opens a comment, as the button does (the button can be hidden in Settings). */
  private onKey = (e: KeyboardEvent) => {
    const mod = MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
    if (e.key !== "Enter" || !mod || e.shiftKey || e.altKey || !this.plus) return;
    e.preventDefault();
    e.stopPropagation();
    this.startInput();
  };

  private startInput(): void {
    const spot = this.plus;
    if (!spot) return;
    this.input = { ...spot, n: ++this.inputs };
    this.plus = null;
    this.mark();
    this.render();
  }

  private markUnder(target: EventTarget | null): string | null {
    const span = target instanceof Element ? target.closest<HTMLElement>("span.cm-comment[data-comment]") : null;
    return span?.dataset.comment ?? null;
  }

  private onOver = (e: MouseEvent) => {
    this.hover.over(this.markUnder(e.target));
  };

  private onOut = (e: MouseEvent) => {
    const to = e.relatedTarget;
    if (to instanceof Node && this.shown?.dom.contains(to)) return;
    this.hover.over(this.markUnder(to));
  };

  private onClick = (e: MouseEvent) => {
    const id = this.markUnder(e.target);
    if (!id || !document.getSelection()?.isCollapsed) return;
    this.hovered = id;
    this.render();
  };

  // ---- The window ----

  /** Shows the one window that should show, keeping the one there if it is the same. */
  private render(): void {
    let key: string | null = null;
    if (this.input) key = `input:${this.input.n}`;
    else if (this.held) key = `comment:${this.held}`;
    else if (this.plus) key = "plus";
    else if (this.hovered) key = `comment:${this.hovered}`;
    if (this.shown?.key !== key) {
      this.shown?.destroy();
      this.shown = null;
      if (key === "plus") this.shown = this.plusWindow();
      else if (key?.startsWith("input:")) this.shown = this.inputWindow();
      else if (key) this.shown = this.commentWindow(key.slice("comment:".length));
    }
    this.place();
  }

  private place = () => {
    cancelAnimationFrame(this.frame);
    this.frame = requestAnimationFrame(() => this.shown?.place());
  };

  /** The text a window is about, as a range. */
  private rangeOf(spot: { itemId: number; from: number; to: number }): Range | null {
    const md = this.mdFor(spot.itemId);
    return md ? rangeAt(md, spot.from, spot.to) : null;
  }

  /**
   * Puts a window under the last screen line of its text, by the docs'
   * rule (see `under` in commentMarks.ts): while narrower than the text on
   * that line, its right edge at the text's end; wider, from where that
   * line's text starts. Never narrower than the button, so the box opens
   * where the button was. Kept within the text's column; hidden when its
   * text is scrolled out of view.
   */
  private hang(dom: HTMLElement, range: Range | null): void {
    const rects = range ? [...range.getClientRects()].filter((r) => r.width > 0) : [];
    const view = this.scroller.getBoundingClientRect();
    const last = rects[rects.length - 1];
    if (!last || last.bottom < view.top || last.top > view.bottom) {
      dom.style.visibility = "hidden";
      return;
    }
    const line = rects.filter((r) => Math.abs(r.top - last.top) < 2);
    const start = Math.min(...line.map((r) => r.left));
    const width = dom.offsetWidth;
    if (!this.buttonWidth && dom.classList.contains("cm-comment-plus")) this.buttonWidth = width;
    const column = (range!.commonAncestorContainer instanceof Element
      ? range!.commonAncestorContainer
      : range!.commonAncestorContainer.parentElement
    )?.closest(".md")?.getBoundingClientRect() ?? view;
    let left = Math.max(start, last.right - Math.max(width, this.buttonWidth));
    left = Math.max(column.left, Math.min(left, column.right - width));
    dom.style.visibility = "";
    dom.style.left = `${left}px`;
    dom.style.top = `${last.bottom}px`;
  }

  private frameFor(cls: string): HTMLElement {
    const dom = el("div", `cm-tooltip ${cls}`);
    dom.style.position = "fixed";
    dom.style.zIndex = "50";
    dom.style.left = "0";
    dom.style.top = "0";
    this.layer.append(dom);
    return dom;
  }

  /** The button follows the selection as it grows: it reads the selection when used, not when made. */
  private plusWindow(): Shown {
    const dom = this.frameFor("cm-comment-plus");
    dom.append(action("+ comment", () => this.startInput()));
    return { key: "plus", dom, place: () => this.plus && this.hang(dom, this.rangeOf(this.plus)), destroy: () => dom.remove() };
  }

  private inputWindow(): Shown {
    const input = this.input!;
    const dom = this.frameFor("cm-comment-window cm-comment-input");
    const parts = commentBox(
      dom,
      "",
      (typed, back) => {
        if (this.input?.n !== input.n) return;
        const text = typed?.trim() ?? "";
        if (text) this.chat.addChatComment({ itemId: input.itemId, from: input.from, to: input.to, quote: input.quote, text });
        this.input = null;
        // The selection went into the box; none is left to offer a comment on.
        document.getSelection()?.removeAllRanges();
        this.plus = null;
        this.mark();
        this.render();
        // Saved with Enter: the cursor goes back to the message box, to type
        // more or press Enter again to send.
        if (text && back) this.chat.focusDraft++;
      },
      () => this.place(),
    );
    dom.prepend(parts.box);
    parts.mount();
    // After the window is placed.
    requestAnimationFrame(parts.focus);
    return {
      key: `input:${input.n}`,
      dom,
      place: () => this.hang(dom, this.rangeOf(input)),
      destroy: () => {
        parts.destroy();
        dom.remove();
      },
    };
  }

  /**
   * A written comment's window: its text is the box, so clicking in and
   * typing edits it; × removes it. While its box has the cursor it stays.
   */
  private commentWindow(id: string): Shown {
    const find = (): ChatComment | undefined => this.chat.chatComments.find((c) => c.id === id);
    const dom = this.frameFor("cm-comment-window cm-comment-written");
    const parts: CommentBox = commentBox(
      dom,
      find()?.text ?? "",
      (typed, back) => {
        const c = find();
        this.held = null;
        if (c && typed !== null) {
          if (typed.trim()) this.chat.editChatComment(id, typed.trim());
          else this.chat.removeChatComment(id);
        }
        // Given up: the box shows the comment as it was.
        if (c && typed === null) parts.set(c.text);
        if (back) this.hovered = null;
        this.update();
        if (c && typed !== null && back) this.chat.focusDraft++;
      },
      () => this.place(),
    );
    parts.box.addEventListener("focus", () => (this.held = id));
    dom.addEventListener("mouseenter", () => this.hover.over(id));
    dom.addEventListener("mouseleave", (e) => this.hover.over(this.markUnder(e.relatedTarget)));
    dom.prepend(
      parts.box,
      action("×", () => {
        this.chat.removeChatComment(id);
        this.held = null;
        this.hovered = null;
        this.update();
      }),
    );
    parts.mount();
    return {
      key: `comment:${id}`,
      dom,
      place: () => {
        const c = find();
        this.hang(dom, c ? this.rangeOf(c) : null);
      },
      destroy: () => {
        parts.destroy();
        dom.remove();
      },
    };
  }
}
