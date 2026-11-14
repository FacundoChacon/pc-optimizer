import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { CLEANABLE_CATEGORIES } from "../../features/cleanup/cleanupText";
import type { CleanupEvent, JunkScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { cleanupSummary, preview, previewCategory } from "../support/cleanupFixtures";
import { summary } from "../support/junkFixtures";
import { type BackendState, installBackend, type MockBackend } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const advanced = { uiMode: "advanced", autostartTray: false, language: "es" } as const;
const names = es.cleanup.categoryNames;

const RESULT: JunkScanSummary = summary({
  temp_user: { scope: "per_user", itemCount: 3, sizeBytes: 5 * 1024 * 1024 },
  temp_windows: { itemCount: 2, sizeBytes: 2048 },
  recycle_bin: { scope: "per_user", itemCount: 12, sizeBytes: 340_000_000 },
  old_logs: { status: "unavailable" },
});

const STARTED: CleanupEvent = { type: "started", previewId: 1, total: 12 };

function gateOf(): { gate: Promise<void>; release: () => void } {
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { gate, release };
}

async function openCleanup(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend({ lastScan: RESULT, ...overrides });
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.cleanup }));
  await screen.findByRole("heading", { level: 1, name: es.cleanup.title });
  await screen.findByRole("region", { name: names.temp_user });
  return backend;
}

const cleanButton = () => screen.getByRole("button", { name: es.cleanup.clean });
const scanButton = () => screen.getByRole("button", { name: es.cleanup.scan });
const confirmName = (count: number) => format(es.cleanup.confirmMove, { count: String(count) });

async function openPreview(): Promise<HTMLElement> {
  const trigger = cleanButton();
  trigger.focus();
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
  await screen.findByRole("button", { name: confirmName(5) });
  return dialog;
}

function callsOf(backend: MockBackend, cmd: string) {
  return backend.calls.filter((call) => call.cmd === cmd);
}

