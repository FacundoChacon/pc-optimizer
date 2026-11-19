import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import type { CleanupEvent, DuplicateScanEvent } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime, formatPlural } from "../../shared/i18n/format";
import {
  duplicatePreview,
  duplicateSummary,
  file,
  group,
  MODIFIED,
  roots,
  scanSummary,
  twoGroups,
} from "../support/duplicateFixtures";
import { type BackendState, installBackend, type MockBackend, OTHER_USER } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const advancedSettings = { uiMode: "advanced", autostartTray: false, language: "es" } as const;
const t = es.duplicates;

async function openDuplicates(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend(overrides);
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.duplicates }));
  await screen.findByRole("heading", { level: 1, name: t.title });
  return backend;
}

/** Con un resultado previo ya cargado y los dos grupos habituales. */
async function openWithGroups(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = await openDuplicates({
    dupLast: scanSummary(),
    dupGroups: twoGroups(),
    ...overrides,
  });
  await screen.findByRole("region", { name: "foto.jpg" });
  return backend;
}

function callsOf(backend: MockBackend, cmd: string) {
  return backend.calls.filter((call) => call.cmd === cmd);
}

const scanButton = () => screen.getByRole("button", { name: /Buscar (duplicados|de nuevo)/ });
const reviewButton = (count = 1) =>
  screen.getByRole("button", {
    name: count === 1 ? t.reviewOne : format(t.review, { count: String(count) }),
  });
const photo = () => screen.getByRole("region", { name: "foto.jpg" });
const video = () => screen.getByRole("region", { name: "video.mp4" });

function gate(): { gate: Promise<void>; release: () => void } {
  let release: () => void = () => undefined;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { gate: promise, release };
}

describe("Duplicados: navegacion y estado inicial", () => {
  it("la entrada Duplicados lleva a la pantalla, queda como actual y tiene su encabezado", async () => {
    await openDuplicates();
    const nav = screen.getByRole("navigation", { name: es.nav.label });
    expect(within(nav).getByRole("button", { name: es.nav.duplicates })).toHaveAttribute("aria-current", "page");
    const labels = within(nav).getAllByRole("button").map((button) => button.textContent);
    expect(labels).toEqual([es.nav.home, es.nav.cleanup, es.nav.duplicates, es.nav.settings]);
  });

  it("sin resultados 'Buscar duplicados' es la unica accion primaria y explica que no se mueve nada", async () => {
    await openDuplicates();
    expect(scanButton()).toHaveAttribute("data-variant", "primary");
    expect(document.body.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);
    expect(screen.getByText(/No se mueve nada sin que elijas/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Revisar y mover/ })).toBeNull();
  });

  it("al montar lee el ultimo escaneo y muestra el resumen y los grupos", async () => {
    const backend = await openWithGroups();
    expect(callsOf(backend, "get_last_duplicate_scan")).toHaveLength(1);
    expect(
      screen.getByText(
        format(t.resultSummary, { count: "2", size: formatBytes(scanSummary().reclaimableBytes) }),
      ),
    ).toBeInTheDocument();
    expect(screen.getByText(format(t.lastScan, { date: formatDateTime(1_800_000_005_000) }))).toBeInTheDocument();
  });

  it("un error al leer el ultimo escaneo se muestra en espanol sin el mensaje tecnico", async () => {
    await openDuplicates({
      failures: { get_last_duplicate_scan: { kind: "store_unavailable", message: "SQLITE_BUSY" } },
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.store_unavailable);
    expect(screen.queryByText(/SQLITE_BUSY/)).toBeNull();
  });

  it("en mismatch reemplaza el boton por el motivo y no ofrece buscar", async () => {
    await openDuplicates({ privilege: OTHER_USER });
    expect(await screen.findByRole("region", { name: t.blockedTitle })).toHaveTextContent(
      t.blockedReasons.different_user,
    );
    expect(screen.queryByRole("button", { name: t.scan })).toBeNull();
  });
});

