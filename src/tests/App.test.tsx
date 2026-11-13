import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { es } from "../shared/i18n/es";
import { installBackend } from "./support/mockBackend";

describe("App", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("muestra el nombre de la app del catalogo y el h1 de Inicio", async () => {
    installBackend();
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: es.home.title })).toBeInTheDocument();
    expect(screen.getByText(es.app.title)).toBeInTheDocument();
  });

  it("Inicio es un estado vacio sin accion primaria", async () => {
    installBackend();
    const { container } = render(<App />);
    expect(await screen.findByText(es.home.emptyTitle)).toBeInTheDocument();
    expect(screen.getByText(es.home.emptyBody)).toBeInTheDocument();
    expect(container.querySelector('[data-variant="primary"]')).toBeNull();
  });

  it.each(["0.1.0", "9.9.9"])("muestra la version %s devuelta por el backend", async (version) => {
    installBackend({ appInfo: { name: "pc-optimizer", version } });
    render(<App />);
    expect(await screen.findByText(new RegExp(version.replaceAll(".", "\\.")))).toBeInTheDocument();
  });

  it("ante fallo del IPC muestra es.app.ipcError sin lanzar", async () => {
    installBackend({ failures: { get_app_info: { kind: "internal", message: "roto" } } });
    render(<App />);
    expect(await screen.findByText(es.app.ipcError)).toBeInTheDocument();
  });

  it("el contenedor raiz usa las clases de tokens oscuros", async () => {
    installBackend();
    const { container } = render(<App />);
    await screen.findByRole("heading", { level: 1, name: es.home.title });
    const root = container.firstElementChild;
    expect(root).toHaveClass("bg-surface");
    expect(root).toHaveClass("text-content");
  });
});
