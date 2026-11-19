import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { AppProviders } from "../../app/providers/AppProviders";
import { useSettings } from "../../app/providers/SettingsProvider";
import { Shell } from "../../app/shell/Shell";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { diskSummary } from "../support/diskFixtures";
import { duplicatesRun, emptyBinRun, historyOf, junkRun } from "../support/historyFixtures";
import { summary as junkSummary } from "../support/junkFixtures";
import { type BackendState, installBackend, type MockBackend } from "../support/mockBackend";
import { sample, sampleEvent, snapshot, summary } from "../support/monitorFixtures";

afterEach(() => {
  cleanup();
  clearMocks();
});

const ADVANCED = { uiMode: "advanced", autostartTray: false, language: "es" } as const;
const GB = 1024 * 1024 * 1024;
const adv = es.home.advanced;

const SCAN = junkSummary(
  {
    temp_user: { scope: "per_user", itemCount: 3, sizeBytes: 5 * 1024 * 1024, skippedCount: 2, inaccessibleCount: 1 },
    browser_cache_edge: { scope: "per_user", status: "partial", blockReason: "different_user", itemCount: 4, sizeBytes: 2048 },
    recycle_bin: { scope: "per_user", itemCount: 9, sizeBytes: 2 * GB },
  },
  { finishedAt: 1_800_000_005_000 },
);

async function openAdvancedHome(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend({
    settings: ADVANCED,
    monitorSnapshot: snapshot([sample(1), sample(2), sample(3)], { summary: summary() }),
    lastScan: SCAN,
    ...overrides,
  });
  render(<App />);
  await screen.findByRole("heading", { level: 1, name: es.home.title });
  return backend;
}

const chart = () => screen.findByRole("group", { name: adv.chartLabel });
const chartPath = async () => (await chart()).querySelector("path[data-series]")?.getAttribute("d") ?? "";

describe("Inicio avanzado: Sistema", () => {
  it("cuatro medidores y un grafico de dos series con leyenda, sin atributo style", async () => {
    await openAdvancedHome();
    const meters = await screen.findByRole("region", { name: es.monitor.advanced.metersTitle });
    expect(meters.querySelectorAll("rect[data-meter='fill']")).toHaveLength(4);
    const plot = await chart();
    expect(plot.querySelectorAll("path[data-series]")).toHaveLength(2);
    const legend = screen.getByRole("list", { name: adv.chartLabel });
    expect(within(legend).getByText(adv.series.cpu)).toBeInTheDocument();
    expect(within(legend).getByText(adv.series.memory)).toBeInTheDocument();
    expect(plot.querySelector("[style]")).toBeNull();
    expect(within(plot).getByText(es.monitor.advanced.chart.axisStart)).toBeInTheDocument();
  });

  it("una muestra nueva se incorpora al grafico", async () => {
    const backend = await openAdvancedHome();
    const before = await chartPath();
    act(() => backend.emitMonitor([sampleEvent(4, summary(), { cpuPct: 80 })]));
    expect(await chartPath()).not.toBe(before);
  });

  it("un hueco corta la linea de CPU", async () => {
    await openAdvancedHome({
      monitorSnapshot: snapshot([sample(1), sample(2), sample(3, { gapBefore: true }), sample(4)], { summary: summary() }),
    });
    expect((await chartPath()).match(/M/g)).toHaveLength(2);
  });

  it("monitor caido: estado no disponible con su codigo y sin grafico", async () => {
    await openAdvancedHome({ failures: { start_monitor_stream: { kind: "monitor_unavailable", message: "x" } } });
    const system = await screen.findByRole("region", { name: adv.systemTitle });
    expect(await within(system).findByText(es.home.unavailable)).toBeInTheDocument();
    expect(within(system).getByText(format(es.home.codeLabel, { code: "monitor_unavailable" }))).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: adv.chartLabel })).toBeNull();
  });

  it("Ver todos los graficos lleva a Monitor", async () => {
    await openAdvancedHome();
    fireEvent.click(await screen.findByRole("button", { name: adv.allCharts }));
    expect(await screen.findByRole("heading", { level: 1, name: es.monitor.title })).toBeInTheDocument();
  });

  it("hay una sola accion primaria: Limpiar ahora", async () => {
    await openAdvancedHome();
    await chart();
    const primaries = document.body.querySelectorAll('[data-variant="primary"]');
    expect(primaries).toHaveLength(1);
    expect(primaries[0]).toHaveTextContent(es.home.cleanNow);
  });
});