describe("Duplicados: escaneo", () => {
  it("en simple busca sin carpetas ni tamano, muestra el progreso por etapa y termina con el resultado", async () => {
    const hold = gate();
    const backend = await openDuplicates({
      dupScanResult: scanSummary(),
      dupGroups: twoGroups(),
      gates: { start_duplicate_scan: hold.gate },
    });
    fireEvent.click(scanButton());
    const panel = await screen.findByRole("region", { name: t.progressTitle });
    expect(panel).toHaveAttribute("aria-live", "polite");
    expect(callsOf(backend, "start_duplicate_scan")[0]?.args).toEqual({
      onProgress: expect.anything(),
    });

    const events: DuplicateScanEvent[] = [
      { type: "started", scanId: 1 },
      { type: "walking", filesSeen: 256 },
    ];
    await act(async () => backend.emitDuplicates(events));
    expect(within(panel).getByText(formatPlural(t.progressWalking, 256))).toBeInTheDocument();
    await act(async () => backend.emitDuplicates([{ type: "partial", done: 64, total: 130 }]));
    expect(within(panel).getByText(format(t.progressPartial, { done: "64", total: "130" }))).toBeInTheDocument();
    await act(async () => backend.emitDuplicates([{ type: "full", doneBytes: 1_048_576, totalBytes: 4_194_304 }]));
    expect(
      within(panel).getByText(format(t.progressFull, { done: "1 MB", total: "4 MB" })),
    ).toBeInTheDocument();

    hold.release();
    expect(await screen.findByRole("region", { name: t.resultTitle })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: t.progressTitle })).toBeNull();
    // Al terminar el foco vuelve al boton de buscar.
    await waitFor(() => expect(scanButton()).toHaveFocus());
  });

  it("Cancelar pide cancelar el escaneo por su id y avisa que se cancelo", async () => {
    const hold = gate();
    const backend = await openDuplicates({
      dupScanResult: scanSummary({ status: "cancelled", roots: [], groupCount: 0, openGroupCount: 0 }),
      gates: { start_duplicate_scan: hold.gate },
    });
    fireEvent.click(scanButton());
    await screen.findByRole("region", { name: t.progressTitle });
    await act(async () => backend.emitDuplicates([{ type: "started", scanId: 9 }]));
    fireEvent.click(screen.getByRole("button", { name: t.cancel }));
    await waitFor(() => expect(callsOf(backend, "cancel_scan")[0]?.args).toEqual({ scanId: 9 }));
    hold.release();
    expect(await screen.findByText(t.cancelledNotice, { selector: "p" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: t.resultTitle })).toBeNull();
  });

  it("cancelar antes de recibir el id espera al evento started y recien ahi cancela", async () => {
    const hold = gate();
    const backend = await openDuplicates({
      dupScanResult: scanSummary({ status: "cancelled", roots: [], groupCount: 0, openGroupCount: 0 }),
      gates: { start_duplicate_scan: hold.gate },
    });
    fireEvent.click(scanButton());
    await screen.findByRole("region", { name: t.progressTitle });
    fireEvent.click(screen.getByRole("button", { name: t.cancel }));
    expect(callsOf(backend, "cancel_scan")).toHaveLength(0);
    await act(async () => backend.emitDuplicates([{ type: "started", scanId: 4 }]));
    await waitFor(() => expect(callsOf(backend, "cancel_scan")[0]?.args).toEqual({ scanId: 4 }));
    hold.release();
  });

  it("un error del backend se muestra en espanol y el boton vuelve a estar disponible", async () => {
    await openDuplicates({ failures: { start_duplicate_scan: { kind: "cleanup_in_progress", message: "x" } } });
    fireEvent.click(scanButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.cleanup_in_progress);
    expect(scanButton()).toBeEnabled();
  });

  it("duplicates_blocked tambien se explica en espanol", async () => {
    await openDuplicates({ failures: { start_duplicate_scan: { kind: "duplicates_blocked", message: "x" } } });
    fireEvent.click(scanButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.duplicates_blocked);
  });

  it("en avanzado elige carpetas (al menos una) y tamano minimo y los envia como claves y valor cerrado", async () => {
    const backend = await openDuplicates({
      settings: advancedSettings,
      dupScanResult: scanSummary({ minSizeBytes: 10_485_760 }),
    });
    const folders = screen.getByRole("group", { name: t.advanced.foldersLegend });
    const boxes = within(folders).getAllByRole("checkbox");
    expect(boxes).toHaveLength(6);
    for (const box of boxes) expect(box).toBeChecked();
    for (const name of ["Documentos", "Música", "Imágenes", "Videos", "Escritorio"]) {
      fireEvent.click(within(folders).getByRole("checkbox", { name }));
    }
    // La ultima carpeta elegida no se puede desmarcar.
    fireEvent.click(within(folders).getByRole("checkbox", { name: "Descargas" }));
    expect(within(folders).getByRole("checkbox", { name: "Descargas" })).toBeChecked();
    fireEvent.change(screen.getByRole("combobox", { name: t.advanced.minSizeLabel }), {
      target: { value: "10485760" },
    });
    const options = within(screen.getByRole("combobox", { name: t.advanced.minSizeLabel })).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual(["100 KB", "1 MB", "10 MB", "100 MB"]);
    fireEvent.click(scanButton());
    await waitFor(() => expect(callsOf(backend, "start_duplicate_scan")).toHaveLength(1));
    expect(callsOf(backend, "start_duplicate_scan")[0]?.args).toEqual({
      roots: ["downloads"],
      minSizeBytes: 10_485_760,
      onProgress: expect.anything(),
    });
  });

  it("explica lo omitido en lenguaje llano y las carpetas no disponibles", async () => {
    await openWithGroups({
      dupLast: scanSummary({
        cloudCount: 3,
        protectedCount: 2,
        hardLinkedCount: 1,
        roots: roots({ videos: { status: "unavailable", reason: "unsupported_volume" } }),
      }),
    });
    expect(screen.getByText(formatPlural(t.skippedCloud, 3))).toBeInTheDocument();
    expect(screen.getByText(formatPlural(t.skippedProtected, 2))).toBeInTheDocument();
    expect(screen.getByText(formatPlural(t.skippedLinks, 1))).toBeInTheDocument();
    expect(
      screen.getByText(format(t.unavailableFolder, { folder: "Videos", reason: t.rootReasons.unsupported_volume })),
    ).toBeInTheDocument();
  });

  it("avanzado muestra el estado por carpeta con su codigo crudo y los omitidos por motivo", async () => {
    await openWithGroups({
      settings: advancedSettings,
      dupLast: scanSummary({
        belowMinSize: 7,
        roots: roots({ videos: { status: "unavailable", reason: "unsupported_volume" }, music: { status: "nested" } }),
      }),
    });
    const table = screen.getByRole("table", { name: t.advanced.rootsCaption });
    const videos = within(table).getByRole("row", { name: new RegExp("Videos") });
    expect(within(videos).getByText("unsupported_volume")).toBeInTheDocument();
    expect(within(videos).getByText(/No disponible: está en otro disco/)).toBeInTheDocument();
    expect(within(table).getByRole("row", { name: new RegExp("Música") })).toHaveTextContent("nested");
    const skipped = screen.getByRole("table", { name: t.advanced.skippedCaption });
    expect(within(skipped).getByRole("row", { name: new RegExp(t.advanced.belowMinSize) })).toHaveTextContent("7");
  });
});

