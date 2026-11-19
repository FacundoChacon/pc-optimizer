import type { Channel } from "@tauri-apps/api/core";
import { mockIPC } from "@tauri-apps/api/mocks";
import type {
  AppError,
  CleanupEvent,
  CleanupPreview,
  CleanupSummary,
  JunkCategory,
  JunkFinding,
  JunkScanSummary,
  PrivilegeContext,
  ScanEvent,
  Settings,
  StoreStatus,
} from "../../lib/ipc";
import { preview } from "./cleanupFixtures";
import { sendChannelMessages } from "./junkFixtures";

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
  | "cancel_cleanup";

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
  let previewCounter = 0;

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
  };
}
