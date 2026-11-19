import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import type { JunkScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { diskSummary } from "../support/diskFixtures";
import { summary as junkSummary } from "../support/junkFixtures";
import { type BackendState, installBackend, type MockBackend } from "../support/mockBackend";
import { metric, sample, sampleEvent, snapshot, summary } from "../support/monitorFixtures";

afterEach(() => {
  cleanup();
  clearMocks();
});

const GB = 1024 * 1024 * 1024;
const cards = es.home.cards;
const names = es.cleanup.categoryNames;

/** Escaneo con 1,2 GB limpiables que termino hace 10 minutos (el reloj es el real: se usa `Date.now()`). */
function recentScan(overrides: Partial<JunkScanSummary> = {}, bin = { itemCount: 0, sizeBytes: 0 }): JunkScanSummary {
  const finishedAt = Date.now() - 10 * 60_000;
  return junkSummary(
    {
      temp_user: { scope: "per_user", itemCount: 3, sizeBytes: Math.round(1.2 * GB) },
      recycle_bin: { scope: "per_user", ...bin },
    },
    { startedAt: finishedAt - 5000, finishedAt, ...overrides },
  );
}

const MONITOR: Partial<BackendState> = {
  monitorSnapshot: snapshot(
    [sample(1, { volumeFreeBytes: 120 * GB, volumeTotalBytes: 476 * GB, memUsedBytes: Math.round(9.8 * GB), memTotalBytes: 16 * GB })],
    { summary: summary({ memory: metric(62) }) },
  ),
};

async function openHome(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend({ ...MONITOR, lastScan: recentScan(), ...overrides });
  render(<App />);
  await screen.findByRole("heading", { level: 1, name: es.home.title });
  return backend;
}

const card = (name: string) => screen.findByRole("region", { name });
const cleanNowButton = () => screen.getByRole("button", { name: es.home.cleanNow });

describe("Inicio en modo simple", () => {
  it("tres tarjetas en orden con las cifras y una sola accion primaria con su ayuda accesible", async () => {
    await openHome();
    const space = await card(cards.space.title);
    expect(
      await within(space).findByText(format(cards.space.value, { free: formatBytes(120 * GB), total: formatBytes(476 * GB) })),
    ).toBeInTheDocument();
    expect(within(space).getByText(es.monitor.spaceLevels.normal)).toBeInTheDocument();
    const junk = await card(cards.junk.title);
    expect(within(junk).getByText(format(cards.junk.value, { size: formatBytes(Math.round(1.2 * GB)) }))).toBeInTheDocument();
    const memory = await card(cards.memory.title);
    expect(
      await within(memory).findByText(format(cards.memory.value, { pct: "62 %", used: formatBytes(Math.round(9.8 * GB)), total: formatBytes(16 * GB) })),
    ).toBeInTheDocument();
    expect(within(memory).getByText(es.monitor.levels.normal)).toBeInTheDocument();

    const titles = screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent);
    expect(titles).toEqual([cards.space.title, cards.junk.title, cards.memory.title]);

    const primaries = document.body.querySelectorAll('[data-variant="primary"]');
    expect(primaries).toHaveLength(1);
    expect(primaries[0]).toBe(cleanNowButton());
    const help = screen.getByText(es.home.cleanNowHelp);
    expect(cleanNowButton().getAttribute("aria-describedby")?.split(" ")).toContain(help.id);
  });

  it("no muestra rutas ni codigos", async () => {
    await openHome({ diskLast: diskSummary(), lastScan: recentScan() });
    await card(cards.junk.title);
    const text = screen.getByRole("main").textContent ?? "";
    expect(text).not.toMatch(/[A-Za-z]:\\/);
    expect(text).not.toMatch(/\b[a-z]+_[a-z_]+\b/);
    expect(text).not.toContain(es.home.codeLabel.replace("{code}", ""));
  });

  it.each([
    [cards.space.title, es.nav.disk, es.disk.title],
    [cards.junk.title, es.nav.cleanup, es.cleanup.title],
    [cards.memory.title, es.nav.monitor, es.monitor.title],
  ])("Ver detalle de %s lleva a %s", async (cardName, _label, heading) => {
    await openHome();
    const region = await card(cardName);
    fireEvent.click(within(region).getByRole("button", { name: new RegExp(es.home.viewDetail) }));
    expect(await screen.findByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
  });

  it("monitor caido: Espacio y Memoria dicen No disponible con su explicacion y ninguno muestra 0", async () => {
    await openHome({ failures: { start_monitor_stream: { kind: "monitor_unavailable", message: "x" } } });
    const space = await card(cards.space.title);
    const memory = await card(cards.memory.title);
    await waitFor(() => expect(within(memory).getByText(es.home.unavailable)).toBeInTheDocument());
    expect(within(space).getByText(es.home.unavailable)).toBeInTheDocument();
    expect(within(memory).getByText(es.errors.monitor_unavailable)).toBeInTheDocument();
    expect(within(space).queryByText(/\b0\b/)).toBeNull();
    expect(within(memory).queryByText(/\b0\b/)).toBeNull();
  });

  it("monitor caido con un analisis de disco: Espacio usa el respaldo con su fecha", async () => {
    const disk = diskSummary({ volume: { totalBytes: 476 * GB, freeBytes: 100 * GB } });
    await openHome({ diskLast: disk, failures: { start_monitor_stream: { kind: "monitor_unavailable", message: "x" } } });
    const space = await card(cards.space.title);
    const date = formatDateTime(disk.finishedAt ?? disk.startedAt);
    expect(
      await within(space).findByText(
        format(cards.space.fallback, { date, free: formatBytes(100 * GB), total: formatBytes(476 * GB) }),
      ),
    ).toBeInTheDocument();
  });

  it("memoria en calentamiento: Midiendo y nunca 0", async () => {
    const warming = summary({
      memory: metric(null, "unknown"),
      availability: { ...summary().availability, memory: "warming_up" },
    });
    await openHome({ monitorSnapshot: snapshot([sample(1, { memUsedBytes: null, memTotalBytes: null })], { summary: warming }) });
    const memory = await card(cards.memory.title);
    expect(within(memory).getByText(es.home.measuring)).toBeInTheDocument();
    expect(within(memory).queryByText(/\b0\b/)).toBeNull();
  });

  it("sin escaneo: Todavia no buscaste basura", async () => {
    await openHome({ lastScan: null });
    const junk = await card(cards.junk.title);
    expect(await within(junk).findByText(cards.junk.none)).toBeInTheDocument();
    expect(within(junk).queryByText(es.home.unavailable)).toBeNull();
  });

  it("escaneo ya limpiado: lo dice con la fecha", async () => {
    const cleanedAt = Date.now() - 60_000;
    await openHome({ lastScan: recentScan({ cleanedAt }) });
    const junk = await card(cards.junk.title);
    expect(await within(junk).findByText(format(cards.junk.cleaned, { date: formatDateTime(cleanedAt) }))).toBeInTheDocument();
  });

  it("la Papelera con 2 GB se menciona sin boton de vaciar", async () => {
    await openHome({ lastScan: recentScan({}, { itemCount: 9, sizeBytes: 2 * GB }) });
    const junk = await card(cards.junk.title);
    expect(await within(junk).findByText(format(cards.junk.bin, { size: formatBytes(2 * GB) }))).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /vaciar/i })).toBeNull();
    expect(within(junk).getAllByRole("button")).toHaveLength(1);
  });

  it("base no disponible al leer el escaneo: No disponible con el mensaje del catalogo y sin codigo", async () => {
    await openHome({ lastScan: null, failures: { get_last_junk_scan: { kind: "store_unavailable", message: "x" } } });
    const junk = await card(cards.junk.title);
    expect(await within(junk).findByText(es.errors.store_unavailable)).toBeInTheDocument();
    expect(within(junk).getByText(es.home.unavailable)).toBeInTheDocument();
    expect(within(junk).queryByText(/store_unavailable/)).toBeNull();
  });

  it("el monitor entrega una muestra nueva y la tarjeta Memoria se actualiza", async () => {
    const backend = await openHome();
    const memory = await card(cards.memory.title);
    await within(memory).findByText("62 %", { exact: false });
    act(() => backend.emitMonitor([sampleEvent(2, summary({ memory: metric(80, "high") }), { memUsedBytes: Math.round(12.8 * GB), memTotalBytes: 16 * GB })]));
    expect(await within(memory).findByText(es.monitor.levels.high)).toBeInTheDocument();
  });
});

describe("Limpiar ahora en Inicio", () => {
  it("con un escaneo reciente lleva a Limpieza con el preview abierto y sin mover archivos", async () => {
    const backend = await openHome();
    fireEvent.click(cleanNowButton());
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(screen.getByRole("heading", { level: 1, name: es.cleanup.title })).toBeInTheDocument();
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_user) })).toBeChecked();
    expect(within(dialog).getByRole("button", { name: es.cleanup.cancel })).toHaveFocus();
    expect(backend.count("start_scan")).toBe(0);
    expect(backend.count("confirm_cleanup")).toBe(0);
  });

  it("sin escaneo busca basura y abre el preview al terminar", async () => {
    const backend = await openHome({ lastScan: null, scanResult: recentScan() });
    fireEvent.click(await screen.findByRole("button", { name: es.home.cleanNow }));
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(within(dialog).getByRole("button", { name: es.cleanup.cancel })).toHaveFocus();
    expect(backend.count("start_scan")).toBe(1);
    expect(backend.count("confirm_cleanup")).toBe(0);
  });
});
