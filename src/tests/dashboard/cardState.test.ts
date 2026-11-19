import { describe, expect, it } from "vitest";
import { junkCardState, memoryCardState, spaceCardState } from "../../features/dashboard/cardState";
import type { MonitorState } from "../../features/monitor/MonitorProvider";
import { diskSummary } from "../support/diskFixtures";
import { summary as junkSummary } from "../support/junkFixtures";
import { metric, sample, summary as monitorSummary } from "../support/monitorFixtures";

const GB = 1_000_000_000;

function monitorState(overrides: Partial<MonitorState> = {}): MonitorState {
  return {
    status: "live",
    samples: [sample(1, { volumeFreeBytes: 120 * GB, volumeTotalBytes: 476 * GB })],
    summary: monitorSummary(),
    intervalMs: 1000,
    windowMs: 300_000,
    stale: false,
    error: null,
    ...overrides,
  };
}

const down = (kind = "monitor_unavailable") =>
  monitorState({ status: "unavailable", samples: [], summary: null, error: { kind, message: "x" } });
const loading = () => monitorState({ status: "loading", samples: [], summary: null });

describe("spaceCardState", () => {
  it("monitor live con 120 de 476 GB y nivel normal -> datos del monitor con nivel", () => {
    expect(spaceCardState(monitorState(), null)).toEqual({
      kind: "data",
      freeBytes: 120 * GB,
      totalBytes: 476 * GB,
      level: "normal",
      analyzedAt: null,
      stale: false,
    });
  });

  it("nivel casi lleno se conserva", () => {
    const almost = monitorState({ summary: monitorSummary({ volume: { usedPct: 95, level: "almost_full" } }) });
    expect(spaceCardState(almost, null)).toMatchObject({ kind: "data", level: "almost_full" });
  });

  it("monitor caido con un analisis de disco terminado -> respaldo con fecha y sin nivel", () => {
    const disk = diskSummary({ volume: { totalBytes: 476 * GB, freeBytes: 100 * GB }, finishedAt: 1_700_000_000_000 });
    expect(spaceCardState(down(), disk)).toEqual({
      kind: "data",
      freeBytes: 100 * GB,
      totalBytes: 476 * GB,
      level: null,
      analyzedAt: 1_700_000_000_000,
      stale: false,
    });
  });

  it("sin monitor ni analisis -> no disponible con el codigo del motivo", () => {
    expect(spaceCardState(down("monitor_unavailable"), null)).toEqual({
      kind: "unavailable",
      code: "monitor_unavailable",
    });
    const noVolume = monitorState({
      summary: monitorSummary({
        volume: { usedPct: null, level: "unknown" },
        availability: { cpu: "ok", memory: "ok", volume: "source_error", diskActive: "ok", diskIo: "ok" },
      }),
    });
    expect(spaceCardState(noVolume, null)).toEqual({ kind: "unavailable", code: "source_error" });
  });

  it("analisis cancelado o sin volumen no sirve de respaldo", () => {
    expect(spaceCardState(down(), diskSummary({ volume: null }))).toMatchObject({ kind: "unavailable" });
    expect(spaceCardState(down(), diskSummary({ status: "cancelled" }))).toMatchObject({ kind: "unavailable" });
  });

  it("monitor cargando -> cargando; stale -> datos marcados sin datos recientes", () => {
    expect(spaceCardState(loading(), diskSummary())).toEqual({ kind: "loading" });
    expect(spaceCardState(monitorState({ stale: true }), null)).toMatchObject({ kind: "data", stale: true });
  });
});

describe("memoryCardState", () => {
  it("memoria al 62 % en nivel normal -> datos con bytes", () => {
    const state = monitorState({
      samples: [sample(1, { memUsedBytes: 9_800_000_000, memTotalBytes: 16_000_000_000 })],
    });
    expect(memoryCardState(state)).toEqual({
      kind: "data",
      pct: 62,
      level: "normal",
      usedBytes: 9_800_000_000,
      totalBytes: 16_000_000_000,
      stale: false,
    });
  });

  it("warming_up -> midiendo; source_error -> no disponible con codigo; monitor caido -> no disponible", () => {
    const base = monitorSummary().availability;
    const warming = monitorState({
      summary: monitorSummary({ memory: metric(null, "unknown"), availability: { ...base, memory: "warming_up" } }),
    });
    expect(memoryCardState(warming)).toEqual({ kind: "measuring" });
    const broken = monitorState({
      summary: monitorSummary({ memory: metric(null, "unknown"), availability: { ...base, memory: "source_error" } }),
    });
    expect(memoryCardState(broken)).toEqual({ kind: "unavailable", code: "source_error" });
    expect(memoryCardState(down("monitor_unavailable"))).toEqual({ kind: "unavailable", code: "monitor_unavailable" });
  });

  it("cargando -> cargando; stale -> datos marcados sin datos recientes", () => {
    expect(memoryCardState(loading())).toEqual({ kind: "loading" });
    expect(memoryCardState(monitorState({ stale: true }))).toMatchObject({ kind: "data", stale: true });
  });
});

describe("junkCardState", () => {
  const cleanable = { temp_user: { scope: "per_user" as const, itemCount: 4, sizeBytes: 1_200_000_000 } };

  it("sin leer todavia -> cargando", () => {
    expect(junkCardState({ loaded: false, last: null, error: null })).toEqual({ kind: "loading" });
  });

  it("sin escaneo -> sin analisis (no es no disponible)", () => {
    expect(junkCardState({ loaded: true, last: null, error: null })).toEqual({ kind: "no_analysis" });
  });

  it("escaneo con 1,2 GB limpiables -> datos con fecha y sin Papelera", () => {
    const last = junkSummary(cleanable, { finishedAt: 1_800_000_005_000 });
    expect(junkCardState({ loaded: true, last, error: null })).toEqual({
      kind: "data",
      bytes: 1_200_000_000,
      scannedAt: 1_800_000_005_000,
      binBytes: null,
    });
  });

  it("la Papelera con elementos se informa aparte y no suma al total", () => {
    const last = junkSummary({ ...cleanable, recycle_bin: { scope: "per_user", itemCount: 9, sizeBytes: 2 * GB } });
    expect(junkCardState({ loaded: true, last, error: null })).toMatchObject({
      kind: "data",
      bytes: 1_200_000_000,
      binBytes: 2 * GB,
    });
  });

  it("registros antiguos no se cuentan como limpiables", () => {
    const last = junkSummary({ ...cleanable, old_logs: { itemCount: 2, sizeBytes: 5 * GB } });
    expect(junkCardState({ loaded: true, last, error: null })).toMatchObject({ bytes: 1_200_000_000 });
  });

  it("escaneo sin nada limpiable -> nada que limpiar con la Papelera aparte", () => {
    const last = junkSummary({ recycle_bin: { scope: "per_user", itemCount: 3, sizeBytes: GB } });
    expect(junkCardState({ loaded: true, last, error: null })).toMatchObject({ kind: "nothing", binBytes: GB });
  });

  it("escaneo ya limpiado -> ya limpiado con la fecha de la limpieza", () => {
    const last = junkSummary(cleanable, { cleanedAt: 1_800_000_900_000 });
    expect(junkCardState({ loaded: true, last, error: null })).toEqual({
      kind: "cleaned",
      cleanedAt: 1_800_000_900_000,
      binBytes: null,
    });
  });

  it("error de lectura sin escaneo -> no disponible con el kind", () => {
    const error = { kind: "store_unavailable", message: "x" };
    expect(junkCardState({ loaded: true, last: null, error })).toEqual({
      kind: "unavailable",
      code: "store_unavailable",
    });
  });
});
