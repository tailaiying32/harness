// Find in page (Cmd+F): one small box, in whichever view the user was last
// in: the chat, a side chat, or a doc. Each view with a box registers a spot;
// Cmd+F opens the spot nearest to where the user last clicked or typed.

/** What a find box searches. */
export interface Searcher {
  /** Marks every match of `query` (any case); returns how many. */
  search(query: string): number;
  /** Lights match `i` and scrolls it into view. */
  show(i: number): void;
  /** Takes all the marks away. */
  clear(): void;
  /** Hears when the text changes under the box; returns a way to stop. */
  watch?(changed: () => void): () => void;
  /** Escape on match `i`: true if the searcher put the cursor somewhere itself. */
  finish?(i: number): boolean;
}

export type Spot = {
  root: HTMLElement;
  /** The main chat: the pick when nothing nearer has one. */
  main?: boolean;
  open: () => void;
  close: () => void;
};

const spots = new Set<Spot>();
let shown: Spot | null = null;
let last: Node | null = null;

if (typeof window !== "undefined") {
  const note = (e: Event) => {
    if (e.target instanceof Node) last = e.target;
  };
  window.addEventListener("pointerdown", note, true);
  window.addEventListener("focusin", note, true);
}

export function addSpot(spot: Spot): () => void {
  spots.add(spot);
  return () => {
    spots.delete(spot);
    if (shown === spot) shown = null;
  };
}

/** Marks `spot` as the open box, closing any other: one is open at a time. */
export function opened(spot: Spot): void {
  if (shown && shown !== spot) shown.close();
  shown = spot;
}

export function closed(spot: Spot): void {
  if (shown === spot) shown = null;
}

/**
 * Cmd+F: opens the box nearest to where the user last was. From that place
 * outward, the first element holding a spot wins (the main chat on a tie),
 * so the message box finds in the chat above it. False if there is none.
 */
export function openFind(): boolean {
  const live = [...spots].filter((s) => s.root.isConnected);
  if (!live.length) return false;
  const from = last?.isConnected ? last : null;
  for (let at: Node | null = from; at; at = at.parentNode) {
    const near = live.filter((s) => at!.contains(s.root));
    if (near.length) {
      (near.find((s) => s.main) ?? near[0]).open();
      return true;
    }
  }
  const main = live.find((s) => s.main);
  if (!main) return false;
  main.open();
  return true;
}

/** Where `query` (lowercased) is in `text` (lowercased), start positions. */
export function matchesIn(text: string, query: string): number[] {
  const out: number[] = [];
  if (!query) return out;
  for (let i = text.indexOf(query); i >= 0; i = text.indexOf(query, i + query.length)) out.push(i);
  return out;
}

/**
 * Searches the text drawn under `root` (which scrolls), marked with the
 * browser's highlights so the page itself is never changed. Text out of
 * sight (a closed fold, a comment's window) is skipped.
 */
export function domSearcher(root: HTMLElement): Searcher {
  let ranges: Range[] = [];
  const can = typeof CSS !== "undefined" && "highlights" in CSS;

  const visible = (node: Node): boolean => {
    const el = node.parentElement;
    if (!el || el.closest(".find-bar, .cm-comment-window, script, style")) return false;
    const fold = el.closest("details:not([open])");
    if (fold && !el.closest("summary")) return false;
    return el.checkVisibility?.() ?? true;
  };

  return {
    search(query) {
      ranges = [];
      const q = query.toLowerCase();
      if (q) {
        const nodes: Text[] = [];
        const starts: number[] = [];
        let text = "";
        const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          const value = n.nodeValue ?? "";
          if (!value || !visible(n)) continue;
          nodes.push(n as Text);
          starts.push(text.length);
          text += value;
        }
        const lower = text.toLowerCase();
        // Lowercasing changed a length (rare letters): match as typed instead.
        const hay = lower.length === text.length ? lower : text;
        const at = (pos: number): [Text, number] => {
          let lo = 0;
          let hi = starts.length - 1;
          while (lo < hi) {
            const mid = (lo + hi + 1) >> 1;
            if (starts[mid] <= pos) lo = mid;
            else hi = mid - 1;
          }
          return [nodes[lo], pos - starts[lo]];
        };
        for (const i of matchesIn(hay, hay === lower ? q : query)) {
          const range = document.createRange();
          range.setStart(...at(i));
          // The end is inside the match's last letter's node.
          const [endNode, endOffset] = at(i + q.length - 1);
          range.setEnd(endNode, endOffset + 1);
          ranges.push(range);
        }
      }
      if (can) {
        CSS.highlights.set("find", new Highlight(...ranges));
        CSS.highlights.delete("find-current");
      }
      return ranges.length;
    },
    show(i) {
      const range = ranges[i];
      if (!range) return;
      if (can) CSS.highlights.set("find-current", new Highlight(range));
      // Blocks off screen are sized roughly until drawn, so measure again once
      // the first jump has drawn them.
      const center = () => {
        const box = root.getBoundingClientRect();
        const rect = range.getBoundingClientRect();
        if (rect.top < box.top + 40 || rect.bottom > box.bottom - 40) {
          root.scrollTop += rect.top - box.top - box.height / 2;
        }
      };
      center();
      requestAnimationFrame(center);
    },
    clear() {
      ranges = [];
      if (can) {
        CSS.highlights.delete("find");
        CSS.highlights.delete("find-current");
      }
    },
    watch(changed) {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const observer = new MutationObserver((records) => {
        // The box's own count changing is not the page changing.
        if (records.every((r) => (r.target as Element).closest?.(".find-bar") || r.target.parentElement?.closest(".find-bar"))) return;
        clearTimeout(timer);
        timer = setTimeout(changed, 200);
      });
      observer.observe(root, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["open"] });
      return () => {
        clearTimeout(timer);
        observer.disconnect();
      };
    },
  };
}
