import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { SCREENS } from "../../app/navigation";
import { es } from "../../shared/i18n/es";
import { CORRUPT, installBackend, OTHER_USER } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const advanced = { uiMode: "advanced", autostartTray: false, language: "es" } as const;

describe("Shell gate de carga (8.1)", () => {
  it("con get_settings pendiente solo se ve la carga y no hay nav", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    installBackend({ gates: { get_settings: gate } });
    render(<App />);
    expect(screen.getByText(es.app.loading)).toBeInTheDocument();
    expect(screen.queryByRole("navigation")).toBeNull();
    expect(screen.queryByRole("complementary")).toBeNull();
    await act(async () => {
      release();
      await gate;
    });
    expect(await screen.findByRole("navigation")).toBeInTheDocument();
  });

  it("con modo avanzado aparece directo el layout avanzado, nunca el simple antes", async () => {
    installBackend({ settings: advanced });
    const { container } = render(<App />);
    expect(container.querySelector('[data-density="simple"]')).toBeNull();
    expect(await screen.findByRole("complementary")).toBeInTheDocument();
    expect(container.querySelector('[data-density="simple"]')).toBeNull();
    expect(container.querySelector('[data-density="advanced"]')).not.toBeNull();
  });

  it("con get_settings fallido aparece el layout simple", async () => {
    installBackend({ failures: { get_settings: { kind: "store_corrupt", message: "x" } } });
    const { container } = render(<App />);
    expect(await screen.findByRole("navigation")).toBeInTheDocument();
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(container.querySelector('[data-density="simple"]')).not.toBeNull();
  });
});

describe("Layouts y NavList (8.2)", () => {
  it("simple: nav en la cabecera, sin aside, entrada actual con aria-current", async () => {
    installBackend();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(screen.getByRole("banner")).toContainElement(nav);
    const home = screen.getByRole("button", { name: es.nav.home });
    const settings = screen.getByRole("button", { name: es.nav.settings });
    expect(home).toHaveAttribute("aria-current", "page");
    expect(settings).not.toHaveAttribute("aria-current");
    expect(SCREENS.map((s) => s.label)).toEqual([es.nav.home, es.nav.cleanup, es.nav.settings]);
  });

  it("avanzado: aside con nav y las mismas entradas, layout responsivo", async () => {
    installBackend({ settings: advanced });
    const { container } = render(<App />);
    const aside = await screen.findByRole("complementary");
    const nav = screen.getByRole("navigation", { name: es.nav.label });
    expect(aside).toContainElement(nav);
    expect(screen.getByRole("button", { name: es.nav.home })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: es.nav.cleanup })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: es.nav.settings })).toBeInTheDocument();
    const root = container.firstElementChild;
    expect(root).toHaveAttribute("data-density", "advanced");
    expect(root?.querySelector(".flex-col.md\\:flex-row")).not.toBeNull();
  });

  it.each([
    ["simple", undefined],
    ["advanced", advanced],
  ] as const)("modo %s conserva los tokens oscuros en la raiz y muestra nombre y version", async (_mode, settings) => {
    installBackend({ ...(settings === undefined ? {} : { settings }), appInfo: { name: "pc-optimizer", version: "9.9.9" } });
    const { container } = render(<App />);
    await screen.findByRole("navigation");
    expect(container.firstElementChild).toHaveClass("bg-surface", "text-content");
    expect(await screen.findByText(/9\.9\.9/)).toBeInTheDocument();
    expect(screen.getByText(es.app.title)).toBeInTheDocument();
  });

  it("error de get_app_info muestra el texto del catalogo", async () => {
    installBackend({ failures: { get_app_info: { kind: "internal", message: "roto" } } });
    render(<App />);
    expect(await screen.findByText(es.app.ipcError)).toBeInTheDocument();
  });
});