describe("Duplicados: lista de grupos y eleccion", () => {
  it("simple: ninguna copia viene elegida ni recomendada y la revision esta deshabilitada con su motivo", async () => {
    await openWithGroups();
    for (const region of [photo(), video()]) {
      expect(within(region).getAllByRole("radio").every((radio) => !(radio as HTMLInputElement).checked)).toBe(true);
      expect(within(region).getByText(t.question)).toBeInTheDocument();
      expect(within(region).getByText(t.help)).toBeInTheDocument();
    }
    expect(screen.queryByText(/recomendad|más antigu|más reciente/i)).toBeNull();
    expect(screen.queryByText(t.goesToTrash)).toBeNull();
    const review = screen.getByRole("button", { name: t.reviewNone });
    expect(review).toBeDisabled();
    expect(review).toHaveAttribute("aria-describedby");
    expect(screen.getByText(t.reviewDisabledReason)).toHaveAttribute("id", review.getAttribute("aria-describedby"));
    expect(review).toHaveAttribute("data-variant", "primary");
    expect(scanButton()).toHaveAttribute("data-variant", "secondary");
  });

  it("cada copia muestra carpeta, ruta relativa y fecha, y la que no tiene fecha lo dice", async () => {
    await openWithGroups();
    const radios = within(photo()).getAllByRole("radio");
    expect(radios).toHaveLength(2);
    expect(within(photo()).getByText("Imágenes · Viajes\\foto (1).jpg")).toBeInTheDocument();
    expect(within(photo()).getByText("Descargas · foto.jpg")).toBeInTheDocument();
    expect(
      within(photo()).getAllByText(format(t.modified, { date: formatDateTime(MODIFIED) })),
    ).toHaveLength(2);
    expect(within(video()).getByText(t.modifiedUnknown)).toBeInTheDocument();
    expect(within(photo()).queryByText(/C:/)).toBeNull();
  });

  it("al elegir una copia las demas se indican 'Va a la Papelera' y la revision se habilita", async () => {
    await openWithGroups();
    fireEvent.click(within(photo()).getByRole("radio", { name: /Descargas · foto\.jpg/ }));
    expect(within(photo()).getAllByText(new RegExp(t.goesToTrash))).toHaveLength(1);
    // El texto vive dentro de la etiqueta de la copia que va a la Papelera, no de la elegida.
    expect(within(photo()).getByRole("radio", { name: new RegExp(t.goesToTrash) })).toHaveAccessibleName(/Imágenes/);
    expect(reviewButton(1)).toBeEnabled();
    expect(screen.queryByText(t.reviewDisabledReason)).toBeNull();
    // Elegir en el otro grupo suma sus dos copias a mover.
    fireEvent.click(within(video()).getByRole("radio", { name: /Descargas/ }));
    expect(reviewButton(3)).toBeEnabled();
  });

  it("avanzado: casillas sin marcar, ruta completa y hash; conservar 2 de 3 deja una a mover", async () => {
    await openWithGroups({ settings: advancedSettings });
    expect(within(video()).getAllByRole("checkbox").every((box) => !(box as HTMLInputElement).checked)).toBe(true);
    expect(within(video()).getByText("C:\\Users\\ana\\documents\\video.mp4", { exact: false })).toBeInTheDocument();
    const hash = "02".repeat(32);
    expect(within(video()).getByText(hash)).toBeInTheDocument();
    fireEvent.click(within(video()).getByRole("checkbox", { name: /documents/ }));
    fireEvent.click(within(video()).getByRole("checkbox", { name: /downloads/ }));
    expect(reviewButton(1)).toBeEnabled();
  });

  it("avanzado: no deja desmarcar la ultima copia conservada", async () => {
    await openWithGroups({ settings: advancedSettings });
    const box = within(photo()).getAllByRole("checkbox")[0];
    if (box === undefined) throw new Error("sin casilla");
    fireEvent.click(box);
    expect(box).toBeChecked();
    fireEvent.click(box);
    expect(box).toBeChecked();
    expect(screen.getByRole("status")).toHaveTextContent(t.advanced.keepLastHint);
  });

  it("avanzado: conservar todas las copias de un grupo no cuenta como decision", async () => {
    await openWithGroups({ settings: advancedSettings });
    for (const box of within(photo()).getAllByRole("checkbox")) fireEvent.click(box);
    expect(screen.getByRole("button", { name: t.reviewNone })).toBeDisabled();
  });

  it("'Ver más' agrega la pagina siguiente y muestra cuantos se ven", async () => {
    const many = Array.from({ length: 25 }, (_, i) =>
      group(100 + i, [file(1000 + i * 2, "downloads", `g${i}.bin`), file(1001 + i * 2, "pictures", `g${i}.bin`)]),
    );
    const backend = await openDuplicates({ dupLast: scanSummary({ groupCount: 25, openGroupCount: 25 }), dupGroups: many });
    await screen.findByRole("region", { name: "g0.bin" });
    expect(screen.getAllByRole("region", { name: /^g\d+\.bin$/ })).toHaveLength(20);
    expect(screen.getByText(format(t.shown, { shown: "20", total: "25" }))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: t.loadMore }));
    await screen.findByRole("region", { name: "g24.bin" });
    expect(screen.getAllByRole("region", { name: /^g\d+\.bin$/ })).toHaveLength(25);
    expect(screen.queryByRole("button", { name: t.loadMore })).toBeNull();
    expect(callsOf(backend, "list_duplicate_groups").map((c) => c.args)).toEqual([
      { scanId: 1, offset: 0, limit: 20 },
      { scanId: 1, offset: 20, limit: 20 },
    ]);
  });

  it("avanzado: los filtros por carpeta y tamano vuelven a consultar al backend", async () => {
    const backend = await openWithGroups({ settings: advancedSettings });
    fireEvent.change(screen.getByRole("combobox", { name: t.advanced.filterFolder }), { target: { value: "pictures" } });
    await waitFor(() => expect(screen.queryByRole("region", { name: "video.mp4" })).toBeNull());
    expect(photo()).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: t.advanced.filterMinSize }), { target: { value: "104857600" } });
    await waitFor(() => expect(screen.queryByRole("region", { name: "foto.jpg" })).toBeNull());
    const last = callsOf(backend, "list_duplicate_groups").at(-1);
    expect(last?.args).toEqual({ scanId: 1, offset: 0, limit: 20, root: "pictures", minSizeBytes: 104_857_600 });
  });

  it("un grupo resuelto se pliega con la copia conservada y no ofrece eleccion", async () => {
    const [first, second] = twoGroups();
    if (first === undefined || second === undefined) throw new Error("fixture");
    const resolved = {
      ...first,
      resolvedAt: 5,
      files: first.files.map((f, i) => ({ ...f, kept: i === 0 })),
    };
    await openDuplicates({
      dupLast: scanSummary({ openGroupCount: 1 }),
      dupGroups: [resolved, second],
    });
    const region = await screen.findByRole("region", { name: "foto.jpg" });
    expect(within(region).getByText(format(t.resolved, { path: "Descargas · foto.jpg" }))).toBeInTheDocument();
    expect(within(region).queryByRole("radio")).toBeNull();
    expect(within(video()).getAllByRole("radio")).toHaveLength(3);
  });

  it("sin grupos lo dice y con todos resueltos no ofrece revisar", async () => {
    await openDuplicates({ dupLast: scanSummary({ groupCount: 0, openGroupCount: 0, reclaimableBytes: 0 }) });
    expect(await screen.findByText(t.noGroups)).toBeInTheDocument();
    cleanup();
    clearMocks();
    await openDuplicates({ dupLast: scanSummary({ openGroupCount: 0, reclaimableBytes: 0 }), dupGroups: [] });
    expect(await screen.findByText(t.allResolved)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Revisar y mover/ })).toBeNull();
  });
});

