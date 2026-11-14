import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  confirmDuplicateCleanup,
  type CleanupEvent,
  type DuplicateScanEvent,
  getLastDuplicateScan,
  listDuplicateGroups,
  previewDuplicateCleanup,
  startDuplicateScan,
} from "../../lib/ipc";
import { duplicatePreview, duplicateSummary, file, group, scanSummary, twoGroups } from "../support/duplicateFixtures";
import { sendChannelMessages } from "../support/junkFixtures";
import { installBackend } from "../support/mockBackend";

afterEach(() => {
  clearMocks();
});

/** Respuesta cruda como la serializa el backend para un preview de duplicados. */
function rawPreview(overrides: Record<string, unknown> = {}) {
  const { groupCount, totalItems, recentCount, recentBytes, ...base } = duplicatePreview();
  return {
    ...base,
    totalItems,
    groupCount,
    categories: [
      { category: "duplicates", status: "included", itemCount: totalItems, sizeBytes: base.totalBytes, recentCount, recentBytes },
    ],
    ...overrides,
  };
}

function rawSummary(overrides: Record<string, unknown> = {}) {
  return { ...duplicateSummary(), categories: [{ category: "duplicates", movedCount: 3, movedBytes: 1, skippedCount: 0, errorCount: 0 }], ...overrides };
}

async function rejection(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (error: unknown) {
    return error;
  }
  throw new Error("se esperaba un rechazo");
}

const INVALID = { kind: "invalid_response" };

describe("startDuplicateScan", () => {
  it("sin opciones no envia carpetas ni tamano y decodifica el resumen", async () => {
    const backend = installBackend({ dupScanResult: scanSummary() });
    const summary = await startDuplicateScan({}, () => undefined);
    expect(summary).toEqual(scanSummary());
    const [call] = backend.calls;
    const args = call?.args as Record<string, unknown>;
    expect(Object.keys(args).sort()).toEqual(["onProgress"]);
  });

  it("envia solo claves de carpeta y el tamano minimo elegido, nunca rutas", async () => {
    const backend = installBackend({ dupScanResult: scanSummary() });
    await startDuplicateScan({ roots: ["downloads", "pictures"], minSizeBytes: 10_485_760 }, () => undefined);
    const args = backend.calls[0]?.args as Record<string, unknown>;
    expect(args["roots"]).toEqual(["downloads", "pictures"]);
    expect(args["minSizeBytes"]).toBe(10_485_760);
    expect(JSON.stringify({ ...args, onProgress: null })).not.toMatch(/\\\\|C:/);
  });

  it("entrega los eventos validos por el canal e ignora los invalidos", async () => {
    const events: DuplicateScanEvent[] = [];
    const batch = [
      { type: "started", scanId: 4 },
      { type: "walking", filesSeen: 256 },
      { type: "partial", done: 64, total: 130 },
      { type: "full", doneBytes: 16, totalBytes: 40 },
      { type: "walking", filesSeen: -1 },
      { type: "otro" },
      "basura",
    ];
    mockIPC((_cmd, args) => {
      const { onProgress } = args as { onProgress: import("@tauri-apps/api/core").Channel<unknown> };
      sendChannelMessages(onProgress, batch);
      return scanSummary();
    });
    await startDuplicateScan({}, (event) => events.push(event));
    expect(events).toEqual([
      { type: "started", scanId: 4 },
      { type: "walking", filesSeen: 256 },
      { type: "partial", done: 64, total: 130 },
      { type: "full", doneBytes: 16, totalBytes: 40 },
    ]);
  });

  it("un escaneo cancelado no trae carpetas y se acepta; uno done con carpetas de menos se rechaza", async () => {
    mockIPC(() => scanSummary({ status: "cancelled", roots: [], groupCount: 0 }));
    expect((await startDuplicateScan({}, () => undefined)).status).toBe("cancelled");
    mockIPC(() => scanSummary({ roots: scanSummary().roots.slice(0, 5) }));
    expect(await rejection(startDuplicateScan({}, () => undefined))).toMatchObject(INVALID);
  });

  it.each([
    ["carpeta desconocida", () => scanSummary({ roots: [{ root: "appdata" as never, status: "done", reason: null, filesSeen: 1 }, ...scanSummary().roots.slice(1)] })],
    ["estado desconocido", () => scanSummary({ roots: [{ root: "desktop", status: "gone" as never, reason: null, filesSeen: 1 }, ...scanSummary().roots.slice(1)] })],
    ["unavailable sin motivo", () => scanSummary({ roots: [{ root: "desktop", status: "unavailable", reason: null, filesSeen: 1 }, ...scanSummary().roots.slice(1)] })],
    ["done con motivo", () => scanSummary({ roots: [{ root: "desktop", status: "done", reason: "cloud", filesSeen: 1 }, ...scanSummary().roots.slice(1)] })],
    ["contador inseguro", () => scanSummary({ reclaimableBytes: 2 ** 60 })],
    ["tamano minimo fuera de la lista", () => scanSummary({ minSizeBytes: 5 })],
    ["campo faltante", () => ({ ...scanSummary(), groupCount: undefined })],
  ])("rechaza una respuesta con %s", async (_name, make) => {
    mockIPC(() => make());
    expect(await rejection(startDuplicateScan({}, () => undefined))).toMatchObject(INVALID);
  });
});

