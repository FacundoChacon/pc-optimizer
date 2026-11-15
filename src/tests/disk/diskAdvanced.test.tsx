import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime, formatPlural } from "../../shared/i18n/format";
import { shareText } from "../../features/disk/diskText";
import {
  defaultPages,
  diskSummary,
  FILE_DATE,
  folderEntry,
  GB,
  largeFile,
  rootChildren,
  topFiles,
} from "../support/diskFixtures";
import { type BackendState, installBackend, type MockBackend } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const t = es.disk;
const advancedSettings = { uiMode: "advanced", autostartTray: false, language: "es" } as const;

function withScan(overrides: Partial<BackendState> = {}): Partial<BackendState> {
  return {
    settings: advancedSettings,
    diskLast: diskSummary(),
    diskPages: defaultPages(),
    diskFiles: topFiles(),
    ...overrides,
  };
}

async function openAdvanced(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = installBackend({ settings: advancedSettings, ...overrides });
  render(<App />);
  const nav = await screen.findByRole("navigation", { name: es.nav.label });
  fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
  await screen.findByRole("heading", { level: 1, name: t.title });
  return backend;
}

async function openResults(overrides: Partial<BackendState> = {}): Promise<MockBackend> {
  const backend = await openAdvanced(withScan(overrides));
  await screen.findByRole("table", { name: t.topFoldersTitle });
  return backend;
}

const foldersTable = () => screen.getByRole("table", { name: t.topFoldersTitle });
const filesTable = () => screen.getByRole("table", { name: t.topFilesTitle });
const rowOf = (table: HTMLElement, text: string | RegExp) =>
  within(table)
    .getAllByRole("row")
    .find((row) => (typeof text === "string" ? row.textContent?.includes(text) : text.test(row.textContent ?? "")));
const pickButton = () => screen.getByRole("button", { name: t.pickButton });
const diskRadio = () => screen.getByRole("radio", { name: t.rootDisk });
const folderRadio = () => screen.getByRole("radio", { name: t.rootFolder });
const scanButton = () => screen.getByRole("button", { name: /Analizar (disco C:|carpeta|de nuevo)/ });
const calls = (backend: MockBackend, cmd: string) => backend.calls.filter((call) => call.cmd === cmd);

