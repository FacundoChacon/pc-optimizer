import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  cancelScan,
  getLastJunkScan,
  type JunkFinding,
  listJunkFindings,
  type ScanEvent,
  startJunkScan,
} from "../../lib/ipc";
import { summary } from "./junkFixtures";
import { installBackend } from "./mockBackend";

afterEach(() => {
  clearMocks();
});

function finding(id: number): JunkFinding {
  return { id, path: `C:/tmp/${id}`, sizeBytes: 1000 - id, modifiedAt: null, skipReason: null };
}

describe("mockBackend: escaneo de basura", () => {
  it("start_scan envia los eventos programados, devuelve el resultado y lo deja como ultimo escaneo", async () => {
    const result = summary({ temp_user: { itemCount: 1, sizeBytes: 5 } });
    const backend = installBackend({
      scanResult: result,
      scanEvents: [
        { type: "started", scanId: 1 },
        { type: "category", category: "temp_user" },
      ],
    });
    expect(await getLastJunkScan()).toBeNull();
    const events: ScanEvent[] = [];
    await expect(startJunkScan((event) => events.push(event))).resolves.toEqual(result);
    expect(events).toHaveLength(2);
    expect(await getLastJunkScan()).toEqual(result);
    expect(backend.count("start_scan")).toBe(1);
  });

  it("emit entrega eventos posteriores en orden mientras start_scan espera su gate", async () => {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const backend = installBackend({
      scanResult: summary(),
      scanEvents: [{ type: "started", scanId: 1 }],
      gates: { start_scan: gate },
    });
    const events: ScanEvent[] = [];
    const running = startJunkScan((event) => events.push(event));
    await Promise.resolve();
    backend.emit([{ type: "category", category: "wu_cache" }]);
    backend.emit([{ type: "progress", category: "wu_cache", filesSeen: 256, eligibleBytes: 1 }]);
    expect(events.map((event) => event.type)).toEqual(["started", "category", "progress"]);
    release();
    await running;
  });

  it("un resumen cancelado no reemplaza al ultimo escaneo terminado", async () => {
    const done = summary({}, { scanId: 3 });
    installBackend({
      lastScan: done,
      scanResult: summary({}, { scanId: 4, status: "cancelled", categories: [] }),
    });
    await startJunkScan(() => undefined);
    expect(await getLastJunkScan()).toEqual(done);
  });

  it("list_junk_findings pagina por offset y limit y cancel_scan responde", async () => {
    const backend = installBackend({
      findings: { temp_user: Array.from({ length: 250 }, (_, i) => finding(i + 1)) },
    });
    const first = await listJunkFindings(1, "temp_user", 0, 100);
    expect([first.total, first.items.length, first.items[0]?.id]).toEqual([250, 100, 1]);
    const last = await listJunkFindings(1, "temp_user", 200, 100);
    expect([last.items.length, last.items[0]?.id]).toEqual([50, 201]);
    expect((await listJunkFindings(1, "wu_cache", 0, 100)).total).toBe(0);
    await cancelScan(1);
    expect(backend.count("cancel_scan")).toBe(1);
  });

  it("failures hace fallar el comando con el error tipado", async () => {
    installBackend({ failures: { start_scan: { kind: "scan_in_progress", message: "x" } } });
    await expect(startJunkScan(() => undefined)).rejects.toMatchObject({ kind: "scan_in_progress" });
  });
});