describe("getLastDuplicateScan", () => {
  it("devuelve null sin escaneos y el resumen cuando existe", async () => {
    installBackend();
    expect(await getLastDuplicateScan()).toBeNull();
    installBackend({ dupLast: scanSummary({ groupCount: 7 }) });
    expect((await getLastDuplicateScan())?.groupCount).toBe(7);
  });

  it("conserva el error tipado del backend", async () => {
    installBackend({ failures: { get_last_duplicate_scan: { kind: "store_unavailable", message: "x" } } });
    expect(await rejection(getLastDuplicateScan())).toMatchObject({ kind: "store_unavailable" });
  });
});

describe("listDuplicateGroups", () => {
  it("pide la pagina y los filtros con ids y claves", async () => {
    const backend = installBackend({ dupGroups: twoGroups() });
    const page = await listDuplicateGroups(1, 0, 20, { root: "pictures", minSizeBytes: 1_048_576 });
    expect(page.total).toBe(1);
    expect(page.groups[0]?.files[1]?.relativePath).toBe("Viajes\\foto (1).jpg");
    expect(backend.calls[0]?.args).toEqual({ scanId: 1, offset: 0, limit: 20, root: "pictures", minSizeBytes: 1_048_576 });
  });

  it("sin filtros no envia root ni minSizeBytes", async () => {
    const backend = installBackend({ dupGroups: twoGroups() });
    await listDuplicateGroups(1, 0, 20);
    expect(backend.calls[0]?.args).toEqual({ scanId: 1, offset: 0, limit: 20 });
  });

  it.each([
    ["hash corto", { blake3: "abc" }],
    ["hash con mayusculas", { blake3: "A".repeat(64) }],
    ["hash no hexadecimal", { blake3: "g".repeat(64) }],
    ["una sola copia", { files: [file(1, "downloads", "a.bin")] }],
    ["copia con carpeta desconocida", { files: [file(1, "downloads", "a"), { ...file(2, "downloads", "b"), root: "appdata" }] }],
    ["copia sin kept", { files: [file(1, "downloads", "a"), { ...file(2, "downloads", "b"), kept: undefined }] }],
  ])("rechaza un grupo con %s", async (_name, patch) => {
    mockIPC(() => ({ scanId: 1, total: 1, groups: [{ ...group(1, [file(1, "downloads", "a"), file(2, "pictures", "b")]), ...patch }] }));
    expect(await rejection(listDuplicateGroups(1, 0, 20))).toMatchObject(INVALID);
  });

  it("rechaza un id inseguro y un total faltante", async () => {
    mockIPC(() => ({ scanId: 1, total: 1, groups: [group(2 ** 60, [file(1, "downloads", "a"), file(2, "pictures", "b")])] }));
    expect(await rejection(listDuplicateGroups(1, 0, 20))).toMatchObject(INVALID);
    mockIPC(() => ({ scanId: 1, groups: [] }));
    expect(await rejection(listDuplicateGroups(1, 0, 20))).toMatchObject(INVALID);
  });
});