describe("Inicio avanzado: Basura y Disco", () => {
  it("la tabla tiene una fila por categoria con estado y codigo", async () => {
    await openAdvancedHome();
    const junk = await screen.findByRole("region", { name: adv.junkTitle });
    const rows = await within(junk).findAllByRole("row");
    expect(rows).toHaveLength(1 + 11);
    const edge = within(junk).getByRole("row", { name: new RegExp(es.cleanup.categoryNames.browser_cache_edge) });
    expect(within(edge).getByText(/\(browser_cache_edge\)/)).toBeInTheDocument();
    expect(within(edge).getByText(/\(partial\)/)).toBeInTheDocument();
    expect(within(edge).getByText(formatBytes(2048))).toBeInTheDocument();
    const user = within(junk).getByRole("row", { name: new RegExp(es.cleanup.categoryNames.temp_user) });
    expect(within(user).getByText(formatBytes(5 * 1024 * 1024))).toBeInTheDocument();
    expect(within(junk).getByText(format(adv.junkScanDate, { date: formatDateTime(1_800_000_005_000) }))).toBeInTheDocument();
  });

  it("escaneo ya limpiado muestra la fecha de limpieza", async () => {
    await openAdvancedHome({ lastScan: { ...SCAN, cleanedAt: 1_800_000_900_000 } });
    const junk = await screen.findByRole("region", { name: adv.junkTitle });
    await within(junk).findAllByRole("row");
    expect(within(junk).getByText(format(adv.junkCleanedAt, { date: formatDateTime(1_800_000_900_000) }))).toBeInTheDocument();
  });

  it("sin escaneo: estado sin analisis y sin tabla", async () => {
    await openAdvancedHome({ lastScan: null });
    const junk = await screen.findByRole("region", { name: adv.junkTitle });
    expect(await within(junk).findByText(es.home.cards.junk.none)).toBeInTheDocument();
    expect(within(junk).queryByRole("table")).toBeNull();
  });

  it("error store_unavailable al leer el escaneo muestra su codigo", async () => {
    await openAdvancedHome({ lastScan: null, failures: { get_last_junk_scan: { kind: "store_unavailable", message: "x" } } });
    const junk = await screen.findByRole("region", { name: adv.junkTitle });
    expect(await within(junk).findByText(format(es.home.codeLabel, { code: "store_unavailable" }))).toBeInTheDocument();
    expect(within(junk).getByText(es.errors.store_unavailable)).toBeInTheDocument();
  });

  it("resumen de disco con la ruta completa de la raiz, fecha y totales", async () => {
    const disk = diskSummary({ unaccountedBytes: GB, inaccessibleDirs: 4, cloudFiles: 7 });
    await openAdvancedHome({ diskLast: disk });
    const block = await screen.findByRole("region", { name: adv.diskTitle });
    expect(await within(block).findByText("C:\\")).toBeInTheDocument();
    expect(within(block).getByText(format(adv.diskDate, { date: formatDateTime(disk.finishedAt ?? disk.startedAt) }))).toBeInTheDocument();
    expect(within(block).getByText(formatBytes(disk.totalBytes))).toBeInTheDocument();
    expect(within(block).getByText(formatBytes(GB))).toBeInTheDocument();
    expect(within(block).getByText("4")).toBeInTheDocument();
    expect(within(block).getByText("7")).toBeInTheDocument();
  });

  it("raiz elegida (picked) muestra su ruta", async () => {
    const disk = diskSummary({ root: { kind: "picked", path: "D:\\Proyectos", folderId: 5 } });
    await openAdvancedHome({ diskLast: disk });
    const block = await screen.findByRole("region", { name: adv.diskTitle });
    expect(await within(block).findByText("D:\\Proyectos")).toBeInTheDocument();
  });

  it("sin analisis de disco: texto y enlace a Espacio en disco", async () => {
    await openAdvancedHome({ diskLast: null });
    const block = await screen.findByRole("region", { name: adv.diskTitle });
    expect(await within(block).findByText(adv.diskNone)).toBeInTheDocument();
    fireEvent.click(within(block).getByRole("button", { name: adv.diskLink }));
    expect(await screen.findByRole("heading", { level: 1, name: es.disk.title })).toBeInTheDocument();
  });
});

