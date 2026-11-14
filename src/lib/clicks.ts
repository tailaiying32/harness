// Clicks inside formatted text, shared by the chat and the side panel.

import { openUrl } from "@tauri-apps/plugin-opener";

/** Handles "copy" on a code block, and opens links in the default browser, never in this window. */
export async function contentClick(e: MouseEvent): Promise<void> {
  const target = e.target as HTMLElement;

  const copy = target.closest<HTMLElement>("[data-copy]");
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

  const link = target.closest("a");
  if (link?.href) {
    e.preventDefault();
    void openUrl(link.href);
  }
}
