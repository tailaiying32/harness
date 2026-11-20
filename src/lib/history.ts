// Undo and redo (Cmd+Z, Cmd+Y, Cmd+Shift+Z on Mac; Ctrl equivalents elsewhere)
// across every text input in the window. CodeMirror editors keep their own
// history and register themselves on load so CodeMirror stays lazy-loaded;
// plain inputs, textareas and comment boxes use native undo/redo.

import { nativeEdit } from "./ipc";
import { MAC } from "./state/tabKeys";
import { terminals } from "./state/terminals.svelte";

export type HistoryKind = "undo" | "redo";
type HistoryRunner = (kind: HistoryKind) => boolean;

const editors = new WeakMap<Element, HistoryRunner>();

/** Registers a mounted CodeMirror editor so menu and key shortcuts drive its history. */
export function registerEditorHistory(root: Element, run: HistoryRunner): () => void {
  editors.set(root, run);
  return () => {
    if (editors.get(root) === run) editors.delete(root);
  };
}

function isEditable(el: Element | null): el is HTMLElement {
  return (
    el instanceof HTMLTextAreaElement ||
    el instanceof HTMLInputElement ||
    (el instanceof HTMLElement && el.isContentEditable)
  );
}

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Runs undo or redo on whichever text box holds the cursor. */
export function runHistory(kind: HistoryKind): boolean {
  if (terminals.focused()) return false;
  const active = document.activeElement;
  if (!(active instanceof HTMLElement)) return false;

  const cm = active.closest(".cm-editor");
  if (cm) {
    const run = editors.get(cm);
    if (run) {
      run(kind);
      // Always consume inside CodeMirror so WebKit never runs a stale native undo on its DOM.
      return true;
    }
  }

  if (isEditable(active)) {
    if (MAC && inTauri) {
      void nativeEdit(kind).catch(() => document.execCommand(kind));
    } else {
      document.execCommand(kind);
    }
    return true;
  }

  return false;
}

/**
 * Listens in capture phase so comment boxes (which stop bubble propagation)
 * still get Cmd+Y / Ctrl+Shift+Z. Returns a cleanup function.
 */
export function installHistoryKeys(): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (e.isComposing || e.altKey || terminals.focused()) return;
    const mod = MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
    if (!mod) return;
    const key = e.key.toLowerCase();

    if (MAC) {
      // Cmd+Z and Cmd+Shift+Z arrive from the Mac Edit menu in Tauri;
      // Cmd+Y always comes straight here (and Cmd+Z/Shift+Z in browser preview).
      let kind: HistoryKind | null = null;
      if (key === "y" && !e.shiftKey) kind = "redo";
      else if (!inTauri && key === "z") kind = e.shiftKey ? "redo" : "undo";
      if (!kind) return;
      if (runHistory(kind)) {
        e.preventDefault();
        e.stopPropagation();
      }
      return;
    }

    // Off Mac: CodeMirror handles Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z in its own
    // keymap, and native fields handle Ctrl+Z / Ctrl+Y natively; add Ctrl+Shift+Z.
    if (key === "z" && e.shiftKey) {
      const active = document.activeElement;
      if (active instanceof HTMLElement && !active.closest(".cm-editor") && isEditable(active)) {
        e.preventDefault();
        e.stopPropagation();
        document.execCommand("redo");
      }
    }
  };
  window.addEventListener("keydown", onKey, true);
  return () => window.removeEventListener("keydown", onKey, true);
}
