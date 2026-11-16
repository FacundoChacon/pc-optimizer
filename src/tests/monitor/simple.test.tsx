import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import type { MonitorSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { type BackendState, installBackend, type MockBackend } from "../support/mockBackend";
import { metric, sample, sampleEvent, snapshot, summary } from "../support/monitorFixtures";

afterEach(() => {
  cleanup();
  clearMocks();
});

async function openMonitor(
  sum: MonitorSummary | null = summary(),
  overrides: Partial<BackendState> = {},
  samples = [sample(1)],
): Promise<MockBackend> {
  const backend = installBackend({
    monitorSnapshot: snapshot(samples, { summary: sum }),
    ...overrides,
  });
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.monitor }));
  await screen.findByRole("heading", { level: 1, name: es.monitor.title });
  return backend;
}

function tile(name: string): HTMLElement {
  return screen.getByRole("region", { name });
}

describe("Monitor en modo simple", () => {
  it("tres tarjetas con valor y nivel en palabras y sin boton primario", async () => {
    await openMonitor();
    const cpu = await screen.findByRole("region", { name: es.monitor.metrics.cpu });
    expect(within(cpu).getByText("23 %")).toBeInTheDocument();
    expect(within(cpu).getByText(es.monitor.levels.normal)).toBeInTheDocument();
    const memory = tile(es.monitor.metrics.memory);
    expect(within(memory).getByText("62 %")).toBeInTheDocument();
    expect(
      within(memory).getByText(format(es.monitor.tiles.memoryDetail, { used: formatBytes(10_000_000_000), total: formatBytes(16_000_000_000) })),
    ).toBeInTheDocument();
    const disk = tile(es.monitor.metrics.disk);
    expect(within(disk).getByText(format(es.monitor.tiles.diskActive, { pct: "12 %" }))).toBeInTheDocument();
    expect(
      within(disk).getByText(format(es.monitor.tiles.spaceDetail, { free: formatBytes(120_000_000_000), total: formatBytes(500_000_000_000) })),
    ).toBeInTheDocument();
    expect(within(screen.getByRole("main")).queryAllByRole("button")).toHaveLength(0);
  });

  it("CPU en calentamiento muestra 'Midiendo…' y nunca 0 %", async () => {
    await openMonitor(
      summary({ cpu: metric(null, "unknown"), availability: { ...summary().availability, cpu: "warming_up" } }),
      {},
      [sample(1, { cpuPct: null })],
    );
    const cpu = await screen.findByRole("region", { name: es.monitor.metrics.cpu });
    expect(within(cpu).getByText(es.monitor.measuring)).toBeInTheDocument();
    expect(within(cpu).queryByText(/^0 %$/)).toBeNull();
  });

  it("disco activo no soportado: 'No disponible' con su explicacion y sin 0 %", async () => {
    await openMonitor(
      summary({ diskActive: metric(null, "unknown"), availability: { ...summary().availability, diskActive: "not_supported" } }),
      {},
      [sample(1, { diskActivePct: null })],
    );
    const disk = await screen.findByRole("region", { name: es.monitor.metrics.disk });
    expect(within(disk).getByText(es.monitor.unavailable)).toBeInTheDocument();
    expect(within(disk).getByText(es.monitor.availabilityNotes.not_supported)).toBeInTheDocument();
    expect(within(disk).queryByText(/0 %/)).toBeNull();
  });

  it("espacio casi lleno se dice en palabras", async () => {
    await openMonitor(summary({ volume: { usedPct: 92, level: "almost_full" } }));
    const disk = await screen.findByRole("region", { name: es.monitor.metrics.disk });
    expect(within(disk).getByText(es.monitor.spaceLevels.almost_full)).toBeInTheDocument();
  });

  it("un nivel alto agrega una frase llana", async () => {
    await openMonitor(summary({ cpu: metric(92, "very_high") }));
    const cpu = await screen.findByRole("region", { name: es.monitor.metrics.cpu });
    expect(within(cpu).getByText(es.monitor.phrases.cpu.very_high)).toBeInTheDocument();
    expect(within(cpu).getByText(es.monitor.levels.very_high)).toBeInTheDocument();
  });

  it("el cambio de nivel se anuncia por la region polite y los valores no llevan aria-live", async () => {
    const backend = await openMonitor();
    await screen.findByRole("region", { name: es.monitor.metrics.cpu });
    const announcer = screen.getByRole("status");
    expect(announcer).toHaveTextContent("");
    act(() => backend.emitMonitor([sampleEvent(2, summary({ cpu: metric(75, "high") }))]));
    expect(announcer).toHaveTextContent(format(es.monitor.announce, { metric: es.monitor.metrics.cpu, level: "alto" }));
    // Mismo nivel otra vez: no hay un anuncio nuevo (el texto no cambia a otra cosa).
    act(() => backend.emitMonitor([sampleEvent(3, summary({ cpu: metric(78, "high") }))]));
    expect(announcer).toHaveTextContent(format(es.monitor.announce, { metric: es.monitor.metrics.cpu, level: "alto" }));
    expect(screen.getByRole("main").querySelectorAll("[aria-live]")).toHaveLength(0);
  });

  it("monitor no disponible muestra su mensaje y ninguna tarjeta", async () => {
    await openMonitor(null, { failures: { start_monitor_stream: { kind: "monitor_unavailable", message: "x" } } });
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.monitor_unavailable);
    expect(screen.queryByRole("region", { name: es.monitor.metrics.cpu })).toBeNull();
  });

  it("sin muestras todavia: espera la primera medicion", async () => {
    await openMonitor(null, {}, []);
    expect(await screen.findByText(es.monitor.waiting)).toBeInTheDocument();
  });
});
