import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { entryDetailText, runPhrase } from "../../features/history/historyText";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import {
  duplicatesRun,
  emptyBinRun,
  historyEntries,
  historyEntry,
  historyOf,
  junkRun,
  sampleHistory,
} from "../support/historyFixtures";
import { type BackendState, installBackend, type MockBackend, OTHER_USER } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const ADVANCED = { uiMode: "advanced", autostartTray: false, language: "es" } as const;
const text = es.history;

async function openHistory(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend(overrides);
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.history }));
  await screen.findByRole("heading", { level: 1, name: text.title });
  return backend;
}

const list = () => screen.findByRole("list", { name: text.listLabel });

describe("Historial en modo simple", () => {
  it("lista las limpiezas con fecha y frase llana, sin rutas ni codigos y sin boton primario", async () => {
    await openHistory({ history: sampleHistory() });
    const items = within(await list()).getAllByRole("listitem");
    expect(items).toHaveLength(3);
    const [empty, dups, junk] = [emptyBinRun(30), duplicatesRun(20), junkRun(10)];
    expect(items[0]).toHaveTextContent(runPhrase(empty));
    expect(items[0]).toHaveTextContent(formatDateTime(empty.finishedAt));
    expect(items[1]).toHaveTextContent(runPhrase(dups));
    expect(items[2]).toHaveTextContent(runPhrase(junk));
    expect(items[2]).toHaveTextContent("340");
    expect(items[2]).toHaveTextContent(formatBytes(junk.movedBytes));
    const main = screen.getByRole("main").textContent ?? "";
    expect(main).not.toMatch(/[A-Za-z]:\\/);
    expect(main).not.toMatch(/\b[a-z]+_[a-z_]+\b/);
    expect(document.body.querySelector('[data-variant="primary"]')).toBeNull();
    expect(screen.queryByRole("button", { name: text.advanced.viewFiles })).toBeNull();
  });

  it("historial vacio: Todavia no limpiaste nada con la app", async () => {
    await openHistory({ history: [] });
    expect(await screen.findByText(text.empty)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: text.listLabel })).toBeNull();
    expect(screen.queryByRole("button", { name: text.loadMore })).toBeNull();
  });

  it("Ver mas agrega la pagina siguiente sin repetir y desaparece al llegar al final", async () => {
    const runs = Array.from({ length: 25 }, (_, index) => historyOf(junkRun(index + 1)));
    const backend = await openHistory({ history: runs });
    expect(within(await list()).getAllByRole("listitem")).toHaveLength(20);
    fireEvent.click(screen.getByRole("button", { name: text.loadMore }));
    await waitFor(() => expect(within(screen.getByRole("list", { name: text.listLabel })).getAllByRole("listitem")).toHaveLength(25));
    expect(screen.queryByRole("button", { name: text.loadMore })).toBeNull();
    const calls = backend.calls.filter((call) => call.cmd === "list_cleanup_runs");
    expect(calls).toHaveLength(2);
    expect((calls[1]?.args as { before: unknown }).before).toEqual({ finishedAt: junkRun(6).finishedAt, key: { kind: "scan", scanId: 6 } });
  });

  it("error store_unavailable: mensaje del catalogo y sin el texto tecnico", async () => {
    await openHistory({ failures: { list_cleanup_runs: { kind: "store_unavailable", message: "sqlite exploto" } } });
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.store_unavailable);
    expect(screen.queryByText(/sqlite exploto/)).toBeNull();
  });
});

