import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { listCleanupEntries, listCleanupRuns } from "../../lib/ipc";
import { emptyBinRun, historyEntry, junkRun, runPage } from "../support/historyFixtures";

afterEach(() => {
  clearMocks();
});

/** Respuesta cruda de un comando: `mockIPC` devuelve exactamente `raw`. */
function answer(raw: unknown): void {
  mockIPC(() => raw);
}

describe("listCleanupRuns", () => {
  it("decodifica una pagina con una limpieza de basura y un vaciado parcial de la Papelera", async () => {
    const cursor = { finishedAt: 1_800_000_010_000, key: { kind: "scan", scanId: 10 } } as const;
    answer(runPage([emptyBinRun(30), junkRun(10)], cursor));

    const page = await listCleanupRuns(null, 2);

    expect(page.items).toHaveLength(2);
    expect(page.items[0]).toMatchObject({
      key: { kind: "entry", entryId: 30 },
      kind: "recycle_bin_empty",
      emptyDetail: "partial",
      emptiedBytes: 2_147_483_648,
    });
    expect(page.items[1]?.outcomes.movedToTrash).toBe(340);
    expect(page.nextCursor).toEqual(cursor);
  });
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

describe("listCleanupRuns: respuestas invalidas", () => {
  it("rechaza una limpieza de tipo desconocido", async () => {
    answer(runPage([{ ...junkRun(1), kind: "wipe" } as unknown as ReturnType<typeof junkRun>]));

    expect(await rejection(listCleanupRuns(null, 10))).toMatchObject(INVALID);
  });

  it("rechaza una clave de tipo desconocido, ya sea en la limpieza o en el cursor", async () => {
    answer({ items: [{ ...junkRun(1), key: { kind: "folder", id: 1 } }], nextCursor: null });
    expect(await rejection(listCleanupRuns(null, 10))).toMatchObject(INVALID);

    answer({ items: [], nextCursor: { finishedAt: 5, key: { kind: "folder", id: 1 } } });
    expect(await rejection(listCleanupRuns(null, 10))).toMatchObject(INVALID);
  });

  it("rechaza numeros inseguros, negativos o fraccionarios", async () => {
    for (const bad of [Number.MAX_SAFE_INTEGER + 1, -1, 1.5]) {
      answer(runPage([{ ...junkRun(1), movedBytes: bad }]));
      expect(await rejection(listCleanupRuns(null, 10))).toMatchObject(INVALID);
    }
  });

  it("rechaza un campo faltante y un detalle de vaciado desconocido", async () => {
    const { outcomes: _outcomes, ...withoutOutcomes } = junkRun(1);
    answer({ items: [withoutOutcomes], nextCursor: null });
    expect(await rejection(listCleanupRuns(null, 10))).toMatchObject(INVALID);

    answer(runPage([{ ...emptyBinRun(2), emptyDetail: "failed" as never }]));
    expect(await rejection(listCleanupRuns(null, 10))).toMatchObject(INVALID);
  });

  it("rechaza una respuesta que no es una pagina", async () => {
    for (const raw of [null, "x", { items: "no" }, { items: [], nextCursor: undefined }]) {
      answer(raw);
      expect(await rejection(listCleanupRuns(null, 10))).toMatchObject(INVALID);
    }
  });

  it("ignora los campos de mas", async () => {
    answer({ items: [{ ...junkRun(1), futuro: true }], nextCursor: null, otro: 1 });

    const page = await listCleanupRuns(null, 10);

    expect(page.items[0]).toEqual(junkRun(1));
  });

  it("envia el cursor y el limite tal cual", async () => {
    const seen: unknown[] = [];
    mockIPC((cmd, args) => {
      seen.push({ cmd, args });
      return { items: [], nextCursor: null };
    });
    const cursor = { finishedAt: 99, key: { kind: "entry", entryId: 4 } } as const;

    await listCleanupRuns(cursor, 20);
    await listCleanupRuns(null, 3);

    expect(seen).toEqual([
      { cmd: "list_cleanup_runs", args: { before: cursor, limit: 20 } },
      { cmd: "list_cleanup_runs", args: { before: null, limit: 3 } },
    ]);
  });
});

describe("listCleanupEntries", () => {
  it("decodifica los items y acepta la ruta nula", async () => {
    answer({
      total: 2,
      pathsHidden: true,
      items: [historyEntry(1, { path: null }), historyEntry(2, { path: null, outcome: "error", errorDetail: "access_denied" })],
    });

    const page = await listCleanupEntries({ kind: "scan", scanId: 7 }, 0, 100);

    expect(page).toMatchObject({ total: 2, pathsHidden: true });
    expect(page.items.map((item) => item.path)).toEqual([null, null]);
    expect(page.items[1]).toMatchObject({ outcome: "error", errorDetail: "access_denied" });
  });

  it("decodifica rutas completas cuando no estan ocultas y manda la clave, el desplazamiento y el limite", async () => {
    const seen: unknown[] = [];
    mockIPC((cmd, args) => {
      seen.push({ cmd, args });
      return { total: 1, pathsHidden: false, items: [historyEntry(1)] };
    });

    const page = await listCleanupEntries({ kind: "entry", entryId: 9 }, 200, 100);

    expect(page.items[0]?.path).toBe("C:\\Users\\ana\\AppData\\Local\\Temp\\archivo-1.tmp");
    expect(seen).toEqual([
      { cmd: "list_cleanup_entries", args: { key: { kind: "entry", entryId: 9 }, offset: 200, limit: 100 } },
    ]);
  });

  it("rechaza un resultado desconocido, una ruta que no es texto y pathsHidden ausente", async () => {
    answer({ total: 1, pathsHidden: false, items: [{ ...historyEntry(1), outcome: "deleted" }] });
    expect(await rejection(listCleanupEntries({ kind: "scan", scanId: 1 }, 0, 10))).toMatchObject(INVALID);

    answer({ total: 1, pathsHidden: false, items: [{ ...historyEntry(1), path: 5 }] });
    expect(await rejection(listCleanupEntries({ kind: "scan", scanId: 1 }, 0, 10))).toMatchObject(INVALID);

    answer({ total: 1, items: [historyEntry(1)] });
    expect(await rejection(listCleanupEntries({ kind: "scan", scanId: 1 }, 0, 10))).toMatchObject(INVALID);
  });

  it("rechaza un total inseguro", async () => {
    answer({ total: Number.MAX_SAFE_INTEGER + 1, pathsHidden: false, items: [] });

    expect(await rejection(listCleanupEntries({ kind: "scan", scanId: 1 }, 0, 10))).toMatchObject(INVALID);
  });
});
