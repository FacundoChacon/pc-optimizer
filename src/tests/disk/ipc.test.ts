import type { Channel } from "@tauri-apps/api/core";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  type FolderScanEvent,
  getLastFolderScan,
  listFolderChildren,
  listLargeFiles,
  pickFolderRoot,
  revealInExplorer,
  startFolderScan,
} from "../../lib/ipc";
import { diskSummary, folderEntry, largeFile, rootChildren, rootFolder, rootPage, topFiles } from "../support/diskFixtures";
import { sendChannelMessages } from "../support/junkFixtures";
import { installBackend } from "../support/mockBackend";

afterEach(() => {
  clearMocks();
});

const INVALID = { kind: "invalid_response" };

async function rejection(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (error: unknown) {
    return error;
  }
  throw new Error("se esperaba un rechazo");
}

/** Respuesta cruda de un comando: `mockIPC` devuelve exactamente `raw`. */
function answer(raw: unknown): void {
  mockIPC(() => raw);
}

describe("startFolderScan", () => {
  it("con el disco del sistema envia solo el tipo de raiz y el canal, y decodifica el resumen", async () => {
    const backend = installBackend({ diskScanResult: diskSummary() });
    const summary = await startFolderScan({ kind: "system_drive" }, () => undefined);
    expect(summary).toEqual(diskSummary());
    const args = backend.calls[0]?.args as Record<string, unknown>;
    expect(Object.keys(args).sort()).toEqual(["onProgress", "root"]);
    expect(args["root"]).toEqual({ kind: "system_drive" });
  });

  it("con una carpeta elegida envia su pickId y nunca una ruta", async () => {
    const backend = installBackend({ diskScanResult: diskSummary({ root: { kind: "picked", path: "C:\\x", folderId: 1 } }) });
    await startFolderScan({ kind: "picked", pickId: 7 }, () => undefined);
    const args = backend.calls[0]?.args as { root: unknown } & Record<string, unknown>;
    expect(args.root).toEqual({ kind: "picked", pickId: 7 });
    expect(JSON.stringify({ ...args, onProgress: null })).not.toMatch(/\\\\|C:/);
  });

  it("entrega los eventos validos por el canal e ignora los invalidos", async () => {
    const events: FolderScanEvent[] = [];
    const batch = [
      { type: "started", scanId: 4, rootKind: "system_drive" },
      { type: "walking", filesSeen: 4096, dirsSeen: 300, bytesSeen: 123 },
      { type: "saving" },
      { type: "walking", filesSeen: -1, dirsSeen: 0, bytesSeen: 0 },
      { type: "started", scanId: 5, rootKind: "otro" },
      { type: "otro" },
      "basura",
    ];
    mockIPC((_cmd, args) => {
      sendChannelMessages((args as { onProgress: Channel<unknown> }).onProgress, batch);
      return diskSummary();
    });
    await startFolderScan({ kind: "system_drive" }, (event) => events.push(event));
    expect(events).toEqual([
      { type: "started", scanId: 4, rootKind: "system_drive" },
      { type: "walking", filesSeen: 4096, dirsSeen: 300, bytesSeen: 123 },
      { type: "saving" },
    ]);
  });

  it("acepta un resumen cancelado sin carpeta raiz y rechaza la combinacion inconsistente", async () => {
    const cancelled = diskSummary({
      status: "cancelled",
      root: { kind: "system_drive", path: "", folderId: null },
      volume: null,
      unaccountedBytes: null,
      totalBytes: 0,
      foldersStored: 0,
    });
    answer(cancelled);
    expect(await startFolderScan({ kind: "system_drive" }, () => undefined)).toEqual(cancelled);
    answer({ ...cancelled, root: { kind: "system_drive", path: "", folderId: 3 } });
    expect(await rejection(startFolderScan({ kind: "system_drive" }, () => undefined))).toMatchObject(INVALID);
    answer(diskSummary({ root: { kind: "system_drive", path: "C:\\", folderId: null } }));
    expect(await rejection(startFolderScan({ kind: "system_drive" }, () => undefined))).toMatchObject(INVALID);
  });

  it("propaga el error tipado del backend", async () => {
    installBackend({ failures: { start_folder_scan: { kind: "folder_pick_not_found", message: "x" } } });
    expect(await rejection(startFolderScan({ kind: "picked", pickId: 1 }, () => undefined))).toEqual({
      kind: "folder_pick_not_found",
      message: "x",
    });
  });
});

