import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { SCREENS } from "../../app/navigation";
import { AppProviders } from "../../app/providers/AppProviders";
import { storeStatusText } from "../../features/settings/storeStatusText";
import type { StoreStatus } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { DensityProvider } from "../../shared/ui/density";
import { CORRUPT, type BackendState, installBackend, type MockBackend } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

async function openSettings(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend(overrides);
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.settings }));
  await screen.findByRole("heading", { level: 1, name: es.settings.title });
  return backend;
}

const section = (heading: string) => screen.getByRole("region", { name: heading });
const advanced = { uiMode: "advanced", autostartTray: false, language: "es" } as const;
const MIGRATION: StoreStatus = { status: "unavailable", reason: "migration_failed", recoverable: true };
const TOO_NEW: StoreStatus = { status: "unavailable", reason: "schema_too_new", recoverable: false };
const GENERIC: StoreStatus = { status: "unavailable", reason: "unavailable", recoverable: false };

describe("SettingsScreen y AboutSection (9.1)", () => {
  it("muestra h1 y las cuatro secciones", async () => {
    await openSettings();
    for (const heading of [
      es.settings.appearance.heading,
      es.settings.autostart.heading,
      es.settings.database.heading,
      es.settings.about.heading,
    ]) {
      expect(section(heading)).toBeInTheDocument();
    }
  });

  it.each(["0.1.0", "2.3.4"])("Acerca de muestra nombre y la version %s del backend", async (version) => {
    await openSettings({ appInfo: { name: "pc-optimizer", version } });
    const about = section(es.settings.about.heading);
    expect(within(about).getByText(es.app.title)).toBeInTheDocument();
    expect(within(about).getByText(version)).toBeInTheDocument();
  });
});

describe("AppearanceSection (9.2)", () => {
  it("con uiMode advanced la opcion Avanzado esta seleccionada", async () => {
    await openSettings({ settings: advanced });
    expect(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.advanced) })).toBeChecked();
  });

  it("elegir Avanzado pasa al layout avanzado, conserva Configuracion y anuncia; Simple vuelve", async () => {
    await openSettings();
    expect(screen.queryByRole("complementary")).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.advanced) }));
    expect(await screen.findByRole("complementary")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: es.settings.title })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(es.settings.appearance.announceAdvanced);
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.advanced) })).toHaveFocus(),
    );
    fireEvent.click(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.simple) }));
    await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
    expect(screen.getByRole("heading", { level: 1, name: es.settings.title })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(es.settings.appearance.announceSimple);
  });

  it("durante la escritura las opciones estan deshabilitadas", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const backend = await openSettings();
    backend.state.gates.set_setting = gate;
    fireEvent.click(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.advanced) }));
    await waitFor(() => {
      for (const radio of screen.getAllByRole("radio")) expect(radio).toBeDisabled();
    });
    await act(async () => {
      release();
      await gate;
    });
    await waitFor(() => expect(screen.getByRole("complementary")).toBeInTheDocument());
  });

  it("error store_unavailable deja el modo simple y muestra un alert con el texto del catalogo", async () => {
    const backend = await openSettings();
    backend.state.failures.set_setting = { kind: "store_unavailable", message: "SQL secreto" };
    fireEvent.click(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.advanced) }));
    const alert = await screen.findByText(errorMessage({ kind: "store_unavailable", message: "" }));
    expect(alert).toHaveAttribute("role", "alert");
    expect(screen.queryByText(/SQL secreto/)).toBeNull();
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.simple) })).toBeChecked();
  });
});

describe("AutostartSection (9.3)", () => {
  it("el switch esta apagado, aria-disabled, enfocable y descrito; ninguna accion llama set_setting", async () => {
    const backend = await openSettings();
    const sw = screen.getByRole("switch", { name: es.settings.autostart.label });
    expect(sw).toHaveAttribute("aria-checked", "false");
    expect(sw).toHaveAttribute("aria-disabled", "true");
    expect(sw).not.toBeDisabled();
    expect(sw).toHaveAccessibleDescription(es.settings.autostart.unavailable);
    sw.focus();
    expect(sw).toHaveFocus();
    fireEvent.click(sw);
    fireEvent.keyDown(sw, { key: "Enter" });
    fireEvent.keyDown(sw, { key: " " });
    expect(backend.count("set_setting")).toBe(0);
    expect(sw).toHaveAttribute("aria-checked", "false");
  });
});

describe("DatabaseSection estado (9.4)", () => {
  it("storeStatusText cubre los cuatro motivos y ready", () => {
    expect(storeStatusText({ status: "ready" })).toMatchObject({ tone: "success", canRecover: false });
    expect(storeStatusText(CORRUPT)).toMatchObject({ tone: "danger", canRecover: true, explanation: es.store.reasons.corrupt });
    expect(storeStatusText(MIGRATION)).toMatchObject({ canRecover: true, explanation: es.store.reasons.migration_failed });
    expect(storeStatusText(TOO_NEW)).toMatchObject({ canRecover: false, explanation: es.store.reasons.schema_too_new });
    expect(storeStatusText(GENERIC)).toMatchObject({ canRecover: false, explanation: es.store.reasons.unavailable });
  });

  it("ready muestra Lista sin boton de recuperar", async () => {
    await openSettings();
    const db = section(es.settings.database.heading);
    expect(await within(db).findByText(es.settings.database.ready)).toBeInTheDocument();
    expect(within(db).queryByRole("button")).toBeNull();
  });

  it.each([
    ["corrupt", CORRUPT, true],
    ["migration_failed", MIGRATION, true],
    ["schema_too_new", TOO_NEW, false],
    ["unavailable", GENERIC, false],
  ] as const)("motivo %s: No disponible + explicacion y boton=%s", async (reason, store, hasButton) => {
    await openSettings({ store });
    const db = section(es.settings.database.heading);
    expect(await within(db).findByText(es.settings.database.unavailable)).toBeInTheDocument();
    expect(within(db).getByText(es.store.reasons[reason])).toBeInTheDocument();
    const button = within(db).queryByRole("button", { name: es.settings.database.recoverAction });
    expect(button !== null).toBe(hasButton);
  });
});

