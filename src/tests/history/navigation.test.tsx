import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { SCREENS } from "../../app/navigation";
import { es } from "../../shared/i18n/es";
import { installBackend } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const ORDER = [es.nav.home, es.nav.disk, es.nav.cleanup, es.nav.duplicates, es.nav.history, es.nav.monitor, es.nav.settings];

describe("navegacion: Historial", () => {
  it("el registro tiene 7 entradas con Historial entre Duplicados y Monitor y un icono propio", () => {
    expect(SCREENS.map((screen) => screen.id)).toEqual([
      "home",
      "disk",
      "cleanup",
      "duplicates",
      "history",
      "monitor",
      "settings",
    ]);
    expect(SCREENS.map((screen) => screen.label)).toEqual(ORDER);
    const icons = SCREENS.map((screen) => screen.icon);
    expect(new Set(icons).size).toBe(icons.length);
  });

  it("la entrada muestra el encabezado de Historial y queda como actual", async () => {
    installBackend();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    expect(within(nav).getAllByRole("button").map((button) => button.textContent)).toEqual(ORDER);
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.history }));
    expect(await screen.findByRole("heading", { level: 1, name: es.history.title })).toBeInTheDocument();
    expect(within(nav).getByRole("button", { name: es.nav.history })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getByRole("button", { name: es.nav.home })).not.toHaveAttribute("aria-current");
    expect(screen.getByRole("main")).toHaveFocus();
  });
});
