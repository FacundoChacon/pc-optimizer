import type { Channel } from "@tauri-apps/api/core";
import { mockIPC } from "@tauri-apps/api/mocks";
import type {
  AppError,
  CleanupEntry,
  CleanupEvent,
  CleanupPreview,
  CleanupRun,
  CleanupRunCursor,
  CleanupRunKey,
  CleanupSummary,
  DuplicateCleanupPreview,
  DuplicateCleanupSummary,
  DuplicateGroup,
  DuplicateScanEvent,
  DuplicateScanSummary,
  EmptyAck,
  EmptyRequest,
  EmptyResult,
  FolderCrumb,
  FolderEntry,
  FolderScanEvent,
  FolderScanSummary,
  JunkCategory,
  JunkFinding,
  JunkScanSummary,
  LargeFileEntry,
  PickResult,
  PrivilegeContext,
  ScanEvent,
  Settings,
  StoreStatus,
} from "../../lib/ipc";
import { preview } from "./cleanupFixtures";
import { duplicatePreview } from "./duplicateFixtures";
import { sendChannelMessages } from "./junkFixtures";
import { snapshot } from "./monitorFixtures";

export type BackendCommand =
  | "get_app_info"
  | "get_settings"
  | "set_setting"
  | "get_store_status"
  | "recover_store"
  | "get_privilege_context"
  | "start_scan"
  | "cancel_scan"
  | "get_last_junk_scan"
  | "list_junk_findings"
  | "preview_cleanup"
  | "confirm_cleanup"
  | "cancel_cleanup"
  | "prepare_empty_recycle_bin"
  | "acknowledge_empty_recycle_bin"
  | "empty_recycle_bin"
  | "start_duplicate_scan"
  | "get_last_duplicate_scan"
  | "list_duplicate_groups"
  | "preview_duplicate_cleanup"
  | "confirm_duplicate_cleanup"
  | "start_folder_scan"
  | "get_last_folder_scan"
  | "list_folder_children"
  | "list_large_files"
  | "pick_folder_root"
  | "reveal_in_explorer"
  | "start_monitor_stream"
  | "stop_monitor_stream"
  | "list_cleanup_runs"
  | "list_cleanup_entries";

/** Una carpeta del analisis simulado con sus hijas y sus migas de pan. */
export interface DiskPageFixture {
  folder: FolderEntry;
  breadcrumb: FolderCrumb[];
  children: FolderEntry[];
  remainderBytes: number;
}

/** Una limpieza del historial simulado con sus items (las rutas se ocultan con otra cuenta). */
export interface HistoryFixture {
  run: CleanupRun;
  entries: CleanupEntry[];
}

export interface BackendCall {
  cmd: string;
  args: unknown;
}

