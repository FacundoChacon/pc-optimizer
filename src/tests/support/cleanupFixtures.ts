import type {
  CleanupPreview,
  CleanupSummary,
  JunkCategory,
  PreviewCategory,
} from "../../lib/ipc";

/** Categoria `included` del preview sin recientes; `overrides` ajusta lo que cada test necesita. */
export function previewCategory(
  name: JunkCategory,
  overrides: Partial<PreviewCategory> = {},
): PreviewCategory {
  return {
    category: name,
    status: "included",
    itemCount: 0,
    sizeBytes: 0,
    recentCount: 0,
    recentBytes: 0,
    ...overrides,
  };
}

/** Preview con `temp_user` (3 archivos, 5 MB) y `temp_windows` (2 archivos, 2 KB). */
export function preview(overrides: Partial<CleanupPreview> = {}): CleanupPreview {
  const categories = overrides.categories ?? [
    previewCategory("temp_user", { itemCount: 3, sizeBytes: 5 * 1024 * 1024, recentCount: 1, recentBytes: 100 }),
    previewCategory("temp_windows", { itemCount: 2, sizeBytes: 2048 }),
  ];
  return {
    previewId: 1,
    scanId: 1,
    createdAt: 1_800_000_010_000,
    expiresAt: 1_800_000_610_000,
    totalItems: categories.reduce((n, c) => n + c.itemCount, 0),
    totalBytes: categories.reduce((n, c) => n + c.sizeBytes, 0),
    recycleBinAvailable: true,
    ...overrides,
    categories,
  };
}

/** Resumen `completed`: 10 movidos (1 MB) y 2 en uso; `overrides` ajusta el resto. */
export function cleanupSummary(overrides: Partial<CleanupSummary> = {}): CleanupSummary {
  return {
    previewId: 1,
    scanId: 1,
    status: "completed",
    abortReason: null,
    startedAt: 1_800_000_020_000,
    finishedAt: 1_800_000_030_000,
    totalItems: 12,
    processed: 12,
    notProcessed: 0,
    movedCount: 10,
    movedBytes: 1024 * 1024,
    skippedLocked: 2,
    skippedProtected: 0,
    skippedCloud: 0,
    errors: 0,
    reasons: [{ outcome: "skipped_locked", detail: "in_use", count: 2 }],
    categories: [{ category: "temp_user", movedCount: 10, movedBytes: 1024 * 1024, skippedCount: 2, errorCount: 0 }],
    ...overrides,
  };
}
