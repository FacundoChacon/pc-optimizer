import { Channel, type InvokeArgs, invoke } from "@tauri-apps/api/core";

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

/** Las 11 categorias de la whitelist (espejo de `JunkCategory::as_str`). */
export type JunkCategory =
  | "temp_user"
  | "temp_windows"
  | "browser_cache_chrome"
  | "browser_cache_edge"
  | "browser_cache_firefox"
  | "browser_cache_brave"
  | "old_logs"
  | "wu_cache"
  | "thumbnail_cache"
  | "crash_dumps"
  | "recycle_bin";

const JUNK_CATEGORIES: readonly JunkCategory[] = [
  "temp_user",
  "temp_windows",
  "browser_cache_chrome",
  "browser_cache_edge",
  "browser_cache_firefox",
  "browser_cache_brave",
  "old_logs",
  "wu_cache",
  "thumbnail_cache",
  "crash_dumps",
  "recycle_bin",
];

/** Alcance de una categoria (espejo de `CategoryScope::as_str`). */
export type CategoryScope = "machine" | "per_user" | "mixed";

/** Estado de una categoria al terminar el escaneo (espejo de `CategoryStatus::as_str`). */
export type CategoryStatus = "done" | "partial" | "blocked" | "unavailable";

/** Espejo de `commands::scan::CategoryDto` (Rust). */
export interface CategorySummary {
  category: JunkCategory;
  scope: CategoryScope;
  status: CategoryStatus;
  /** Solo con `partial` y `blocked`. */
  blockReason?: PrivilegeMismatchReason;
  itemCount: number;
  sizeBytes: number;
  skippedCount: number;
  inaccessibleCount: number;
}

/** Espejo de `commands::scan::JunkScanSummaryDto` (Rust). Un escaneo `cancelled` no trae categorias. */
export interface JunkScanSummary {
  scanId: number;
  status: "done" | "cancelled";
  startedAt: number;
  finishedAt: number | null;
  totalItems: number;
  totalBytes: number;
  categories: CategorySummary[];
}

/** Espejo de `commands::scan::ScanEventDto` (Rust). */
export type ScanEvent =
  | { type: "started"; scanId: number }
  | { type: "category"; category: JunkCategory }
  | { type: "progress"; category: JunkCategory; filesSeen: number; eligibleBytes: number };

/** Espejo de `commands::scan::FindingDto` (Rust). `skipReason` es null en los elegibles. */
export interface JunkFinding {
  id: number;
  path: string;
  sizeBytes: number;
  modifiedAt: number | null;
  skipReason: string | null;
}

/** Espejo de `commands::scan::FindingPageDto` (Rust). */
export interface JunkFindingPage {
  total: number;
  items: JunkFinding[];
}

function isJunkCategory(value: unknown): value is JunkCategory {
  return JUNK_CATEGORIES.some((category) => category === value);
}

function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isMismatchReason(value: unknown): value is PrivilegeMismatchReason {
  return value === "different_user" || value === "undetermined";
}

function decodeScope(value: unknown): CategoryScope | null {
  return value === "machine" || value === "per_user" || value === "mixed" ? value : null;
}

function decodeCategoryStatus(value: unknown): CategoryStatus | null {
  return value === "done" || value === "partial" || value === "blocked" || value === "unavailable"
    ? value
    : null;
}

function decodeCategory(raw: unknown): CategorySummary | null {
  if (!isRecord(raw)) return null;
  const category = raw["category"];
  const scope = decodeScope(raw["scope"]);
  const status = decodeCategoryStatus(raw["status"]);
  const { itemCount, sizeBytes, skippedCount, inaccessibleCount } = raw;
  if (
    !isJunkCategory(category) ||
    scope === null ||
    status === null ||
    !isCount(itemCount) ||
    !isCount(sizeBytes) ||
    !isCount(skippedCount) ||
    !isCount(inaccessibleCount)
  ) {
    return null;
  }
  const base = { category, scope, status, itemCount, sizeBytes, skippedCount, inaccessibleCount };
  if (status === "partial" || status === "blocked") {
    const blockReason = raw["blockReason"];
    return isMismatchReason(blockReason) ? { ...base, blockReason } : null;
  }
  return base;
}

