import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CleanupEntry, CleanupEntryPage, CleanupRunKey, CleanupRunPage } from "../../lib/ipc";
import { historyEntries, junkRun, runPage } from "../support/historyFixtures";

const ipc = vi.hoisted(() => ({
  listCleanupEntries: vi.fn(),
  listCleanupRuns: vi.fn(),
}));

vi.mock("../../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/ipc")>()),
  listCleanupEntries: ipc.listCleanupEntries,
  listCleanupRuns: ipc.listCleanupRuns,
}));

import { useCleanupRuns } from "../../features/history/useCleanupRuns";
import { useRunEntries } from "../../features/history/useRunEntries";

afterEach(() => vi.resetAllMocks());

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const STABLE_KEY: CleanupRunKey = { kind: "scan", scanId: 9 };
const entryPage = (items: CleanupEntry[]): CleanupEntryPage => ({ items, total: items.length, pathsHidden: false });

describe("useRunEntries: respuestas obsoletas", () => {
  it("una respuesta lenta de la clave anterior no se aplica despues de cambiar la clave", async () => {
    const first = deferred<CleanupEntryPage>();
    const second = deferred<CleanupEntryPage>();
    ipc.listCleanupEntries.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const keyA: CleanupRunKey = { kind: "scan", scanId: 1 };
    const keyB: CleanupRunKey = { kind: "scan", scanId: 2 };
    const { result, rerender } = renderHook(({ key }) => useRunEntries(key), { initialProps: { key: keyA } });
    rerender({ key: keyB });
    await act(async () => second.resolve(entryPage(historyEntries(2, 100))));
    await waitFor(() => expect(result.current.items.map((item) => item.id)).toEqual([100, 101]));
    await act(async () => first.resolve(entryPage(historyEntries(3, 1))));
    expect(result.current.items.map((item) => item.id)).toEqual([100, 101]);
    expect(result.current.total).toBe(2);
  });

  it("sin cambio de clave la respuesta se aplica y Cargar mas concatena", async () => {
    ipc.listCleanupEntries
      .mockResolvedValueOnce({ items: historyEntries(2, 1), total: 3, pathsHidden: false })
      .mockResolvedValueOnce({ items: historyEntries(1, 3), total: 3, pathsHidden: false });
    const { result } = renderHook(() => useRunEntries(STABLE_KEY));
    await waitFor(() => expect(result.current.items).toHaveLength(2));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.items.map((item) => item.id)).toEqual([1, 2, 3]));
  });
});

describe("useCleanupRuns: respuestas obsoletas", () => {
  it("una respuesta lenta del tamano de pagina anterior no se aplica despues de cambiarlo", async () => {
    const first = deferred<CleanupRunPage>();
    const second = deferred<CleanupRunPage>();
    ipc.listCleanupRuns.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { result, rerender } = renderHook(({ size }) => useCleanupRuns(size), { initialProps: { size: 3 } });
    rerender({ size: 20 });
    await act(async () => second.resolve(runPage([junkRun(7)])));
    await waitFor(() => expect(result.current.items).toHaveLength(1));
    await act(async () => first.resolve(runPage([junkRun(1), junkRun(2), junkRun(3)])));
    expect(result.current.items.map((run) => run.key)).toEqual([{ kind: "scan", scanId: 7 }]);
  });

  it("sin cambio de tamano la primera pagina se aplica", async () => {
    ipc.listCleanupRuns.mockResolvedValueOnce(runPage([junkRun(4), junkRun(5)]));
    const { result } = renderHook(() => useCleanupRuns(3));
    await waitFor(() => expect(result.current.items).toHaveLength(2));
    expect(result.current.hasMore).toBe(false);
  });
});
