import type { Channel } from "@tauri-apps/api/core";
import type { CategorySummary, JunkCategory, JunkScanSummary } from "../../lib/ipc";

/** Las 11 categorias en el orden estable de la UI (espejo de `JunkCategory::ALL`). */
export const ALL_CATEGORIES: readonly JunkCategory[] = [
  "temp_user",
  "temp_windows",
  "browser_cache_chrome",
  "browser_cache_edge",
  "browser_cache_firefox",
  "browser_cache_brave",
  "old_logs",
  "wu_cache",
  "thumbnail_cache",
  "crash_dumps",
  "recycle_bin",
];

/** Categoria `done` sin hallazgos; `overrides` ajusta lo que cada test necesita. */
export function category(name: JunkCategory, overrides: Partial<CategorySummary> = {}): CategorySummary {
  return {
    category: name,
    scope: "machine",
    status: "done",
    itemCount: 0,
    sizeBytes: 0,
    skippedCount: 0,
    inaccessibleCount: 0,
    ...overrides,
  };
}

/** Resumen `done` con las 11 categorias; `byCategory` ajusta algunas. */
export function summary(
  byCategory: Partial<Record<JunkCategory, Partial<CategorySummary>>> = {},
  overrides: Partial<JunkScanSummary> = {},
): JunkScanSummary {
  const categories = ALL_CATEGORIES.map((name) => category(name, byCategory[name]));
  const counted = categories.filter((c) => c.category !== "recycle_bin");
  return {
    scanId: 1,
    status: "done",
    startedAt: 1_800_000_000_000,
    finishedAt: 1_800_000_005_000,
    totalItems: counted.reduce((n, c) => n + c.itemCount, 0),
    totalBytes: counted.reduce((n, c) => n + c.sizeBytes, 0),
    categories,
    ...overrides,
  };
}

interface TauriInternals {
  runCallback: (id: number, data: unknown) => void;
}

/** Entrega mensajes al `Channel` como lo hace el runtime: ordenados por `index`. */
export function sendChannelMessages(
  channel: Channel<unknown>,
  messages: readonly unknown[],
  startIndex = 0,
): void {
  const internals = (window as unknown as { __TAURI_INTERNALS__: TauriInternals }).__TAURI_INTERNALS__;
  messages.forEach((message, offset) => {
    internals.runCallback(channel.id, { index: startIndex + offset, message });
  });
}
