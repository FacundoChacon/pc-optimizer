import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import type { MonitorSample, MonitorSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { installBackend, type MockBackend } from "../support/mockBackend";
import { sample, sampleEvent, snapshot, summary } from "../support/monitorFixtures";

afterEach(() => {
  cleanup();
  clearMocks();
});

const ADVANCED = { uiMode: "advanced", autostartTray: false, language: "es" } as const;

async function openAdvanced(sum: MonitorSummary, samples: MonitorSample[] = [sample(1), sample(2), sample(3)]): Promise<MockBackend> {
  const backend = installBackend({ settings: ADVANCED, monitorSnapshot: snapshot(samples, { summary: sum }) });
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.monitor }));
  await screen.findByRole("heading", { level: 1, name: es.monitor.title });
  await screen.findByRole("region", { name: es.monitor.advanced.metersTitle });
  return backend;
}

function chartPath(name: string): string {
  const chart = screen.getByRole("group", { name });
  return chart.querySelector("path[data-series]")?.getAttribute("d") ?? "";
}

const rich = summary({
  cpu: { current: 40, avg60: 35, peak: 90, level: "normal" },
  memory: { current: 62, avg60: 60, peak: 70, level: "normal" },
  diskActive: { current: 12, avg60: 10, peak: 55, level: "normal" },
});

describe("Monitor en modo avanzado", () => {
  it("cuatro medidores con valor actual, promedio de 60 s y pico", async () => {
    await openAdvanced(rich);
    const meters = screen.getByRole("region", { name: es.monitor.advanced.metersTitle });
    expect(within(meters).getByText(es.monitor.metrics.cpu)).toBeInTheDocument();
    expect(within(meters).getByText(es.monitor.metrics.memory)).toBeInTheDocument();
    expect(within(meters).getByText(es.monitor.metrics.diskActive)).toBeInTheDocument();
    expect(within(meters).getByText(es.monitor.metrics.spaceUsed)).toBeInTheDocument();
    expect(within(meters).getByText("40 %")).toBeInTheDocument();
    expect(within(meters).getByText(format(es.monitor.advanced.meterDetail, { avg: "35 %", peak: "90 %" }))).toBeInTheDocument();
    expect(within(meters).getByText(format(es.monitor.advanced.meterDetail, { avg: "10 %", peak: "55 %" }))).toBeInTheDocument();
    expect(meters.querySelectorAll("rect[data-meter='fill']")).toHaveLength(4);
  });

  it("una metrica ausente no se dibuja como 0: sin relleno y con su motivo", async () => {
    await openAdvanced(
      summary({
        cpu: { current: null, avg60: null, peak: null, level: "unknown" },
        availability: { ...summary().availability, cpu: "warming_up" },
      }),
      [sample(1, { cpuPct: null })],
    );
    const meters = screen.getByRole("region", { name: es.monitor.advanced.metersTitle });
    expect(meters.querySelectorAll("rect[data-meter='fill']")).toHaveLength(3);
    expect(within(meters).getByText(es.monitor.measuring)).toBeInTheDocument();
  });

  it("cuatro graficos que incorporan una muestra nueva", async () => {
    const backend = await openAdvanced(rich);
    for (const name of [
      es.monitor.metrics.cpu,
      es.monitor.metrics.memory,
      es.monitor.metrics.diskActive,
      es.monitor.metrics.diskIo,
    ]) {
      expect(screen.getByRole("group", { name })).toBeInTheDocument();
    }
    const before = chartPath(es.monitor.metrics.cpu);
    act(() => backend.emitMonitor([sampleEvent(4, rich, { cpuPct: 80 })]));
    expect(chartPath(es.monitor.metrics.cpu)).not.toBe(before);
  });

  it("lectura y escritura son dos series con leyenda", async () => {
    await openAdvanced(rich);
    const chart = screen.getByRole("group", { name: es.monitor.metrics.diskIo });
    expect(chart.querySelectorAll("path[data-series]")).toHaveLength(2);
    const legend = screen.getByRole("list", { name: es.monitor.metrics.diskIo });
    expect(within(legend).getByText(es.monitor.advanced.series.read)).toBeInTheDocument();
    expect(within(legend).getByText(es.monitor.advanced.series.write)).toBeInTheDocument();
  });

  it("un hueco corta la linea y un valor ausente no se une con el siguiente", async () => {
    await openAdvanced(rich, [sample(1), sample(2), sample(3, { gapBefore: true }), sample(4)]);
    expect(chartPath(es.monitor.metrics.cpu).match(/M/g)).toHaveLength(2);
  });

  it("el cursor con el teclado lee hora y valor y la tabla muestra las muestras", async () => {
    await openAdvanced(rich, [sample(1, { cpuPct: 11 }), sample(2, { cpuPct: 22 })]);
    const chart = screen.getByRole("group", { name: es.monitor.metrics.cpu });
    fireEvent.keyDown(chart, { key: "ArrowLeft" });
    expect(within(chart).getByText(`${es.monitor.metrics.cpu}: 22 %`)).toBeInTheDocument();
    fireEvent.keyDown(chart, { key: "ArrowLeft" });
    expect(within(chart).getByText(`${es.monitor.metrics.cpu}: 11 %`)).toBeInTheDocument();
  });

  it("el panel de detalles da intervalo, muestras, huecos, costo y los codigos crudos", async () => {
    await openAdvanced(
      summary({
        gapsInWindow: 2,
        avgReadMicros: 1800,
        availability: { cpu: "warming_up", memory: "ok", volume: "ok", diskActive: "not_supported", diskIo: "ok" },
      }),
    );
    const details = screen.getByRole("region", { name: es.monitor.details.title });
    expect(within(details).getByText(format(es.monitor.details.intervalValue, { seconds: "1" }))).toBeInTheDocument();
    expect(within(details).getByText("3")).toBeInTheDocument();
    expect(within(details).getByText("2")).toBeInTheDocument();
    expect(within(details).getByText(format(es.monitor.details.readCostValue, { ms: "1,8" }))).toBeInTheDocument();
    expect(within(details).getByText("warming_up")).toBeInTheDocument();
    expect(within(details).getByText("not_supported")).toBeInTheDocument();
  });

  it("sin costo medio dice 'Sin dato' y no 0", async () => {
    await openAdvanced(summary({ avgReadMicros: null }));
    const details = screen.getByRole("region", { name: es.monitor.details.title });
    expect(within(details).getByText(es.monitor.details.none)).toBeInTheDocument();
  });

  it("ningun atributo style en la pantalla (CSP estricta)", async () => {
    await openAdvanced(rich);
    expect(screen.getByRole("main").querySelectorAll("[style]")).toHaveLength(0);
  });
});
