// The tab keys, the same in every panel view that has tabs: Cmd+T opens a
// tab, Cmd+W closes the one on show, Cmd+Shift+[ and ] step through them.
// They act on the view that has the keys (the terminal, a side chat, or the
// browser, a clicked page included), never on one the cursor isn't in. With
// none, Cmd+W closes the window as in any Mac app, and the rest do nothing.
//
// On a Mac they come from the app menu (see TAB_KEYS in lib.rs), which hears
// them even while a browser page holds the keys. Elsewhere there is no menu,
// so the window's own key handler calls `tabKeyFrom`.

import { closeWindow, type TabAction } from "../ipc";
import type { Chat } from "./chat.svelte";
import { terminals } from "./terminals.svelte";

export const MAC = /Mac/.test(navigator.userAgent);

type Owner = "terminal" | "side" | "browser" | "info";

/** Which tabbed view has the keys, if any. */
function owner(chat: Chat): Owner | null {
  if (terminals.focused()) return "terminal";
  if (!chat.panelOpen) return null;
  const at = document.activeElement;
  if (chat.panelView === "side" && at?.closest("[data-tabs='side']")) return "side";
  if (chat.panelView === "info" && at?.closest("[data-tabs='info']")) return "info";
  if (chat.panelView === "browser") {
    // A page the user clicked into holds the keys outside this document.
    if (at?.closest("[data-tabs='browser']") || !document.hasFocus()) return "browser";
  }
  return null;
}

export function tabKey(chat: Chat, action: TabAction): void {
  const project = chat.projectPath.trim();
  switch (owner(chat)) {
    case "terminal": {
      if (action === "new") void terminals.open(project).catch((e) => chat.showError(String(e)));
      else if (action === "close") {
        const tab = terminals.shown(project);
        if (tab) void terminals.askClose(tab.id);
      } else terminals.step(project, action === "prev" ? -1 : 1);
      return;
    }
    case "side": {
      if (action === "new") chat.newSide();
      else if (action === "close") void chat.endSide();
      else chat.stepSide(action === "prev" ? -1 : 1);
      return;
    }
    case "browser": {
      if (action === "new") void chat.newBrowserTab();
      else if (action === "close") void chat.closeBrowserTab();
      else chat.stepBrowserTab(action === "prev" ? -1 : 1);
      return;
    }
    case "info": {
      // Its "tabs" are the overview and each subagent: nothing to open or close.
      if (action === "prev" || action === "next") chat.info.step(action === "prev" ? -1 : 1);
      return;
    }
    default:
      if (action === "close") void closeWindow().catch(() => {});
  }
}

/** Off a Mac: a key press that is a tab key, handled; true if it was one. */
export function tabKeyFrom(chat: Chat, e: KeyboardEvent): boolean {
  if (MAC || !e.metaKey || e.altKey || e.ctrlKey) return false;
  const key = e.key.toLowerCase();
  let action: TabAction | null = null;
  if (!e.shiftKey && key === "t") action = "new";
  else if (!e.shiftKey && key === "w") action = "close";
  else if (e.shiftKey && e.code === "BracketLeft") action = "prev";
  else if (e.shiftKey && e.code === "BracketRight") action = "next";
  if (!action) return false;
  // Off a Mac, Cmd+W with nothing to close was never the window's: leave it.
  if (action === "close" && !owner(chat)) return false;
  tabKey(chat, action);
  return true;
}
