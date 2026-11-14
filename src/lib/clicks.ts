// Clicks inside formatted text, shared by the chat and the side panel.

import { homeDir } from "@tauri-apps/api/path";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { localPath } from "./paths";

/** Where a click's links go: the folder relative links start from, and how to show a markdown file. */
export type LinkTarget = { base: () => string; openDoc: (path: string) => void };

/**
 * Handles "copy" on a code block, and links. Web links open in the default
 * browser, never in this window. A link to a markdown file on disk opens in
 * the side panel; a link to any other local file shows it in Finder or
 * Explorer, rather than running it.
 */
export function contentClick(target: LinkTarget): (e: MouseEvent) => Promise<void> {
  return async (e) => {
    const el = e.target as HTMLElement;

    const copy = el.closest<HTMLElement>("[data-copy]");
    if (copy) {
      const code = copy.closest(".code")?.querySelector("pre")?.textContent ?? "";
      try {
        await navigator.clipboard.writeText(code);
        copy.textContent = "copied";
        setTimeout(() => (copy.textContent = "copy"), 1200);
      } catch {
        copy.textContent = "couldn't copy";
      }
      return;
    }

    const link = el.closest("a");
    if (!link) return;
    e.preventDefault();
    // The link as written. `link.href` would read a bare file name as a page
    // of this app, and the browser would open a broken page.
    const href = link.getAttribute("href") ?? "";
    if (/^(https?|mailto|tel):/i.test(href)) {
      void openUrl(href);
      return;
    }
    const path = localPath(href, target.base(), href.startsWith("~") ? await homeDir() : "");
    if (!path) return;
    if (/\.(md|markdown)$/i.test(path)) target.openDoc(path);
    else void revealItemInDir(path).catch(() => {});
  };
}
