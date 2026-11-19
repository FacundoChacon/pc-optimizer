import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { SCREENS } from "../../app/navigation";
import { AppProviders } from "../../app/providers/AppProviders";
import { useNavigation } from "../../app/providers/NavigationProvider";
import { useCleanNow } from "../../features/cleanup/CleanupProvider";
import type { CleanNowPlan } from "../../features/dashboard/cleanNowPlan";
import { es } from "../../shared/i18n/es";
import { summary } from "../support/junkFixtures";
import { type BackendState, installBackend, type MockBackend } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const names = es.cleanup.categoryNames;
const withItems = { temp_user: { scope: "per_user" as const, itemCount: 3, sizeBytes: 5 * 1024 * 1024 } };
const RESULT = summary({ ...withItems, temp_windows: { itemCount: 2, sizeBytes: 2048 } });

/** Hace de Inicio: un boton que pide "Limpiar ahora" con el plan dado y navega sin mover el foco. */
function Harness({ plan, focusMain }: { plan: CleanNowPlan; focusMain: boolean }) {
  const { current, navigate } = useNavigation();
  const cleanNow = useCleanNow();
  const Screen = SCREENS.find((screen) => screen.id === current)?.Component;
  return (
    <>
      <button type="button" onClick={() => navigate("monitor")}>
        salir a Monitor
      </button>
      <button type="button" onClick={() => navigate("cleanup")}>
        volver a Limpieza
      </button>
      <main id="main" tabIndex={-1}>
      {current === "home" ? (
        <button
          type="button"
          onClick={() => {
            cleanNow.request(plan);
            navigate("cleanup", { focus: "none" });
            // Como NavigationProvider al cambiar de pantalla: el foco queda en <main>, que sigue conectado.
            if (focusMain) document.getElementById("main")?.focus();
          }}
        >
          pedir
        </button>
      ) : (
        Screen !== undefined && <Screen />
      )}
      </main>
    </>
  );
}

function gateOf(): { gate: Promise<void>; release: () => void } {
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { gate, release };
}

async function request(
  plan: CleanNowPlan,
  overrides: Partial<BackendState> = {},
  options: { strict?: boolean; focusMain?: boolean } = {},
): Promise<MockBackend> {
  const backend = installBackend(overrides);
  const tree = (
    <AppProviders>
      <Harness plan={plan} focusMain={options.focusMain === true} />
    </AppProviders>
  );
  render(options.strict === true ? <StrictMode>{tree}</StrictMode> : tree);
  const trigger = await screen.findByRole("button", { name: "pedir" });
  trigger.focus();
  fireEvent.click(trigger);
  return backend;
}

const callsOf = (backend: MockBackend, cmd: string) => backend.calls.filter((call) => call.cmd === cmd);

describe("Limpiar ahora: pedido de preview (plan preview)", () => {
  it("al montar Limpieza el dialogo esta abierto, con las limpiables marcadas y el foco en Cancelar", async () => {
    const backend = await request("preview", { lastScan: RESULT });
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_user) })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_windows) })).toBeChecked();
    expect(within(dialog).getByRole("button", { name: es.cleanup.cancel })).toHaveFocus();
    expect(callsOf(backend, "start_scan")).toHaveLength(0);
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("Escape cierra el dialogo, el foco vuelve a Limpiar y no se invoca confirm_cleanup", async () => {
    const backend = await request("preview", { lastScan: RESULT });
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    await within(dialog).findByRole("button", { name: /^Mover/ });
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.getByRole("button", { name: es.cleanup.clean })).toHaveFocus();
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("sin boton Limpiar (escaneo ya limpiado) el foco de respaldo es el encabezado", async () => {
    const cleaned = { ...RESULT, cleanedAt: 1_800_000_100_000 };
    const backend = await request("preview", { lastScan: cleaned });
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.getByRole("heading", { level: 1, name: es.cleanup.title })).toHaveFocus();
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });
});

