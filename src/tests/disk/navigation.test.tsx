import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { DEFAULT_SCREEN, SCREENS } from "../../app/navigation";
import { es } from "../../shared/i18n/es";
import { installBackend } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("navegacion: Espacio en disco", () => {
  it("el registro tiene 6 entradas con Espacio en disco entre Inicio y Limpieza", () => {
    expect(SCREENS.map((screen) => screen.id)).toEqual(["home", "disk", "cleanup", "duplicates", "monitor", "settings"]);
    expect(SCREENS.map((screen) => screen.label)).toEqual([
      es.nav.home,
      es.nav.disk,
      es.nav.cleanup,
      es.nav.duplicates,
      es.nav.monitor,
      es.nav.settings,
    ]);
    expect(DEFAULT_SCREEN).toBe("home");
  });

  it("la entrada lleva a la pantalla, queda como actual y su encabezado recibe el foco del contenido", async () => {
    installBackend();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    const labels = within(nav).getAllByRole("button").map((button) => button.textContent);
    expect(labels).toEqual([es.nav.home, es.nav.disk, es.nav.cleanup, es.nav.duplicates, es.nav.monitor, es.nav.settings]);
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
    expect(await screen.findByRole("heading", { level: 1, name: es.disk.title })).toBeInTheDocument();
    expect(within(nav).getByRole("button", { name: es.nav.disk })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getByRole("button", { name: es.nav.home })).not.toHaveAttribute("aria-current");
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("se puede volver a Inicio y la pantalla de disco se conserva al navegar", async () => {
    installBackend();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
    await screen.findByRole("heading", { level: 1, name: es.disk.title });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.home }));
    expect(await screen.findByRole("heading", { level: 1, name: es.home.title })).toBeInTheDocument();
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
    expect(await screen.findByRole("heading", { level: 1, name: es.disk.title })).toBeInTheDocument();
  });
});
