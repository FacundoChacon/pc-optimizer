import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import type { CleanupEvent, JunkScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { cleanupSummary } from "../support/cleanupFixtures";
import { summary } from "../support/junkFixtures";
import {
  type BackendState,
  installBackend,
  type MockBackend,
  OTHER_USER,
} from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const names = es.cleanup.categoryNames;

const RESULT: JunkScanSummary = summary({
  temp_user: { scope: "per_user", itemCount: 3, sizeBytes: 5 * 1024 * 1024 },
  temp_windows: { itemCount: 2, sizeBytes: 2048 },
  recycle_bin: { scope: "per_user", itemCount: 12, sizeBytes: 340_000_000 },
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
  await screen.findByRole("region", { name: names.recycle_bin });
  return backend;
}

const binCard = () => screen.getByRole("region", { name: names.recycle_bin });
const emptyButton = () => within(binCard()).getByRole("button", { name: es.recycleBin.emptyAction });
const anyEmptyButtons = () => screen.queryAllByRole("button", { name: es.recycleBin.emptyAction });
const calls = (backend: MockBackend, cmd: string) => backend.calls.filter((call) => call.cmd === cmd);
const items12 = format(es.recycleBin.step1Body, { items: "12 elementos", size: formatBytes(340_000_000) });
const confirmName = format(es.recycleBin.confirmEmpty, { count: "12" });

async function openStep1(): Promise<HTMLElement> {
  const trigger = emptyButton();
  trigger.focus();
  fireEvent.click(trigger);
  return screen.findByRole("alertdialog", { name: es.recycleBin.step1Title });
}

async function openStep2(): Promise<HTMLElement> {
  const first = await openStep1();
  fireEvent.click(within(first).getByRole("button", { name: es.recycleBin.continue }));
  return screen.findByRole("alertdialog", { name: es.recycleBin.step2Title });
}

async function confirmEmptying(second: HTMLElement): Promise<void> {
  fireEvent.click(within(second).getByRole("checkbox", { name: es.recycleBin.acknowledgeCheckbox }));
  fireEvent.click(within(second).getByRole("button", { name: confirmName }));
}

describe("Vaciar Papelera: puntos de entrada", () => {
  it("la tarjeta de la Papelera con elementos ofrece Vaciar Papelera como accion secundaria", async () => {
    await openCleanup();
    expect(emptyButton()).toBeEnabled();
    expect(emptyButton()).toHaveAttribute("data-variant", "secondary");
    expect(document.body.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);
  });

  it("no se ofrece con la Papelera en 0 elementos ni en una categoria bloqueada o no disponible", async () => {
    const empty = summary({ recycle_bin: { scope: "per_user", itemCount: 0, sizeBytes: 0 } });
    await openCleanup({ lastScan: empty });
    expect(anyEmptyButtons()).toHaveLength(0);
    cleanup();
    clearMocks();
    const unavailable = summary({ recycle_bin: { status: "unavailable" } });
    await openCleanup({ lastScan: unavailable });
    expect(anyEmptyButtons()).toHaveLength(0);
  });

  it("en mismatch no se ofrece en ningun lado y el resumen explica por que", async () => {
    const mismatch = summary({
      temp_windows: { itemCount: 2, sizeBytes: 2048 },
      recycle_bin: { scope: "per_user", status: "blocked", blockReason: "different_user" },
    });
    await openCleanup({
      privilege: OTHER_USER,
      lastScan: mismatch,
      cleanupResult: cleanupSummary(),
      cleanupEvents: [STARTED],
    });
    expect(anyEmptyButtons()).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.clean }));
    await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    fireEvent.click(await screen.findByRole("button", { name: format(es.cleanup.confirmMove, { count: "5" }) }));
    await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
    expect(screen.getByText(es.recycleBin.blockedMismatch)).toBeInTheDocument();
    expect(anyEmptyButtons()).toHaveLength(0);
  });

  it("el resumen de una limpieza que movio archivos tambien lo ofrece, junto a la nota del espacio", async () => {
    await openCleanup({ cleanupResult: cleanupSummary(), cleanupEvents: [STARTED] });
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.clean }));
    await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    fireEvent.click(await screen.findByRole("button", { name: format(es.cleanup.confirmMove, { count: "5" }) }));
    const title = await screen.findByRole("heading", { name: es.cleanup.summaryTitle });
    const card = title.closest("section") as HTMLElement;
    expect(within(card).getByText(es.cleanup.summarySpaceNote)).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: es.recycleBin.emptyAction })).toBeEnabled();
  });

  it("no se ofrece mientras corre un escaneo ni una limpieza", async () => {
    const hold = gateOf();
    await openCleanup({ scanResult: RESULT, gates: { start_scan: hold.gate } });
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.scan }));
    await screen.findByText(es.cleanup.progressStarting);
    expect(anyEmptyButtons()).toHaveLength(0);
    await act(async () => {
      hold.release();
    });
    await waitFor(() => expect(anyEmptyButtons().length).toBeGreaterThan(0));
    cleanup();
    clearMocks();

    const cleaning = gateOf();
    await openCleanup({
      cleanupResult: cleanupSummary(),
      cleanupEvents: [STARTED],
      gates: { confirm_cleanup: cleaning.gate },
    });
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.clean }));
    await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    fireEvent.click(await screen.findByRole("button", { name: format(es.cleanup.confirmMove, { count: "5" }) }));
    await screen.findByText(format(es.cleanup.cleaningProgress, { done: "0", total: "12" }));
    expect(anyEmptyButtons()).toHaveLength(0);
    await act(async () => {
      cleaning.release();
    });
  });
});