describe("Inicio avanzado: ultimas limpiezas", () => {
  it("tres limpiezas en una linea cada una y enlace a Historial", async () => {
    const history = [junkRun(1), duplicatesRun(2), emptyBinRun(1), junkRun(3)].map((run) => historyOf(run));
    const backend = await openAdvancedHome({ history });
    const block = await screen.findByRole("region", { name: adv.recentTitle });
    await waitFor(() => expect(within(block).getAllByRole("listitem")).toHaveLength(3));
    expect(backend.calls.find((call) => call.cmd === "list_cleanup_runs")?.args).toEqual({ before: null, limit: 3 });
    fireEvent.click(within(block).getByRole("button", { name: adv.recentLink }));
    expect(await screen.findByRole("heading", { level: 1, name: es.history.title })).toBeInTheDocument();
  });

  it("cada linea trae la frase de la limpieza y su fecha", async () => {
    await openAdvancedHome({ history: [historyOf(junkRun(1))] });
    const block = await screen.findByRole("region", { name: adv.recentTitle });
    const item = await within(block).findByRole("listitem");
    expect(item).toHaveTextContent(formatDateTime(junkRun(1).finishedAt));
    expect(item).toHaveTextContent("340");
  });

  it("historial vacio: dice que todavia no se limpio nada", async () => {
    await openAdvancedHome({ history: [] });
    const block = await screen.findByRole("region", { name: adv.recentTitle });
    expect(await within(block).findByText(adv.recentEmpty)).toBeInTheDocument();
  });

  it("error de lectura: mensaje por kind", async () => {
    await openAdvancedHome({ failures: { list_cleanup_runs: { kind: "store_unavailable", message: "x" } } });
    const block = await screen.findByRole("region", { name: adv.recentTitle });
    expect(await within(block).findByText(es.errors.store_unavailable)).toBeInTheDocument();
  });
});

describe("Inicio: alternancia de modo", () => {
  function Toggle() {
    const { setUiMode } = useSettings();
    return (
      <>
        <button type="button" onClick={() => void setUiMode("advanced")}>
          a-avanzado
        </button>
        <button type="button" onClick={() => void setUiMode("simple")}>
          a-simple
        </button>
      </>
    );
  }

  it("con Inicio en pantalla, cambiar el modo persiste y muestra los bloques del otro modo con la misma accion primaria", async () => {
    const backend = installBackend({
      monitorSnapshot: snapshot([sample(1)], { summary: summary() }),
      lastScan: SCAN,
    });
    render(
      <AppProviders>
        <Toggle />
        <Shell />
      </AppProviders>,
    );
    await screen.findByRole("region", { name: es.home.cards.space.title });
    expect(screen.queryByRole("region", { name: adv.systemTitle })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "a-avanzado" }));
    expect(await screen.findByRole("region", { name: adv.systemTitle })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: es.home.cards.space.title })).toBeNull();
    expect(screen.getAllByRole("button", { name: es.home.cleanNow })).toHaveLength(1);
    expect(backend.calls.filter((call) => call.cmd === "set_setting")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "a-simple" }));
    expect(await screen.findByRole("region", { name: es.home.cards.space.title })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: adv.systemTitle })).toBeNull();
    expect(screen.getAllByRole("button", { name: es.home.cleanNow })).toHaveLength(1);
  });

  it("desde Configuracion y volviendo a Inicio muestra los bloques avanzados", async () => {
    installBackend({ monitorSnapshot: snapshot([sample(1)], { summary: summary() }), lastScan: SCAN });
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.settings }));
    fireEvent.click(await screen.findByRole("radio", { name: new RegExp(es.settings.appearance.advanced) }));
    await waitFor(() => expect(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.advanced) })).toBeChecked());
    fireEvent.click(within(screen.getByRole("navigation", { name: es.nav.label })).getByRole("button", { name: es.nav.home }));
    expect(await screen.findByRole("region", { name: adv.systemTitle })).toBeInTheDocument();
  });
});