describe("Limpiar ahora: busca y luego abre el preview (plan scan_then_preview)", () => {
  it("muestra el progreso y, al terminar con elegibles, abre el dialogo con el foco en Cancelar", async () => {
    const { gate, release } = gateOf();
    const backend = await request("scan_then_preview", { scanResult: RESULT, gates: { start_scan: gate } });
    expect(await screen.findByText(es.cleanup.progressTitle)).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await act(async () => release());
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(within(dialog).getByRole("button", { name: es.cleanup.cancel })).toHaveFocus();
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_user) })).toBeChecked();
    expect(callsOf(backend, "start_scan")).toHaveLength(1);
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("mientras busca, el foco queda en el encabezado de Limpieza y no se pierde en el documento", async () => {
    const { gate, release } = gateOf();
    await request("scan_then_preview", { scanResult: RESULT, gates: { start_scan: gate } });
    await screen.findByText(es.cleanup.progressTitle);
    expect(screen.getByRole("heading", { level: 1, name: es.cleanup.title })).toHaveFocus();
    await act(async () => release());
    await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
  });

  it("sin elegibles no abre el dialogo y anuncia que no hay archivos para limpiar", async () => {
    const { gate, release } = gateOf();
    const empty = summary({ temp_user: { itemCount: 0 }, recycle_bin: { itemCount: 3, sizeBytes: 9 } });
    await request("scan_then_preview", { scanResult: empty, gates: { start_scan: gate } });
    await screen.findByText(es.cleanup.progressTitle);
    await act(async () => release());
    await waitFor(() => expect(screen.queryByText(es.cleanup.progressTitle)).toBeNull());
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.getByRole("status")).toHaveTextContent(es.cleanup.cleanNowNothing);
  });

  it("una busqueda cancelada no abre el dialogo", async () => {
    const { gate, release } = gateOf();
    const cancelled = summary({}, { status: "cancelled", finishedAt: null, categories: [] });
    await request("scan_then_preview", { scanResult: cancelled, gates: { start_scan: gate } });
    await screen.findByText(es.cleanup.progressTitle);
    await act(async () => release());
    await waitFor(() => expect(screen.queryByText(es.cleanup.progressTitle)).toBeNull());
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.getByRole("status")).not.toHaveTextContent(es.cleanup.cleanNowNothing);
  });

  it("una busqueda con error no abre el dialogo y muestra el error del catalogo", async () => {
    await request("scan_then_preview", { failures: { start_scan: { kind: "scan_in_progress", message: "x" } } });
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.scan_in_progress);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("salir de Limpieza antes de que termine descarta el pedido: sin dialogo al volver y con Limpiar disponible", async () => {
    const { gate, release } = gateOf();
    await request("scan_then_preview", { scanResult: RESULT, gates: { start_scan: gate } });
    await screen.findByText(es.cleanup.progressTitle);
    fireEvent.click(screen.getByRole("button", { name: "salir a Monitor" }));
    await screen.findByRole("heading", { level: 1, name: es.monitor.title });
    await act(async () => release());
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "volver a Limpieza" }));
    await screen.findByRole("heading", { level: 1, name: es.cleanup.title });
    expect(await screen.findByRole("button", { name: es.cleanup.clean })).toBeEnabled();
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });
});

describe("Limpiar ahora: robustez de la espera (revision independiente)", () => {
  it("con StrictMode (doble efecto de montaje) la busqueda pedida igual abre el preview al terminar", async () => {
    const { gate, release } = gateOf();
    const backend = await request("scan_then_preview", { scanResult: RESULT, gates: { start_scan: gate } }, { strict: true });
    expect(await screen.findByText(es.cleanup.progressTitle)).toBeInTheDocument();
    await act(async () => release());
    const dialog = await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(within(dialog).getByRole("button", { name: es.cleanup.cancel })).toHaveFocus();
    expect(callsOf(backend, "start_scan")).toHaveLength(1);
    expect(callsOf(backend, "confirm_cleanup")).toHaveLength(0);
  });

  it("con StrictMode y plan preview el dialogo sigue abierto", async () => {
    await request("preview", { lastScan: RESULT }, { strict: true });
    expect(await screen.findByRole("alertdialog", { name: es.cleanup.previewTitle })).toBeInTheDocument();
  });

  it("con StrictMode, salir de Limpieza antes de terminar sigue descartando el pedido", async () => {
    const { gate, release } = gateOf();
    await request("scan_then_preview", { scanResult: RESULT, gates: { start_scan: gate } }, { strict: true });
    await screen.findByText(es.cleanup.progressTitle);
    fireEvent.click(screen.getByRole("button", { name: "salir a Monitor" }));
    await screen.findByRole("heading", { level: 1, name: es.monitor.title });
    await act(async () => release());
    fireEvent.click(screen.getByRole("button", { name: "volver a Limpieza" }));
    await screen.findByRole("heading", { level: 1, name: es.cleanup.title });
    expect(await screen.findByRole("button", { name: es.cleanup.clean })).toBeEnabled();
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("si el inicio falla de inmediato, una busqueda manual posterior con elegibles NO abre el preview", async () => {
    await request("scan_then_preview", {
      scanResult: RESULT,
      failureQueue: { start_scan: [{ kind: "scan_in_progress", message: "x" }] },
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.scan_in_progress);
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.scan }));
    expect(await screen.findByRole("button", { name: es.cleanup.clean })).toBeEnabled();
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });
});