describe("Vaciar Papelera: primer paso", () => {
  it("pulsar la accion mide la Papelera y abre el dialogo con cantidad, tamano, aviso y foco en Cancelar", async () => {
    const backend = await openCleanup();
    const dialog = await openStep1();
    expect(calls(backend, "prepare_empty_recycle_bin")).toHaveLength(1);
    expect(calls(backend, "prepare_empty_recycle_bin")[0]?.args).toEqual({});
    expect(within(dialog).getByText(items12)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: es.recycleBin.cancel })).toHaveFocus();
    expect(within(dialog).getByRole("button", { name: es.recycleBin.continue })).toBeEnabled();
    expect(calls(backend, "acknowledge_empty_recycle_bin")).toHaveLength(0);
    expect(calls(backend, "empty_recycle_bin")).toHaveLength(0);
  });

  it("Escape cierra el dialogo, devuelve el foco a la accion y no reconoce ni vacia nada", async () => {
    const backend = await openCleanup();
    const dialog = await openStep1();
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(emptyButton()).toHaveFocus();
    expect(calls(backend, "acknowledge_empty_recycle_bin")).toHaveLength(0);
    expect(calls(backend, "empty_recycle_bin")).toHaveLength(0);
  });

  it("Cancelar tampoco reconoce ni vacia, y se puede volver a empezar", async () => {
    const backend = await openCleanup();
    const dialog = await openStep1();
    fireEvent.click(within(dialog).getByRole("button", { name: es.recycleBin.cancel }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(calls(backend, "acknowledge_empty_recycle_bin")).toHaveLength(0);
    await openStep1();
    expect(calls(backend, "prepare_empty_recycle_bin")).toHaveLength(2);
  });

  it("un doble clic en Continuar reconoce una sola vez y muestra el segundo dialogo sin vaciar", async () => {
    const hold = gateOf();
    const backend = await openCleanup({ gates: { acknowledge_empty_recycle_bin: hold.gate } });
    const dialog = await openStep1();
    const next = within(dialog).getByRole("button", { name: es.recycleBin.continue });
    fireEvent.click(next);
    fireEvent.click(next);
    expect(next).toBeDisabled();
    await act(async () => {
      hold.release();
    });
    await screen.findByRole("alertdialog", { name: es.recycleBin.step2Title });
    expect(calls(backend, "acknowledge_empty_recycle_bin")).toHaveLength(1);
    expect(calls(backend, "acknowledge_empty_recycle_bin")[0]?.args).toEqual({ requestId: 1 });
    expect(calls(backend, "empty_recycle_bin")).toHaveLength(0);
  });
});

