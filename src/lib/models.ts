// The model list shown in the picker. It comes from Claude Code itself (see
// `list_models` in the core), so names and versions are always current. The
// last list is kept so the picker is right from the first paint next time.

import type { ModelInfo } from "./ipc";

const KEY = "harness.models";

/** Shown only until Claude Code has answered once on this machine. */
export const fallbackModels: ModelInfo[] = [
  { value: "default", name: "Default", detail: null },
  { value: "fable", name: "Fable", detail: null },
  { value: "opus", name: "Opus", detail: null },
  { value: "sonnet", name: "Sonnet", detail: null },
  { value: "haiku", name: "Haiku", detail: null },
];

export function loadCachedModels(): ModelInfo[] {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "[]") as ModelInfo[];
    if (Array.isArray(saved) && saved.length > 0 && saved.every((m) => m && typeof m.value === "string")) {
      return saved;
    }
  } catch {
    // Fall through to the built-in list.
  }
  return fallbackModels;
}

export function saveCachedModels(models: ModelInfo[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(models));
  } catch {
    // Storage can be unavailable; the list is then fetched again next run.
  }
}