/** Un resumen `done` trae las 11 categorias, cada una una sola vez; uno `cancelled`, ninguna. */
function hasExpectedCategories(status: "done" | "cancelled", categories: CategorySummary[]): boolean {
  if (status === "cancelled") return categories.length === 0;
  const distinct = new Set(categories.map((category) => category.category));
  return categories.length === JUNK_CATEGORIES.length && distinct.size === JUNK_CATEGORIES.length;
}

function decodeJunkScanSummary(raw: unknown, command: string): JunkScanSummary {
  if (isRecord(raw) && Array.isArray(raw["categories"])) {
    const { scanId, status, startedAt, finishedAt, totalItems, totalBytes } = raw;
    const decoded = raw["categories"].map(decodeCategory);
    const categories = decoded.filter((category): category is CategorySummary => category !== null);
    if (
      (status === "done" || status === "cancelled") &&
      isCount(scanId) &&
      isCount(startedAt) &&
      (finishedAt === null || isCount(finishedAt)) &&
      isCount(totalItems) &&
      isCount(totalBytes) &&
      categories.length === decoded.length &&
      hasExpectedCategories(status, categories)
    ) {
      return { scanId, status, startedAt, finishedAt, totalItems, totalBytes, categories };
    }
  }
  throw invalidResponse(command);
}

function decodeScanEvent(raw: unknown): ScanEvent | null {
  if (!isRecord(raw)) return null;
  const category = raw["category"];
  switch (raw["type"]) {
    case "started":
      return isCount(raw["scanId"]) ? { type: "started", scanId: raw["scanId"] } : null;
    case "category":
      return isJunkCategory(category) ? { type: "category", category } : null;
    case "progress": {
      const { filesSeen, eligibleBytes } = raw;
      return isJunkCategory(category) && isCount(filesSeen) && isCount(eligibleBytes)
        ? { type: "progress", category, filesSeen, eligibleBytes }
        : null;
    }
    default:
      return null;
  }
}

function decodeFinding(raw: unknown): JunkFinding | null {
  if (!isRecord(raw)) return null;
  const { id, path, sizeBytes, modifiedAt, skipReason } = raw;
  if (
    isCount(id) &&
    typeof path === "string" &&
    isCount(sizeBytes) &&
    (modifiedAt === null || isCount(modifiedAt)) &&
    (skipReason === null || typeof skipReason === "string")
  ) {
    return { id, path, sizeBytes, modifiedAt, skipReason };
  }
  return null;
}

function decodeFindingPage(raw: unknown, command: string): JunkFindingPage {
  if (isRecord(raw) && isCount(raw["total"]) && Array.isArray(raw["items"])) {
    const items = raw["items"].map(decodeFinding);
    const decoded = items.filter((item): item is JunkFinding => item !== null);
    if (decoded.length === items.length) return { total: raw["total"], items: decoded };
  }
  throw invalidResponse(command);
}

/**
 * Inicia un escaneo de basura. El progreso llega por el canal de la invocacion (los eventos
 * invalidos se ignoran) y la promesa se resuelve con el resumen final (`done` o `cancelled`).
 */
export async function startJunkScan(onEvent: (event: ScanEvent) => void): Promise<JunkScanSummary> {
  const channel = new Channel<unknown>((raw) => {
    const event = decodeScanEvent(raw);
    if (event !== null) onEvent(event);
  });
  const raw = await call<unknown>("start_scan", { kind: "junk", onProgress: channel });
  return decodeJunkScanSummary(raw, "start_scan");
}

/** Pide cancelar el escaneo en curso (`scan_not_found` si no es el activo). */
export async function cancelScan(scanId: number): Promise<void> {
  await call<unknown>("cancel_scan", { scanId });
}

/** Resumen del ultimo escaneo de basura terminado, o `null` si todavia no hay ninguno. */
export async function getLastJunkScan(): Promise<JunkScanSummary | null> {
  const raw = await call<unknown>("get_last_junk_scan");
  return raw === null ? null : decodeJunkScanSummary(raw, "get_last_junk_scan");
}

/** Una pagina de hallazgos de una categoria del ultimo escaneo terminado. */
export async function listJunkFindings(
  scanId: number,
  category: JunkCategory,
  offset: number,
  limit: number,
): Promise<JunkFindingPage> {
  return decodeFindingPage(
    await call<unknown>("list_junk_findings", { scanId, category, offset, limit }),
    "list_junk_findings",
  );
}
