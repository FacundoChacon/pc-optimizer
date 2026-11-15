import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import type { FolderScanEvent } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatPlural } from "../../shared/i18n/format";
import { defaultPages, diskSummary, GB, rootChildren, topFiles } from "../support/diskFixtures";
import { type BackendState, installBackend, type MockBackend } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const t = es.disk;

/** Un backend con un analisis previo ya guardado (y sus carpetas y archivos). */
export function withLastScan(overrides: Partial<BackendState> = {}): Partial<BackendState> {
  return { diskLast: diskSummary(), diskPages: defaultPages(), diskFiles: topFiles(), ...overrides };
}

export async function openDisk(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend(overrides);
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
  await screen.findByRole("heading", { level: 1, name: t.title });
  return backend;
}

function gate(): { gate: Promise<void>; release: () => void } {
  let release: () => void = () => undefined;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { gate: promise, release };
}

const scanButton = () => screen.getByRole("button", { name: /Analizar (disco C:|de nuevo)/ });
const started = (scanId: number): FolderScanEvent => ({ type: "started", scanId, rootKind: "system_drive" });

describe("Espacio en disco: sin analisis", () => {
  it("explica que solo se lee y 'Analizar disco C:' es la unica accion primaria", async () => {
    await openDisk();
    expect(await screen.findByText(t.intro)).toBeInTheDocument();
    expect(scanButton()).toHaveTextContent(t.scanDisk);
    expect(scanButton()).toHaveAttribute("data-variant", "primary");
    expect(document.body.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);
    expect(screen.queryByRole("region", { name: t.topFoldersTitle })).toBeNull();
  });

  it("no muestra la invitacion cuando ya hay un analisis guardado", async () => {
    const backend = await openDisk(withLastScan());
    await screen.findByRole("region", { name: t.topFoldersTitle });
    expect(screen.queryByText(t.intro)).toBeNull();
    expect(backend.count("get_last_folder_scan")).toBe(1);
  });
});

