import {
  type DuplicateCleanupPreview,
  type DuplicateCleanupSummary,
  type DuplicateFile,
  type DuplicateGroup,
  type DuplicateScanSummary,
  ROOT_KEYS,
  type RootKey,
  type RootResult,
} from "../../lib/ipc";

export const MODIFIED = 1_700_000_000_000;

/** Una copia en `root`; la ruta completa y la relativa salen del nombre. */
export function file(
  fileId: number,
  root: RootKey,
  name: string,
  overrides: Partial<DuplicateFile> = {},
): DuplicateFile {
  return {
    fileId,
    root,
    path: `C:\\Users\\ana\\${root}\\${name}`,
    relativePath: name,
    modifiedAt: MODIFIED,
    kept: false,
    ...overrides,
  };
}

/** Grupo de copias identicas de `sizeBytes`; el hash sale del id. */
export function group(
  groupId: number,
  files: DuplicateFile[],
  sizeBytes = 4 * 1024 * 1024,
  overrides: Partial<DuplicateGroup> = {},
): DuplicateGroup {
  return {
    groupId,
    sizeBytes,
    blake3: groupId.toString(16).padStart(2, "0").repeat(32),
    reclaimableBytes: sizeBytes * (files.length - 1),
    resolvedAt: null,
    files,
    ...overrides,
  };
}

/** Los dos grupos habituales: 2 copias (foto) y 3 copias (video). */
export function twoGroups(): DuplicateGroup[] {
  return [
    group(1, [file(11, "downloads", "foto.jpg"), file(12, "pictures", "Viajes\\foto (1).jpg")]),
    group(
      2,
      [
        file(21, "documents", "video.mp4", { modifiedAt: null }),
        file(22, "downloads", "video.mp4"),
        file(23, "videos", "video.mp4"),
      ],
      10 * 1024 * 1024,
    ),
  ];
}

export function roots(overrides: Partial<Record<RootKey, Partial<RootResult>>> = {}): RootResult[] {
  return ROOT_KEYS.map((root) => ({
    root,
    status: "done",
    reason: null,
    filesSeen: 5,
    ...overrides[root],
  }));
}

/** Resumen `done` con 2 grupos; `overrides` ajusta lo que cada test necesita. */
export function scanSummary(overrides: Partial<DuplicateScanSummary> = {}): DuplicateScanSummary {
  return {
    scanId: 1,
    status: "done",
    startedAt: 1_800_000_000_000,
    finishedAt: 1_800_000_005_000,
    minSizeBytes: 1_048_576,
    roots: roots(),
    filesSeen: 30,
    belowMinSize: 4,
    protectedCount: 0,
    cloudCount: 0,
    hardLinkedCount: 0,
    changedCount: 0,
    unreadableCount: 0,
    inaccessibleDirs: 0,
    excludedDirs: 0,
    hashedBytes: 1_000_000,
    groupCount: 2,
    duplicateFileCount: 5,
    reclaimableBytes: 4 * 1024 * 1024 + 20 * 1024 * 1024,
    openGroupCount: 2,
    ...overrides,
  };
}

export function duplicatePreview(overrides: Partial<DuplicateCleanupPreview> = {}): DuplicateCleanupPreview {
  return {
    previewId: 1,
    scanId: 1,
    createdAt: 1_800_000_010_000,
    expiresAt: 1_800_000_610_000,
    totalItems: 3,
    totalBytes: 24 * 1024 * 1024,
    recycleBinAvailable: true,
    groupCount: 2,
    recentCount: 0,
    recentBytes: 0,
    ...overrides,
  };
}

export function duplicateSummary(overrides: Partial<DuplicateCleanupSummary> = {}): DuplicateCleanupSummary {
  return {
    previewId: 1,
    scanId: 1,
    status: "completed",
    abortReason: null,
    startedAt: 1_800_000_020_000,
    finishedAt: 1_800_000_030_000,
    totalItems: 3,
    processed: 3,
    notProcessed: 0,
    movedCount: 3,
    movedBytes: 24 * 1024 * 1024,
    skippedLocked: 0,
    skippedProtected: 0,
    skippedCloud: 0,
    errors: 0,
    reasons: [],
    groupsResolved: true,
    resolvedGroupIds: [1, 2],
    ...overrides,
  };
}
