import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { SCREENS } from "../../app/navigation";
import { AppProviders } from "../../app/providers/AppProviders";
import { es } from "../../shared/i18n/es";
import { DensityProvider } from "../../shared/ui/density";
import { diskSummary } from "../support/diskFixtures";
import { scanSummary, twoGroups } from "../support/duplicateFixtures";
import { sampleHistory } from "../support/historyFixtures";
import { summary as junkSummary } from "../support/junkFixtures";
import { type BackendState, installBackend } from "../support/mockBackend";
import { sample, snapshot, summary as monitorSummary } from "../support/monitorFixtures";

afterEach(() => {
  cleanup();
  clearMocks();
});

type Mode = "simple" | "advanced";

const RESULTS: Partial<BackendState> = {
  lastScan: junkSummary({ temp_user: { scope: "per_user", itemCount: 3, sizeBytes: 5 * 1024 * 1024 } }),
  diskLast: diskSummary(),
  dupLast: scanSummary(),
  dupGroups: twoGroups(),
  history: sampleHistory(),
  monitorSnapshot: snapshot([sample(1), sample(2)], { summary: monitorSummary() }),
};

const BACKENDS: ReadonlyArray<readonly [string, Partial<BackendState>]> = [
  ["backend vacio", {}],
  ["backend con resultados", RESULTS],
];

/** Botones primarios que se ven: los de un dialogo cerrado no cuentan. */
function visiblePrimaries(container: HTMLElement): Element[] {
  return [...container.querySelectorAll('[data-variant="primary"]')].filter(
    (button) => button.closest("dialog:not([open])") === null,
  );
}

describe("Una sola accion primaria por pantalla (D-13)", () => {
  describe.each(BACKENDS)("%s", (_label, overrides) => {
    it.each(["simple", "advanced"] as const)("cada pantalla registrada tiene como maximo un boton primario en modo %s", async (mode: Mode) => {
      installBackend(overrides);
      for (const screenDef of SCREENS) {
        const { container, unmount } = render(
          <AppProviders>
            <DensityProvider value={mode}>
              <screenDef.Component />
            </DensityProvider>
          </AppProviders>,
        );
        await screen.findAllByRole("heading", { level: 1 });
        // Deja que terminen las lecturas iniciales: un primario aparece recien con los datos.
        await new Promise((resolve) => setTimeout(resolve, 30));
        expect(visiblePrimaries(container).length, `${screenDef.id} (${mode})`).toBeLessThanOrEqual(1);
        unmount();
      }
    });

    it.each(["simple", "advanced"] as const)("Inicio tiene exactamente un primario, Limpiar ahora, en modo %s", async (mode: Mode) => {
      installBackend({ settings: { uiMode: mode, autostartTray: false, language: "es" }, ...overrides });
      const { container } = render(<App />);
      await screen.findByRole("heading", { level: 1, name: es.home.title });
      await waitFor(() => expect(visiblePrimaries(container)).toHaveLength(1));
      expect(visiblePrimaries(container)[0]).toHaveTextContent(es.home.cleanNow);
    });
  });
});