describe("Historial en modo avanzado", () => {
  const big = historyOf(junkRun(1, { entryCount: 150 }), historyEntries(150));

  async function openAdvanced(overrides: Partial<BackendState> = {}) {
    return openHistory({ settings: ADVANCED, ...overrides });
  }

  it("cada limpieza muestra sus conteos por resultado con el codigo", async () => {
    await openAdvanced({ history: sampleHistory() });
    const items = within(await list()).getAllByRole("listitem");
    const junk = items[2];
    expect(junk).toBeDefined();
    const label = format(text.advanced.codeLabel, { text: text.outcomes.moved_to_trash, code: "moved_to_trash" });
    expect(junk).toHaveTextContent(format(text.advanced.outcomeCount, { label, count: "340" }));
    const locked = format(text.advanced.codeLabel, { text: text.outcomes.skipped_locked, code: "skipped_locked" });
    expect(junk).toHaveTextContent(format(text.advanced.outcomeCount, { label: locked, count: "3" }));
  });

  it("desplegar una limpieza de 150 items muestra 100 filas con la ruta completa y Cargar mas agrega 50", async () => {
    const backend = await openAdvanced({ history: [big] });
    const toggle = await screen.findByRole("button", { name: text.advanced.viewFiles });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    const table = await screen.findByRole("table", { name: text.advanced.entriesLabel });
    expect(screen.getByRole("button", { name: text.advanced.hideFiles })).toHaveAttribute("aria-expanded", "true");
    expect(within(table).getAllByRole("row")).toHaveLength(1 + 100);
    expect(within(table).getByText(historyEntry(1).path ?? "")).toBeInTheDocument();
    expect(backend.calls.find((call) => call.cmd === "list_cleanup_entries")?.args).toEqual({
      key: { kind: "scan", scanId: 1 },
      offset: 0,
      limit: 100,
    });
    fireEvent.click(screen.getByRole("button", { name: text.advanced.loadMoreEntries }));
    await waitFor(() => expect(within(table).getAllByRole("row")).toHaveLength(1 + 150));
    expect(screen.queryByRole("button", { name: text.advanced.loadMoreEntries })).toBeNull();
  });

  it("cada Ver archivos se describe con la fecha y la frase de su limpieza para distinguirlos", async () => {
    await openAdvanced({ history: sampleHistory() });
    const buttons = await screen.findAllByRole("button", { name: text.advanced.viewFiles });
    expect(buttons[2]).toHaveAccessibleDescription(`${formatDateTime(junkRun(10).finishedAt)} ${runPhrase(junkRun(10))}`);
    expect(buttons[0]).toHaveAccessibleDescription(`${formatDateTime(emptyBinRun(30).finishedAt)} ${runPhrase(emptyBinRun(30))}`);
  });

  it("plegar y volver a desplegar no duplica filas", async () => {
    await openAdvanced({ history: [historyOf(junkRun(1), historyEntries(5))] });
    fireEvent.click(await screen.findByRole("button", { name: text.advanced.viewFiles }));
    const table = await screen.findByRole("table", { name: text.advanced.entriesLabel });
    await waitFor(() => expect(within(table).getAllByRole("row")).toHaveLength(6));
    fireEvent.click(screen.getByRole("button", { name: text.advanced.hideFiles }));
    expect(screen.queryByRole("table", { name: text.advanced.entriesLabel })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: text.advanced.viewFiles }));
    const again = await screen.findByRole("table", { name: text.advanced.entriesLabel });
    await waitFor(() => expect(within(again).getAllByRole("row")).toHaveLength(6));
  });

  it("resultado y detalle van junto a su codigo", async () => {
    const entries = [
      historyEntry(1, { outcome: "skipped_locked", errorDetail: "in_use", sizeBytes: 2048 }),
      historyEntry(2),
    ];
    await openAdvanced({ history: [historyOf(junkRun(1), entries)] });
    fireEvent.click(await screen.findByRole("button", { name: text.advanced.viewFiles }));
    const table = await screen.findByRole("table", { name: text.advanced.entriesLabel });
    const rows = await waitFor(() => {
      const found = within(table).getAllByRole("row");
      expect(found).toHaveLength(3);
      return found;
    });
    const first = rows[1];
    expect(first).toHaveTextContent(format(text.advanced.codeLabel, { text: text.outcomes.skipped_locked, code: "skipped_locked" }));
    expect(first).toHaveTextContent(`${entryDetailText("skipped_locked", "in_use") ?? ""} (in_use)`);
    expect(first).toHaveTextContent(formatBytes(2048));
    expect(first).toHaveTextContent(`${es.cleanup.categoryNames.temp_user} (temp_user)`);
    expect(rows[2]).toHaveTextContent(text.advanced.noDetail);
  });

  it("con rutas ocultas la columna lo explica y hay un aviso sobre la tabla", async () => {
    await openAdvanced({ history: [historyOf(junkRun(1), historyEntries(3))], privilege: OTHER_USER });
    fireEvent.click(await screen.findByRole("button", { name: text.advanced.viewFiles }));
    const table = await screen.findByRole("table", { name: text.advanced.entriesLabel });
    await waitFor(() => expect(within(table).getAllByText(text.advanced.pathHidden)).toHaveLength(3));
    expect(screen.getByText(text.advanced.pathsHiddenNotice)).toBeInTheDocument();
    expect(within(table).queryByText(/archivo-1\.tmp/)).toBeNull();
  });

  it("un vaciado de la Papelera tambien se puede desplegar", async () => {
    await openAdvanced({ history: sampleHistory() });
    expect(await screen.findAllByRole("button", { name: text.advanced.viewFiles })).toHaveLength(3);
  });

  it("error al leer los items muestra el mensaje del catalogo", async () => {
    await openAdvanced({
      history: [historyOf(junkRun(1), historyEntries(2))],
      failures: { list_cleanup_entries: { kind: "store_unavailable", message: "x" } },
    });
    fireEvent.click(await screen.findByRole("button", { name: text.advanced.viewFiles }));
    expect(await screen.findByText(es.errors.store_unavailable)).toBeInTheDocument();
  });
});