describe("Vaciar Papelera: segundo paso", () => {
  it("es un dialogo distinto, con la casilla sin marcar, el boton destructivo bloqueado y foco en Cancelar", async () => {
    const second = await (async () => {
      await openCleanup();
      return openStep2();
    })();
    expect(screen.queryByRole("alertdialog", { name: es.recycleBin.step1Title })).toBeNull();
    expect(within(second).getByText(format(es.recycleBin.step2Body, {
      items: "12 elementos",
      size: formatBytes(340_000_000),
    }))).toBeInTheDocument();
    const box = within(second).getByRole("checkbox", { name: es.recycleBin.acknowledgeCheckbox });
    expect(box).not.toBeChecked();
    const confirm = within(second).getByRole("button", { name: confirmName });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute("data-variant", "danger");
    const hint = confirm.getAttribute("aria-describedby");
    expect(hint).not.toBeNull();
    expect(document.getElementById(hint ?? "")).toHaveTextContent(es.recycleBin.acknowledgeHint);
    expect(within(second).getByRole("button", { name: es.recycleBin.cancel })).toHaveFocus();
    fireEvent.click(box);
    expect(confirm).toBeEnabled();
    fireEvent.click(box);
    expect(confirm).toBeDisabled();
  });

  it("Cancelar y Escape no piden el vaciado; la casilla vuelve a estar sin marcar al reabrir", async () => {
    const backend = await openCleanup();
    const second = await openStep2();
    fireEvent.click(within(second).getByRole("checkbox", { name: es.recycleBin.acknowledgeCheckbox }));
    fireEvent(second, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(emptyButton()).toHaveFocus();
    const again = await openStep2();
    expect(within(again).getByRole("checkbox", { name: es.recycleBin.acknowledgeCheckbox })).not.toBeChecked();
    fireEvent.click(within(again).getByRole("button", { name: es.recycleBin.cancel }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(calls(backend, "empty_recycle_bin")).toHaveLength(0);
    expect(backend.state.bin.itemCount).toBe(12);
  });

  it("solo el boton confirmado pide el vaciado, con el id y el token recibidos y sin rutas", async () => {
    const backend = await openCleanup();
    const second = await openStep2();
    await confirmEmptying(second);
    await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    expect(calls(backend, "empty_recycle_bin")).toHaveLength(1);
    expect(calls(backend, "empty_recycle_bin")[0]?.args).toEqual({
      requestId: 1,
      confirmToken: 4_294_967_297,
    });
  });
});

describe("Vaciar Papelera: ejecucion y resultado", () => {
  it("durante el vaciado anuncia 'Vaciando la Papelera' en aria-live y no ofrece otras operaciones", async () => {
    const hold = gateOf();
    const backend = await openCleanup({ gates: { empty_recycle_bin: hold.gate } });
    await confirmEmptying(await openStep2());
    const live = (await screen.findByText(es.recycleBin.emptying)).closest("[aria-live]");
    expect(live).toHaveAttribute("aria-live", "polite");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeDisabled();
    expect(screen.queryByRole("button", { name: es.cleanup.clean })).toBeNull();
    expect(anyEmptyButtons()).toHaveLength(0);
    await act(async () => {
      hold.release();
    });
    await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    expect(calls(backend, "empty_recycle_bin")).toHaveLength(1);
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toBeEnabled();
  });

  it("emptied: dice cuanto se libero, enfoca el titulo y deja la tarjeta en 0 sin ofrecer vaciar", async () => {
    await openCleanup();
    await confirmEmptying(await openStep2());
    const region = await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    const title = within(region).getByRole("heading", { name: es.recycleBin.resultTitle });
    await waitFor(() => expect(title).toHaveFocus());
    expect(within(region).getByText(format(es.recycleBin.result.emptied, { size: formatBytes(340_000_000) }))).toBeInTheDocument();
    expect(within(region).queryByText(es.recycleBin.notLogged)).toBeNull();
    expect(within(binCard()).getByText(format(es.cleanup.recycleBinItems, { count: "0" }))).toBeInTheDocument();
    expect(within(binCard()).getByText(formatBytes(0))).toBeInTheDocument();
    expect(anyEmptyButtons()).toHaveLength(0);
  });

  it("partial: informa lo liberado y los elementos que quedaron, y la tarjeta muestra la medicion posterior", async () => {
    await openCleanup({
      emptyOutcome: {
        status: "partial",
        itemsAfter: 2,
        bytesAfter: 40_000_000,
        freedBytes: 300_000_000,
        detail: "partial",
        logged: true,
      },
    });
    await confirmEmptying(await openStep2());
    const region = await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    expect(
      within(region).getByText(
        format(es.recycleBin.result.partial, { size: formatBytes(300_000_000), items: "2 elementos" }),
      ),
    ).toBeInTheDocument();
    expect(within(binCard()).getByText(format(es.cleanup.recycleBinItems, { count: "2" }))).toBeInTheDocument();
    expect(within(binCard()).getByText(formatBytes(40_000_000))).toBeInTheDocument();
    expect(within(binCard()).getByRole("button", { name: es.recycleBin.emptyAction })).toBeEnabled();
  });

  it("failed: dice que no se elimino nada con el motivo en lenguaje llano y sin mostrar el codigo", async () => {
    await openCleanup({
      emptyOutcome: {
        status: "failed",
        itemsAfter: 12,
        bytesAfter: 340_000_000,
        freedBytes: 0,
        detail: "permission_denied",
        logged: true,
      },
    });
    await confirmEmptying(await openStep2());
    const region = await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    expect(within(region).getByText(es.recycleBin.result.failed)).toBeInTheDocument();
    expect(within(region).getByText(es.recycleBin.details.permission_denied)).toBeInTheDocument();
    expect(region.textContent).not.toContain("permission_denied");
    expect(within(binCard()).getByText(format(es.cleanup.recycleBinItems, { count: "12" }))).toBeInTheDocument();
  });

  it("unverified: avisa que no se pudo comprobar y deja la tarjeta como estaba", async () => {
    await openCleanup({
      emptyOutcome: {
        status: "unverified",
        itemsAfter: null,
        bytesAfter: null,
        freedBytes: 340_000_000,
        detail: "unverified",
        logged: true,
      },
    });
    await confirmEmptying(await openStep2());
    const region = await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    expect(within(region).getByText(es.recycleBin.result.unverified)).toBeInTheDocument();
    expect(within(binCard()).getByText(format(es.cleanup.recycleBinItems, { count: "12" }))).toBeInTheDocument();
  });

  it("avisa cuando el vaciado ocurrio pero no se pudo guardar el registro", async () => {
    await openCleanup({
      emptyOutcome: {
        status: "emptied",
        itemsAfter: 0,
        bytesAfter: 0,
        freedBytes: 340_000_000,
        detail: null,
        logged: false,
      },
    });
    await confirmEmptying(await openStep2());
    const region = await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    expect(within(region).getByText(es.recycleBin.notLogged)).toBeInTheDocument();
  });

  it("un nuevo escaneo descarta la medicion posterior y vuelve a mostrar la del escaneo", async () => {
    const fresh = summary(
      { recycle_bin: { scope: "per_user", itemCount: 5, sizeBytes: 5_000 } },
      { scanId: 2 },
    );
    await openCleanup({ scanResult: fresh });
    await confirmEmptying(await openStep2());
    await screen.findByRole("region", { name: es.recycleBin.resultTitle });
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.scan }));
    await waitFor(() =>
      expect(within(binCard()).getByText(format(es.cleanup.recycleBinItems, { count: "5" }))).toBeInTheDocument(),
    );
  });
});

