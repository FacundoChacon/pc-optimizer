import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime, formatPlural } from "../../shared/i18n/format";
import { defaultPages, diskSummary, FILE_DATE, folderEntry, GB, rootChildren, topFiles } from "../support/diskFixtures";
import { type BackendState, installBackend, type MockBackend, OTHER_USER } from "../support/mockBackend";
import { shareText } from "../../features/disk/diskText";

afterEach(() => {
  cleanup();
  clearMocks();
});

const t = es.disk;

function withScan(overrides: Partial<BackendState> = {}): Partial<BackendState> {
  return { diskLast: diskSummary(), diskPages: defaultPages(), diskFiles: topFiles(), ...overrides };
}

async function openResults(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend(withScan(overrides));
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
  await screen.findByRole("heading", { level: 1, name: t.title });
  await screen.findByRole("region", { name: t.topFilesTitle });
  await screen.findByRole("region", { name: t.topFoldersTitle });
  return backend;
}

const summaryCard = () => screen.getByRole("region", { name: t.summaryTitle });
const foldersCard = () => screen.getByRole("region", { name: t.topFoldersTitle });
const filesCard = () => screen.getByRole("region", { name: t.topFilesTitle });
const rowOf = (card: HTMLElement, name: string | RegExp) =>
  within(card)
    .getAllByRole("listitem")
    .find((item) => (typeof name === "string" ? item.textContent?.includes(name) : name.test(item.textContent ?? "")));

function pagesWith(children: ReturnType<typeof rootChildren>) {
  const pages = defaultPages();
  const root = pages["root"];
  if (root !== undefined) pages["root"] = { ...root, children };
  return pages;
}

describe("Resumen del disco", () => {
  it("dice cuanto se uso, cuanto se analizo y en cuantos archivos", async () => {
    await openResults();
    expect(summaryCard()).toHaveTextContent(
      format(t.summaryDrive, {
        used: formatBytes(380 * GB),
        total: formatBytes(500 * GB),
        analyzed: formatBytes(380 * GB),
        files: formatPlural(t.files, 412_000),
      }),
    );
    expect(within(summaryCard()).queryByText(/No contabilizado/)).toBeNull();
  });

  it("explica el espacio no contabilizado con su ayuda", async () => {
    await openResults({ diskLast: diskSummary({ unaccountedBytes: 12 * GB, totalBytes: 368 * GB }) });
    expect(summaryCard()).toHaveTextContent(format(t.unaccounted, { size: formatBytes(12 * GB) }));
    expect(summaryCard()).toHaveTextContent(t.unaccountedHelp);
  });

  it("avisa cuando lo analizado supera lo usado", async () => {
    await openResults({ diskLast: diskSummary({ unaccountedBytes: 0, overCounted: true }) });
    expect(summaryCard()).toHaveTextContent(t.overCounted);
    expect(within(summaryCard()).queryByText(/No contabilizado/)).toBeNull();
  });

  it("con una carpeta elegida nombra la carpeta y no habla del disco", async () => {
    await openResults({
      diskLast: diskSummary({
        root: { kind: "picked", path: "C:\\Users\\ana\\Downloads", folderId: 1 },
        volume: null,
        unaccountedBytes: null,
        totalBytes: 8 * GB,
        fileCount: 21,
      }),
    });
    expect(summaryCard()).toHaveTextContent(
      format(t.summaryFolder, {
        path: "C:\\Users\\ana\\Downloads",
        analyzed: formatBytes(8 * GB),
        files: formatPlural(t.files, 21),
      }),
    );
  });

  it("avisa en lenguaje llano de carpetas sin acceso y archivos solo en la nube", async () => {
    await openResults({ diskLast: diskSummary({ inaccessibleDirs: 3, cloudFiles: 1 }) });
    expect(summaryCard()).toHaveTextContent(formatPlural(t.warnInaccessible, 3));
    expect(summaryCard()).toHaveTextContent(formatPlural(t.warnCloud, 1));
  });

  it("sin avisos no inventa lineas de aviso", async () => {
    await openResults();
    expect(within(summaryCard()).queryByText(/no se pudo/)).toBeNull();
    expect(within(summaryCard()).queryByText(/nube/)).toBeNull();
  });

  it("'Analizar de nuevo' es secundaria y no hay accion primaria con resultados", async () => {
    await openResults();
    expect(screen.getByRole("button", { name: t.scanAgain })).toHaveAttribute("data-variant", "secondary");
    expect(document.body.querySelectorAll('[data-variant="primary"]')).toHaveLength(0);
  });
});

