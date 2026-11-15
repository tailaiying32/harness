// The model list shown in the picker. It comes from Claude Code itself (see
// `list_models` in the core), so names and versions are always current: only
// the newest of each family, no "Default", the recommended one first. The
// last list is kept so the picker is right from the first paint next time.

import type { ModelInfo } from "./ipc";

const KEY = "harness.models";

/** Shown only until Claude Code has answered once on this machine. */
export const fallbackModels: ModelInfo[] = [
  { value: "opus", name: "Opus", detail: null },
  { value: "fable", name: "Fable", detail: null },
  { value: "sonnet", name: "Sonnet", detail: null },
  { value: "haiku", name: "Haiku", detail: null },
];

export function loadCachedModels(): ModelInfo[] {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "[]") as ModelInfo[];
    if (Array.isArray(saved) && saved.every((m) => m && typeof m.value === "string")) {
      // A list saved before the picker was trimmed: the same trim (see
      // `parse_models` in the core).
      const kept = saved.filter((m) => m.value !== "default" && !m.value.includes("-"));
      if (kept.length > 0) return kept;
    }
  } catch {
    // Fall through to the built-in list.
  }
  return fallbackModels;
}

/**
 * The model a new chat starts on: the last one picked, or the recommended
 * one (listed first) if that was Claude Code's "default", which the picker
 * no longer offers.
 */
export function startingModel(saved: string, models: ModelInfo[]): string {
  return !saved || saved === "default" ? (models[0]?.value ?? "default") : saved;
}

export function saveCachedModels(models: ModelInfo[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(models));
  } catch {
    // Storage can be unavailable; the list is then fetched again next run.
  }
}
