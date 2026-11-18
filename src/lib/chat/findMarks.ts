// Find in a doc (see find.ts): the editor draws only the lines on screen, so
// matches are found in its text and marked as decorations, not in the page.
import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import { matchesIn, type Searcher } from "../find";

type Found = { ranges: { from: number; to: number }[]; current: number };

const setFind = StateEffect.define<Found>();
const match = Decoration.mark({ class: "cm-find" });
const currentMatch = Decoration.mark({ class: "cm-find cm-find-current" });

const findField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    marks = marks.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setFind)) {
        marks = Decoration.set(e.value.ranges.map((r, i) => (i === e.value.current ? currentMatch : match).range(r.from, r.to)));
      }
    }
    return marks;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** Who hears each editor's text change while its find box is open. */
const listeners = new WeakMap<EditorView, () => void>();

export const findExtension = [
  findField,
  EditorView.updateListener.of((u) => {
    if (u.docChanged) listeners.get(u.view)?.();
  }),
];

export function editorSearcher(view: EditorView): Searcher {
  let ranges: { from: number; to: number }[] = [];
  return {
    search(query) {
      const q = query.toLowerCase();
      const text = view.state.doc.toString();
      const lower = text.toLowerCase();
      const same = lower.length === text.length;
      ranges = matchesIn(same ? lower : text, same ? q : query).map((from) => ({ from, to: from + q.length }));
      view.dispatch({ effects: setFind.of({ ranges, current: -1 }) });
      return ranges.length;
    },
    show(i) {
      const r = ranges[i];
      if (!r) return;
      view.dispatch({ effects: [setFind.of({ ranges, current: i }), EditorView.scrollIntoView(r.from, { y: "center" })] });
    },
    clear() {
      ranges = [];
      if (view.dom.isConnected) view.dispatch({ effects: setFind.of({ ranges, current: -1 }) });
    },
    watch(changed) {
      let timer: ReturnType<typeof setTimeout> | undefined;
      listeners.set(view, () => {
        clearTimeout(timer);
        timer = setTimeout(changed, 200);
      });
      return () => {
        clearTimeout(timer);
        listeners.delete(view);
      };
    },
    // Escape leaves the match picked in the doc, ready to type over.
    finish(i) {
      const r = ranges[i];
      if (!r || !view.dom.isConnected) return false;
      view.dispatch({ selection: { anchor: r.from, head: r.to } });
      view.focus();
      return true;
    },
  };
}