describe("Lo que mas ocupa", () => {
  it("muestra las 5 carpetas mas grandes de la raiz con su tamano y porcentaje en texto", async () => {
    await openResults();
    const items = within(foldersCard()).getAllByRole("listitem");
    expect(items).toHaveLength(5);
    const names = items.map((item) => /Windows|Users|Program Files|ProgramData|Juegos/.exec(item.textContent ?? "")?.[0]);
    expect(names).toEqual(["Windows", "Users", "Program Files", "ProgramData", "Juegos"]);
    expect(items[0]).toHaveTextContent(shareText(120 * GB, 380 * GB));
    expect(items[1]).toHaveTextContent(shareText(100 * GB, 380 * GB));
  });

  it("cada barra es decorativa, no usa estilos en linea y el tamano esta en texto", async () => {
    await openResults();
    const card = foldersCard();
    const bars = card.querySelectorAll("svg[aria-hidden='true'] rect[data-bar='value']");
    expect(bars).toHaveLength(5);
    expect(card.querySelectorAll("[style]")).toHaveLength(0);
    for (const item of within(card).getAllByRole("listitem")) expect(item.textContent).toMatch(/\d %/);
  });

  it("marca las de sistema con una etiqueta y su explicacion", async () => {
    await openResults();
    const windows = rowOf(foldersCard(), "Windows");
    expect(windows).toHaveTextContent(t.systemLabel);
    expect(windows).toHaveTextContent(t.systemHint);
    expect(rowOf(foldersCard(), "Juegos")).not.toHaveTextContent(t.systemLabel);
  });

  it("una carpeta opaca se muestra por su etiqueta, sin su nombre ni acciones", async () => {
    const children = [
      folderEntry(9, "bob", 130 * GB, { opaque: "other_user_profile", hasChildren: false }),
      folderEntry(5, "Papelera-x", 110 * GB, { opaque: "recycle_bin", hasChildren: false }),
      ...rootChildren().slice(0, 3),
    ];
    await openResults({ diskPages: pagesWith(children) });
    const profile = rowOf(foldersCard(), t.opaque.other_user_profile);
    expect(profile).toBeDefined();
    expect(profile).toHaveTextContent(shareText(130 * GB, 380 * GB));
    expect(profile).toHaveTextContent(t.opaqueHint);
    expect(profile?.textContent).not.toContain("bob");
    expect(within(profile as HTMLElement).queryByRole("button")).toBeNull();
    const bin = rowOf(foldersCard(), t.opaque.recycle_bin);
    expect(bin?.textContent).not.toContain("Papelera-x");
    expect(within(bin as HTMLElement).queryByRole("button")).toBeNull();
  });

  it("explica una carpeta incompleta o sin acceso con texto, no solo con color", async () => {
    const children = [
      folderEntry(2, "Datos", 90 * GB, { access: "incomplete" }),
      folderEntry(3, "Cerrada", 0, { access: "inaccessible", hasChildren: false }),
    ];
    await openResults({ diskPages: pagesWith(children) });
    expect(rowOf(foldersCard(), "Datos")).toHaveTextContent(t.access.incomplete);
    expect(rowOf(foldersCard(), "Cerrada")).toHaveTextContent(t.access.inaccessible);
  });

  it("sin carpetas guardadas lo dice", async () => {
    await openResults({ diskPages: pagesWith([]) });
    expect(foldersCard()).toHaveTextContent(t.noFolders);
  });
});

