import { type InvokeArgs, invoke } from "@tauri-apps/api/core";

/** Espejo de `commands::app_info::AppInfo` (Rust). */
export interface AppInfo {
  name: string;
  version: string;
}

/** Espejo de `error::AppError` serializado (Rust): `kind` estable en snake_case. */
export interface AppError {
  kind: string;
  message: string;
}

const UNKNOWN_ERROR_MESSAGE = "Unknown error";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** Normaliza cualquier valor rechazado a un `AppError`. */
export function toAppError(error: unknown): AppError {
  if (isRecord(error) && typeof error["kind"] === "string" && typeof error["message"] === "string") {
    return { kind: error["kind"], message: error["message"] };
  }
  if (typeof error === "string" && error.length > 0) {
    return { kind: "unknown", message: error };
  }
  if (error instanceof Error && error.message.length > 0) {
    return { kind: "unknown", message: error.message };
  }
  return { kind: "unknown", message: UNKNOWN_ERROR_MESSAGE };
}

async function call<T>(command: string, args?: InvokeArgs): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw toAppError(error);
  }
}

function invalidResponse(command: string): AppError {
  return { kind: "invalid_response", message: `Unexpected response from ${command}` };
}

/** Nombre y version de la app, desde el backend. */
export function getAppInfo(): Promise<AppInfo> {
  return call<AppInfo>("get_app_info");
}

/** Modo de interfaz (espejo de `domain::settings::UiMode`). */
export type UiMode = "simple" | "advanced";

/** Espejo de `commands::settings::SettingsDto` (Rust). */
export interface Settings {
  uiMode: UiMode;
  autostartTray: boolean;
  language: "es";
}

/** Ajustes escribibles hoy (allowlist del backend): clave -> tipo de valor. */
export interface WritableSettings {
  ui_mode: UiMode;
}

function decodeSettings(raw: unknown, command: string): Settings {
  if (
    isRecord(raw) &&
    (raw["uiMode"] === "simple" || raw["uiMode"] === "advanced") &&
    typeof raw["autostartTray"] === "boolean" &&
    raw["language"] === "es"
  ) {
    return { uiMode: raw["uiMode"], autostartTray: raw["autostartTray"], language: "es" };
  }
  throw invalidResponse(command);
}

/** Configuracion efectiva de la app. */
export async function getSettings(): Promise<Settings> {
  return decodeSettings(await call<unknown>("get_settings"), "get_settings");
}

/** Guarda un ajuste escribible y devuelve la configuracion releida por el backend. */
export async function setSetting<K extends keyof WritableSettings>(
  key: K,
  value: WritableSettings[K],
): Promise<Settings> {
  return decodeSettings(await call<unknown>("set_setting", { key, value }), "set_setting");
}

/** Motivo por el que la base no esta disponible (espejo de `UnavailableReason`). */
export type StoreUnavailableReason =
  | "corrupt"
  | "schema_too_new"
  | "migration_failed"
  | "unavailable";

/** Espejo de `commands::store::StoreStatusDto` (Rust). */
export type StoreStatus =
  | { status: "ready" }
  | { status: "unavailable"; reason: StoreUnavailableReason; recoverable: boolean };

/** Espejo de `commands::store::RecoveryDto` (Rust): solo el nombre del respaldo. */
export interface RecoveryResult {
  backupFileName: string;
}

const STORE_REASONS: readonly StoreUnavailableReason[] = [
  "corrupt",
  "schema_too_new",
  "migration_failed",
  "unavailable",
];

function isStoreReason(value: unknown): value is StoreUnavailableReason {
  return STORE_REASONS.some((reason) => reason === value);
}

function decodeStoreStatus(raw: unknown, command: string): StoreStatus {
  if (isRecord(raw)) {
    if (raw["status"] === "ready") return { status: "ready" };
    const reason = raw["reason"];
    const recoverable = raw["recoverable"];
    if (raw["status"] === "unavailable" && isStoreReason(reason) && typeof recoverable === "boolean") {
      return { status: "unavailable", reason, recoverable };
    }
  }
  throw invalidResponse(command);
}

/** Motivo del aviso de privilegios (espejo de `MismatchReason::as_str`). */
export type PrivilegeMismatchReason = "different_user" | "undetermined";

/** Espejo de `commands::privileges::PrivilegeContextDto` (Rust): sin SID, nombres ni rutas. */
export type PrivilegeContext =
  | { status: "match"; elevated: boolean }
  | { status: "mismatch"; reason: PrivilegeMismatchReason; elevated: boolean };

function decodePrivilegeContext(raw: unknown, command: string): PrivilegeContext {
  if (isRecord(raw) && typeof raw["elevated"] === "boolean") {
    const elevated = raw["elevated"];
    if (raw["status"] === "match") return { status: "match", elevated };
    const reason = raw["reason"];
    if (raw["status"] === "mismatch" && (reason === "different_user" || reason === "undetermined")) {
      return { status: "mismatch", reason, elevated };
    }
  }
  throw invalidResponse(command);
}

/** Contexto de privilegios calculado por el backend al arrancar. */
export async function getPrivilegeContext(): Promise<PrivilegeContext> {
  return decodePrivilegeContext(await call<unknown>("get_privilege_context"), "get_privilege_context");
}

/** Estado de la base local. */
export async function getStoreStatus(): Promise<StoreStatus> {
  return decodeStoreStatus(await call<unknown>("get_store_status"), "get_store_status");
}

/** Respalda y recrea la base (solo tras confirmacion del usuario). */
export async function recoverStore(): Promise<RecoveryResult> {
  const raw = await call<unknown>("recover_store");
  if (isRecord(raw) && typeof raw["backupFileName"] === "string" && raw["backupFileName"].length > 0) {
    return { backupFileName: raw["backupFileName"] };
  }
  throw invalidResponse("recover_store");
}
