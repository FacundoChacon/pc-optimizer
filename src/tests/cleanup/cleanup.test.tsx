import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import type { JunkFinding, JunkScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { summary } from "../support/junkFixtures";
import { type BackendState, installBackend, type MockBackend, OTHER_USER } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const advanced = { uiMode: "advanced", autostartTray: false, language: "es" } as const;

async function openCleanup(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend(overrides);
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.cleanup }));
  await screen.findByRole("heading", { level: 1, name: es.cleanup.title });
  return backend;
}

const card = (name: string) => screen.getByRole("region", { name });
const names = es.cleanup.categoryNames;

const RESULT: JunkScanSummary = summary({
  temp_user: { scope: "per_user", itemCount: 3, sizeBytes: 5 * 1024 * 1024 },
  temp_windows: { itemCount: 2, sizeBytes: 2048 },
  recycle_bin: { scope: "per_user", itemCount: 12, sizeBytes: 340_000_000 },
  old_logs: { status: "unavailable" },
});

function findings(count: number): JunkFinding[] {
  return Array.from({ length: count }, (_, index) => ({
    id: index + 1,
    path: `C:\\Users\\ana\\AppData\\Local\\Temp\\f${String(index + 1).padStart(3, "0")}.tmp`,
    sizeBytes: 10_000 - index,
    modifiedAt: 1_800_000_000_000,
    skipReason: null,
  }));
}

describe("CleanupScreen: estados y resultados", () => {
  it("sin escaneos muestra el estado vacio con Buscar basura como unica accion primaria", async () => {
    await openCleanup();
    expect(screen.getByText(es.cleanup.emptyTitle)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeEnabled();
    expect(document.body.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);
    expect(screen.queryByRole("button", { name: es.cleanup.cancel })).toBeNull();
  });

  it("con un ultimo escaneo muestra las tarjetas sin volver a escanear", async () => {
    const backend = await openCleanup({ lastScan: RESULT });
    await screen.findByRole("region", { name: names.temp_user });
    expect(backend.count("start_scan")).toBe(0);
    expect(screen.queryByText(es.cleanup.emptyTitle)).toBeNull();
    for (const label of Object.values(names)) expect(card(label)).toBeInTheDocument();
  });

  it("muestra el tamano legible por tarjeta y el total liberable sin sumar la Papelera", async () => {
    await openCleanup({ lastScan: RESULT });
    await screen.findByRole("region", { name: names.temp_user });
    expect(within(card(names.temp_user)).getByText(formatBytes(5 * 1024 * 1024))).toBeInTheDocument();
    expect(within(card(names.temp_windows)).getByText(formatBytes(2048))).toBeInTheDocument();
    expect(within(card(names.recycle_bin)).getByText(formatBytes(340_000_000))).toBeInTheDocument();
    const total = screen.getByRole("region", { name: es.cleanup.resultsTitle });
    expect(within(total).getByText(formatBytes(RESULT.totalBytes))).toBeInTheDocument();
    expect(RESULT.totalBytes).toBe(5 * 1024 * 1024 + 2048);
    expect(screen.getByText(es.cleanup.inUseNotice)).toBeInTheDocument();
  });

  it("no ofrece ninguna accion de limpiar, borrar ni vaciar", async () => {
    await openCleanup({ lastScan: RESULT });
    await screen.findByRole("region", { name: names.temp_user });
    expect(screen.queryByRole("button", { name: /limpiar|borrar|eliminar|vaciar/i })).toBeNull();
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(
      expect.not.arrayContaining([expect.stringMatching(/limpiar|borrar|eliminar|vaciar/i)]),
    );
  });

  it("blocked por otra cuenta: texto del catalogo, insignia de advertencia y sin tamano", async () => {
    const blocked = summary({
      temp_user: { scope: "per_user", status: "blocked", blockReason: "different_user" },
      temp_windows: { itemCount: 1, sizeBytes: 4096 },
    });
    await openCleanup({ lastScan: blocked });
    await screen.findByRole("region", { name: names.temp_user });
    const blockedCard = card(names.temp_user);
    expect(within(blockedCard).getByText(es.cleanup.blockReasons.different_user)).toBeInTheDocument();
    expect(blockedCard.querySelector('[data-tone="warning"]')).not.toBeNull();
    expect(within(blockedCard).queryByText(/\b(B|KB|MB|GB)\b/)).toBeNull();
  });

  it("blocked por cuenta no verificada usa su propio texto", async () => {
    const blocked = summary({
      thumbnail_cache: { scope: "per_user", status: "blocked", blockReason: "undetermined" },
    });
    await openCleanup({ lastScan: blocked });
    await screen.findByRole("region", { name: names.thumbnail_cache });
    expect(
      within(card(names.thumbnail_cache)).getByText(es.cleanup.blockReasons.undetermined),
    ).toBeInTheDocument();
  });

  it("partial muestra el tamano de lo escaneado mas la nota", async () => {
    const partial = summary({
      crash_dumps: {
        scope: "mixed",
        status: "partial",
        blockReason: "different_user",
        itemCount: 4,
        sizeBytes: 1536,
      },
    });
    await openCleanup({ lastScan: partial });
    await screen.findByRole("region", { name: names.crash_dumps });
    const dumps = card(names.crash_dumps);
    expect(within(dumps).getByText(formatBytes(1536))).toBeInTheDocument();
    expect(within(dumps).getByText(new RegExp(es.cleanup.partialNote))).toBeInTheDocument();
    expect(dumps.querySelector('[data-tone="warning"]')).not.toBeNull();
  });

  it("old_logs unavailable dice que no esta disponible en esta version y nunca muestra 0 B", async () => {
    await openCleanup({ lastScan: RESULT });
    await screen.findByRole("region", { name: names.old_logs });
    const logs = card(names.old_logs);
    expect(within(logs).getByText(es.cleanup.status.notInThisVersion)).toBeInTheDocument();
    expect(logs.textContent).not.toContain("0 B");
  });

  it("el aviso global de privilegios convive con la pantalla", async () => {
    await openCleanup({ privilege: OTHER_USER, lastScan: RESULT });
    expect(await screen.findByText(es.privilege.title)).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: es.cleanup.title })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeEnabled();
  });

  it("un error de lectura del ultimo escaneo se muestra con el texto del catalogo", async () => {
    await openCleanup({ failures: { get_last_junk_scan: { kind: "store_unavailable", message: "x" } } });
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.store_unavailable);
  });
});