describe("Archivos mas grandes", () => {
  it("muestra los 5 primeros con tamano y fecha, y 'Sin fecha' cuando no hay", async () => {
    const backend = await openResults();
    const items = within(filesCard()).getAllByRole("listitem");
    expect(items).toHaveLength(5);
    expect(items[0]).toHaveTextContent("viaje.mp4");
    expect(items[0]).toHaveTextContent(formatBytes(12 * GB));
    expect(items[0]).toHaveTextContent(format(t.modified, { date: formatDateTime(FILE_DATE) }));
    expect(items[3]).toHaveTextContent("backup.zip");
    expect(items[3]).toHaveTextContent(t.noDate);
    expect(within(filesCard()).queryByText("foto.raw")).toBeNull();
    const call = backend.calls.find((c) => c.cmd === "list_large_files");
    expect(call?.args).toEqual({ scanId: 1, sort: "size", offset: 0, limit: 5 });
  });

  it("sin archivos lo dice", async () => {
    await openResults({ diskFiles: [] });
    expect(filesCard()).toHaveTextContent(t.noFiles);
  });
});

describe("Mostrar en el Explorador", () => {
  it("una carpeta se muestra por su id, nunca por su ruta", async () => {
    const backend = await openResults();
    fireEvent.click(within(rowOf(foldersCard(), "Windows") as HTMLElement).getByRole("button", { name: format(t.revealLabel, { name: "Windows" }) }));
    await waitFor(() => expect(backend.count("reveal_in_explorer")).toBe(1));
    const args = backend.calls.find((c) => c.cmd === "reveal_in_explorer")?.args;
    expect(args).toEqual({ scanId: 1, target: { kind: "folder", id: 2 } });
    expect(JSON.stringify(args)).not.toMatch(/C:/);
  });

  it("un archivo se muestra por su id", async () => {
    const backend = await openResults();
    fireEvent.click(within(rowOf(filesCard(), "viaje.mp4") as HTMLElement).getByRole("button", { name: format(t.revealLabel, { name: "viaje.mp4" }) }));
    await waitFor(() => expect(backend.count("reveal_in_explorer")).toBe(1));
    expect(backend.calls.find((c) => c.cmd === "reveal_in_explorer")?.args).toEqual({
      scanId: 1,
      target: { kind: "file", id: 1 },
    });
  });

  it("un fallo se informa con el motivo y se puede reintentar", async () => {
    const backend = await openResults({
      failureQueue: {
        reveal_in_explorer: [{ kind: "reveal_unavailable", message: "reveal unavailable: target_missing" }],
      },
    });
    const button = () => within(rowOf(filesCard(), "viaje.mp4") as HTMLElement).getByRole("button", { name: format(t.revealLabel, { name: "viaje.mp4" }) });
    fireEvent.click(button());
    expect(await screen.findByRole("alert")).toHaveTextContent(t.revealReasons.target_missing);
    fireEvent.click(button());
    await waitFor(() => expect(backend.count("reveal_in_explorer")).toBe(2));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("en over-the-shoulder no hay boton: se explica el motivo y se avisa de los perfiles", async () => {
    const backend = await openResults({ privilege: OTHER_USER });
    expect(within(foldersCard()).queryByRole("button", { name: /Mostrar/ })).toBeNull();
    expect(within(filesCard()).queryByRole("button", { name: /Mostrar/ })).toBeNull();
    expect(foldersCard()).toHaveTextContent(t.revealBlocked.different_user);
    expect(filesCard()).toHaveTextContent(t.revealBlocked.different_user);
    expect(summaryCard()).toHaveTextContent(t.mismatchNote);
    expect(backend.count("reveal_in_explorer")).toBe(0);
  });
});

describe("Revisar en Limpieza", () => {
  it("solo se ofrece en carpetas con sugerencia y solo navega a Limpieza", async () => {
    const children = [
      folderEntry(2, "Cache de Edge", 50 * GB, { junkHint: true }),
      folderEntry(3, "Juegos", 40 * GB),
    ];
    const backend = await openResults({ diskPages: pagesWith(children) });
    expect(within(rowOf(foldersCard(), "Juegos") as HTMLElement).queryByRole("button", { name: /Limpieza/ })).toBeNull();
    const row = rowOf(foldersCard(), "Cache de Edge") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: format(t.reviewInCleanupLabel, { name: "Cache de Edge" }) }));
    expect(await screen.findByRole("heading", { level: 1, name: es.cleanup.title })).toBeInTheDocument();
    for (const cmd of ["start_scan", "preview_cleanup", "confirm_cleanup", "reveal_in_explorer"]) {
      expect(backend.count(cmd as never)).toBe(0);
    }
  });
});