describe("Vaciar Papelera: errores", () => {
  it("recycle_bin_changed explica el cambio, no vacia y 'Revisar de nuevo' vuelve al primer paso con otra medicion", async () => {
    const backend = await openCleanup();
    const second = await openStep2();
    backend.state.bin = { itemCount: 13, sizeBytes: 341_000_000 };
    await confirmEmptying(second);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(es.errors.recycle_bin_changed);
    expect(backend.state.bin.itemCount).toBe(13);
    fireEvent.click(within(alert.closest("section") as HTMLElement).getByRole("button", { name: es.recycleBin.reviewAgain }));
    const first = await screen.findByRole("alertdialog", { name: es.recycleBin.step1Title });
    expect(
      within(first).getByText(
        format(es.recycleBin.step1Body, { items: "13 elementos", size: formatBytes(341_000_000) }),
      ),
    ).toBeInTheDocument();
    expect(calls(backend, "prepare_empty_recycle_bin")).toHaveLength(2);
  });

  it("empty_request_expired tambien ofrece revisar de nuevo", async () => {
    const backend = await openCleanup();
    const second = await openStep2();
    backend.state.failureQueue.empty_recycle_bin = [{ kind: "empty_request_expired", message: "x" }];
    await confirmEmptying(second);
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.empty_request_expired);
    expect(screen.getByRole("button", { name: es.recycleBin.reviewAgain })).toBeInTheDocument();
  });

  it("recycle_bin_already_empty lo dice y deja la tarjeta en 0 sin ofrecer vaciar", async () => {
    const backend = await openCleanup();
    backend.state.bin = { itemCount: 0, sizeBytes: 0 };
    fireEvent.click(emptyButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.recycle_bin_already_empty);
    expect(within(binCard()).getByText(format(es.cleanup.recycleBinItems, { count: "0" }))).toBeInTheDocument();
    expect(anyEmptyButtons()).toHaveLength(0);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("un error del backend al preparar se muestra con el texto del catalogo y no abre ningun dialogo", async () => {
    await openCleanup({
      failures: { prepare_empty_recycle_bin: { kind: "recycle_bin_unavailable", message: "x" } },
    });
    fireEvent.click(emptyButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.recycle_bin_unavailable);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.queryByRole("button", { name: es.recycleBin.reviewAgain })).toBeNull();
  });

  it("un error al reconocer vuelve al estado inicial con el mensaje y sin pedir el vaciado", async () => {
    const backend = await openCleanup({
      failures: { acknowledge_empty_recycle_bin: { kind: "empty_request_expired", message: "x" } },
    });
    const first = await openStep1();
    fireEvent.click(within(first).getByRole("button", { name: es.recycleBin.continue }));
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.empty_request_expired);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(calls(backend, "empty_recycle_bin")).toHaveLength(0);
  });
});