describe("Duplicados: revision, ejecucion y resumen", () => {
  async function chooseBoth(backend: MockBackend): Promise<void> {
    fireEvent.click(within(photo()).getByRole("radio", { name: /Descargas · foto\.jpg/ }));
    fireEvent.click(within(video()).getByRole("radio", { name: /Descargas/ }));
    expect(backend.state.dupGroups).toHaveLength(2);
  }

  async function openReview(backend: MockBackend): Promise<HTMLElement> {
    await chooseBoth(backend);
    const trigger = reviewButton(3);
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("alertdialog", { name: t.reviewTitle });
    await screen.findByRole("button", { name: format(t.confirmMove, { count: "3" }) });
    return dialog;
  }

  it("revisar pide el preview solo con ids de grupo y de las copias a conservar y enfoca Cancelar", async () => {
    const backend = await openWithGroups();
    const dialog = await openReview(backend);
    const [call] = callsOf(backend, "preview_duplicate_cleanup");
    expect(call?.args).toEqual({
      scanId: 1,
      decisions: [
        { groupId: 1, keepFileIds: [11] },
        { groupId: 2, keepFileIds: [22] },
      ],
    });
    expect(JSON.stringify(call?.args)).not.toMatch(/\\\\|C:/);
    expect(within(dialog).getByRole("button", { name: t.cancel })).toHaveFocus();
    expect(
      within(dialog).getByText(formatPlural(t.reviewTotal, 3, { groups: formatPlural(t.groupsCount, 2), size: "24 MB" })),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(t.reviewKeepNote)).toBeInTheDocument();
    expect(within(dialog).getAllByRole("button")).toHaveLength(2);
    expect(callsOf(backend, "confirm_duplicate_cleanup")).toHaveLength(0);
  });

  it("Esc y Cancelar cierran sin ejecutar nada y devuelven el foco a Revisar", async () => {
    const backend = await openWithGroups();
    const dialog = await openReview(backend);
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(callsOf(backend, "confirm_duplicate_cleanup")).toHaveLength(0);
    expect(reviewButton(3)).toHaveFocus();
    // Nada quedo marcado como conservado en el backend.
    expect(backend.state.dupGroups.every((g) => g.files.every((f) => !f.kept))).toBe(true);
    fireEvent.click(reviewButton(3));
    const again = await screen.findByRole("alertdialog", { name: t.reviewTitle });
    fireEvent.click(within(again).getByRole("button", { name: t.cancel }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(callsOf(backend, "confirm_duplicate_cleanup")).toHaveLength(0);
  });

  it("informa las copias recientes que no se mueven", async () => {
    const backend = await openWithGroups({ dupPreview: duplicatePreview({ recentCount: 2, recentBytes: 8 * 1024 * 1024 }) });
    const dialog = await openReview(backend);
    expect(
      within(dialog).getByText(formatPlural(t.reviewRecentNote, 2, { size: "8 MB" })),
    ).toBeInTheDocument();
  });

  it("con la Papelera deshabilitada avisa y no deja confirmar", async () => {
    const backend = await openWithGroups({ dupPreview: duplicatePreview({ recycleBinAvailable: false }) });
    const dialog = await openReview(backend);
    expect(within(dialog).getByRole("alert")).toHaveTextContent(t.recycleBinDisabled);
    expect(within(dialog).getByRole("button", { name: format(t.confirmMove, { count: "3" }) })).toBeDisabled();
  });

  it("confirmar mueve una sola vez aunque se haga doble clic, muestra el progreso y enfoca el resumen", async () => {
    const hold = gate();
    const backend = await openWithGroups({
      dupCleanupResult: duplicateSummary(),
      gates: { confirm_duplicate_cleanup: hold.gate },
    });
    const dialog = await openReview(backend);
    const confirmButton = within(dialog).getByRole("button", { name: format(t.confirmMove, { count: "3" }) });
    fireEvent.click(confirmButton);
    fireEvent.click(confirmButton);
    await waitFor(() => expect(callsOf(backend, "confirm_duplicate_cleanup")).toHaveLength(1));
    expect(callsOf(backend, "confirm_duplicate_cleanup")[0]?.args).toEqual({
      previewId: 1,
      onProgress: expect.anything(),
    });
    const events: CleanupEvent[] = [
      { type: "started", previewId: 1, total: 3 },
      { type: "progress", processed: 1, total: 3, movedCount: 1, movedBytes: 4 },
    ];
    await act(async () => backend.emitDuplicateCleanup(events));
    const progress = await screen.findByRole("region", { name: t.movingTitle });
    expect(progress).toHaveAttribute("aria-live", "polite");
    expect(within(progress).getByText(format(t.movingProgress, { done: "1", total: "3" }))).toBeInTheDocument();
    hold.release();
    const heading = await screen.findByRole("heading", { name: t.summaryTitle });
    await waitFor(() => expect(heading).toHaveFocus());
    expect(
      screen.getByText(formatPlural(t.summaryMoved, 3, { size: "24 MB" })),
    ).toBeInTheDocument();
    expect(screen.getByText(t.summarySpaceNote)).toBeInTheDocument();
    // Los grupos quedan resueltos, plegados, y no se vuelve a pedir otro escaneo.
    await waitFor(() => expect(within(photo()).queryByRole("radio")).toBeNull());
    expect(callsOf(backend, "start_duplicate_scan")).toHaveLength(0);
  });

  it("el resumen explica en lenguaje llano lo omitido por familia y, en avanzado, con el codigo crudo", async () => {
    const result = duplicateSummary({
      movedCount: 2,
      skippedLocked: 1,
      reasons: [{ outcome: "skipped_locked", detail: "content_changed", count: 1 }],
    });
    const backend = await openWithGroups({ settings: advancedSettings, dupCleanupResult: result });
    fireEvent.click(within(photo()).getAllByRole("checkbox")[0] as HTMLElement);
    fireEvent.click(within(video()).getAllByRole("checkbox")[0] as HTMLElement);
    fireEvent.click(screen.getByRole("button", { name: /Revisar y mover/ }));
    const confirmButton = await screen.findByRole("button", { name: /Mover \d+ copias a la Papelera/ });
    await waitFor(() => expect(confirmButton).toBeEnabled());
    fireEvent.click(confirmButton);
    await screen.findByRole("heading", { name: t.summaryTitle });
    expect(
      screen.getByText(
        formatPlural(t.summarySkipped, 1, { reason: es.cleanup.cleanupReasons.changed.toLowerCase() }),
      ),
    ).toBeInTheDocument();
    expect(screen.getByText(t.summaryRetry)).toBeInTheDocument();
    const table = screen.getByRole("table", { name: t.advanced.reasonsCaption });
    expect(within(table).getByText("content_changed")).toBeInTheDocument();
    expect(backend.state.dupGroups.length).toBeGreaterThan(0);
  });

  it("si no se pudo guardar la resolucion lo avisa", async () => {
    const backend = await openWithGroups({
      dupCleanupResult: duplicateSummary({ groupsResolved: false, resolvedGroupIds: [] }),
    });
    const dialog = await openReview(backend);
    fireEvent.click(within(dialog).getByRole("button", { name: format(t.confirmMove, { count: "3" }) }));
    expect(await screen.findByText(t.summaryNotResolved)).toBeInTheDocument();
  });

  it("un resumen cancelado dice cuantas copias no se procesaron y un aborto explica el motivo", async () => {
    const backend = await openWithGroups({
      dupCleanupResult: duplicateSummary({
        status: "aborted",
        abortReason: "recycle_bin_unavailable",
        movedCount: 0,
        movedBytes: 0,
        processed: 0,
        notProcessed: 3,
        resolvedGroupIds: [],
      }),
    });
    const dialog = await openReview(backend);
    fireEvent.click(within(dialog).getByRole("button", { name: format(t.confirmMove, { count: "3" }) }));
    expect(await screen.findByRole("alert")).toHaveTextContent(t.summaryAborted.recycle_bin_unavailable);
    cleanup();
    clearMocks();
    const cancelled = await openWithGroups({
      dupCleanupResult: duplicateSummary({ status: "cancelled", movedCount: 1, processed: 1, notProcessed: 2, resolvedGroupIds: [1] }),
    });
    const second = await openReview(cancelled);
    fireEvent.click(within(second).getByRole("button", { name: format(t.confirmMove, { count: "3" }) }));
    expect(await screen.findByText(formatPlural(t.summaryCancelled, 2))).toBeInTheDocument();
  });

  it("Cancelar durante la ejecucion pide cancelar la limpieza por su id", async () => {
    const hold = gate();
    const backend = await openWithGroups({
      dupCleanupResult: duplicateSummary({ status: "cancelled", movedCount: 1, notProcessed: 2 }),
      gates: { confirm_duplicate_cleanup: hold.gate },
    });
    const dialog = await openReview(backend);
    fireEvent.click(within(dialog).getByRole("button", { name: format(t.confirmMove, { count: "3" }) }));
    await waitFor(() => expect(callsOf(backend, "confirm_duplicate_cleanup")).toHaveLength(1));
    await act(async () => backend.emitDuplicateCleanup([{ type: "started", previewId: 1, total: 3 }]));
    fireEvent.click(await screen.findByRole("button", { name: t.cancelMoving }));
    await waitFor(() => expect(callsOf(backend, "cancel_cleanup")[0]?.args).toEqual({ previewId: 1 }));
    hold.release();
    await screen.findByRole("heading", { name: t.summaryTitle });
  });

  it("si el preview vencio pide uno nuevo, avisa y no confirma solo", async () => {
    const backend = await openWithGroups();
    backend.state.failureQueue.confirm_duplicate_cleanup = [{ kind: "preview_expired", message: "x" }];
    backend.state.dupCleanupResult = duplicateSummary();
    const dialog = await openReview(backend);
    fireEvent.click(within(dialog).getByRole("button", { name: format(t.confirmMove, { count: "3" }) }));
    expect(await screen.findByText(t.previewUpdated)).toBeInTheDocument();
    expect(screen.getByRole("alertdialog", { name: t.reviewTitle })).toBeInTheDocument();
    expect(callsOf(backend, "preview_duplicate_cleanup")).toHaveLength(2);
    expect(callsOf(backend, "confirm_duplicate_cleanup")).toHaveLength(1);
    // Un segundo intento, ahora si pedido por el usuario, usa el id del preview nuevo.
    fireEvent.click(screen.getByRole("button", { name: format(t.confirmMove, { count: "3" }) }));
    await screen.findByRole("heading", { name: t.summaryTitle });
    expect(callsOf(backend, "confirm_duplicate_cleanup")[1]?.args).toMatchObject({ previewId: 2 });
  });

  it("un error del preview cierra el dialogo y lo explica en espanol", async () => {
    const backend = await openWithGroups({
      failures: { preview_duplicate_cleanup: { kind: "duplicate_keep_required", message: "x" } },
    });
    await chooseBoth(backend);
    fireEvent.click(reviewButton(3));
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.duplicate_keep_required);
    expect(screen.queryByRole("alertdialog", { name: t.reviewTitle })).toBeNull();
  });

  it("no hay ninguna accion que borre de forma permanente", async () => {
    const backend = await openWithGroups();
    await openReview(backend);
    expect(screen.queryByRole("button", { name: /borrar|eliminar|vaciar/i })).toBeNull();
  });
});

describe("Duplicados: singular y plural (hallazgo 4)", () => {
  it("el resumen dice 'copia' y 'se movio' en singular y los descartes en singular", async () => {
    const result = duplicateSummary({
      movedCount: 1,
      movedBytes: 2 * 1024 * 1024,
      skippedLocked: 1,
      reasons: [{ outcome: "skipped_locked", detail: "content_changed", count: 1 }],
    });
    await openWithGroups({ dupCleanupResult: result });
    fireEvent.click(within(photo()).getAllByRole("radio")[0] as HTMLElement);
    fireEvent.click(within(video()).getAllByRole("radio")[0] as HTMLElement);
    fireEvent.click(screen.getByRole("button", { name: /Revisar y mover/ }));
    const confirmButton = await screen.findByRole("button", { name: /Mover \d+ copias a la Papelera/ });
    await waitFor(() => expect(confirmButton).toBeEnabled());
    fireEvent.click(confirmButton);
    await screen.findByRole("heading", { name: t.summaryTitle });
    expect(screen.getByText("Se movió 1 copia (2 MB) a la Papelera.")).toBeInTheDocument();
    expect(screen.getByText(/^1 copia no se movió: /)).toBeInTheDocument();
    expect(screen.queryByText(/1 copias/)).toBeNull();
  });

  it("los omitidos del escaneo usan singular con uno y plural con varios", async () => {
    await openWithGroups({
      dupLast: scanSummary({ cloudCount: 1, protectedCount: 1, hardLinkedCount: 2, inaccessibleDirs: 1, unreadableCount: 1 }),
    });
    expect(screen.getByText("1 archivo en la nube: no se leyó.")).toBeInTheDocument();
    expect(screen.getByText("1 archivo protegido: no se toca.")).toBeInTheDocument();
    expect(screen.getByText("2 archivos con varios nombres: se omitieron.")).toBeInTheDocument();
    expect(screen.getByText("1 carpeta sin acceso.")).toBeInTheDocument();
    expect(screen.getByText("1 archivo no se pudo leer: se omitió.")).toBeInTheDocument();
  });

  it("el dialogo de revision y la nota de recientes concuerdan en singular", async () => {
    await openWithGroups({
      dupPreview: duplicatePreview({
        totalItems: 1,
        totalBytes: 4 * 1024 * 1024,
        groupCount: 1,
        recentCount: 1,
        recentBytes: 2 * 1024 * 1024,
      }),
    });
    fireEvent.click(within(photo()).getAllByRole("radio")[0] as HTMLElement);
    fireEvent.click(screen.getByRole("button", { name: /Revisar y mover/ }));
    const dialog = await screen.findByRole("alertdialog", { name: t.reviewTitle });
    expect(await within(dialog).findByText(/1 copia de 1 grupo \(4 MB\)/)).toBeInTheDocument();
    expect(within(dialog).getByText(/\(1 copia, 2 MB\)/)).toBeInTheDocument();
  });
});

describe("Duplicados: Revisar deshabilitado tras limpiar (hallazgo 2)", () => {
  it("despues de mover, el boton explica que hay que buscar de nuevo y esta referenciado", async () => {
    // Solo se decide el primer grupo: el segundo sigue abierto y el boton sigue a la vista.
    const backend = await openWithGroups({
      dupCleanupResult: duplicateSummary({ movedCount: 1, totalItems: 1, processed: 1, resolvedGroupIds: [1] }),
      dupPreview: duplicatePreview({ totalItems: 1, groupCount: 1 }),
    });
    fireEvent.click(within(photo()).getAllByRole("radio")[0] as HTMLElement);
    fireEvent.click(reviewButton(1));
    await screen.findByRole("alertdialog", { name: t.reviewTitle });
    fireEvent.click(await screen.findByRole("button", { name: t.confirmMoveOne }));
    await screen.findByRole("heading", { name: t.summaryTitle });
    expect(callsOf(backend, "confirm_duplicate_cleanup")).toHaveLength(1);
    const review = screen.getByRole("button", { name: /Revisar y mover/ });
    expect(review).toBeDisabled();
    const reasonId = review.getAttribute("aria-describedby");
    expect(reasonId).not.toBeNull();
    expect(document.getElementById(reasonId ?? "")).toHaveTextContent(t.reviewDisabledAfterCleanup);
  });

  it("sin elegir nada el motivo sigue siendo elegir una copia", async () => {
    await openWithGroups();
    const review = screen.getByRole("button", { name: t.reviewNone });
    expect(review).toBeDisabled();
    const reasonId = review.getAttribute("aria-describedby");
    expect(document.getElementById(reasonId ?? "")).toHaveTextContent(t.reviewDisabledReason);
  });
});

describe("Duplicados: opciones del escaneo desde el ultimo escaneo (hallazgo 3)", () => {
  const checked = (folders: HTMLElement) =>
    within(folders)
      .getAllByRole("checkbox")
      .filter((box) => (box as HTMLInputElement).checked);

  it("al abrir la app en avanzado marca las carpetas y el tamano del ultimo escaneo y los envia", async () => {
    const backend = await openDuplicates({
      settings: advancedSettings,
      dupLast: scanSummary({
        minSizeBytes: 10_485_760,
        roots: roots({
          desktop: { status: "not_selected" },
          documents: { status: "not_selected" },
          music: { status: "not_selected" },
          pictures: { status: "not_selected" },
          videos: { status: "not_selected" },
        }),
      }),
      dupScanResult: scanSummary(),
    });
    const folders = screen.getByRole("group", { name: t.advanced.foldersLegend });
    await waitFor(() => expect(within(folders).getByRole("checkbox", { name: "Escritorio" })).not.toBeChecked());
    expect(within(folders).getByRole("checkbox", { name: "Descargas" })).toBeChecked();
    expect(checked(folders)).toHaveLength(1);
    expect(screen.getByRole("combobox", { name: t.advanced.minSizeLabel })).toHaveValue("10485760");
    fireEvent.click(scanButton());
    await waitFor(() => expect(callsOf(backend, "start_duplicate_scan")).toHaveLength(1));
    expect(callsOf(backend, "start_duplicate_scan")[0]?.args).toEqual({
      roots: ["downloads"],
      minSizeBytes: 10_485_760,
      onProgress: expect.anything(),
    });
  });

  it("una carpeta revisada, anidada o no disponible cuenta como elegida; solo 'no elegida' queda sin marcar", async () => {
    await openDuplicates({
      settings: advancedSettings,
      dupLast: scanSummary({
        roots: roots({
          desktop: { status: "not_selected" },
          documents: { status: "nested" },
          music: { status: "unavailable", reason: "not_found" },
        }),
      }),
    });
    const folders = screen.getByRole("group", { name: t.advanced.foldersLegend });
    await waitFor(() => expect(within(folders).getByRole("checkbox", { name: "Escritorio" })).not.toBeChecked());
    for (const name of ["Documentos", "Descargas", "Música", "Imágenes", "Videos"]) {
      expect(within(folders).getByRole("checkbox", { name })).toBeChecked();
    }
  });

  it("sin escaneo previo conserva las seis carpetas y 1 MB", async () => {
    await openDuplicates({ settings: advancedSettings });
    const folders = screen.getByRole("group", { name: t.advanced.foldersLegend });
    expect(checked(folders)).toHaveLength(6);
    expect(screen.getByRole("combobox", { name: t.advanced.minSizeLabel })).toHaveValue("1048576");
  });

  it("un ultimo escaneo sin ninguna carpeta elegida cae en los valores por defecto", async () => {
    await openDuplicates({
      settings: advancedSettings,
      dupLast: scanSummary({
        roots: roots({
          desktop: { status: "not_selected" },
          documents: { status: "not_selected" },
          downloads: { status: "not_selected" },
          music: { status: "not_selected" },
          pictures: { status: "not_selected" },
          videos: { status: "not_selected" },
        }),
      }),
    });
    await screen.findByText(format(t.lastScan, { date: formatDateTime(1_800_000_005_000) }));
    const folders = screen.getByRole("group", { name: t.advanced.foldersLegend });
    expect(checked(folders)).toHaveLength(6);
  });

  it("en simple sigue buscando con los valores de D-01 aunque el ultimo escaneo fuera parcial", async () => {
    const backend = await openDuplicates({
      dupLast: scanSummary({ roots: roots({ desktop: { status: "not_selected" } }), minSizeBytes: 10_485_760 }),
      dupScanResult: scanSummary(),
    });
    await screen.findByText(format(t.lastScan, { date: formatDateTime(1_800_000_005_000) }));
    fireEvent.click(scanButton());
    await waitFor(() => expect(callsOf(backend, "start_duplicate_scan")).toHaveLength(1));
    expect(callsOf(backend, "start_duplicate_scan")[0]?.args).toEqual({ onProgress: expect.anything() });
  });
});
