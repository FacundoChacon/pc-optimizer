import type { CleanupEntry, CleanupRun, CleanupRunPage, CleanupRunOutcomes } from "../../lib/ipc";
import type { HistoryFixture } from "./mockBackend";

const NO_OUTCOMES: CleanupRunOutcomes = {
  movedToTrash: 0,
  skippedLocked: 0,
  skippedProtected: 0,
  skippedCloud: 0,
  error: 0,
  recycleBinEmptied: 0,
};

/** 19/11/2026 10:30 (hora local no importa: los textos formatean con `formatDateTime`). */
export const HISTORY_BASE_TS = 1_800_000_000_000;

/** Limpieza de basura del escaneo `scanId`: 340 movidos (1,2 GB aprox.), 3 en uso y 2 protegidos. */
export function junkRun(scanId: number, overrides: Partial<CleanupRun> = {}): CleanupRun {
  return {
    key: { kind: "scan", scanId },
    kind: "junk",
    startedAt: HISTORY_BASE_TS + scanId * 1_000,
    finishedAt: HISTORY_BASE_TS + scanId * 1_000 + 500,
    entryCount: 345,
    outcomes: { ...NO_OUTCOMES, movedToTrash: 340, skippedLocked: 3, skippedProtected: 2 },
    movedBytes: 1_288_490_188,
    emptiedBytes: 0,
    emptyDetail: null,
    categories: ["browser_cache_edge", "temp_user"],
    ...overrides,
  };
}

/** Limpieza de copias duplicadas: 3 archivos (2 GB) movidos a la Papelera. */
export function duplicatesRun(scanId: number, overrides: Partial<CleanupRun> = {}): CleanupRun {
  return {
    key: { kind: "scan", scanId },
    kind: "duplicates",
    startedAt: HISTORY_BASE_TS + scanId * 1_000,
    finishedAt: HISTORY_BASE_TS + scanId * 1_000 + 200,
    entryCount: 3,
    outcomes: { ...NO_OUTCOMES, movedToTrash: 3 },
    movedBytes: 2_147_483_648,
    emptiedBytes: 0,
    emptyDetail: null,
    categories: ["duplicates"],
    ...overrides,
  };
}

/** Vaciado de la Papelera (fila sin escaneo): `emptyDetail` `partial` libero 2 GB. */
export function emptyBinRun(entryId: number, overrides: Partial<CleanupRun> = {}): CleanupRun {
  const at = HISTORY_BASE_TS + entryId * 1_000;
  return {
    key: { kind: "entry", entryId },
    kind: "recycle_bin_empty",
    startedAt: at,
    finishedAt: at,
    entryCount: 1,
    outcomes: { ...NO_OUTCOMES, recycleBinEmptied: 1 },
    movedBytes: 0,
    emptiedBytes: 2_147_483_648,
    emptyDetail: "partial",
    categories: ["recycle_bin"],
    ...overrides,
  };
}

/** Un item del log; la ruta es `null` cuando esta oculta. */
export function historyEntry(id: number, overrides: Partial<CleanupEntry> = {}): CleanupEntry {
  return {
    id,
    ts: HISTORY_BASE_TS + id,
    category: "temp_user",
    path: `C:\Users\ana\AppData\Local\Temp\archivo-${id}.tmp`,
    sizeBytes: 1_048_576,
    outcome: "moved_to_trash",
    errorDetail: null,
    ...overrides,
  };
}

/** `n` items consecutivos de un mismo escaneo. */
export function historyEntries(n: number, firstId = 1): CleanupEntry[] {
  return Array.from({ length: n }, (_, index) => historyEntry(firstId + index));
}

/** Una pagina de limpiezas tal como la devuelve el backend. */
export function runPage(items: CleanupRun[], nextCursor: CleanupRunPage["nextCursor"] = null): CleanupRunPage {
  return { items, nextCursor };
}

/** Limpieza con sus items para el backend simulado. */
export function historyOf(run: CleanupRun, entries: CleanupEntry[] = []): HistoryFixture {
  return { run, entries };
}

/** El log simulado de las pruebas: basura, duplicados y un vaciado parcial, de mas reciente a mas antiguo. */
export function sampleHistory(): HistoryFixture[] {
  return [
    historyOf(emptyBinRun(30), [
      historyEntry(300, { category: "recycle_bin", path: "C:\\", outcome: "recycle_bin_emptied", sizeBytes: 2_147_483_648, errorDetail: "partial" }),
    ]),
    historyOf(duplicatesRun(20), historyEntries(3, 200).map((entry) => ({ ...entry, category: "duplicates" }))),
    historyOf(junkRun(10), historyEntries(5, 100)),
  ];
}
