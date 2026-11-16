import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { type MonitorEvent, startMonitorStream, stopMonitorStream } from "../../lib/ipc";
import { installBackend } from "../support/mockBackend";
import { sample, sampleEvent, snapshot, summary } from "../support/monitorFixtures";

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

function answer(raw: unknown): void {
  mockIPC(() => raw);
}

describe("startMonitorStream", () => {
  it("decodifica un snapshot con metricas nulas (nunca 0) y sin resumen", async () => {
    const raw = snapshot([sample(1, { cpuPct: null, diskActivePct: null, diskReadBps: null, diskWriteBps: null })]);
    installBackend({ monitorSnapshot: { ...raw, summary: null } });
    const { snapshot: got } = await startMonitorStream(() => undefined);
    expect(got.samples[0]).toMatchObject({ cpuPct: null, diskActivePct: null, diskReadBps: null, memUsedBytes: 10_000_000_000 });
    expect(got.summary).toBeNull();
    expect(got.subscriptionId).toBe(1);
    expect(got.intervalMs).toBe(1000);
  });

  it("decodifica un snapshot con resumen completo y descarta campos desconocidos (aditivo)", async () => {
    const raw = { ...snapshot([sample(1)]), extra: 1, summary: { ...summary(), temperatures: [] } };
    installBackend({ monitorSnapshot: raw });
    const { snapshot: got } = await startMonitorStream(() => undefined);
    expect(got.summary?.cpu.level).toBe("normal");
    expect(got.summary?.availability.diskIo).toBe("ok");
    expect(got.summary?.volume.usedPct).toBeCloseTo(74.6);
  });

  it.each([
    ["nivel desconocido", { ...summary(), cpu: { current: 1, avg60: 1, peak: 1, level: "enorme" } }],
    ["disponibilidad desconocida", { ...summary(), availability: { ...summary().availability, cpu: "roto" } }],
    ["nivel de espacio desconocido", { ...summary(), volume: { usedPct: 1, level: "lleno" } }],
    ["campo faltante", (() => { const { gapsInWindow: _g, ...rest } = summary(); return rest; })()],
  ])("rechaza un resumen invalido: %s", async (_name, badSummary) => {
    answer({ ...snapshot([sample(1)]), summary: badSummary });
    expect(await rejection(startMonitorStream(() => undefined))).toMatchObject(INVALID);
  });

  it.each([
    ["numero inseguro", { ...sample(1), seq: 2 ** 53 }],
    ["campo faltante", (() => { const { gapBefore: _g, ...rest } = sample(1); return rest; })()],
    ["porcentaje no numerico", { ...sample(1), cpuPct: "23" }],
  ])("rechaza una muestra invalida: %s", async (_name, badSample) => {
    answer({ ...snapshot([]), samples: [badSample] });
    expect(await rejection(startMonitorStream(() => undefined))).toMatchObject(INVALID);
  });

  it("rechaza una respuesta que no es un objeto", async () => {
    answer("hola");
    expect(await rejection(startMonitorStream(() => undefined))).toMatchObject(INVALID);
  });

  it("entrega por el canal las muestras en orden y descarta los eventos invalidos", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([]) });
    const seen: MonitorEvent[] = [];
    await startMonitorStream((event) => seen.push(event));
    backend.emitMonitor([
      sampleEvent(1),
      { kind: "desconocido" },
      { kind: "sample", sample: { seq: "x" }, summary: summary() },
      sampleEvent(2),
      { kind: "stopped", reason: "sampler_failed" },
    ]);
    expect(seen.map((e) => (e.kind === "sample" ? e.sample.seq : e.kind))).toEqual([1, 2, "stopped"]);
  });

  it("un stopped con motivo desconocido se descarta", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([]) });
    const seen: MonitorEvent[] = [];
    await startMonitorStream((event) => seen.push(event));
    backend.emitMonitor([{ kind: "stopped", reason: "otro" }, { kind: "stopped", reason: "sampler_failed" }]);
    expect(seen).toHaveLength(1);
  });

  it("propaga monitor_unavailable como AppError", async () => {
    installBackend({ failures: { start_monitor_stream: { kind: "monitor_unavailable", message: "x" } } });
    expect(await rejection(startMonitorStream(() => undefined))).toMatchObject({ kind: "monitor_unavailable" });
  });
});

describe("stopMonitorStream", () => {
  it("envia el id de suscripcion como numero", async () => {
    const backend = installBackend();
    await stopMonitorStream(7);
    expect(backend.calls.filter((c) => c.cmd === "stop_monitor_stream")[0]?.args).toEqual({ subscriptionId: 7 });
  });

  it("otro id viaja tal cual", async () => {
    const backend = installBackend();
    await stopMonitorStream(0);
    expect(backend.calls.at(-1)?.args).toEqual({ subscriptionId: 0 });
  });
});