describe("Espacio en disco: analizar y cancelar", () => {
  it("analiza el disco del sistema por tipo de raiz y muestra el progreso en una region aria-live", async () => {
    const hold = gate();
    const backend = await openDisk({
      diskScanEvents: [
        started(9),
        { type: "walking", filesSeen: 4096, dirsSeen: 300, bytesSeen: 2 * GB },
      ],
      diskScanResult: diskSummary({ scanId: 9 }),
      diskPages: defaultPages(),
      diskFiles: topFiles(),
      gates: { start_folder_scan: hold.gate },
    });
    await screen.findByText(t.intro);
    fireEvent.click(scanButton());
    const panel = await screen.findByRole("region", { name: t.progressTitle });
    expect(panel).toHaveAttribute("aria-live", "polite");
    await waitFor(() =>
      expect(panel).toHaveTextContent(
        format(t.progressRunning, { files: formatPlural(t.files, 4096), size: formatBytes(2 * GB) }),
      ),
    );
    expect(within(panel).getByRole("button", { name: t.cancel })).toBeInTheDocument();
    expect(backend.calls.find((call) => call.cmd === "start_folder_scan")?.args).toMatchObject({
      root: { kind: "system_drive" },
    });
    expect(screen.queryByRole("button", { name: t.scanDisk })).toBeNull();
    await act(async () => {
      hold.release();
    });
    expect(await screen.findByRole("region", { name: t.topFoldersTitle })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: t.progressTitle })).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent(format(t.announceDone, { size: formatBytes(380 * GB) }));
  });

  it("anuncia que esta guardando antes de mostrar el resultado", async () => {
    const hold = gate();
    await openDisk({
      diskScanEvents: [started(9), { type: "saving" }],
      diskScanResult: diskSummary({ scanId: 9 }),
      diskPages: defaultPages(),
      gates: { start_folder_scan: hold.gate },
    });
    await screen.findByText(t.intro);
    fireEvent.click(scanButton());
    const panel = await screen.findByRole("region", { name: t.progressTitle });
    await waitFor(() => expect(panel).toHaveTextContent(t.progressSaving));
    await act(async () => {
      hold.release();
    });
    await screen.findByRole("region", { name: t.topFoldersTitle });
  });

  it("cancelar pide cancel_scan con el id del analisis y conserva el resultado anterior", async () => {
    const hold = gate();
    const backend = await openDisk(
      withLastScan({
        diskScanEvents: [started(9)],
        diskScanResult: diskSummary({ scanId: 9, status: "cancelled", root: { kind: "system_drive", path: "", folderId: null } }),
        gates: { start_folder_scan: hold.gate },
      }),
    );
    await screen.findByRole("region", { name: t.topFoldersTitle });
    fireEvent.click(scanButton());
    const panel = await screen.findByRole("region", { name: t.progressTitle });
    fireEvent.click(within(panel).getByRole("button", { name: t.cancel }));
    await waitFor(() => expect(backend.count("cancel_scan")).toBe(1));
    expect(backend.calls.find((call) => call.cmd === "cancel_scan")?.args).toEqual({ scanId: 9 });
    await act(async () => {
      hold.release();
    });
    expect(await within(screen.getByRole("main")).findByText(t.cancelledNotice)).toBeInTheDocument();
    expect(screen.getByRole("region", { name: t.topFoldersTitle })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(t.announceCancelled);
    expect(backend.state.diskLast?.scanId).toBe(1);
  });

  it("cancelar antes de que llegue el id lo pide apenas llega", async () => {
    const hold = gate();
    const backend = await openDisk({
      diskScanResult: diskSummary({ scanId: 9, status: "cancelled", root: { kind: "system_drive", path: "", folderId: null } }),
      gates: { start_folder_scan: hold.gate },
    });
    await screen.findByText(t.intro);
    fireEvent.click(scanButton());
    const panel = await screen.findByRole("region", { name: t.progressTitle });
    fireEvent.click(within(panel).getByRole("button", { name: t.cancel }));
    expect(backend.count("cancel_scan")).toBe(0);
    await act(async () => {
      backend.emitFolderScan([started(12)]);
    });
    await waitFor(() => expect(backend.count("cancel_scan")).toBe(1));
    expect(backend.calls.find((call) => call.cmd === "cancel_scan")?.args).toEqual({ scanId: 12 });
    await act(async () => {
      hold.release();
    });
    expect(await within(screen.getByRole("main")).findByText(t.cancelledNoPrevious)).toBeInTheDocument();
  });

  it("un error del backend se muestra con el texto del catalogo y se puede reintentar", async () => {
    const backend = await openDisk({
      failureQueue: { start_folder_scan: [{ kind: "cleanup_in_progress", message: "x" }] },
      diskScanResult: diskSummary(),
      diskPages: defaultPages(),
    });
    await screen.findByText(t.intro);
    fireEvent.click(scanButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.cleanup_in_progress);
    expect(scanButton()).toBeEnabled();
    fireEvent.click(scanButton());
    await screen.findByRole("region", { name: t.topFoldersTitle });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(backend.count("start_folder_scan")).toBe(2);
  });

  it("si el ultimo analisis no se puede leer lo informa sin romper la pantalla", async () => {
    await openDisk({ failures: { get_last_folder_scan: { kind: "store_unavailable", message: "x" } } });
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.store_unavailable);
    expect(scanButton()).toBeEnabled();
  });

  it("al volver a la pantalla el analisis en curso sigue visible", async () => {
    const hold = gate();
    await openDisk({
      diskScanEvents: [started(9)],
      diskScanResult: diskSummary({ scanId: 9 }),
      diskPages: defaultPages(),
      gates: { start_folder_scan: hold.gate },
    });
    await screen.findByText(t.intro);
    fireEvent.click(scanButton());
    await screen.findByRole("region", { name: t.progressTitle });
    const nav = screen.getByRole("navigation", { name: es.nav.label });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.home }));
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
    expect(await screen.findByRole("region", { name: t.progressTitle })).toBeInTheDocument();
    await act(async () => {
      hold.release();
    });
    await screen.findByRole("region", { name: t.topFoldersTitle });
  });
});

describe("Espacio en disco: carpetas de la raiz", () => {
  it("la cantidad pedida a la raiz en simple son las 5 mas grandes por tamano", async () => {
    const backend = await openDisk(withLastScan());
    await screen.findByRole("region", { name: t.topFoldersTitle });
    const call = backend.calls.find((c) => c.cmd === "list_folder_children");
    expect(call?.args).toEqual({ scanId: 1, folderId: null, sort: "size", offset: 0, limit: 5 });
    expect(rootChildren()).toHaveLength(6);
  });
});
