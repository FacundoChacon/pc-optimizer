import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { es } from "../shared/i18n/es";

describe("App", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("muestra el titulo del catalogo como heading", async () => {
    mockIPC(() => ({ name: "pc-optimizer", version: "0.1.0" }));
    render(<App />);
    expect(await screen.findByRole("heading", { name: es.app.title })).toBeInTheDocument();
  });

  it.each(["0.1.0", "9.9.9"])("muestra la version %s devuelta por el backend", async (version) => {
    mockIPC(() => ({ name: "pc-optimizer", version }));
    render(<App />);
    expect(await screen.findByText(new RegExp(version.replaceAll(".", "\.")))).toBeInTheDocument();
  });

  it("ante fallo del IPC muestra es.app.ipcError sin lanzar", async () => {
    mockIPC(() => {
      throw { kind: "internal", message: "roto" };
    });
    render(<App />);
    expect(await screen.findByText(es.app.ipcError)).toBeInTheDocument();
  });

  it("el contenedor raiz usa las clases de tokens oscuros", async () => {
    mockIPC(() => ({ name: "pc-optimizer", version: "0.1.0" }));
    const { container } = render(<App />);
    await screen.findByRole("heading", { name: es.app.title });
    const root = container.firstElementChild;
    expect(root).toHaveClass("bg-surface");
    expect(root).toHaveClass("text-content");
  });
});