describe("getLastFolderScan", () => {
  it("devuelve null sin analisis y el resumen decodificado con uno", async () => {
    installBackend();
    expect(await getLastFolderScan()).toBeNull();
    clearMocks();
    installBackend({ diskLast: diskSummary({ unaccountedBytes: null, volume: null }) });
    expect(await getLastFolderScan()).toEqual(diskSummary({ unaccountedBytes: null, volume: null }));
  });

  it("conserva la muestra de carpetas sin acceso con su motivo", async () => {
    const issues = [
      { path: "C:\\Windows\\CSC", reason: "access_denied" as const },
      { path: "C:\\x", reason: "not_found" as const },
      { path: "C:\\y", reason: "io_error" as const },
    ];
    installBackend({ diskLast: diskSummary({ inaccessibleDirs: 3, issues }) });
    expect((await getLastFolderScan())?.issues).toEqual(issues);
  });

  const invalidCases: Array<[string, (valid: Record<string, unknown>) => unknown]> = [
    ["estado desconocido", (v) => ({ ...v, status: "partial" })],
    ["contador negativo", (v) => ({ ...v, fileCount: -1 })],
    ["contador inseguro", (v) => ({ ...v, totalBytes: 2 ** 53 })],
    ["contador faltante", (v) => ({ ...v, cloudFiles: undefined })],
    ["tipo de raiz desconocido", (v) => ({ ...v, root: { kind: "other", path: "C:\\", folderId: 1 } })],
    ["ruta de raiz vacia en un analisis terminado", (v) => ({ ...v, root: { kind: "system_drive", path: "", folderId: 1 } })],
    ["volumen incompleto", (v) => ({ ...v, volume: { totalBytes: 5 } })],
    ["no contabilizado negativo", (v) => ({ ...v, unaccountedBytes: -3 })],
    ["overCounted no booleano", (v) => ({ ...v, overCounted: "no" })],
    ["truncado no booleano", (v) => ({ ...v, foldersTruncated: 1 })],
    ["motivo de incidencia desconocido", (v) => ({ ...v, issues: [{ path: "C:\\x", reason: "timeout" }] })],
    ["incidencia sin ruta", (v) => ({ ...v, issues: [{ path: "", reason: "io_error" }] })],
    ["mas de 50 incidencias", (v) => ({ ...v, issues: Array.from({ length: 51 }, (_, i) => ({ path: `C:\\${i}`, reason: "io_error" })) })],
    ["respuesta que no es un objeto", () => "hola"],
  ];
  it.each(invalidCases)("rechaza como invalid_response: %s", async (_name, mutate) => {
    answer(mutate(diskSummary() as unknown as Record<string, unknown>));
    expect(await rejection(getLastFolderScan())).toMatchObject(INVALID);
  });
});

describe("listFolderChildren", () => {
  it("envia el analisis, la carpeta (null = raiz), el orden y la paginacion, y decodifica la pagina", async () => {
    const backend = installBackend({
      diskPages: { root: { folder: rootFolder(), breadcrumb: rootPage().breadcrumb, children: rootChildren(), remainderBytes: 10 } },
    });
    const page = await listFolderChildren(1, null, "size", 0, 50);
    expect(page.total).toBe(6);
    expect(page.items[0]?.name).toBe("Windows");
    expect(page.remainderBytes).toBe(10);
    expect(backend.calls[0]?.args).toEqual({ scanId: 1, folderId: null, sort: "size", offset: 0, limit: 50 });
    await listFolderChildren(1, 3, "name", 50, 50).catch(() => undefined);
    expect(backend.calls[1]?.args).toEqual({ scanId: 1, folderId: 3, sort: "name", offset: 50, limit: 50 });
  });

  it("decodifica carpetas opacas, de sistema, sin acceso y con sugerencia de limpieza", async () => {
    const items = [
      folderEntry(9, "bob", 5, { opaque: "other_user_profile", hasChildren: false }),
      folderEntry(10, "Windows", 5, { system: true }),
      folderEntry(11, "locked", 0, { access: "inaccessible", hasChildren: false }),
      folderEntry(12, "Cache", 5, { access: "incomplete", junkHint: true }),
      folderEntry(13, "Papelera", 5, { opaque: "recycle_bin", hasChildren: false }),
      folderEntry(14, "SVI", 5, { opaque: "system_volume_information", hasChildren: false }),
    ];
    answer(rootPage(items));
    const page = await listFolderChildren(1, null, "size", 0, 50);
    expect(page.items).toEqual(items);
  });

  const valid = (): Record<string, unknown> => ({ ...rootPage() });
  const invalidCases: Array<[string, unknown]> = [
    ["acceso desconocido", { ...valid(), items: [{ ...folderEntry(2, "a", 1), access: "partial" }] }],
    ["opacidad desconocida", { ...valid(), items: [{ ...folderEntry(2, "a", 1), opaque: "appdata" }] }],
    ["carpeta sin campo", { ...valid(), items: [{ ...folderEntry(2, "a", 1), junkHint: undefined }] }],
    ["tamano inseguro", { ...valid(), items: [{ ...folderEntry(2, "a", 1), sizeBytes: 2 ** 53 }] }],
    ["sin migas de pan", { ...valid(), breadcrumb: [] }],
    ["miga sin nombre", { ...valid(), breadcrumb: [{ id: 1, name: "" }] }],
    ["resto negativo", { ...valid(), remainderBytes: -1 }],
    ["total faltante", { ...valid(), total: undefined }],
    ["items que no es una lista", { ...valid(), items: "x" }],
  ];
  it.each(invalidCases)("rechaza como invalid_response: %s", async (_name, raw) => {
    answer(raw);
    expect(await rejection(listFolderChildren(1, null, "size", 0, 50))).toMatchObject(INVALID);
  });
});