describe("CleanupScreen: Limpiar y el preview", () => {
  it("con elegibles Limpiar es la unica accion primaria y Buscar basura pasa a secundaria", async () => {
    await openCleanup();
    expect(cleanButton()).toBeEnabled();
    expect(cleanButton()).toHaveAttribute("data-variant", "primary");
    expect(scanButton()).toHaveAttribute("data-variant", "secondary");
    expect(document.body.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);
  });

  it("sin elegibles no hay Limpiar y Buscar basura sigue siendo la accion primaria", async () => {
    const nothing = summary({ temp_user: { scope: "per_user", itemCount: 0 }, recycle_bin: { itemCount: 3, sizeBytes: 9 } });
    await openCleanup({ lastScan: nothing });
    expect(screen.queryByRole("button", { name: es.cleanup.clean })).toBeNull();
    expect(scanButton()).toHaveAttribute("data-variant", "primary");
  });

  it("una categoria bloqueada o solo la Papelera no habilitan Limpiar", async () => {
    const blocked = summary({
      temp_user: { scope: "per_user", status: "blocked", blockReason: "different_user" },
      recycle_bin: { itemCount: 3, sizeBytes: 9 },
    });
    await openCleanup({ lastScan: blocked });
    expect(screen.queryByRole("button", { name: es.cleanup.clean })).toBeNull();
  });

  it("no ofrece ninguna accion que borre o elimine; vaciar la Papelera es su propio flujo y no aparece dentro del preview", async () => {
    await openCleanup();
    expect(screen.queryByRole("button", { name: /borrar|eliminar/i })).toBeNull();
    fireEvent.click(cleanButton());
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(within(dialog).queryByRole("button", { name: /borrar|eliminar|vaciar/i })).toBeNull();
  });

  it("con el escaneo ya limpiado muestra el aviso con la fecha, sin Limpiar, y Buscar basura primaria", async () => {
    const cleaned = { ...RESULT, cleanedAt: 1_800_000_099_000 };
    await openCleanup({ lastScan: cleaned });
    expect(
      screen.getByText(format(es.cleanup.alreadyCleaned, { date: formatDateTime(1_800_000_099_000) })),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: es.cleanup.clean })).toBeNull();
    expect(scanButton()).toBeEnabled();
    expect(scanButton()).toHaveAttribute("data-variant", "primary");
  });

  it("Limpiar pide el preview solo con id y categorias, abre el dialogo con todo marcado y foco en Cancelar", async () => {
    const backend = await openCleanup();
    const dialog = await openPreview();
    const [call] = callsOf(backend, "preview_cleanup");
    expect(call?.args).toEqual({
      scanId: RESULT.scanId,
      categories: [...CLEANABLE_CATEGORIES],
    });
    expect(JSON.stringify(call?.args)).not.toMatch(/\\\\|C:/);
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_user) })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_windows) })).toBeChecked();
    expect(within(dialog).getByRole("button", { name: es.cleanup.cancel })).toHaveFocus();
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("desmarcar una categoria pide un preview nuevo y deshabilita Confirmar hasta recibirlo", async () => {
    const backend = await openCleanup();
    await openPreview();
    const second = gateOf();
    backend.state.gates.preview_cleanup = second.gate;
    fireEvent.click(screen.getByRole("checkbox", { name: new RegExp(names.temp_windows) }));
    await waitFor(() => expect(callsOf(backend, "preview_cleanup")).toHaveLength(2));
    const requested = callsOf(backend, "preview_cleanup")[1]?.args as { categories: string[] };
    expect(requested.categories).not.toContain("temp_windows");
    expect(requested.categories).toContain("temp_user");
    expect(screen.getByText(es.cleanup.previewLoading)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: confirmName(5) })).toBeDisabled();
    await act(async () => {
      second.release();
    });
    expect(await screen.findByRole("button", { name: confirmName(3) })).toBeEnabled();
    expect(screen.getByRole("checkbox", { name: new RegExp(names.temp_windows) })).not.toBeChecked();
  });

  it("volver a marcar la categoria la incluye de nuevo en el siguiente preview", async () => {
    const backend = await openCleanup();
    await openPreview();
    const box = () => screen.getByRole("checkbox", { name: new RegExp(names.temp_windows) });
    fireEvent.click(box());
    await screen.findByRole("button", { name: confirmName(3) });
    fireEvent.click(box());
    await screen.findByRole("button", { name: confirmName(5) });
    const last = callsOf(backend, "preview_cleanup")[2]?.args as { categories: string[] };
    expect(last.categories).toContain("temp_windows");
  });

  it("desmarcar todas deja Confirmar deshabilitado sin pedir otro preview ni confirmar", async () => {
    const backend = await openCleanup();
    await openPreview();
    fireEvent.click(screen.getByRole("checkbox", { name: new RegExp(names.temp_user) }));
    await screen.findByRole("button", { name: confirmName(2) });
    fireEvent.click(screen.getByRole("checkbox", { name: new RegExp(names.temp_windows) }));
    const confirm = await screen.findByRole("button", { name: confirmName(0) });
    expect(confirm).toBeDisabled();
    expect(screen.getByText(es.cleanup.previewNothingSelected)).toBeInTheDocument();
    expect(callsOf(backend, "preview_cleanup")).toHaveLength(2);
    fireEvent.click(confirm);
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("Escape cierra el dialogo, devuelve el foco a Limpiar y no invoca confirm_cleanup", async () => {
    const backend = await openCleanup();
    const dialog = await openPreview();
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(cleanButton()).toHaveFocus();
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("Cancelar cierra el dialogo sin ejecutar nada", async () => {
    const backend = await openCleanup();
    const dialog = await openPreview();
    fireEvent.click(within(dialog).getByRole("button", { name: es.cleanup.cancel }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
    expect(cleanButton()).toHaveFocus();
  });

  it("con la Papelera deshabilitada el dialogo advierte y no deja confirmar", async () => {
    const backend = await openCleanup({ preview: preview({ recycleBinAvailable: false }) });
    fireEvent.click(cleanButton());
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(es.cleanup.recycleBinDisabled);
    expect(within(dialog).getByRole("button", { name: confirmName(5) })).toBeDisabled();
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("un error al pedir el preview se muestra con el texto del catalogo y no abre el dialogo", async () => {
    await openCleanup({ failures: { preview_cleanup: { kind: "scan_already_cleaned", message: "x" } } });
    fireEvent.click(cleanButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.scan_already_cleaned);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});

describe("CleanupScreen: confirmar, progreso y resumen", () => {
  it("confirmar envia solo el id, anuncia el progreso en aria-live y bloquea escanear y limpiar", async () => {
    const hold = gateOf();
    const backend = await openCleanup({
      cleanupResult: cleanupSummary(),
      cleanupEvents: [STARTED],
      gates: { confirm_cleanup: hold.gate },
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    const text = format(es.cleanup.cleaningProgress, { done: "0", total: "12" });
    const live = (await screen.findByText(text)).closest("[aria-live]");
    expect(live).toHaveAttribute("aria-live", "polite");
    expect(callsOf(backend, "confirm_cleanup")[0]?.args).toMatchObject({ previewId: 1 });
    expect(JSON.stringify(Object.keys(callsOf(backend, "confirm_cleanup")[0]?.args as object))).not.toMatch(/path|ruta/i);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(scanButton()).toBeDisabled();
    expect(screen.queryByRole("button", { name: es.cleanup.clean })).toBeNull();
    await act(async () => {
      backend.emitCleanup([{ type: "progress", processed: 5, total: 12, movedCount: 5, movedBytes: 9 }]);
    });
    expect(screen.getByText(format(es.cleanup.cleaningProgress, { done: "5", total: "12" }))).toBeInTheDocument();
    await act(async () => {
      hold.release();
    });
    await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
  });

  it("al terminar muestra el resumen simple sin rutas ni codigos, con el foco en su titulo y Buscar de nuevo", async () => {
    const backend = await openCleanup({ cleanupResult: cleanupSummary(), cleanupEvents: [STARTED] });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    const title = await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
    await waitFor(() => expect(title).toHaveFocus());
    const card = title.closest("section") as HTMLElement;
    expect(
      within(card).getByText(format(es.cleanup.summaryMoved, { count: "10", size: formatBytes(1024 * 1024) })),
    ).toBeInTheDocument();
    expect(within(card).getByText(es.cleanup.summarySpaceNote)).toBeInTheDocument();
    expect(within(card).getByText(format(es.cleanup.summaryLocked, { count: "2" }))).toBeInTheDocument();
    expect(card.textContent).not.toMatch(/in_use|\\\\|C:/);
    expect(within(card).queryByRole("table")).toBeNull();
    expect(within(card).getByRole("button", { name: es.cleanup.scanAgain })).toHaveAttribute("data-variant", "primary");
    // El escaneo quedo limpiado: aviso oculto por el resumen, Limpiar ya no esta.
    expect(screen.queryByRole("button", { name: es.cleanup.clean })).toBeNull();
    expect(callsOf(backend, "get_last_junk_scan").length).toBeGreaterThanOrEqual(2);
  });

  it("Buscar de nuevo arranca un escaneo y quita el resumen", async () => {
    const backend = await openCleanup({
      cleanupResult: cleanupSummary(),
      cleanupEvents: [STARTED],
      scanResult: summary({ temp_windows: { itemCount: 1, sizeBytes: 5 } }, { scanId: 2 }),
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.scanAgain }));
    await waitFor(() => expect(callsOf(backend, "start_scan")).toHaveLength(1));
    await waitFor(() => expect(screen.queryByRole("heading", { name: es.cleanup.summaryTitle })).toBeNull());
  });

  it("cancelar pide cancel_cleanup con el id y muestra el resumen cancelado con los no procesados", async () => {
    const hold = gateOf();
    const backend = await openCleanup({
      cleanupResult: cleanupSummary({ status: "cancelled", notProcessed: 4, processed: 8, totalItems: 12 }),
      cleanupEvents: [STARTED],
      gates: { confirm_cleanup: hold.gate },
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    fireEvent.click(await screen.findByRole("button", { name: es.cleanup.cancelCleaning }));
    await waitFor(() => expect(callsOf(backend, "cancel_cleanup")).toHaveLength(1));
    expect(callsOf(backend, "cancel_cleanup")[0]?.args).toEqual({ previewId: 1 });
    await act(async () => {
      hold.release();
    });
    expect(await screen.findByText(format(es.cleanup.summaryCancelled, { count: "4" }))).toBeInTheDocument();
  });

  it("un aborto muestra el texto de su motivo", async () => {
    await openCleanup({
      cleanupResult: cleanupSummary({
        status: "aborted",
        abortReason: "recycle_bin_unavailable",
        movedCount: 0,
        movedBytes: 0,
        skippedLocked: 0,
        reasons: [],
        categories: [],
        processed: 0,
        notProcessed: 12,
      }),
      cleanupEvents: [STARTED],
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    expect(await screen.findByText(es.cleanup.summaryAborted.recycle_bin_unavailable)).toBeInTheDocument();
  });

  it("el resumen cuenta cada familia no vacia: protegidos, nube y errores", async () => {
    await openCleanup({
      cleanupResult: cleanupSummary({
        skippedLocked: 0,
        skippedProtected: 3,
        skippedCloud: 1,
        errors: 2,
        reasons: [{ outcome: "skipped_protected", detail: "identity_changed", count: 3 }],
      }),
      cleanupEvents: [STARTED],
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
    expect(screen.getByText(format(es.cleanup.summaryProtected, { count: "3" }))).toBeInTheDocument();
    expect(screen.getByText(format(es.cleanup.summaryCloud, { count: "1" }))).toBeInTheDocument();
    expect(screen.getByText(format(es.cleanup.summaryErrors, { count: "2" }))).toBeInTheDocument();
    expect(screen.queryByText(format(es.cleanup.summaryLocked, { count: "0" }))).toBeNull();
  });

  it("modo avanzado: tablas por categoria y por motivo con el codigo junto al texto", async () => {
    await openCleanup({
      settings: advanced,
      cleanupResult: cleanupSummary({
        skippedLocked: 0,
        skippedProtected: 3,
        reasons: [{ outcome: "skipped_protected", detail: "identity_changed", count: 3 }],
      }),
      cleanupEvents: [STARTED],
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    const title = await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
    const card = title.closest("section") as HTMLElement;
    const tables = within(card).getAllByRole("table");
    expect(tables).toHaveLength(2);
    const byReason = within(tables[1] as HTMLElement);
    expect(byReason.getByText(es.cleanup.cleanupReasons.changed)).toBeInTheDocument();
    expect(byReason.getByText("identity_changed")).toBeInTheDocument();
    expect(byReason.getByText("3")).toBeInTheDocument();
    const byCategory = within(tables[0] as HTMLElement);
    expect(byCategory.getByText(names.temp_user)).toBeInTheDocument();
  });

  it("preview_expired al confirmar pide un preview nuevo, avisa y no confirma solo", async () => {
    const backend = await openCleanup({
      cleanupResult: cleanupSummary(),
      failureQueue: { confirm_cleanup: [{ kind: "preview_expired", message: "x" }] },
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    expect(await screen.findByText(es.cleanup.previewUpdated)).toBeInTheDocument();
    expect(screen.getByRole("alertdialog", { name: es.cleanup.previewTitle })).toBeInTheDocument();
    await waitFor(() => expect(callsOf(backend, "preview_cleanup")).toHaveLength(2));
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(1);
    const confirm = await screen.findByRole("button", { name: confirmName(5) });
    await waitFor(() => expect(confirm).toBeEnabled());
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(1);
    expect(screen.queryByRole("heading", { name: es.cleanup.summaryTitle })).toBeNull();
    // El segundo intento, ya manual, usa el id del preview nuevo.
    fireEvent.click(confirm);
    await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
    expect(callsOf(backend, "confirm_cleanup")[1]?.args).toMatchObject({ previewId: 2 });
  });

  it.each([
    ["cleanup_in_progress", es.errors.cleanup_in_progress],
    ["scan_already_cleaned", es.errors.scan_already_cleaned],
    ["preview_stale", es.errors.preview_stale],
    ["nothing_to_clean", es.errors.nothing_to_clean],
  ])("un %s al confirmar cierra el dialogo y muestra el texto del catalogo", async (kind, text) => {
    await openCleanup({ failures: { confirm_cleanup: { kind, message: "tecnico" } } });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    expect(await screen.findByRole("alert")).toHaveTextContent(text);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.queryByText("tecnico")).toBeNull();
  });

  it("el progreso sigue visible al ir a otra pantalla y volver", async () => {
    const hold = gateOf();
    const backend = await openCleanup({
      cleanupResult: cleanupSummary(),
      cleanupEvents: [STARTED],
      gates: { confirm_cleanup: hold.gate },
    });
    await openPreview();
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    await screen.findByText(format(es.cleanup.cleaningProgress, { done: "0", total: "12" }));
    const nav = screen.getByRole("navigation", { name: es.nav.label });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.home }));
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.cleanup }));
    expect(await screen.findByText(format(es.cleanup.cleaningProgress, { done: "0", total: "12" }))).toBeInTheDocument();
    expect(scanButton()).toBeDisabled();
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(1);
    await act(async () => {
      hold.release();
    });
    await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
  });

  it("modo avanzado: el dialogo muestra los recientes por categoria", async () => {
    await openCleanup({
      settings: advanced,
      preview: preview({
        categories: [
          previewCategory("temp_user", { itemCount: 3, sizeBytes: 5 * 1024 * 1024, recentCount: 4, recentBytes: 2048 }),
          previewCategory("temp_windows", { itemCount: 2, sizeBytes: 2048 }),
        ],
      }),
    });
    await openPreview();
    expect(
      screen.getByText(format(es.cleanup.previewRecentColumn, { count: "4", size: formatBytes(2048) })),
    ).toBeInTheDocument();
  });
});
