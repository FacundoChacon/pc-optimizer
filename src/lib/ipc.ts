import { invoke } from "@tauri-apps/api/core";

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

async function call<T>(command: string): Promise<T> {
  try {
    return await invoke<T>(command);
  } catch (error: unknown) {
    throw toAppError(error);
  }
}

/** Nombre y version de la app, desde el backend. */
export function getAppInfo(): Promise<AppInfo> {
  return call<AppInfo>("get_app_info");
}