describe("Avanzado: eleccion de la raiz", () => {
  it("por defecto analiza el disco y no ofrece elegir carpeta hasta marcar 'Una carpeta...'", async () => {
    await openAdvanced();
    await screen.findByText(t.intro);
    expect(screen.getByRole("group", { name: t.rootLegend })).toBeInTheDocument();
    expect(diskRadio()).toBeChecked();
    expect(folderRadio()).not.toBeChecked();
    expect(screen.queryByRole("button", { name: t.pickButton })).toBeNull();
    expect(scanButton()).toHaveTextContent(t.scanDisk);
    expect(scanButton()).toBeEnabled();
  });

  it("en modo simple no hay selector de raiz", async () => {
    installBackend();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
    await screen.findByText(t.intro);
    expect(screen.queryByRole("group", { name: t.rootLegend })).toBeNull();
    expect(screen.queryByRole("radio")).toBeNull();
  });

  it("'Analizar' queda deshabilitado con su motivo hasta elegir una carpeta valida", async () => {
    await openAdvanced();
    await screen.findByText(t.intro);
    fireEvent.click(folderRadio());
    expect(screen.getByText(t.pickNone)).toBeInTheDocument();
    const button = scanButton();
    expect(button).toHaveTextContent(t.scanFolder);
    expect(button).toBeDisabled();
    const reasonId = button.getAttribute("aria-describedby");
    expect(reasonId).not.toBeNull();
    expect(document.getElementById(reasonId ?? "")).toHaveTextContent(t.scanDisabledReason);
  });

  it("elegir una carpeta muestra su ruta y analizarla envia solo el pickId", async () => {
    const backend = await openAdvanced({
      pickQueue: [{ status: "picked", pickId: 7, path: "C:\\Users\\ana\\Downloads" }],
      diskScanResult: diskSummary({ root: { kind: "picked", path: "C:\\Users\\ana\\Downloads", folderId: 1 } }),
      diskPages: defaultPages(),
      diskFiles: topFiles(),
    });
    await screen.findByText(t.intro);
    fireEvent.click(folderRadio());
    fireEvent.click(pickButton());
    expect(await screen.findByText(format(t.pickedPath, { path: "C:\\Users\\ana\\Downloads" }))).toBeInTheDocument();
    expect(scanButton()).toBeEnabled();
    expect(scanButton()).not.toHaveAttribute("aria-describedby");
    fireEvent.click(scanButton());
    await screen.findByRole("table", { name: t.topFoldersTitle });
    const args = calls(backend, "start_folder_scan")[0]?.args as { root: unknown };
    expect(args.root).toEqual({ kind: "picked", pickId: 7 });
    expect(JSON.stringify(args.root)).not.toMatch(/C:/);
  });

  it("una carpeta rechazada se explica con su motivo y no cambia la seleccion", async () => {
    await openAdvanced({
      pickQueue: [{ kind: "folder_root_rejected", message: "folder root rejected: reparse_point" }],
    });
    await screen.findByText(t.intro);
    fireEvent.click(folderRadio());
    fireEvent.click(pickButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      format(t.pickRejected, { reason: t.rootRejectedReasons.reparse_point }),
    );
    expect(scanButton()).toBeDisabled();
    expect(screen.getByText(t.pickNone)).toBeInTheDocument();
  });

  it("cancelar el dialogo conserva la carpeta anterior y lo avisa", async () => {
    await openAdvanced({
      pickQueue: [{ status: "picked", pickId: 3, path: "C:\\Games" }, { status: "cancelled" }],
    });
    await screen.findByText(t.intro);
    fireEvent.click(folderRadio());
    fireEvent.click(pickButton());
    await screen.findByText(format(t.pickedPath, { path: "C:\\Games" }));
    fireEvent.click(pickButton());
    expect(await screen.findByText(t.pickCancelled)).toBeInTheDocument();
    expect(screen.getByText(format(t.pickedPath, { path: "C:\\Games" }))).toBeInTheDocument();
    expect(scanButton()).toBeEnabled();
  });

  it("un selector ocupado se informa con el texto del catalogo", async () => {
    await openAdvanced({ pickQueue: [{ kind: "folder_picker_busy", message: "x" }] });
    await screen.findByText(t.intro);
    fireEvent.click(folderRadio());
    fireEvent.click(pickButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.folder_picker_busy);
  });

  it("si la seleccion se perdio en el backend, pide elegir de nuevo", async () => {
    await openAdvanced({
      pickQueue: [{ status: "picked", pickId: 3, path: "C:\\Games" }],
      failureQueue: { start_folder_scan: [{ kind: "folder_pick_not_found", message: "x" }] },
    });
    await screen.findByText(t.intro);
    fireEvent.click(folderRadio());
    fireEvent.click(pickButton());
    await screen.findByText(format(t.pickedPath, { path: "C:\\Games" }));
    fireEvent.click(scanButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(es.errors.folder_pick_not_found);
    expect(screen.getByText(t.pickNone)).toBeInTheDocument();
    expect(scanButton()).toBeDisabled();
  });
});

describe("Avanzado: lista navegable de carpetas", () => {
  it("es una tabla con columnas, tamano en texto y estado con su codigo", async () => {
    const children = [
      folderEntry(2, "Windows", 120 * GB, { system: true, fileCount: 100, dirCount: 7 }),
      folderEntry(3, "Datos", 90 * GB, { access: "incomplete" }),
      folderEntry(4, "Cerrada", 0, { access: "inaccessible", hasChildren: false }),
    ];
    const pages = defaultPages();
    const root = pages["root"];
    if (root !== undefined) pages["root"] = { ...root, children };
    await openResults({ diskPages: pages });
    const headers = within(foldersTable())
      .getAllByRole("columnheader")
      .map((header) => header.textContent);
    expect(headers).toEqual([
      t.columns.name,
      t.columns.size,
      t.columns.files,
      t.columns.folders,
      t.columns.state,
      t.columns.actions,
    ]);
    const windows = rowOf(foldersTable(), "Windows") as HTMLElement;
    expect(windows).toHaveTextContent(shareText(120 * GB, 380 * GB));
    expect(windows).toHaveTextContent("100");
    expect(windows).toHaveTextContent(t.systemLabel);
    expect(windows.querySelector("svg[aria-hidden='true']")).not.toBeNull();
    expect(rowOf(foldersTable(), "Datos")).toHaveTextContent(`${t.access.incomplete} (incomplete)`);
    expect(rowOf(foldersTable(), "Cerrada")).toHaveTextContent(`${t.access.inaccessible} (inaccessible)`);
    expect(foldersTable().querySelectorAll("[style]")).toHaveLength(0);
  });

  it("pide 50 por pagina a la raiz y muestra las filas de carpetas mas chicas y archivos sueltos", async () => {
    const backend = await openResults();
    expect(calls(backend, "list_folder_children")[0]?.args).toEqual({
      scanId: 1,
      folderId: null,
      sort: "size",
      offset: 0,
      limit: 50,
    });
    expect(rowOf(foldersTable(), t.smallerFolders)).toHaveTextContent(shareText(8 * GB, 380 * GB));
    expect(rowOf(foldersTable(), t.looseFiles)).toHaveTextContent(shareText(2 * GB, 380 * GB));
  });

  it("entra en una carpeta, muestra las migas de pan y vuelve a la raiz", async () => {
    const backend = await openResults();
    fireEvent.click(within(foldersTable()).getByRole("button", { name: format(t.open, { name: "Users" }) }));
    await waitFor(() => expect(within(foldersTable()).getByText("ana")).toBeInTheDocument());
    expect(calls(backend, "list_folder_children").at(-1)?.args).toEqual({
      scanId: 1,
      folderId: 3,
      sort: "size",
      offset: 0,
      limit: 50,
    });
    const crumbs = screen.getByRole("navigation", { name: t.breadcrumbLabel });
    expect(within(crumbs).getByText("Users")).toHaveAttribute("aria-current", "page");
    fireEvent.click(within(crumbs).getByRole("button", { name: "C:\\" }));
    await waitFor(() => expect(within(foldersTable()).getByText("Windows")).toBeInTheDocument());
    expect(calls(backend, "list_folder_children").at(-1)?.args).toMatchObject({ folderId: null });
  });

  it("una carpeta opaca se muestra por su etiqueta, sin abrirse ni mostrarse", async () => {
    await openResults();
    fireEvent.click(within(foldersTable()).getByRole("button", { name: format(t.open, { name: "Users" }) }));
    await waitFor(() => expect(within(foldersTable()).getByText("ana")).toBeInTheDocument());
    const profile = rowOf(foldersTable(), t.opaque.other_user_profile) as HTMLElement;
    expect(profile).toBeDefined();
    expect(profile.textContent).not.toContain("bob");
    expect(within(profile).queryByRole("button")).toBeNull();
  });

  it("ordena por nombre y vuelve a pedir la pagina desde el principio", async () => {
    const backend = await openResults();
    fireEvent.change(screen.getByRole("combobox", { name: `${t.sortLabel}: ${t.topFoldersTitle}` }), {
      target: { value: "name" },
    });
    await waitFor(() =>
      expect(calls(backend, "list_folder_children").at(-1)?.args).toEqual({
        scanId: 1,
        folderId: null,
        sort: "name",
        offset: 0,
        limit: 50,
      }),
    );
    const names = within(foldersTable())
      .getAllByRole("row")
      .slice(1)
      .map((row) => row.textContent ?? "");
    const first = names.find((text) => rootChildren().some((folder) => text.includes(folder.name)));
    expect(first).toContain("Juegos");
  });

  it("pagina de a 50 con 'Ver mas' y dice cuantas muestra", async () => {
    const many = Array.from({ length: 120 }, (_, i) => folderEntry(100 + i, `carpeta-${String(i).padStart(3, "0")}`, (200 - i) * GB));
    const pages = defaultPages();
    const root = pages["root"];
    if (root !== undefined) pages["root"] = { ...root, children: many };
    const backend = await openResults({ diskPages: pages });
    expect(screen.getByText(format(t.shown, { shown: "50", total: "120" }))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: `${t.loadMore}: ${t.topFoldersTitle}` }));
    await screen.findByText(format(t.shown, { shown: "100", total: "120" }));
    expect(calls(backend, "list_folder_children").at(-1)?.args).toMatchObject({ offset: 50, limit: 50 });
    fireEvent.click(screen.getByRole("button", { name: `${t.loadMore}: ${t.topFoldersTitle}` }));
    await screen.findByText(format(t.shown, { shown: "120", total: "120" }));
    expect(screen.queryByRole("button", { name: `${t.loadMore}: ${t.topFoldersTitle}` })).toBeNull();
  });

  it("una carpeta de sugerencia de limpieza ofrece solo ir a Limpieza", async () => {
    const children = [folderEntry(2, "Cache de Edge", 50 * GB, { junkHint: true })];
    const pages = defaultPages();
    const root = pages["root"];
    if (root !== undefined) pages["root"] = { ...root, children };
    await openResults({ diskPages: pages });
    const row = rowOf(foldersTable(), "Cache de Edge") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: format(t.reviewInCleanupLabel, { name: "Cache de Edge" }) }));
    expect(await screen.findByRole("heading", { level: 1, name: es.cleanup.title })).toBeInTheDocument();
  });
});