describe("Flujo de recuperacion (9.5)", () => {
  const recoverButton = () =>
    within(section(es.settings.database.heading)).getByRole("button", { name: es.settings.database.recoverAction });

  it("abrir el dialogo pone el foco en Cancelar; Cancelar no invoca IPC y devuelve el foco", async () => {
    const backend = await openSettings({ store: CORRUPT });
    await within(section(es.settings.database.heading)).findByText(es.settings.database.unavailable);
    const trigger = recoverButton();
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("alertdialog", { name: es.settings.database.confirmTitle });
    expect(dialog).toHaveAccessibleDescription(es.settings.database.confirmBody);
    expect(within(dialog).getByRole("button", { name: es.settings.database.cancel })).toHaveFocus();
    fireEvent.click(within(dialog).getByRole("button", { name: es.settings.database.cancel }));
    expect(backend.count("recover_store")).toBe(0);
    expect(trigger).toHaveFocus();
  });

  it("Escape cierra sin invocar recover_store", async () => {
    const backend = await openSettings({ store: CORRUPT });
    await within(section(es.settings.database.heading)).findByText(es.settings.database.unavailable);
    const trigger = recoverButton();
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent(document.querySelector("dialog") as HTMLDialogElement, new Event("cancel", { cancelable: true }));
    expect(backend.count("recover_store")).toBe(0);
    expect(trigger).toHaveFocus();
    expect(document.querySelector("dialog")).not.toHaveAttribute("open");
  });

  it("Confirmar invoca una vez, deshabilita mientras corre y al terminar anuncia, limpia banner y habilita modo", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const backend = await openSettings({
      store: CORRUPT,
      gates: { recover_store: gate },
      failures: { get_settings: { kind: "store_corrupt", message: "x" } },
    });
    await within(section(es.settings.database.heading)).findByText(es.settings.database.unavailable);
    expect(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.simple) })).toBeDisabled();
    fireEvent.click(recoverButton());
    const confirm = within(screen.getByRole("alertdialog")).getByRole("button", {
      name: es.settings.database.confirmAction,
    });
    fireEvent.click(confirm);
    await waitFor(() => expect(confirm).toBeDisabled());
    fireEvent.click(confirm);
    await act(async () => {
      release();
      await gate;
    });
    await waitFor(() => expect(backend.count("recover_store")).toBe(1));
    const db = section(es.settings.database.heading);
    expect(await within(db).findByText(es.settings.database.ready)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      format(es.settings.database.recovered, { file: "data.db.corrupt-1700000000000.bak" }),
    );
    expect(screen.queryByText(es.store.bannerTitle)).toBeNull();
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.simple) })).toBeEnabled(),
    );
  });

  it("error store_unavailable muestra alert de reintento y el boton sigue disponible", async () => {
    const backend = await openSettings({
      store: CORRUPT,
      failures: { recover_store: { kind: "store_unavailable", message: "x" } },
    });
    await within(section(es.settings.database.heading)).findByText(es.settings.database.unavailable);
    fireEvent.click(recoverButton());
    fireEvent.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: es.settings.database.confirmAction }),
    );
    const retry = await screen.findByText(es.settings.database.recoverRetry);
    expect(retry).toHaveAttribute("role", "alert");
    expect(backend.count("recover_store")).toBe(1);
    expect(recoverButton()).toBeEnabled();
  });
});

describe("Controles sin persistencia (9.6)", () => {
  it("con base corrupta y get_settings fallido el selector muestra Simple, deshabilitado y descrito", async () => {
    const backend = await openSettings({
      store: CORRUPT,
      failures: { get_settings: { kind: "store_corrupt", message: "x" } },
    });
    await within(section(es.settings.database.heading)).findByText(es.settings.database.unavailable);
    const simple = screen.getByRole("radio", { name: new RegExp(es.settings.appearance.simple) });
    expect(simple).toBeChecked();
    for (const radio of screen.getAllByRole("radio")) {
      expect(radio).toBeDisabled();
      expect(radio).toHaveAccessibleDescription(es.settings.appearance.needsRecovery);
    }
    fireEvent.click(screen.getByRole("radio", { name: new RegExp(es.settings.appearance.advanced) }));
    expect(backend.count("set_setting")).toBe(0);
  });
});

describe("Accion primaria unica (9.7)", () => {
  it.each(["simple", "advanced"] as const)("cada pantalla registrada tiene como maximo una accion primaria en modo %s", async (mode) => {
    installBackend({ store: CORRUPT });
    for (const screenDef of SCREENS) {
      const { container, unmount } = render(
        <AppProviders>
          <DensityProvider value={mode}>
            <screenDef.Component />
          </DensityProvider>
        </AppProviders>,
      );
      await screen.findAllByRole("heading", { level: 1 });
      expect(container.querySelectorAll('[data-variant="primary"]').length, screenDef.id).toBeLessThanOrEqual(1);
      unmount();
    }
  });
});
