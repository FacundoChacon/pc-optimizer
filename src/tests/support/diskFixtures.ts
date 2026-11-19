import type {
  FolderEntry,
  FolderPage,
  FolderScanSummary,
  LargeFileEntry,
} from "../../lib/ipc";
import type { DiskPageFixture } from "./mockBackend";

export const GB = 1024 ** 3;
export const MB = 1024 ** 2;
export const SCANNED_AT = 1_800_000_000_000;
export const FILE_DATE = 1_700_000_000_000;

/** Una carpeta del analisis; `overrides` ajusta lo que cada test necesita. */
export function folderEntry(
  id: number,
  name: string,
  sizeBytes: number,
  overrides: Partial<FolderEntry> = {},
): FolderEntry {
  return {
    id,
    name,
    path: `C:\\${name}`,
    sizeBytes,
    directBytes: 0,
    fileCount: 10,
    dirCount: 2,
    access: "complete",
    opaque: null,
    system: false,
    junkHint: false,
    hasChildren: true,
    ...overrides,
  };
}

/** Resumen `done` de `C:\` con 100 GB usados de 500 GB y 380 GB analizados. */
export function diskSummary(overrides: Partial<FolderScanSummary> = {}): FolderScanSummary {
  return {
    scanId: 1,
    status: "done",
    startedAt: SCANNED_AT,
    finishedAt: SCANNED_AT + 5_000,
    root: { kind: "system_drive", path: "C:\\", folderId: 1 },
    volume: { totalBytes: 500 * GB, freeBytes: 120 * GB },
    unaccountedBytes: 0,
    overCounted: false,
    totalBytes: 380 * GB,
    fileCount: 412_000,
    dirCount: 52_000,
    inaccessibleDirs: 0,
    reparseSkipped: 0,
    cloudFiles: 0,
    opaqueDirs: 0,
    foldersStored: 52_001,
    foldersTruncated: false,
    issues: [],
    ...overrides,
  };
}

/** La carpeta raiz de `diskSummary`. */
export function rootFolder(overrides: Partial<FolderEntry> = {}): FolderEntry {
  return folderEntry(1, "C:\\", 380 * GB, { path: "C:\\", ...overrides });
}

/** Seis carpetas de la raiz, de mayor a menor. */
export function rootChildren(): FolderEntry[] {
  return [
    folderEntry(2, "Windows", 120 * GB, { system: true }),
    folderEntry(3, "Users", 100 * GB),
    folderEntry(4, "Program Files", 60 * GB, { system: true }),
    folderEntry(5, "ProgramData", 40 * GB, { system: true }),
    folderEntry(6, "Juegos", 30 * GB),
    folderEntry(7, "Otros", 20 * GB),
  ];
}

/** Pagina de los hijos de la raiz (el resto = lo que no cubren los hijos guardados). */
export function rootPage(items: FolderEntry[] = rootChildren(), overrides: Partial<FolderPage> = {}): FolderPage {
  return {
    scanId: 1,
    folder: rootFolder(),
    breadcrumb: [{ id: 1, name: "C:\\" }],
    total: items.length,
    items,
    remainderBytes: 10 * GB,
    ...overrides,
  };
}

/** Un archivo grande del top. */
export function largeFile(
  id: number,
  name: string,
  sizeBytes: number,
  overrides: Partial<LargeFileEntry> = {},
): LargeFileEntry {
  return {
    id,
    name,
    path: `C:\\Users\\ana\\Videos\\${name}`,
    folderId: 3,
    sizeBytes,
    modifiedAt: FILE_DATE,
    ...overrides,
  };
}

/** Seis archivos de mayor a menor tamano; el mas nuevo es el mas chico. */
export function topFiles(): LargeFileEntry[] {
  return [
    largeFile(1, "viaje.mp4", 12 * GB, { modifiedAt: FILE_DATE }),
    largeFile(2, "maquina.vhdx", 9 * GB, { modifiedAt: FILE_DATE + 1_000 }),
    largeFile(3, "juego.pak", 7 * GB, { modifiedAt: FILE_DATE + 2_000 }),
    largeFile(4, "backup.zip", 5 * GB, { modifiedAt: null }),
    largeFile(5, "pelicula.mkv", 4 * GB, { modifiedAt: FILE_DATE + 4_000 }),
    largeFile(6, "foto.raw", 3 * GB, { modifiedAt: FILE_DATE + 5_000 }),
  ];
}

/** Paginas del analisis simulado: la raiz y `Users` (id 3) con dos perfiles. */
export function defaultPages(): Record<string, DiskPageFixture> {
  const users = folderEntry(3, "Users", 100 * GB, { path: "C:\\Users" });
  return {
    root: {
      folder: rootFolder({ directBytes: 2 * GB }),
      breadcrumb: [{ id: 1, name: "C:\\" }],
      children: rootChildren(),
      remainderBytes: 8 * GB,
    },
    "3": {
      folder: users,
      breadcrumb: [
        { id: 1, name: "C:\\" },
        { id: 3, name: "Users" },
      ],
      children: [
        folderEntry(8, "ana", 80 * GB, { path: "C:\\Users\\ana" }),
        folderEntry(9, "bob", 20 * GB, {
          path: "C:\\Users\\bob",
          opaque: "other_user_profile",
          hasChildren: false,
        }),
      ],
      remainderBytes: 0,
    },
  };
}