describe("Avanzado: archivos grandes", () => {
  it("es una tabla con nombre, ruta completa, tamano y fecha", async () => {
    const backend = await openResults();
    await screen.findByRole("table", { name: t.topFilesTitle });
    const headers = within(filesTable())
      .getAllByRole("columnheader")
      .map((header) => header.textContent);
    expect(headers).toEqual([t.columns.name, t.columns.path, t.columns.size, t.columns.modified, t.columns.actions]);
    const row = rowOf(filesTable(), "viaje.mp4") as HTMLElement;
    expect(row).toHaveTextContent("C:\\Users\\ana\\Videos\\viaje.mp4");
    expect(row).toHaveTextContent(formatBytes(12 * GB));
    expect(row).toHaveTextContent(formatDateTime(FILE_DATE));
    expect(rowOf(filesTable(), "backup.zip")).toHaveTextContent(t.noDate);
    expect(screen.getByText(t.topFilesNote)).toBeInTheDocument();
    expect(calls(backend, "list_large_files")[0]?.args).toEqual({ scanId: 1, sort: "size", offset: 0, limit: 50 });
  });

  it("ordena por mas nuevos y por mas viejos sin repetir filas", async () => {
    const backend = await openResults();
    await screen.findByRole("table", { name: t.topFilesTitle });
    const select = screen.getByRole("combobox", { name: `${t.sortLabel}: ${t.topFilesTitle}` });
    fireEvent.change(select, { target: { value: "newest" } });
    await waitFor(() => expect(calls(backend, "list_large_files").at(-1)?.args).toMatchObject({ sort: "newest", offset: 0 }));
    await waitFor(() => {
      const names = within(filesTable()).getAllByRole("row").slice(1).map((row) => row.textContent ?? "");
      expect(names[0]).toContain("foto.raw");
    });
    fireEvent.change(select, { target: { value: "oldest" } });
    await waitFor(() => {
      const names = within(filesTable()).getAllByRole("row").slice(1).map((row) => row.textContent ?? "");
      expect(names[0]).toContain("viaje.mp4");
    });
    expect(within(filesTable()).getAllByRole("row")).toHaveLength(7);
  });

  it("pagina de a 50 con 'Ver mas'", async () => {
    const many = Array.from({ length: 70 }, (_, i) => largeFile(i + 1, `archivo-${i}.bin`, (100 - i) * GB));
    const backend = await openResults({ diskFiles: many });
    await screen.findByRole("table", { name: t.topFilesTitle });
    expect(screen.getByText(format(t.shown, { shown: "50", total: "70" }))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: `${t.loadMore}: ${t.topFilesTitle}` }));
    await screen.findByText(format(t.shown, { shown: "70", total: "70" }));
    expect(calls(backend, "list_large_files").at(-1)?.args).toMatchObject({ offset: 50, limit: 50 });
  });

  it("mostrar en el Explorador usa el id del archivo", async () => {
    const backend = await openResults();
    await screen.findByRole("table", { name: t.topFilesTitle });
    const row = rowOf(filesTable(), "juego.pak") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: format(t.revealLabel, { name: "juego.pak" }) }));
    await waitFor(() => expect(backend.count("reveal_in_explorer")).toBe(1));
    expect(calls(backend, "reveal_in_explorer")[0]?.args).toEqual({ scanId: 1, target: { kind: "file", id: 3 } });
  });
});