describe("CleanupScreen: escaneo en curso, cancelacion y errores", () => {
  function gated(): { gate: Promise<void>; release: () => void } {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    return { gate, release };
  }

  it("anuncia el progreso en una region aria-live, deshabilita Buscar y deja Cancelar alcanzable", async () => {
    const { gate, release } = gated();
    await openCleanup({
      scanResult: RESULT,
      gates: { start_scan: gate },
      scanEvents: [
        { type: "started", scanId: 1 },
        { type: "category", category: "temp_windows" },
        { type: "progress", category: "temp_windows", filesSeen: 256, eligibleBytes: 10 },
      ],
    });
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.scan }));
    const progress = await screen.findByRole("region", { name: es.cleanup.progressTitle });
    expect(progress).toHaveAttribute("aria-live", "polite");
    await waitFor(() => {
      expect(progress).toHaveTextContent(format(es.cleanup.progressCategory, { category: names.temp_windows }));
    });
    expect(progress).toHaveTextContent(format(es.cleanup.progressFiles, { count: "256" }));
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeDisabled();
    const cancel = screen.getByRole("button", { name: es.cleanup.cancel });
    expect(cancel).toBeEnabled();
    expect(cancel.tabIndex).toBeGreaterThanOrEqual(0);
    await act(async () => {
      release();
      await gate;
    });
    await screen.findByRole("region", { name: names.temp_user });
    expect(screen.queryByRole("region", { name: es.cleanup.progressTitle })).toBeNull();
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeEnabled();
  });

  it("al terminar anuncia el total liberable", async () => {
    await openCleanup({ scanResult: RESULT });
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.scan }));
    await screen.findByRole("region", { name: names.temp_user });
    expect(screen.getByRole("status")).toHaveTextContent(
      format(es.cleanup.announceDone, { size: formatBytes(RESULT.totalBytes) }),
    );
  });

  it("cancelar pide cancel_scan con el id, anuncia el cancelado y conserva el resumen anterior", async () => {
    const { gate, release } = gated();
    const previous = summary({ temp_windows: { itemCount: 1, sizeBytes: 9999 } }, { scanId: 3 });
    const backend = await openCleanup({
      lastScan: previous,
      scanResult: summary({}, { scanId: 7, status: "cancelled", categories: [] }),
      gates: { start_scan: gate },
      scanEvents: [{ type: "started", scanId: 7 }],
    });
    await screen.findByRole("region", { name: names.temp_windows });
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.scan }));
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.cancel }));
    await waitFor(() => expect(backend.count("cancel_scan")).toBe(1));
    expect(backend.calls.find((call) => call.cmd === "cancel_scan")?.args).toEqual({ scanId: 7 });
    await act(async () => {
      release();
      await gate;
    });
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent(es.cleanup.announceCancelled);
    });
    expect(within(card(names.temp_windows)).getByText(formatBytes(9999))).toBeInTheDocument();
  });

  it("cancelar antes de conocer el id espera al evento started para pedir la cancelacion", async () => {
    const { gate, release } = gated();
    const backend = await openCleanup({
      scanResult: summary({}, { scanId: 9, status: "cancelled", categories: [] }),
      gates: { start_scan: gate },
    });
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.scan }));
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.cancel }));
    expect(backend.count("cancel_scan")).toBe(0);
    act(() => backend.emit([{ type: "started", scanId: 9 }]));
    await waitFor(() => expect(backend.count("cancel_scan")).toBe(1));
    expect(backend.calls.find((call) => call.cmd === "cancel_scan")?.args).toEqual({ scanId: 9 });
    await act(async () => {
      release();
      await gate;
    });
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent(es.cleanup.cancelledNoPrevious);
    });
  });

  it("el escaneo sigue visible al ir a otra pantalla y volver, y no permite lanzar otro", async () => {
    const { gate, release } = gated();
    const backend = await openCleanup({
      scanResult: RESULT,
      gates: { start_scan: gate },
      scanEvents: [{ type: "started", scanId: 1 }],
    });
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.scan }));
    await screen.findByRole("region", { name: es.cleanup.progressTitle });
    fireEvent.click(screen.getByRole("button", { name: es.nav.home }));
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    fireEvent.click(screen.getByRole("button", { name: es.nav.cleanup }));
    expect(await screen.findByRole("region", { name: es.cleanup.progressTitle })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeDisabled();
    expect(backend.count("start_scan")).toBe(1);
    await act(async () => {
      release();
      await gate;
    });
    expect(await screen.findByRole("region", { name: names.temp_user })).toBeInTheDocument();
  });

  it("al terminar, el foco vuelve a Buscar basura en vez de perderse con el boton Cancelar", async () => {
    const { gate, release } = gated();
    await openCleanup({ scanResult: RESULT, gates: { start_scan: gate } });
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.scan }));
    const cancel = await screen.findByRole("button", { name: es.cleanup.cancel });
    cancel.focus();
    await act(async () => {
      release();
      await gate;
    });
    await screen.findByRole("region", { name: names.temp_user });
    await waitFor(() => expect(screen.getByRole("button", { name: es.cleanup.scan })).toHaveFocus());
  });

  it("un escaneo cancelado deja un aviso visible en la pantalla y el siguiente escaneo lo limpia", async () => {
    const { gate, release } = gated();
    await openCleanup({
      lastScan: RESULT,
      scanResult: summary({}, { scanId: 7, status: "cancelled", categories: [] }),
      gates: { start_scan: gate },
    });
    await screen.findByRole("region", { name: names.temp_user });
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.scan }));
    await screen.findByRole("region", { name: es.cleanup.progressTitle });
    await act(async () => {
      release();
      await gate;
    });
    const main = screen.getByRole("main");
    expect(await within(main).findByText(es.cleanup.announceCancelled)).toBeInTheDocument();
    expect(within(card(names.temp_user)).getByText(formatBytes(5 * 1024 * 1024))).toBeInTheDocument();
  });

  it.each([
    "scan_in_progress",
    "scan_not_found",
    "scan_kind_unsupported",
    "invalid_argument",
    "store_unavailable",
  ] as const)("el error %s se muestra con su texto del catalogo", async (kind) => {
    await openCleanup({ failures: { start_scan: { kind, message: "technical" } } });
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.scan }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(es.errors[kind]);
    expect(alert).not.toHaveTextContent("technical");
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeEnabled();
  });
});

