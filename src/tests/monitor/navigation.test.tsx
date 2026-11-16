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

describe("navegacion: Monitor", () => {
  it("Monitor va entre Duplicados y Configuracion con un icono propio", () => {
    const ids = SCREENS.map((screen) => screen.id);
    expect(ids.indexOf("monitor")).toBe(ids.indexOf("duplicates") + 1);
    expect(ids.indexOf("settings")).toBe(ids.indexOf("monitor") + 1);
    const icons = SCREENS.map((screen) => screen.icon);
    expect(new Set(icons).size).toBe(icons.length);
  });

  it("la entrada abre la pantalla con su encabezado, aria-current y foco en el contenido", async () => {
    installBackend();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    const labels = within(nav).getAllByRole("button").map((button) => button.textContent);
    expect(labels).toEqual([es.nav.home, es.nav.disk, es.nav.cleanup, es.nav.duplicates, es.nav.monitor, es.nav.settings]);
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.monitor }));
    expect(await screen.findByRole("heading", { level: 1, name: es.monitor.title })).toBeInTheDocument();
    expect(within(nav).getByRole("button", { name: es.nav.monitor })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getByRole("button", { name: es.nav.home })).not.toHaveAttribute("aria-current");
    expect(screen.getByRole("main")).toHaveFocus();
  });
});