describe("Avanzado: detalles del analisis", () => {
  it("muestra los contadores y no avisa de truncado cuando se guardo todo", async () => {
    await openResults({
      diskLast: diskSummary({ inaccessibleDirs: 4, reparseSkipped: 2, cloudFiles: 9, opaqueDirs: 3, foldersStored: 77 }),
    });
    const details = screen.getByRole("region", { name: t.detailsTitle });
    expect(details).toHaveTextContent(`${t.detailsCounters.inaccessible}: 4`);
    expect(details).toHaveTextContent(`${t.detailsCounters.reparse}: 2`);
    expect(details).toHaveTextContent(`${t.detailsCounters.cloud}: 9`);
    expect(details).toHaveTextContent(`${t.detailsCounters.opaque}: 3`);
    expect(details).toHaveTextContent(`${t.detailsCounters.stored}: 77`);
    expect(within(details).queryByText(/más grandes; el resto/)).toBeNull();
  });

  it("avisa cuando se guardaron solo las carpetas mas grandes", async () => {
    await openResults({ diskLast: diskSummary({ foldersStored: 20_000, foldersTruncated: true }) });
    expect(screen.getByRole("region", { name: t.detailsTitle })).toHaveTextContent(
      format(t.truncated, { count: "20000" }),
    );
  });

  it("lista la muestra de carpetas sin acceso con su motivo y su codigo", async () => {
    await openResults({
      diskLast: diskSummary({
        inaccessibleDirs: 3,
        issues: [
          { path: "C:\\Windows\\CSC", reason: "access_denied" },
          { path: "C:\\Viejo", reason: "not_found" },
        ],
      }),
    });
    const sample = screen.getByRole("region", { name: t.issuesTitle });
    const items = within(sample).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent("C:\\Windows\\CSC");
    expect(items[0]).toHaveTextContent(`${t.issueReasons.access_denied} (access_denied)`);
    expect(items[1]).toHaveTextContent(`${t.issueReasons.not_found} (not_found)`);
    expect(formatPlural(t.files, 1)).toBe("1 archivo");
  });

  it("en modo simple no hay panel de detalles", async () => {
    installBackend(withScan({ settings: { uiMode: "simple", autostartTray: false, language: "es" } }));
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: es.nav.label });
    fireEvent.click(within(nav).getByRole("button", { name: es.nav.disk }));
    await screen.findByRole("region", { name: t.topFoldersTitle });
    expect(screen.queryByRole("region", { name: t.detailsTitle })).toBeNull();
  });
});

describe("Avanzado: foco al navegar (revision UX 8.8)", () => {
  it("al entrar en una carpeta el foco pasa a la miga actual para no perder el lugar", async () => {
    await openResults();
    fireEvent.click(within(foldersTable()).getByRole("button", { name: format(t.open, { name: "Users" }) }));
    await waitFor(() => expect(within(foldersTable()).getByText("ana")).toBeInTheDocument());
    const crumbs = screen.getByRole("navigation", { name: t.breadcrumbLabel });
    await waitFor(() => expect(within(crumbs).getByText("Users")).toHaveFocus());
  });

  it("al cargar la pantalla por primera vez no roba el foco", async () => {
    await openResults();
    const crumbs = screen.getByRole("navigation", { name: t.breadcrumbLabel });
    expect(crumbs.contains(document.activeElement)).toBe(false);
  });
});
