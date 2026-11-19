import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { listCleanupEntries, listCleanupRuns } from "../../lib/ipc";
import { historyEntries, historyOf, junkRun, sampleHistory } from "./historyFixtures";
import { OTHER_USER, installBackend } from "./mockBackend";

afterEach(() => {
  clearMocks();
});

describe("mockBackend: historial de limpieza", () => {
  it("pagina las limpiezas con el cursor, de la mas reciente a la mas antigua y sin repetir", async () => {
    installBackend({ history: sampleHistory() });

    const first = await listCleanupRuns(null, 2);
    const second = await listCleanupRuns(first.nextCursor, 2);

    expect(first.items.map((run) => run.kind)).toEqual(["recycle_bin_empty", "duplicates"]);
    expect(first.nextCursor).not.toBeNull();
    expect(second.items.map((run) => run.kind)).toEqual(["junk"]);
    expect(second.nextCursor).toBeNull();
  });

  it("sin historial responde una pagina vacia", async () => {
    installBackend();

    await expect(listCleanupRuns(null, 10)).resolves.toEqual({ items: [], nextCursor: null });
  });

  it("pagina los items de una limpieza con su total y no mezcla otras limpiezas", async () => {
    installBackend({
      history: [historyOf(junkRun(10), historyEntries(250)), ...sampleHistory().slice(0, 1)],
    });

    const page = await listCleanupEntries({ kind: "scan", scanId: 10 }, 200, 100);

    expect(page.total).toBe(250);
    expect(page.items).toHaveLength(50);
    expect(page.pathsHidden).toBe(false);
    expect(page.items[0]?.path).not.toBeNull();
  });

  it("con otra cuenta en la sesion oculta las rutas y lo avisa", async () => {
    installBackend({ history: sampleHistory(), privilege: OTHER_USER });

    const page = await listCleanupEntries({ kind: "scan", scanId: 10 }, 0, 100);

    expect(page.pathsHidden).toBe(true);
    expect(page.items.length).toBeGreaterThan(0);
    expect(page.items.every((item) => item.path === null)).toBe(true);
  });

  it("una clave inexistente da una pagina vacia con total 0", async () => {
    installBackend({ history: sampleHistory() });

    await expect(listCleanupEntries({ kind: "scan", scanId: 999 }, 0, 10)).resolves.toMatchObject({
      total: 0,
      items: [],
    });
  });
});