export interface BackendState {
  appInfo: { name: string; version: string };
  settings: Settings;
  store: StoreStatus;
  /** Respuesta cruda de get_privilege_context (puede ser invalida a proposito). */
  privilege: unknown;
  backupFileName: string;
  /** Respuesta de get_last_junk_scan (`null` = todavia no hay escaneos). */
  lastScan: JunkScanSummary | null;
  /** Lo que devuelve start_scan al terminar; un resumen `done` pasa a ser `lastScan`. */
  scanResult: JunkScanSummary | null;
  /** Eventos que start_scan envia por el canal apenas llega la invocacion. */
  scanEvents: ScanEvent[];
  /** Hallazgos por categoria para list_junk_findings (se pagina con offset y limit). */
  findings: Partial<Record<JunkCategory, JunkFinding[]>>;
  /** Plantilla de preview_cleanup: se filtra por categorias pedidas y cada llamada recibe un id nuevo. */
  preview: CleanupPreview;
  /** Eventos que confirm_cleanup envia por el canal apenas llega la invocacion. */
  cleanupEvents: CleanupEvent[];
  /** Lo que devuelve confirm_cleanup al terminar; marca el ultimo escaneo como limpiado. */
  cleanupResult: CleanupSummary | null;
  /** La Papelera simulada: lo que mide `prepare` y lo que `empty` compara antes de vaciar. */
  bin: { itemCount: number; sizeBytes: number };
  /** Como termina `empty_recycle_bin` y en que queda la Papelera simulada. */
  emptyOutcome: Pick<
    EmptyResult,
    "status" | "itemsAfter" | "bytesAfter" | "freedBytes" | "detail" | "logged"
  >;
  /** Respuesta de get_last_duplicate_scan (`null` = todavia no hay escaneos de duplicados). */
  dupLast: DuplicateScanSummary | null;
  /** Lo que devuelve start_duplicate_scan al terminar; un resumen `done` pasa a ser `dupLast`. */
  dupScanResult: DuplicateScanSummary | null;
  /** Eventos que start_duplicate_scan envia por el canal apenas llega la invocacion. */
  dupScanEvents: DuplicateScanEvent[];
  /** Grupos para list_duplicate_groups (filtra por carpeta y tamano y pagina con offset y limit). */
  dupGroups: DuplicateGroup[];
  /** Plantilla de preview_duplicate_cleanup: cada llamada recibe un id nuevo. */
  dupPreview: DuplicateCleanupPreview;
  /** Eventos que confirm_duplicate_cleanup envia por el canal. */
  dupCleanupEvents: CleanupEvent[];
  /** Lo que devuelve confirm_duplicate_cleanup; sus grupos resueltos quedan resueltos en dupGroups. */
  dupCleanupResult: DuplicateCleanupSummary | null;
  /** Respuesta de get_last_folder_scan (`null` = todavia no hay analisis). */
  diskLast: FolderScanSummary | null;
  /** Lo que devuelve start_folder_scan al terminar; un resumen `done` pasa a ser `diskLast`. */
  diskScanResult: FolderScanSummary | null;
  /** Eventos que start_folder_scan envia por el canal apenas llega la invocacion. */
  diskScanEvents: FolderScanEvent[];
  /** Respuesta cruda de start_monitor_stream (puede ser invalida a proposito). */
  monitorSnapshot: unknown;
  /** Eventos crudos que start_monitor_stream envia por el canal apenas llega la invocacion. */
  monitorEvents: unknown[];
  /** Carpetas del ultimo analisis por id (`"root"` = la raiz). */
  diskPages: Record<string, DiskPageFixture>;
  /** Top de archivos grandes para list_large_files. */
  diskFiles: LargeFileEntry[];
  /** Respuestas sucesivas de pick_folder_root (un `AppError` hace fallar la llamada); sin mas, cancela. */
  pickQueue: Array<PickResult | AppError>;
  /** Limpiezas del historial para list_cleanup_runs y list_cleanup_entries (en cualquier orden). */
  history: HistoryFixture[];
  /** Comandos que deben fallar con este error. */
  failures: Partial<Record<BackendCommand, AppError>>;
  /** Errores de una sola vez por comando: cada llamada consume el primero de la cola. */
  failureQueue: Partial<Record<BackendCommand, AppError[]>>;
  /** Comandos que esperan a que se resuelva esta promesa antes de responder. */
  gates: Partial<Record<BackendCommand, Promise<void>>>;
}

export interface MockBackend {
  state: BackendState;
  calls: BackendCall[];
  count: (cmd: BackendCommand) => number;
  /** Envia eventos al canal de la ultima invocacion de start_scan (simula progreso). */
  emit: (events: readonly ScanEvent[]) => void;
  /** Envia eventos al canal de la ultima invocacion de confirm_cleanup (simula progreso). */
  emitCleanup: (events: readonly CleanupEvent[]) => void;
  /** Envia eventos al canal de la ultima invocacion de start_duplicate_scan. */
  emitDuplicates: (events: readonly DuplicateScanEvent[]) => void;
  /** Envia eventos al canal de la ultima invocacion de confirm_duplicate_cleanup. */
  emitDuplicateCleanup: (events: readonly CleanupEvent[]) => void;
  /** Envia eventos al canal de la ultima invocacion de start_folder_scan. */
  emitFolderScan: (events: readonly FolderScanEvent[]) => void;
  /** Envia eventos crudos al canal de la ultima invocacion de start_monitor_stream. */
  emitMonitor: (events: readonly unknown[]) => void;
}

export const READY: StoreStatus = { status: "ready" };
export const CORRUPT: StoreStatus = { status: "unavailable", reason: "corrupt", recoverable: true };