describe("Limpiar ahora: foco al cerrar el preview con el foco real en <main> (smoke H-1)", () => {
  const closeByEscape = (dialog: HTMLElement) =>
    fireEvent(dialog, new Event("cancel", { cancelable: true }));

  for (const strict of [false, true]) {
    const suffix = strict ? " (StrictMode)" : "";

    it(`plan preview: Escape devuelve el foco a Limpiar aunque estuviera en <main>${suffix}`, async () => {
      await request(
        "preview",
        { lastScan: RESULT },
        { focusMain: true, strict },
      );
      const dialog = await screen.findByRole("alertdialog", {
        name: es.cleanup.previewTitle,
      });
      closeByEscape(dialog);
      await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
      expect(
        screen.getByRole("button", { name: es.cleanup.clean }),
      ).toHaveFocus();
    });

    it(`plan preview: Cancelar devuelve el foco a Limpiar${suffix}`, async () => {
      await request(
        "preview",
        { lastScan: RESULT },
        { focusMain: true, strict },
      );
      const dialog = await screen.findByRole("alertdialog", {
        name: es.cleanup.previewTitle,
      });
      fireEvent.click(
        within(dialog).getByRole("button", { name: es.cleanup.cancel }),
      );
      await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
      expect(
        screen.getByRole("button", { name: es.cleanup.clean }),
      ).toHaveFocus();
    });

    it(`scan_then_preview: Escape devuelve el foco a Limpiar y no a Buscar basura${suffix}`, async () => {
      const { gate, release } = gateOf();
      await request(
        "scan_then_preview",
        { scanResult: RESULT, gates: { start_scan: gate } },
        { focusMain: true, strict },
      );
      await screen.findByText(es.cleanup.progressTitle);
      await act(async () => release());
      const dialog = await screen.findByRole("alertdialog", {
        name: es.cleanup.previewTitle,
      });
      closeByEscape(dialog);
      await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
      expect(
        screen.getByRole("button", { name: es.cleanup.clean }),
      ).toHaveFocus();
    });
  }

  it("scan_then_preview: Cancelar devuelve el foco a Limpiar", async () => {
    const { gate, release } = gateOf();
    await request(
      "scan_then_preview",
      { scanResult: RESULT, gates: { start_scan: gate } },
      { focusMain: true },
    );
    await screen.findByText(es.cleanup.progressTitle);
    await act(async () => release());
    const dialog = await screen.findByRole("alertdialog", {
      name: es.cleanup.previewTitle,
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: es.cleanup.cancel }),
    );
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(
      screen.getByRole("button", { name: es.cleanup.clean }),
    ).toHaveFocus();
  });

  it("plan preview sin boton Limpiar: el foco de respaldo es el encabezado aunque el foco estuviera en <main>", async () => {
    await request(
      "preview",
      { lastScan: { ...RESULT, cleanedAt: 1_800_000_100_000 } },
      { focusMain: true },
    );
    const dialog = await screen.findByRole("alertdialog", {
      name: es.cleanup.previewTitle,
    });
    closeByEscape(dialog);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(
      screen.getByRole("heading", { level: 1, name: es.cleanup.title }),
    ).toHaveFocus();
  });

  it("apertura manual con Limpiar en Limpieza: Escape devuelve el foco a Limpiar (el elemento que abrio)", async () => {
    await request("preview", { lastScan: RESULT }, { focusMain: true });
    let dialog = await screen.findByRole("alertdialog", {
      name: es.cleanup.previewTitle,
    });
    closeByEscape(dialog);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    const clean = screen.getByRole("button", { name: es.cleanup.clean });
    screen.getByRole("button", { name: es.cleanup.scan }).focus();
    fireEvent.click(clean);
    dialog = await screen.findByRole("alertdialog", {
      name: es.cleanup.previewTitle,
    });
    closeByEscape(dialog);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.getByRole("button", { name: es.cleanup.scan })).toHaveFocus();
  });
});