describe("CleanupScreen: rutas segun el modo", () => {
  const withFindings = { lastScan: RESULT, findings: { temp_user: findings(250) } };

  it("modo simple: no hay rutas ni boton Ver rutas", async () => {
    await openCleanup(withFindings);
    await screen.findByRole("region", { name: names.temp_user });
    expect(screen.queryByRole("button", { name: es.cleanup.viewPaths })).toBeNull();
    expect(document.body.textContent).not.toContain("AppData");
  });

  it("modo avanzado: Ver rutas carga 100 ordenadas y Cargar mas pide offset 100", async () => {
    const backend = await openCleanup({ ...withFindings, settings: advanced });
    await screen.findByRole("region", { name: names.temp_user });
    const tempUser = card(names.temp_user);
    fireEvent.click(within(tempUser).getByRole("button", { name: es.cleanup.viewPaths }));
    const list = await within(tempUser).findByRole("list", {
      name: format(es.cleanup.pathsListLabel, { category: names.temp_user }),
    });
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(100));
    const items = within(list).getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("f001.tmp");
    expect(items[99]).toHaveTextContent("f100.tmp");
    expect(backend.calls.filter((c) => c.cmd === "list_junk_findings").map((c) => c.args)).toEqual([
      { scanId: 1, category: "temp_user", offset: 0, limit: 100 },
    ]);
    expect(within(tempUser).getByText(format(es.cleanup.pathsShown, { shown: "100", total: "250" }))).toBeInTheDocument();

    fireEvent.click(within(tempUser).getByRole("button", { name: es.cleanup.loadMore }));
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(200));
    expect(backend.calls.filter((c) => c.cmd === "list_junk_findings").at(-1)?.args).toEqual({
      scanId: 1,
      category: "temp_user",
      offset: 100,
      limit: 100,
    });
    fireEvent.click(within(tempUser).getByRole("button", { name: es.cleanup.loadMore }));
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(250));
    expect(within(tempUser).queryByRole("button", { name: es.cleanup.loadMore })).toBeNull();
  });

  it("modo avanzado: un omitido por enlace muestra su texto llano y el codigo", async () => {
    await openCleanup({
      lastScan: RESULT,
      settings: advanced,
      findings: {
        temp_user: [
          { id: 1, path: "C:\\x\\a.tmp", sizeBytes: 5, modifiedAt: null, skipReason: null },
          { id: 2, path: "C:\\x\\link", sizeBytes: 0, modifiedAt: null, skipReason: "reparse_point" },
        ],
      },
    });
    await screen.findByRole("region", { name: names.temp_user });
    const tempUser = card(names.temp_user);
    fireEvent.click(within(tempUser).getByRole("button", { name: es.cleanup.viewPaths }));
    const items = await within(tempUser).findAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent(es.cleanup.eligible);
    expect(items[1]).toHaveTextContent(es.cleanup.skipReasons.link);
    expect(items[1]).toHaveTextContent("reparse_point");
    fireEvent.click(within(tempUser).getByRole("button", { name: es.cleanup.hidePaths }));
    expect(within(tempUser).queryByRole("list")).toBeNull();
  });

  it("modo avanzado: sin Ver rutas en categorias bloqueadas, no disponibles o la Papelera", async () => {
    const mixed = summary({
      temp_user: { scope: "per_user", status: "blocked", blockReason: "different_user" },
      old_logs: { status: "unavailable" },
      recycle_bin: { scope: "per_user", itemCount: 3, sizeBytes: 100 },
      temp_windows: { itemCount: 1, sizeBytes: 10 },
    });
    await openCleanup({ lastScan: mixed, settings: advanced });
    await screen.findByRole("region", { name: names.temp_windows });
    expect(within(card(names.temp_windows)).getByRole("button", { name: es.cleanup.viewPaths })).toBeInTheDocument();
    for (const name of [names.temp_user, names.old_logs, names.recycle_bin]) {
      expect(within(card(name)).queryByRole("button", { name: es.cleanup.viewPaths })).toBeNull();
    }
  });

  it("un error al listar rutas se muestra sin romper la tarjeta", async () => {
    await openCleanup({
      ...withFindings,
      settings: advanced,
      failures: { list_junk_findings: { kind: "scan_not_found", message: "x" } },
    });
    await screen.findByRole("region", { name: names.temp_user });
    const tempUser = card(names.temp_user);
    fireEvent.click(within(tempUser).getByRole("button", { name: es.cleanup.viewPaths }));
    expect(await within(tempUser).findByRole("alert")).toHaveTextContent(es.errors.scan_not_found);
    expect(within(tempUser).getByRole("button", { name: es.cleanup.hidePaths })).toBeInTheDocument();
  });
});
