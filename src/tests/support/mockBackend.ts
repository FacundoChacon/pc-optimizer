import { mockIPC } from "@tauri-apps/api/mocks";
import type { AppError, PrivilegeContext, Settings, StoreStatus } from "../../lib/ipc";

export type BackendCommand =
  | "get_app_info"
  | "get_settings"
  | "set_setting"
  | "get_store_status"
  | "recover_store"
  | "get_privilege_context";

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
  /** Comandos que deben fallar con este error. */
  failures: Partial<Record<BackendCommand, AppError>>;
  /** Comandos que esperan a que se resuelva esta promesa antes de responder. */
  gates: Partial<Record<BackendCommand, Promise<void>>>;
}

export interface MockBackend {
  state: BackendState;
  calls: BackendCall[];
  count: (cmd: BackendCommand) => number;
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

/** Simula los 6 comandos IPC con estado en memoria y registra las llamadas. */
export function installBackend(overrides: Partial<BackendState> = {}): MockBackend {
  const state: BackendState = {
    appInfo: { name: "pc-optimizer", version: "0.1.0" },
    settings: { ...DEFAULT_SETTINGS },
    store: READY,
    privilege: MATCH,
    backupFileName: "data.db.corrupt-1700000000000.bak",
    failures: {},
    gates: {},
    ...overrides,
  };
  const calls: BackendCall[] = [];

  mockIPC(async (cmd, args) => {
    calls.push({ cmd, args });
    const command = cmd as BackendCommand;
    const gate = state.gates[command];
    if (gate !== undefined) await gate;
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
      default:
        throw { kind: "internal", message: `unknown command ${cmd}` };
    }
  });

  return { state, calls, count: (cmd) => calls.filter((call) => call.cmd === cmd).length };
}