describe("previewDuplicateCleanup", () => {
  it("envia solo ids de grupo y de las copias a conservar", async () => {
    const backend = installBackend({ dupGroups: twoGroups() });
    const result = await previewDuplicateCleanup(1, [{ groupId: 1, keepFileIds: [11] }]);
    expect(backend.calls[0]?.args).toEqual({ scanId: 1, decisions: [{ groupId: 1, keepFileIds: [11] }] });
    expect(result.totalItems).toBe(1);
    expect(result.groupCount).toBe(1);
  });

  it("decodifica las copias recientes de la categoria unica", async () => {
    mockIPC(() => rawPreview({ categories: [{ category: "duplicates", status: "included", itemCount: 1, sizeBytes: 1, recentCount: 2, recentBytes: 9 }] }));
    const result = await previewDuplicateCleanup(1, []);
    expect([result.recentCount, result.recentBytes]).toEqual([2, 9]);
  });

  it.each([
    ["sin categoria", { categories: [] }],
    ["categoria de basura", { categories: [{ category: "temp_user", status: "included", itemCount: 1, sizeBytes: 1, recentCount: 0, recentBytes: 0 }] }],
    ["dos categorias", { categories: [{ category: "duplicates", status: "included", itemCount: 1, sizeBytes: 1, recentCount: 0, recentBytes: 0 }, { category: "duplicates", status: "included", itemCount: 1, sizeBytes: 1, recentCount: 0, recentBytes: 0 }] }],
    ["sin groupCount", { groupCount: undefined }],
    ["recycleBinAvailable no booleano", { recycleBinAvailable: "si" }],
  ])("rechaza un preview %s", async (_name, patch) => {
    mockIPC(() => rawPreview(patch));
    expect(await rejection(previewDuplicateCleanup(1, []))).toMatchObject(INVALID);
  });
});

describe("confirmDuplicateCleanup", () => {
  it("entrega eventos de limpieza y decodifica el resumen con la resolucion", async () => {
    const events: CleanupEvent[] = [];
    mockIPC((_cmd, args) => {
      const { onProgress } = args as { onProgress: import("@tauri-apps/api/core").Channel<unknown> };
      sendChannelMessages(onProgress, [
        { type: "started", previewId: 1, total: 3 },
        { type: "progress", processed: 1, total: 3, movedCount: 1, movedBytes: 5 },
        { type: "progress", processed: -1 },
      ]);
      return rawSummary();
    });
    const result = await confirmDuplicateCleanup(1, (event) => events.push(event));
    expect(events).toHaveLength(2);
    expect(result.groupsResolved).toBe(true);
    expect(result.resolvedGroupIds).toEqual([1, 2]);
    expect(result.movedCount).toBe(3);
    expect(result).not.toHaveProperty("categories");
  });

  it.each([
    ["groupsResolved ausente", { groupsResolved: undefined }],
    ["resolvedGroupIds con un negativo", { resolvedGroupIds: [1, -2] }],
    ["abortReason inconsistente", { status: "aborted", abortReason: null }],
    ["detalle con ruta", { reasons: [{ outcome: "error", detail: "C:\\a", count: 1 }] }],
  ])("rechaza un resumen con %s", async (_name, patch) => {
    mockIPC(() => rawSummary(patch));
    expect(await rejection(confirmDuplicateCleanup(1, () => undefined))).toMatchObject(INVALID);
  });

  it("conserva el error tipado del backend", async () => {
    mockIPC(() => {
      throw { kind: "preview_not_found", message: "x" };
    });
    expect(await rejection(confirmDuplicateCleanup(9, () => undefined))).toMatchObject({ kind: "preview_not_found" });
  });
});