export const MATCH: PrivilegeContext = { status: "match", elevated: true };
export const OTHER_USER: PrivilegeContext = {
  status: "mismatch",
  reason: "different_user",
  elevated: true,
};

export const DEFAULT_SETTINGS: Settings = { uiMode: "simple", autostartTray: false, language: "es" };

function sameRunKey(a: CleanupRunKey, b: CleanupRunKey): boolean {
  if (a.kind === "scan") return b.kind === "scan" && a.scanId === b.scanId;
  return b.kind === "entry" && a.entryId === b.entryId;
}

/** Orden total de las limpiezas: fecha y luego tipo de clave (fila suelta antes que escaneo) e id. */
function compareRuns(aAt: number, aKey: CleanupRunKey, bAt: number, bKey: CleanupRunKey): number {
  const rank = (key: CleanupRunKey): number => (key.kind === "scan" ? 1 : 0);
  const id = (key: CleanupRunKey): number => (key.kind === "scan" ? key.scanId : key.entryId);
  return aAt - bAt || rank(aKey) - rank(bKey) || id(aKey) - id(bKey);
}

/** Simula los comandos IPC con estado en memoria y registra las llamadas. */
export function installBackend(overrides: Partial<BackendState> = {}): MockBackend {
  const state: BackendState = {
    appInfo: { name: "pc-optimizer", version: "0.1.0" },
    settings: { ...DEFAULT_SETTINGS },
    store: READY,
    privilege: MATCH,
    backupFileName: "data.db.corrupt-1700000000000.bak",
    lastScan: null,
    scanResult: null,
    scanEvents: [],
    findings: {},
    preview: preview(),
    cleanupEvents: [],
    cleanupResult: null,
    bin: { itemCount: 12, sizeBytes: 340_000_000 },
    emptyOutcome: {
      status: "emptied",
      itemsAfter: 0,
      bytesAfter: 0,
      freedBytes: 340_000_000,
      detail: null,
      logged: true,
    },
    dupLast: null,
    dupScanResult: null,
    dupScanEvents: [],
    dupGroups: [],
    dupPreview: duplicatePreview(),
    dupCleanupEvents: [],
    dupCleanupResult: null,
    diskLast: null,
    diskScanResult: null,
    diskScanEvents: [],
    monitorSnapshot: snapshot(),
    monitorEvents: [],
    diskPages: {},
    diskFiles: [],
    pickQueue: [],
    history: [],
    failures: {},
    failureQueue: {},
    gates: {},
    ...overrides,
  };
  const calls: BackendCall[] = [];
  let scanChannel: Channel<unknown> | null = null;
  let sentEvents = 0;
  let cleanupChannel: Channel<unknown> | null = null;
  let sentCleanupEvents = 0;
  let dupChannel: Channel<unknown> | null = null;
  let sentDup = 0;
  let dupCleanupChannel: Channel<unknown> | null = null;
  let sentDupCleanup = 0;
  let diskChannel: Channel<unknown> | null = null;
  let sentDisk = 0;
  let monitorChannel: Channel<unknown> | null = null;
  let sentMonitor = 0;
  let dupPreviewCounter = 0;
  let previewCounter = 0;
  let emptyCounter = 0;
  // Protocolo de 3 pasos del backend simulado: una solicitud vigente, con lo medido al prepararla.
  let emptyRequest: {
    id: number;
    itemCount: number;
    sizeBytes: number;
    token: number | null;
  } | null = null;

  const blockedInMismatch = (): void => {
    const privilege = state.privilege as { status?: unknown; reason?: unknown };
    if (privilege.status === "mismatch") {
      throw { kind: "recycle_bin_blocked", message: `blocked: ${String(privilege.reason)}` };
    }
  };

  const emit = (events: readonly ScanEvent[]): void => {
    if (scanChannel === null) return;
    // El runtime numera los mensajes del canal: el indice sigue creciendo entre envios.
    sendChannelMessages(scanChannel, events, sentEvents);
    sentEvents += events.length;
  };

  const emitCleanup = (events: readonly CleanupEvent[]): void => {
    if (cleanupChannel === null) return;
    sendChannelMessages(cleanupChannel, events, sentCleanupEvents);
    sentCleanupEvents += events.length;
  };

  const emitDuplicates = (events: readonly DuplicateScanEvent[]): void => {
    if (dupChannel === null) return;
    sendChannelMessages(dupChannel, events, sentDup);
    sentDup += events.length;
  };

  const emitDuplicateCleanup = (events: readonly CleanupEvent[]): void => {
    if (dupCleanupChannel === null) return;
    sendChannelMessages(dupCleanupChannel, events, sentDupCleanup);
    sentDupCleanup += events.length;
  };

  const emitFolderScan = (events: readonly FolderScanEvent[]): void => {
    if (diskChannel === null) return;
    sendChannelMessages(diskChannel, events, sentDisk);
    sentDisk += events.length;
  };

  const emitMonitor = (events: readonly unknown[]): void => {
    if (monitorChannel === null) return;
    sendChannelMessages(monitorChannel, events, sentMonitor);
    sentMonitor += events.length;
  };

  mockIPC(async (cmd, args) => {
    calls.push({ cmd, args });
    const command = cmd as BackendCommand;
    if (command === "start_scan") {
      scanChannel = (args as { onProgress: Channel<unknown> }).onProgress;
      sentEvents = 0;
      emit(state.scanEvents);
    }
    if (command === "confirm_cleanup") {
      cleanupChannel = (args as { onProgress: Channel<unknown> }).onProgress;
      sentCleanupEvents = 0;
      emitCleanup(state.cleanupEvents);
    }
    if (command === "start_duplicate_scan") {
      dupChannel = (args as { onProgress: Channel<unknown> }).onProgress;
      sentDup = 0;
      emitDuplicates(state.dupScanEvents);
    }
    if (command === "confirm_duplicate_cleanup") {
      dupCleanupChannel = (args as { onProgress: Channel<unknown> }).onProgress;
      sentDupCleanup = 0;
      emitDuplicateCleanup(state.dupCleanupEvents);
    }
    if (command === "start_folder_scan") {
      diskChannel = (args as { onProgress: Channel<unknown> }).onProgress;
      sentDisk = 0;
      emitFolderScan(state.diskScanEvents);
    }
    if (command === "start_monitor_stream") {
      monitorChannel = (args as { onSample: Channel<unknown> }).onSample;
      sentMonitor = 0;
      emitMonitor(state.monitorEvents);
    }
    const gate = state.gates[command];
    if (gate !== undefined) await gate;
    const queued = state.failureQueue[command]?.shift();
    if (queued !== undefined) throw queued;
    const failure = state.failures[command];
    if (failure !== undefined) throw failure;
    switch (command) {
      case "get_app_info":
        return state.appInfo;
      case "get_settings":
        return state.settings;
      case "set_setting": {
        const value = (args as { key: string; value: unknown }).value;
        if (value === "simple" || value === "advanced") {
          state.settings = { ...state.settings, uiMode: value };
        }
        return state.settings;
      }
      case "get_store_status":
        return state.store;
      case "get_privilege_context":
        return state.privilege;
      case "recover_store":
        state.store = READY;
        delete state.failures.get_settings;
        delete state.failures.set_setting;
        state.settings = { ...DEFAULT_SETTINGS };
        return { backupFileName: state.backupFileName };
      case "start_scan": {
        const result = state.scanResult;
        if (result !== null && result.status === "done") state.lastScan = result;
        return result;
      }
      case "cancel_scan":
        return null;
      case "get_last_junk_scan":
        return state.lastScan;
      case "list_junk_findings": {
        const request = args as { category: JunkCategory; offset: number; limit: number };
        const all = state.findings[request.category] ?? [];
        return { total: all.length, items: all.slice(request.offset, request.offset + request.limit) };
      }
      case "preview_cleanup": {
        const request = args as { scanId: number; categories: JunkCategory[] };
        const categories = state.preview.categories.filter((c) => request.categories.includes(c.category));
        const included = categories.filter((c) => c.status === "included");
        previewCounter += 1;
        return {
          ...state.preview,
          previewId: state.preview.previewId + previewCounter - 1,
          scanId: request.scanId,
          categories,
          totalItems: included.reduce((n, c) => n + c.itemCount, 0),
          totalBytes: included.reduce((n, c) => n + c.sizeBytes, 0),
        };
      }
      case "confirm_cleanup": {
        const result = state.cleanupResult;
        if (result !== null && state.lastScan !== null && result.processed > 0) {
          state.lastScan = { ...state.lastScan, cleanedAt: result.finishedAt };
        }
        return result;
      }
      case "cancel_cleanup":
        return null;
      case "start_duplicate_scan": {
        const result = state.dupScanResult;
        if (result !== null && result.status === "done") state.dupLast = result;
        return result;
      }
      case "get_last_duplicate_scan":
        return state.dupLast;
      case "list_duplicate_groups": {
        const request = args as { offset: number; limit: number; root?: string; minSizeBytes?: number };
        const matching = state.dupGroups.filter(
          (g) =>
            (request.root === undefined || g.files.some((f) => f.root === request.root)) &&
            (request.minSizeBytes === undefined || g.sizeBytes >= request.minSizeBytes),
        );
        return {
          scanId: state.dupLast?.scanId ?? 1,
          total: matching.length,
          groups: matching.slice(request.offset, request.offset + request.limit),
        };
      }
      case "preview_duplicate_cleanup": {
        const request = args as { scanId: number; decisions: Array<{ groupId: number; keepFileIds: number[] }> };
        dupPreviewCounter += 1;
        const items = request.decisions.reduce((n, d) => {
          const g = state.dupGroups.find((candidate) => candidate.groupId === d.groupId);
          return n + (g === undefined ? 0 : g.files.length - d.keepFileIds.length);
        }, 0);
        return {
          ...state.dupPreview,
          previewId: state.dupPreview.previewId + dupPreviewCounter - 1,
          scanId: request.scanId,
          groupCount: request.decisions.length,
          totalItems: items,
          categories: [
            {
              category: "duplicates",
              status: "included",
              itemCount: items,
              sizeBytes: state.dupPreview.totalBytes,
              recentCount: state.dupPreview.recentCount,
              recentBytes: state.dupPreview.recentBytes,
            },
          ],
        };
      }
      case "confirm_duplicate_cleanup": {
        const result = state.dupCleanupResult;
        if (result !== null) {
          state.dupGroups = state.dupGroups.map((g) =>
            result.resolvedGroupIds.includes(g.groupId) ? { ...g, resolvedAt: result.finishedAt } : g,
          );
          if (state.dupLast !== null) {
            state.dupLast = {
              ...state.dupLast,
              openGroupCount: Math.max(0, state.dupLast.openGroupCount - result.resolvedGroupIds.length),
            };
          }
        }
        return result === null ? null : { ...result, categories: [{ category: "duplicates" }] };
      }
      case "start_folder_scan": {
        const result = state.diskScanResult;
        if (result !== null && result.status === "done") state.diskLast = result;
        return result;
      }
      case "get_last_folder_scan":
        return state.diskLast;
      case "list_folder_children": {
        const request = args as {
          scanId: number;
          folderId: number | null;
          sort: "size" | "name";
          offset: number;
          limit: number;
        };
        const page = state.diskPages[request.folderId === null ? "root" : String(request.folderId)];
        if (page === undefined) throw { kind: "folder_not_found", message: "not found" };
        const children = [...page.children].sort((a, b) =>
          request.sort === "name" ? a.name.localeCompare(b.name, "es") : b.sizeBytes - a.sizeBytes,
        );
        return {
          scanId: request.scanId,
          folder: page.folder,
          breadcrumb: page.breadcrumb,
          total: children.length,
          items: children.slice(request.offset, request.offset + request.limit),
          remainderBytes: page.remainderBytes,
        };
      }
      case "list_large_files": {
        const request = args as {
          scanId: number;
          sort: "size" | "newest" | "oldest";
          offset: number;
          limit: number;
        };
        const files = [...state.diskFiles].sort((a, b) => {
          if (request.sort === "size") return b.sizeBytes - a.sizeBytes;
          if (a.modifiedAt === null) return b.modifiedAt === null ? 0 : 1;
          if (b.modifiedAt === null) return -1;
          return request.sort === "newest" ? b.modifiedAt - a.modifiedAt : a.modifiedAt - b.modifiedAt;
        });
        return {
          scanId: request.scanId,
          total: files.length,
          items: files.slice(request.offset, request.offset + request.limit),
        };
      }
      case "pick_folder_root": {
        const next = state.pickQueue.shift();
        if (next === undefined) return { status: "cancelled" };
        if ("kind" in next) throw next;
        return next;
      }
      case "reveal_in_explorer":
        return null;
      case "prepare_empty_recycle_bin": {
        blockedInMismatch();
        if (state.bin.itemCount === 0) throw { kind: "recycle_bin_already_empty", message: "empty" };
        emptyCounter += 1;
        emptyRequest = { id: emptyCounter, ...state.bin, token: null };
        const created: EmptyRequest = {
          requestId: emptyCounter,
          itemCount: state.bin.itemCount,
          sizeBytes: state.bin.sizeBytes,
          createdAt: 1_800_000_000_000,
          expiresAt: 1_800_000_120_000,
        };
        return created;
      }
      case "acknowledge_empty_recycle_bin": {
        blockedInMismatch();
        const { requestId } = args as { requestId: number };
        if (emptyRequest === null || emptyRequest.id !== requestId || emptyRequest.token !== null) {
          throw { kind: "empty_request_not_found", message: "not found" };
        }
        emptyRequest.token = 4_294_967_296 + emptyRequest.id;
        const acknowledged: EmptyAck = {
          requestId,
          confirmToken: emptyRequest.token,
          itemCount: emptyRequest.itemCount,
          sizeBytes: emptyRequest.sizeBytes,
          expiresAt: 1_800_000_120_000,
        };
        return acknowledged;
      }
      case "empty_recycle_bin": {
        blockedInMismatch();
        const { requestId, confirmToken } = args as { requestId: number; confirmToken: number };
        const taken = emptyRequest;
        emptyRequest = null;
        if (taken === null || taken.id !== requestId) {
          throw { kind: "empty_request_not_found", message: "not found" };
        }
        if (taken.token === null) throw { kind: "empty_not_acknowledged", message: "not acknowledged" };
        if (taken.token !== confirmToken) throw { kind: "empty_request_not_found", message: "not found" };
        if (taken.itemCount !== state.bin.itemCount || taken.sizeBytes !== state.bin.sizeBytes) {
          throw { kind: "recycle_bin_changed", message: "changed" };
        }
        const outcome = state.emptyOutcome;
        if (outcome.itemsAfter !== null && outcome.bytesAfter !== null) {
          state.bin = { itemCount: outcome.itemsAfter, sizeBytes: outcome.bytesAfter };
        }
        const finished: EmptyResult = {
          ...outcome,
          itemsBefore: taken.itemCount,
          bytesBefore: taken.sizeBytes,
          finishedAt: 1_800_000_001_000,
        };
        return finished;
      }
      case "list_cleanup_runs": {
        const { before, limit } = args as { before: CleanupRunCursor | null; limit: number };
        const ordered = [...state.history]
          .map((item) => item.run)
          .sort((a, b) => compareRuns(b.finishedAt, b.key, a.finishedAt, a.key));
        const rest =
          before === null
            ? ordered
            : ordered.filter((run) => compareRuns(run.finishedAt, run.key, before.finishedAt, before.key) < 0);
        const items = rest.slice(0, limit);
        const last = items[items.length - 1];
        const nextCursor =
          rest.length > limit && last !== undefined ? { finishedAt: last.finishedAt, key: last.key } : null;
        return { items, nextCursor };
      }
      case "list_cleanup_entries": {
        const { key, offset, limit } = args as { key: CleanupRunKey; offset: number; limit: number };
        const found = state.history.find((item) => sameRunKey(item.run.key, key));
        const entries = found?.entries ?? [];
        const hidden = (state.privilege as { status?: unknown }).status === "mismatch";
        return {
          total: entries.length,
          pathsHidden: hidden,
          items: entries
            .slice(offset, offset + limit)
            .map((entry) => (hidden ? { ...entry, path: null } : entry)),
        };
      }
      case "start_monitor_stream":
        return state.monitorSnapshot;
      case "stop_monitor_stream":
        return null;
      default:
        throw { kind: "internal", message: `unknown command ${cmd}` };
    }
  });

  return {
    state,
    calls,
    count: (cmd) => calls.filter((call) => call.cmd === cmd).length,
    emit,
    emitCleanup,
    emitDuplicates,
    emitDuplicateCleanup,
    emitFolderScan,
    emitMonitor,
  };
}