describe("listLargeFiles", () => {
  it("envia el orden y la paginacion y decodifica el top con fechas opcionales", async () => {
    const backend = installBackend({ diskFiles: topFiles() });
    const page = await listLargeFiles(1, "newest", 0, 5);
    expect(page.total).toBe(6);
    expect(page.items).toHaveLength(5);
    expect(backend.calls[0]?.args).toEqual({ scanId: 1, sort: "newest", offset: 0, limit: 5 });
    const old = await listLargeFiles(1, "oldest", 0, 6);
    expect(old.items.at(-1)?.modifiedAt).toBeNull();
  });

  const invalidCases: Array<[string, unknown]> = [
    ["archivo sin nombre", { scanId: 1, total: 1, items: [{ ...largeFile(1, "a", 5), name: "" }] }],
    ["tamano cero o negativo", { scanId: 1, total: 1, items: [{ ...largeFile(1, "a", 5), sizeBytes: -1 }] }],
    ["fecha invalida", { scanId: 1, total: 1, items: [{ ...largeFile(1, "a", 5), modifiedAt: "ayer" }] }],
    ["carpeta faltante", { scanId: 1, total: 1, items: [{ ...largeFile(1, "a", 5), folderId: undefined }] }],
    ["total negativo", { scanId: 1, total: -2, items: [] }],
  ];
  it.each(invalidCases)("rechaza como invalid_response: %s", async (_name, raw) => {
    answer(raw);
    expect(await rejection(listLargeFiles(1, "size", 0, 5))).toMatchObject(INVALID);
  });
});

describe("pickFolderRoot", () => {
  it("decodifica una carpeta elegida y una seleccion cancelada", async () => {
    installBackend({
      pickQueue: [{ status: "picked", pickId: 3, path: "C:\\Users\\ana\\Downloads" }, { status: "cancelled" }],
    });
    expect(await pickFolderRoot()).toEqual({ status: "picked", pickId: 3, path: "C:\\Users\\ana\\Downloads" });
    expect(await pickFolderRoot()).toEqual({ status: "cancelled" });
  });

  it("propaga el rechazo tipado", async () => {
    installBackend({ pickQueue: [{ kind: "folder_root_rejected", message: "folder root rejected: reparse_point" }] });
    expect(await rejection(pickFolderRoot())).toEqual({
      kind: "folder_root_rejected",
      message: "folder root rejected: reparse_point",
    });
  });

  it.each([
    ["estado desconocido", { status: "maybe" }],
    ["pickId negativo", { status: "picked", pickId: -1, path: "C:\\x" }],
    ["sin ruta", { status: "picked", pickId: 1, path: "" }],
    ["sin pickId", { status: "picked", path: "C:\\x" }],
  ])("rechaza como invalid_response: %s", async (_name, raw) => {
    answer(raw);
    expect(await rejection(pickFolderRoot())).toMatchObject(INVALID);
  });
});

describe("revealInExplorer", () => {
  it("envia el analisis y el elemento por id, nunca una ruta", async () => {
    const backend = installBackend();
    await revealInExplorer(1, { kind: "file", id: 5 });
    await revealInExplorer(1, { kind: "folder", id: 3 });
    expect(backend.calls.map((call) => call.args)).toEqual([
      { scanId: 1, target: { kind: "file", id: 5 } },
      { scanId: 1, target: { kind: "folder", id: 3 } },
    ]);
  });

  it("propaga reveal_unavailable", async () => {
    installBackend({ failures: { reveal_in_explorer: { kind: "reveal_unavailable", message: "reveal unavailable: opaque" } } });
    expect(await rejection(revealInExplorer(1, { kind: "folder", id: 2 }))).toEqual({
      kind: "reveal_unavailable",
      message: "reveal unavailable: opaque",
    });
  });
});