describe("SkipLink y foco (8.3)", () => {
  it("es el primer enfocable, usa el catalogo y lleva el foco a main", async () => {
    installBackend();
    const { container } = render(<App />);
    await screen.findByRole("navigation");
    const focusable = container.querySelectorAll<HTMLElement>("a[href], button, input, [tabindex]:not([tabindex='-1'])");
    const skip = screen.getByRole("link", { name: es.a11y.skipToContent });
    expect(focusable[0]).toBe(skip);
    fireEvent.click(skip);
    const main = screen.getByRole("main");
    expect(main).toHaveAttribute("id", "main");
    expect(main).toHaveAttribute("tabindex", "-1");
    expect(main).toHaveFocus();
  });

  it("activar una entrada de navegacion cambia de pantalla y mueve el foco", async () => {
    installBackend();
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    const settings = screen.getByRole("button", { name: es.nav.settings });
    settings.focus();
    fireEvent.click(settings);
    expect(await screen.findByRole("heading", { level: 1, name: es.settings.title })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
    expect(screen.getByRole("button", { name: es.nav.settings })).toHaveAttribute("aria-current", "page");
  });

  it("Limpieza queda entre Inicio y Configuracion y se abre desde la navegacion", async () => {
    installBackend();
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    const labels = screen.getAllByRole("button").map((button) => button.textContent);
    const expected: string[] = [es.nav.home, es.nav.cleanup, es.nav.settings];
    const nav = labels.filter((text) => expected.includes(text ?? ""));
    expect(nav).toEqual([es.nav.home, es.nav.cleanup, es.nav.settings]);
    fireEvent.click(screen.getByRole("button", { name: es.nav.cleanup }));
    expect(await screen.findByRole("heading", { level: 1, name: es.cleanup.title })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: es.nav.cleanup })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("main")).toHaveFocus();
  });
});

describe("StoreBanner (8.4)", () => {
  it("con base corrupta se muestra en Inicio y en Configuracion y navega a Configuracion", async () => {
    installBackend({ store: CORRUPT });
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(es.store.bannerTitle);
    expect(alert).toHaveTextContent(es.store.reasons.corrupt);
    expect(alert.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    fireEvent.click(screen.getByRole("button", { name: es.store.bannerAction }));
    expect(await screen.findByRole("heading", { level: 1, name: es.settings.title })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(es.store.bannerTitle);
  });

  it("con base lista no existe", async () => {
    installBackend();
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    await waitFor(() => expect(screen.queryByText(es.store.bannerTitle)).toBeNull());
  });

  it("cada motivo usa su texto del catalogo", async () => {
    installBackend({ store: { status: "unavailable", reason: "schema_too_new", recoverable: false } });
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent(es.store.reasons.schema_too_new);
  });
});

describe("PrivilegeBanner (6.3)", () => {
  it("different_user: aviso con icono y texto, en Inicio y Configuracion, en ambos modos, sin cierre", async () => {
    installBackend({ privilege: OTHER_USER });
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(es.privilege.title);
    expect(alert).toHaveTextContent(es.privilege.reasons.different_user);
    expect(alert.querySelector('[data-tone="warning"] svg')).toHaveAttribute("aria-hidden", "true");
    expect(alert.querySelector("button")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: es.nav.settings }));
    expect(await screen.findByRole("heading", { level: 1, name: es.settings.title })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(es.privilege.title);
    fireEvent.click(await screen.findByRole("radio", { name: new RegExp(es.settings.appearance.advanced) }));
    await screen.findByRole("complementary");
    expect(screen.getByRole("alert")).toHaveTextContent(es.privilege.title);
  });

  it("undetermined: muestra el texto de no se pudo verificar", async () => {
    installBackend({ privilege: { status: "mismatch", reason: "undetermined", elevated: false } });
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent(es.privilege.reasons.undetermined);
  });

  it("match: no hay aviso de privilegios", async () => {
    installBackend();
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    await waitFor(() => expect(screen.queryByText(es.privilege.title)).toBeNull());
  });

  it("si get_privilege_context falla, se muestra el aviso undetermined", async () => {
    installBackend({ failures: { get_privilege_context: { kind: "internal", message: "x" } } });
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent(es.privilege.reasons.undetermined);
  });

  it("con base corrupta y mismatch se ven ambos avisos", async () => {
    installBackend({ store: CORRUPT, privilege: OTHER_USER });
    render(<App />);
    await waitFor(() => expect(screen.getAllByRole("alert")).toHaveLength(2));
    expect(screen.getByText(es.store.bannerTitle)).toBeInTheDocument();
    expect(screen.getByText(es.privilege.title)).toBeInTheDocument();
  });
});
