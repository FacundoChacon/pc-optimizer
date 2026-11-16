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
  /** Fecha de la ultima limpieza que registro items sobre este escaneo; `null` si nunca se limpio. */
  cleanedAt: number | null;
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
    const { scanId, status, startedAt, finishedAt, totalItems, totalBytes, cleanedAt } = raw;
    const decoded = raw["categories"].map(decodeCategory);
    const categories = decoded.filter((category): category is CategorySummary => category !== null);
    if (
      (status === "done" || status === "cancelled") &&
      isCount(scanId) &&
      isCount(startedAt) &&
      (finishedAt === null || isCount(finishedAt)) &&
      isCount(totalItems) &&
      isCount(totalBytes) &&
      (cleanedAt === null || isCount(cleanedAt)) &&
      categories.length === decoded.length &&
      hasExpectedCategories(status, categories)
    ) {
      return { scanId, status, startedAt, finishedAt, totalItems, totalBytes, cleanedAt, categories };
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

/** Estado de una categoria en el preview (espejo de `PreviewCategoryDto.status`). */
export type PreviewCategoryStatus = "included" | "blocked";

/** Espejo de `commands::cleanup::PreviewCategoryDto` (Rust). `blockReason` solo con `blocked`. */
export interface PreviewCategory {
  category: JunkCategory;
  status: PreviewCategoryStatus;
  blockReason?: PrivilegeMismatchReason;
  itemCount: number;
  sizeBytes: number;
  /** Excluidos por haberse escrito hace menos de 24 h: se informan, no se mueven. */
  recentCount: number;
  recentBytes: number;
}

/** Espejo de `commands::cleanup::CleanupPreviewDto` (Rust): sin rutas. */
export interface CleanupPreview {
  previewId: number;
  scanId: number;
  createdAt: number;
  expiresAt: number;
  totalItems: number;
  totalBytes: number;
  /** `false` si la Papelera esta deshabilitada o su politica no se pudo leer. */
  recycleBinAvailable: boolean;
  categories: PreviewCategory[];
}

/** Como termino una limpieza (espejo de `CleanupSummaryDto.status`). */
export type CleanupStatus = "completed" | "cancelled" | "aborted";

/** Por que se aborto una limpieza (espejo de `AbortReason::as_str`). */
export type CleanupAbortReason = "log_unavailable" | "recycle_bin_unavailable" | "trash_unavailable";

/** Resultado de un item que NO se movio (los movidos no figuran en `reasons`). */
export type CleanupReasonOutcome = "skipped_locked" | "skipped_protected" | "skipped_cloud" | "error";

/** Espejo de `commands::cleanup::ReasonDto`: cantidad por resultado y codigo de detalle. */
export interface CleanupReason {
  outcome: CleanupReasonOutcome;
  detail: string;
  count: number;
}

/** Espejo de `commands::cleanup::CategorySummaryDto`. */
export interface CleanupCategorySummary {
  category: JunkCategory;
  movedCount: number;
  movedBytes: number;
  skippedCount: number;
  errorCount: number;
}

/** Espejo de `commands::cleanup::CleanupSummaryDto` (Rust): sin rutas. `abortReason` solo con `aborted`. */
export interface CleanupSummary {
  previewId: number;
  scanId: number;
  status: CleanupStatus;
  abortReason: CleanupAbortReason | null;
  startedAt: number;
  finishedAt: number;
  totalItems: number;
  processed: number;
  notProcessed: number;
  movedCount: number;
  movedBytes: number;
  skippedLocked: number;
  skippedProtected: number;
  skippedCloud: number;
  errors: number;
  reasons: CleanupReason[];
  categories: CleanupCategorySummary[];
}

/** Espejo de `commands::cleanup::CleanupEventDto` (Rust). */
export type CleanupEvent =
  | { type: "started"; previewId: number; total: number }
  | { type: "progress"; processed: number; total: number; movedCount: number; movedBytes: number };

function decodePreviewCategory(raw: unknown): PreviewCategory | null {
  if (!isRecord(raw)) return null;
  const { category, status, itemCount, sizeBytes, recentCount, recentBytes } = raw;
  if (
    !isJunkCategory(category) ||
    !isCount(itemCount) ||
    !isCount(sizeBytes) ||
    !isCount(recentCount) ||
    !isCount(recentBytes)
  ) {
    return null;
  }
  const base = { category, itemCount, sizeBytes, recentCount, recentBytes };
  const blockReason = raw["blockReason"];
  if (status === "included") {
    return blockReason === undefined ? { ...base, status } : null;
  }
  if (status === "blocked") {
    return isMismatchReason(blockReason) ? { ...base, status, blockReason } : null;
  }
  return null;
}

function decodeCleanupPreview(raw: unknown, command: string): CleanupPreview {
  if (isRecord(raw) && Array.isArray(raw["categories"])) {
    const { previewId, scanId, createdAt, expiresAt, totalItems, totalBytes, recycleBinAvailable } = raw;
    const decoded = raw["categories"].map(decodePreviewCategory);
    const categories = decoded.filter((category): category is PreviewCategory => category !== null);
    if (
      isCount(previewId) &&
      isCount(scanId) &&
      isCount(createdAt) &&
      isCount(expiresAt) &&
      isCount(totalItems) &&
      isCount(totalBytes) &&
      typeof recycleBinAvailable === "boolean" &&
      categories.length === decoded.length
    ) {
      return {
        previewId,
        scanId,
        createdAt,
        expiresAt,
        totalItems,
        totalBytes,
        recycleBinAvailable,
        categories,
      };
    }
  }
  throw invalidResponse(command);
}

const REASON_OUTCOMES: readonly CleanupReasonOutcome[] = [
  "skipped_locked",
  "skipped_protected",
  "skipped_cloud",
  "error",
];

const ABORT_REASONS: readonly CleanupAbortReason[] = [
  "log_unavailable",
  "recycle_bin_unavailable",
  "trash_unavailable",
];

/** Codigo de detalle estable del backend: ASCII `snake_case` (puede llevar un HRESULT en hexadecimal). */
const DETAIL_CODE = /^[a-z0-9_]+$/;

function decodeReason(raw: unknown): CleanupReason | null {
  if (!isRecord(raw)) return null;
  const { outcome, detail, count } = raw;
  const known = REASON_OUTCOMES.find((candidate) => candidate === outcome);
  if (known !== undefined && typeof detail === "string" && DETAIL_CODE.test(detail) && isCount(count)) {
    return { outcome: known, detail, count };
  }
  return null;
}

function decodeCleanupCategory(raw: unknown): CleanupCategorySummary | null {
  if (!isRecord(raw)) return null;
  const { category, movedCount, movedBytes, skippedCount, errorCount } = raw;
  if (
    isJunkCategory(category) &&
    isCount(movedCount) &&
    isCount(movedBytes) &&
    isCount(skippedCount) &&
    isCount(errorCount)
  ) {
    return { category, movedCount, movedBytes, skippedCount, errorCount };
  }
  return null;
}

/** `aborted` exige un motivo; cualquier otro estado exige `null`. */
function decodeAbort(status: unknown, abortReason: unknown): CleanupAbortReason | null | undefined {
  if (status === "aborted") return ABORT_REASONS.find((reason) => reason === abortReason);
  return abortReason === null ? null : undefined;
}

function decodeCleanupSummary(raw: unknown, command: string): CleanupSummary {
  if (isRecord(raw) && Array.isArray(raw["reasons"]) && Array.isArray(raw["categories"])) {
    const {
      previewId,
      scanId,
      status,
      startedAt,
      finishedAt,
      totalItems,
      processed,
      notProcessed,
      movedCount,
      movedBytes,
      skippedLocked,
      skippedProtected,
      skippedCloud,
      errors,
    } = raw;
    const abortReason = decodeAbort(status, raw["abortReason"]);
    const decodedReasons = raw["reasons"].map(decodeReason);
    const reasons = decodedReasons.filter((reason): reason is CleanupReason => reason !== null);
    const decodedCategories = raw["categories"].map(decodeCleanupCategory);
    const categories = decodedCategories.filter(
      (category): category is CleanupCategorySummary => category !== null,
    );
    if (
      isCount(previewId) &&
      isCount(scanId) &&
      (status === "completed" || status === "cancelled" || status === "aborted") &&
      abortReason !== undefined &&
      isCount(startedAt) &&
      isCount(finishedAt) &&
      isCount(totalItems) &&
      isCount(processed) &&
      isCount(notProcessed) &&
      isCount(movedCount) &&
      isCount(movedBytes) &&
      isCount(skippedLocked) &&
      isCount(skippedProtected) &&
      isCount(skippedCloud) &&
      isCount(errors) &&
      reasons.length === decodedReasons.length &&
      categories.length === decodedCategories.length
    ) {
      return {
        previewId,
        scanId,
        status,
        abortReason,
        startedAt,
        finishedAt,
        totalItems,
        processed,
        notProcessed,
        movedCount,
        movedBytes,
        skippedLocked,
        skippedProtected,
        skippedCloud,
        errors,
        reasons,
        categories,
      };
    }
  }
  throw invalidResponse(command);
}

function decodeCleanupEvent(raw: unknown): CleanupEvent | null {
  if (!isRecord(raw)) return null;
  switch (raw["type"]) {
    case "started": {
      const { previewId, total } = raw;
      return isCount(previewId) && isCount(total) ? { type: "started", previewId, total } : null;
    }
    case "progress": {
      const { processed, total, movedCount, movedBytes } = raw;
      return isCount(processed) && isCount(total) && isCount(movedCount) && isCount(movedBytes)
        ? { type: "progress", processed, total, movedCount, movedBytes }
        : null;
    }
    default:
      return null;
  }
}

/**
 * Pide el preview de limpieza de las `categories` del escaneo `scanId`. Solo viajan el
 * identificador y los nombres de categoria: nunca rutas.
 */
export async function previewCleanup(scanId: number, categories: readonly JunkCategory[]): Promise<CleanupPreview> {
  return decodeCleanupPreview(
    await call<unknown>("preview_cleanup", { scanId, categories }),
    "preview_cleanup",
  );
}

/**
 * Confirma el preview `previewId`. El progreso llega por el canal de la invocacion (los eventos
 * invalidos se ignoran) y la promesa se resuelve con el resumen (`completed`, `cancelled` o `aborted`).
 */
export async function confirmCleanup(
  previewId: number,
  onEvent: (event: CleanupEvent) => void,
): Promise<CleanupSummary> {
  const channel = new Channel<unknown>((raw) => {
    const event = decodeCleanupEvent(raw);
    if (event !== null) onEvent(event);
  });
  const raw = await call<unknown>("confirm_cleanup", { previewId, onProgress: channel });
  return decodeCleanupSummary(raw, "confirm_cleanup");
}

/** Pide cancelar la limpieza en curso del preview `previewId` (`preview_not_found` si no corre). */
export async function cancelCleanup(previewId: number): Promise<void> {
  await call<unknown>("cancel_cleanup", { previewId });
}

/** Espejo de `commands::recycle_bin::EmptyRequestDto` (Rust): paso 1, la Papelera medida. Sin rutas. */
export interface EmptyRequest {
  requestId: number;
  itemCount: number;
  sizeBytes: number;
  createdAt: number;
  expiresAt: number;
}

/** Espejo de `commands::recycle_bin::EmptyAckDto` (Rust): paso 2, con el token que autoriza el paso 3. */
export interface EmptyAck {
  requestId: number;
  confirmToken: number;
  itemCount: number;
  sizeBytes: number;
  expiresAt: number;
}

/** Como termino un vaciado (espejo de `EmptyStatus::as_str`), decidido por medicion posterior. */
export type EmptyStatus = "emptied" | "partial" | "failed" | "unverified";

/** Espejo de `commands::recycle_bin::EmptyResultDto` (Rust). `itemsAfter`/`bytesAfter` son `null` si no se pudo medir despues. */
export interface EmptyResult {
  status: EmptyStatus;
  itemsBefore: number;
  bytesBefore: number;
  itemsAfter: number | null;
  bytesAfter: number | null;
  freedBytes: number;
  /** Codigo estable (`partial`, `not_emptied`, `unverified`, `permission_denied`, ...); `null` con `emptied`. */
  detail: string | null;
  /** `false` si el vaciado ocurrio pero no se pudo guardar su registro. */
  logged: boolean;
  finishedAt: number;
}

function decodeEmptyRequest(raw: unknown, command: string): EmptyRequest {
  if (isRecord(raw)) {
    const { requestId, itemCount, sizeBytes, createdAt, expiresAt } = raw;
    if (
      isCount(requestId) &&
      isCount(itemCount) &&
      isCount(sizeBytes) &&
      isCount(createdAt) &&
      isCount(expiresAt)
    ) {
      return { requestId, itemCount, sizeBytes, createdAt, expiresAt };
    }
  }
  throw invalidResponse(command);
}

function decodeEmptyAck(raw: unknown, command: string): EmptyAck {
  if (isRecord(raw)) {
    const { requestId, confirmToken, itemCount, sizeBytes, expiresAt } = raw;
    if (
      isCount(requestId) &&
      isCount(confirmToken) &&
      isCount(itemCount) &&
      isCount(sizeBytes) &&
      isCount(expiresAt)
    ) {
      return { requestId, confirmToken, itemCount, sizeBytes, expiresAt };
    }
  }
  throw invalidResponse(command);
}

function decodeEmptyStatus(value: unknown): EmptyStatus | null {
  return value === "emptied" || value === "partial" || value === "failed" || value === "unverified"
    ? value
    : null;
}

function decodeEmptyResult(raw: unknown, command: string): EmptyResult {
  if (isRecord(raw)) {
    const status = decodeEmptyStatus(raw["status"]);
    const { itemsBefore, bytesBefore, itemsAfter, bytesAfter, freedBytes, detail, logged, finishedAt } = raw;
    const afterKnown = isCount(itemsAfter) && isCount(bytesAfter);
    const afterUnknown = itemsAfter === null && bytesAfter === null;
    if (
      status !== null &&
      isCount(itemsBefore) &&
      isCount(bytesBefore) &&
      (afterKnown || afterUnknown) &&
      isCount(freedBytes) &&
      (detail === null || (typeof detail === "string" && DETAIL_CODE.test(detail))) &&
      typeof logged === "boolean" &&
      isCount(finishedAt)
    ) {
      return {
        status,
        itemsBefore,
        bytesBefore,
        itemsAfter: afterKnown ? itemsAfter : null,
        bytesAfter: afterKnown ? bytesAfter : null,
        freedBytes,
        detail,
        logged,
        finishedAt,
      };
    }
  }
  throw invalidResponse(command);
}

/**
 * Paso 1 del vaciado: mide la Papelera del disco del sistema y crea la solicitud. No vacia nada.
 * No lleva argumentos: el volumen lo decide el backend.
 */
export async function prepareEmptyRecycleBin(): Promise<EmptyRequest> {
  return decodeEmptyRequest(await call<unknown>("prepare_empty_recycle_bin"), "prepare_empty_recycle_bin");
}

/** Paso 2: primera confirmacion; el backend entrega el token del paso 3. */
export async function acknowledgeEmptyRecycleBin(requestId: number): Promise<EmptyAck> {
  return decodeEmptyAck(
    await call<unknown>("acknowledge_empty_recycle_bin", { requestId }),
    "acknowledge_empty_recycle_bin",
  );
}

/** Paso 3: segunda confirmacion con el token. IRREVERSIBLE: elimina para siempre lo que haya en la Papelera. */
export async function emptyRecycleBin(requestId: number, confirmToken: number): Promise<EmptyResult> {
  return decodeEmptyResult(
    await call<unknown>("empty_recycle_bin", { requestId, confirmToken }),
    "empty_recycle_bin",
  );
}

// ---------------------------------------------------------------------------------------------
// Duplicados (C-11)
// ---------------------------------------------------------------------------------------------

/** Las 6 carpetas de contenido (espejo de `ContentFolder::as_str`), en el orden de la UI. */
export const ROOT_KEYS = ["desktop", "documents", "downloads", "music", "pictures", "videos"] as const;
export type RootKey = (typeof ROOT_KEYS)[number];

/** Opciones cerradas del tamano minimo por escaneo (bytes): 100 KiB, 1, 10 y 100 MiB. */
export const MIN_SIZE_OPTIONS = [102_400, 1_048_576, 10_485_760, 104_857_600] as const;
export const DEFAULT_MIN_SIZE = 1_048_576;

export type RootStatusKind = "done" | "not_selected" | "nested" | "unavailable";

/** Espejo de `commands::duplicates::RootDto`. `reason` solo con `unavailable`. */
export interface RootResult {
  root: RootKey;
  status: RootStatusKind;
  reason: string | null;
  filesSeen: number;
}

/** Espejo de `commands::duplicates::DuplicateScanSummaryDto` (Rust). Un escaneo `cancelled` no trae carpetas. */
export interface DuplicateScanSummary {
  scanId: number;
  status: "done" | "cancelled";
  startedAt: number;
  finishedAt: number | null;
  minSizeBytes: number;
  roots: RootResult[];
  filesSeen: number;
  belowMinSize: number;
  protectedCount: number;
  cloudCount: number;
  hardLinkedCount: number;
  changedCount: number;
  unreadableCount: number;
  inaccessibleDirs: number;
  excludedDirs: number;
  hashedBytes: number;
  groupCount: number;
  duplicateFileCount: number;
  /** Bytes que aun se pueden recuperar (grupos sin resolver). */
  reclaimableBytes: number;
  openGroupCount: number;
}

/** Espejo de `commands::duplicates::DuplicateScanEventDto`. */
export type DuplicateScanEvent =
  | { type: "started"; scanId: number }
  | { type: "walking"; filesSeen: number }
  | { type: "partial"; done: number; total: number }
  | { type: "full"; doneBytes: number; totalBytes: number };

/** Espejo de `DuplicateFileDto`: una copia. `relativePath` es relativa a su carpeta de contenido. */
export interface DuplicateFile {
  fileId: number;
  root: RootKey;
  path: string;
  relativePath: string;
  modifiedAt: number | null;
  kept: boolean;
}

/** Espejo de `DuplicateGroupDto`. */
export interface DuplicateGroup {
  groupId: number;
  sizeBytes: number;
  blake3: string;
  reclaimableBytes: number;
  resolvedAt: number | null;
  files: DuplicateFile[];
}

/** Espejo de `DuplicateGroupPageDto`. */
export interface DuplicateGroupPage {
  scanId: number;
  total: number;
  groups: DuplicateGroup[];
}

/** Decision del usuario sobre un grupo: las copias que se CONSERVAN. */
export interface DuplicateDecision {
  groupId: number;
  keepFileIds: number[];
}

/** Espejo de `DuplicateCleanupPreviewDto` (aplanado): la categoria unica `duplicates` se resume en `recent*`. */
export interface DuplicateCleanupPreview {
  previewId: number;
  scanId: number;
  createdAt: number;
  expiresAt: number;
  totalItems: number;
  totalBytes: number;
  recycleBinAvailable: boolean;
  groupCount: number;
  /** Copias excluidas por haberse escrito hace menos de 24 h: se informan, no se mueven. */
  recentCount: number;
  recentBytes: number;
}

/** Espejo de `DuplicateCleanupSummaryDto`: el resumen de la limpieza mas la resolucion de grupos. */
export interface DuplicateCleanupSummary extends Omit<CleanupSummary, "categories"> {
  /** `false` si no se pudo guardar la resolucion: los grupos siguen abiertos. */
  groupsResolved: boolean;
  resolvedGroupIds: number[];
}

const HEX_64 = /^[0-9a-f]{64}$/;

function isRootKey(value: unknown): value is RootKey {
  return ROOT_KEYS.some((key) => key === value);
}

function decodeRootStatus(value: unknown): RootStatusKind | null {
  return value === "done" || value === "not_selected" || value === "nested" || value === "unavailable"
    ? value
    : null;
}

function decodeRoot(raw: unknown): RootResult | null {
  if (!isRecord(raw)) return null;
  const { root, filesSeen, reason } = raw;
  const status = decodeRootStatus(raw["status"]);
  if (!isRootKey(root) || status === null || !isCount(filesSeen)) return null;
  if (status === "unavailable") {
    return typeof reason === "string" && DETAIL_CODE.test(reason)
      ? { root, status, reason, filesSeen }
      : null;
  }
  return reason === null ? { root, status, reason: null, filesSeen } : null;
}

const SCAN_COUNTERS = [
  "scanId",
  "startedAt",
  "minSizeBytes",
  "filesSeen",
  "belowMinSize",
  "protectedCount",
  "cloudCount",
  "hardLinkedCount",
  "changedCount",
  "unreadableCount",
  "inaccessibleDirs",
  "excludedDirs",
  "hashedBytes",
  "groupCount",
  "duplicateFileCount",
  "reclaimableBytes",
  "openGroupCount",
] as const;

function decodeDuplicateScanSummary(raw: unknown, command: string): DuplicateScanSummary {
  if (isRecord(raw) && Array.isArray(raw["roots"])) {
    const decoded = raw["roots"].map(decodeRoot);
    const roots = decoded.filter((root): root is RootResult => root !== null);
    const status = raw["status"];
    const finishedAt = raw["finishedAt"];
    const n = (key: (typeof SCAN_COUNTERS)[number]): number => raw[key] as number;
    const valid =
      (status === "done" || status === "cancelled") &&
      roots.length === decoded.length &&
      (finishedAt === null || isCount(finishedAt)) &&
      SCAN_COUNTERS.every((key) => isCount(raw[key])) &&
      MIN_SIZE_OPTIONS.some((option) => option === raw["minSizeBytes"]) &&
      new Set(roots.map((root) => root.root)).size === roots.length &&
      (status === "cancelled" ? roots.length === 0 : roots.length === ROOT_KEYS.length);
    if (valid) {
      return {
        scanId: n("scanId"),
        status,
        startedAt: n("startedAt"),
        finishedAt: finishedAt as number | null,
        minSizeBytes: n("minSizeBytes"),
        roots,
        filesSeen: n("filesSeen"),
        belowMinSize: n("belowMinSize"),
        protectedCount: n("protectedCount"),
        cloudCount: n("cloudCount"),
        hardLinkedCount: n("hardLinkedCount"),
        changedCount: n("changedCount"),
        unreadableCount: n("unreadableCount"),
        inaccessibleDirs: n("inaccessibleDirs"),
        excludedDirs: n("excludedDirs"),
        hashedBytes: n("hashedBytes"),
        groupCount: n("groupCount"),
        duplicateFileCount: n("duplicateFileCount"),
        reclaimableBytes: n("reclaimableBytes"),
        openGroupCount: n("openGroupCount"),
      };
    }
  }
  throw invalidResponse(command);
}

function decodeDuplicateScanEvent(raw: unknown): DuplicateScanEvent | null {
  if (!isRecord(raw)) return null;
  switch (raw["type"]) {
    case "started":
      return isCount(raw["scanId"]) ? { type: "started", scanId: raw["scanId"] } : null;
    case "walking":
      return isCount(raw["filesSeen"]) ? { type: "walking", filesSeen: raw["filesSeen"] } : null;
    case "partial": {
      const { done, total } = raw;
      return isCount(done) && isCount(total) ? { type: "partial", done, total } : null;
    }
    case "full": {
      const { doneBytes, totalBytes } = raw;
      return isCount(doneBytes) && isCount(totalBytes) ? { type: "full", doneBytes, totalBytes } : null;
    }
    default:
      return null;
  }
}

function decodeDuplicateFile(raw: unknown): DuplicateFile | null {
  if (!isRecord(raw)) return null;
  const { fileId, root, path, relativePath, modifiedAt, kept } = raw;
  if (
    isCount(fileId) &&
    isRootKey(root) &&
    typeof path === "string" &&
    path.length > 0 &&
    typeof relativePath === "string" &&
    (modifiedAt === null || isCount(modifiedAt)) &&
    typeof kept === "boolean"
  ) {
    return { fileId, root, path, relativePath, modifiedAt, kept };
  }
  return null;
}

function decodeDuplicateGroup(raw: unknown): DuplicateGroup | null {
  if (!isRecord(raw) || !Array.isArray(raw["files"])) return null;
  const { groupId, sizeBytes, blake3, reclaimableBytes, resolvedAt } = raw;
  const decoded = raw["files"].map(decodeDuplicateFile);
  const files = decoded.filter((file): file is DuplicateFile => file !== null);
  if (
    isCount(groupId) &&
    isCount(sizeBytes) &&
    typeof blake3 === "string" &&
    HEX_64.test(blake3) &&
    isCount(reclaimableBytes) &&
    (resolvedAt === null || isCount(resolvedAt)) &&
    files.length === decoded.length &&
    files.length >= 2
  ) {
    return { groupId, sizeBytes, blake3, reclaimableBytes, resolvedAt, files };
  }
  return null;
}

function decodeDuplicateGroupPage(raw: unknown, command: string): DuplicateGroupPage {
  if (isRecord(raw) && Array.isArray(raw["groups"]) && isCount(raw["scanId"]) && isCount(raw["total"])) {
    const decoded = raw["groups"].map(decodeDuplicateGroup);
    const groups = decoded.filter((group): group is DuplicateGroup => group !== null);
    if (groups.length === decoded.length) return { scanId: raw["scanId"], total: raw["total"], groups };
  }
  throw invalidResponse(command);
}

function decodeDuplicateCleanupPreview(raw: unknown, command: string): DuplicateCleanupPreview {
  if (isRecord(raw) && Array.isArray(raw["categories"])) {
    const { previewId, scanId, createdAt, expiresAt, totalItems, totalBytes, recycleBinAvailable, groupCount } = raw;
    const [only, ...others] = raw["categories"];
    const category = isRecord(only) ? only : null;
    if (
      others.length === 0 &&
      category !== null &&
      category["category"] === "duplicates" &&
      category["status"] === "included" &&
      isCount(category["recentCount"]) &&
      isCount(category["recentBytes"]) &&
      isCount(previewId) &&
      isCount(scanId) &&
      isCount(createdAt) &&
      isCount(expiresAt) &&
      isCount(totalItems) &&
      isCount(totalBytes) &&
      typeof recycleBinAvailable === "boolean" &&
      isCount(groupCount)
    ) {
      return {
        previewId,
        scanId,
        createdAt,
        expiresAt,
        totalItems,
        totalBytes,
        recycleBinAvailable,
        groupCount,
        recentCount: category["recentCount"],
        recentBytes: category["recentBytes"],
      };
    }
  }
  throw invalidResponse(command);
}

function decodeDuplicateCleanupSummary(raw: unknown, command: string): DuplicateCleanupSummary {
  if (isRecord(raw) && Array.isArray(raw["resolvedGroupIds"]) && typeof raw["groupsResolved"] === "boolean") {
    const ids = raw["resolvedGroupIds"];
    // Las categorias del resumen (`duplicates`) no se usan: el decodificador de basura las rechazaria.
    const { categories: _ignored, ...rest } = decodeCleanupSummary({ ...raw, categories: [] }, command);
    if (ids.every(isCount)) return { ...rest, groupsResolved: raw["groupsResolved"], resolvedGroupIds: ids };
  }
  throw invalidResponse(command);
}

/** Opciones de un escaneo de duplicados: carpetas (por clave) y tamano minimo. Nunca rutas. */
export interface DuplicateScanOptions {
  roots?: readonly RootKey[];
  minSizeBytes?: number;
}

/**
 * Inicia un escaneo de duplicados. El progreso llega por el canal de la invocacion (los eventos
 * invalidos se ignoran) y la promesa se resuelve con el resumen final (`done` o `cancelled`).
 */
export async function startDuplicateScan(
  options: DuplicateScanOptions,
  onEvent: (event: DuplicateScanEvent) => void,
): Promise<DuplicateScanSummary> {
  const channel = new Channel<unknown>((raw) => {
    const event = decodeDuplicateScanEvent(raw);
    if (event !== null) onEvent(event);
  });
  const raw = await call<unknown>("start_duplicate_scan", {
    ...(options.roots === undefined ? {} : { roots: [...options.roots] }),
    ...(options.minSizeBytes === undefined ? {} : { minSizeBytes: options.minSizeBytes }),
    onProgress: channel,
  });
  return decodeDuplicateScanSummary(raw, "start_duplicate_scan");
}

/** Resumen del ultimo escaneo de duplicados terminado, o `null` si todavia no hay ninguno. */
export async function getLastDuplicateScan(): Promise<DuplicateScanSummary | null> {
  const raw = await call<unknown>("get_last_duplicate_scan");
  return raw === null ? null : decodeDuplicateScanSummary(raw, "get_last_duplicate_scan");
}

/** Filtros opcionales de `listDuplicateGroups`. */
export interface DuplicateGroupFilters {
  root?: RootKey;
  minSizeBytes?: number;
}

/** Una pagina de grupos del ultimo escaneo terminado (1 a 200 por pagina). */
export async function listDuplicateGroups(
  scanId: number,
  offset: number,
  limit: number,
  filters: DuplicateGroupFilters = {},
): Promise<DuplicateGroupPage> {
  return decodeDuplicateGroupPage(
    await call<unknown>("list_duplicate_groups", {
      scanId,
      offset,
      limit,
      ...(filters.root === undefined ? {} : { root: filters.root }),
      ...(filters.minSizeBytes === undefined ? {} : { minSizeBytes: filters.minSizeBytes }),
    }),
    "list_duplicate_groups",
  );
}

/** Pide el preview de las copias a eliminar: solo identificadores de grupo y de copias a CONSERVAR. */
export async function previewDuplicateCleanup(
  scanId: number,
  decisions: readonly DuplicateDecision[],
): Promise<DuplicateCleanupPreview> {
  return decodeDuplicateCleanupPreview(
    await call<unknown>("preview_duplicate_cleanup", {
      scanId,
      decisions: decisions.map((d) => ({ groupId: d.groupId, keepFileIds: [...d.keepFileIds] })),
    }),
    "preview_duplicate_cleanup",
  );
}

/** Confirma el preview de duplicados; el progreso llega por el canal (mismos eventos que la limpieza). */
export async function confirmDuplicateCleanup(
  previewId: number,
  onEvent: (event: CleanupEvent) => void,
): Promise<DuplicateCleanupSummary> {
  const channel = new Channel<unknown>((raw) => {
    const event = decodeCleanupEvent(raw);
    if (event !== null) onEvent(event);
  });
  const raw = await call<unknown>("confirm_duplicate_cleanup", { previewId, onProgress: channel });
  return decodeDuplicateCleanupSummary(raw, "confirm_duplicate_cleanup");
}


// ---------------------------------------------------------------------------------------------
// Espacio en disco (C-07)
// ---------------------------------------------------------------------------------------------

/** Tipo de raiz del analisis (espejo de `RootKind::as_str`). */
export type DiskRootKind = "system_drive" | "picked";

/** Raiz pedida: el disco del sistema o la carpeta elegida, siempre por tipo o por `pickId`, nunca por ruta. */
export type FolderRootRequest = { kind: "system_drive" } | { kind: "picked"; pickId: number };

/** Cuanto se pudo leer de una carpeta (espejo de `Access::as_str`). */
export type FolderAccess = "complete" | "incomplete" | "inaccessible";

/** Por que una carpeta es opaca: se mide, pero nunca se muestra su interior (espejo de `OpaqueReason`). */
export type OpaqueReason = "other_user_profile" | "recycle_bin" | "system_volume_information";

/** Por que una carpeta no se pudo listar (espejo de `IssueReason`). */
export type FolderIssueReason = "access_denied" | "not_found" | "io_error";

/** Una carpeta sin acceso de la muestra (hasta 50). */
export interface FolderIssue {
  path: string;
  reason: FolderIssueReason;
}

/** Espejo de `commands::disk_usage::FolderScanSummaryDto`. Un analisis `cancelled` no tiene carpetas. */
export interface FolderScanSummary {
  scanId: number;
  status: "done" | "cancelled";
  startedAt: number;
  finishedAt: number | null;
  root: { kind: DiskRootKind; path: string; folderId: number | null };
  volume: { totalBytes: number; freeBytes: number } | null;
  /** Espacio usado del volumen que el analisis no conto; solo con la raiz `system_drive`. */
  unaccountedBytes: number | null;
  /** Lo analizado supera lo usado del volumen (enlaces duros, compresion): se informa 0. */
  overCounted: boolean;
  totalBytes: number;
  fileCount: number;
  dirCount: number;
  inaccessibleDirs: number;
  reparseSkipped: number;
  cloudFiles: number;
  opaqueDirs: number;
  foldersStored: number;
  foldersTruncated: boolean;
  issues: FolderIssue[];
}

/** Espejo de `FolderScanEventDto`: solo conteos, nunca rutas. */
export type FolderScanEvent =
  | { type: "started"; scanId: number; rootKind: DiskRootKind }
  | { type: "walking"; filesSeen: number; dirsSeen: number; bytesSeen: number }
  | { type: "saving" };

/** Espejo de `FolderDto`. */
export interface FolderEntry {
  id: number;
  name: string;
  path: string;
  sizeBytes: number;
  directBytes: number;
  fileCount: number;
  dirCount: number;
  access: FolderAccess;
  opaque: OpaqueReason | null;
  system: boolean;
  junkHint: boolean;
  hasChildren: boolean;
}

export interface FolderCrumb {
  id: number;
  name: string;
}

/** Espejo de `FolderPageDto`. `remainderBytes` = lo que no cubren los hijos guardados ("carpetas mas chicas"). */
export interface FolderPage {
  scanId: number;
  folder: FolderEntry;
  breadcrumb: FolderCrumb[];
  total: number;
  items: FolderEntry[];
  remainderBytes: number;
}

/** Espejo de `LargeFileDto`. */
export interface LargeFileEntry {
  id: number;
  name: string;
  path: string;
  folderId: number;
  sizeBytes: number;
  modifiedAt: number | null;
}

export interface LargeFilePage {
  scanId: number;
  total: number;
  items: LargeFileEntry[];
}

export type FolderSort = "size" | "name";
export type LargeFileSort = "size" | "newest" | "oldest";

/** Respuesta de `pick_folder_root`: la ruta viaja solo para mostrarla; el analisis usa el `pickId`. */
export type PickResult = { status: "picked"; pickId: number; path: string } | { status: "cancelled" };

/** Elemento del ultimo analisis a mostrar en el Explorador (por id, nunca por ruta). */
export interface RevealTarget {
  kind: "folder" | "file";
  id: number;
}

/** Cantidad maxima de incidencias de la muestra. */
const MAX_FOLDER_ISSUES = 50;

function isDiskRootKind(value: unknown): value is DiskRootKind {
  return value === "system_drive" || value === "picked";
}

function isFolderAccess(value: unknown): value is FolderAccess {
  return value === "complete" || value === "incomplete" || value === "inaccessible";
}

function isOpaqueReason(value: unknown): value is OpaqueReason {
  return value === "other_user_profile" || value === "recycle_bin" || value === "system_volume_information";
}

function isIssueReason(value: unknown): value is FolderIssueReason {
  return value === "access_denied" || value === "not_found" || value === "io_error";
}

function decodeFolderIssue(raw: unknown): FolderIssue | null {
  if (!isRecord(raw)) return null;
  const { path, reason } = raw;
  return typeof path === "string" && path.length > 0 && isIssueReason(reason) ? { path, reason } : null;
}

const FOLDER_SCAN_COUNTERS = [
  "scanId",
  "startedAt",
  "totalBytes",
  "fileCount",
  "dirCount",
  "inaccessibleDirs",
  "reparseSkipped",
  "cloudFiles",
  "opaqueDirs",
  "foldersStored",
] as const;

function decodeVolume(raw: unknown): { totalBytes: number; freeBytes: number } | null | undefined {
  if (raw === null) return null;
  if (!isRecord(raw)) return undefined;
  const { totalBytes, freeBytes } = raw;
  return isCount(totalBytes) && isCount(freeBytes) ? { totalBytes, freeBytes } : undefined;
}

function decodeFolderScanSummary(raw: unknown, command: string): FolderScanSummary {
  if (isRecord(raw) && isRecord(raw["root"]) && Array.isArray(raw["issues"])) {
    const root = raw["root"];
    const status = raw["status"];
    const finishedAt = raw["finishedAt"];
    const volume = decodeVolume(raw["volume"]);
    const unaccountedBytes = raw["unaccountedBytes"];
    const issues = raw["issues"].map(decodeFolderIssue);
    const n = (key: (typeof FOLDER_SCAN_COUNTERS)[number]): number => raw[key] as number;
    const folderId = root["folderId"];
    const rootPath = root["path"];
    const valid =
      (status === "done" || status === "cancelled") &&
      isDiskRootKind(root["kind"]) &&
      typeof rootPath === "string" &&
      (status === "done" ? rootPath.length > 0 && isCount(folderId) : folderId === null) &&
      (finishedAt === null || isCount(finishedAt)) &&
      volume !== undefined &&
      (unaccountedBytes === null || isCount(unaccountedBytes)) &&
      typeof raw["overCounted"] === "boolean" &&
      typeof raw["foldersTruncated"] === "boolean" &&
      FOLDER_SCAN_COUNTERS.every((key) => isCount(raw[key])) &&
      issues.length <= MAX_FOLDER_ISSUES &&
      issues.every((issue) => issue !== null);
    if (valid) {
      return {
        scanId: n("scanId"),
        status,
        startedAt: n("startedAt"),
        finishedAt: finishedAt as number | null,
        root: { kind: root["kind"] as DiskRootKind, path: rootPath, folderId: folderId as number | null },
        volume,
        unaccountedBytes: unaccountedBytes as number | null,
        overCounted: raw["overCounted"] as boolean,
        totalBytes: n("totalBytes"),
        fileCount: n("fileCount"),
        dirCount: n("dirCount"),
        inaccessibleDirs: n("inaccessibleDirs"),
        reparseSkipped: n("reparseSkipped"),
        cloudFiles: n("cloudFiles"),
        opaqueDirs: n("opaqueDirs"),
        foldersStored: n("foldersStored"),
        foldersTruncated: raw["foldersTruncated"] as boolean,
        issues: issues.filter((issue): issue is FolderIssue => issue !== null),
      };
    }
  }
  throw invalidResponse(command);
}

function decodeFolderScanEvent(raw: unknown): FolderScanEvent | null {
  if (!isRecord(raw)) return null;
  switch (raw["type"]) {
    case "started": {
      const { scanId, rootKind } = raw;
      return isCount(scanId) && isDiskRootKind(rootKind) ? { type: "started", scanId, rootKind } : null;
    }
    case "walking": {
      const { filesSeen, dirsSeen, bytesSeen } = raw;
      return isCount(filesSeen) && isCount(dirsSeen) && isCount(bytesSeen)
        ? { type: "walking", filesSeen, dirsSeen, bytesSeen }
        : null;
    }
    case "saving":
      return { type: "saving" };
    default:
      return null;
  }
}

function decodeFolderEntry(raw: unknown): FolderEntry | null {
  if (!isRecord(raw)) return null;
  const { id, name, path, sizeBytes, directBytes, fileCount, dirCount, access, opaque, system, junkHint, hasChildren } =
    raw;
  if (
    isCount(id) &&
    typeof name === "string" &&
    name.length > 0 &&
    typeof path === "string" &&
    path.length > 0 &&
    isCount(sizeBytes) &&
    isCount(directBytes) &&
    isCount(fileCount) &&
    isCount(dirCount) &&
    isFolderAccess(access) &&
    (opaque === null || isOpaqueReason(opaque)) &&
    typeof system === "boolean" &&
    typeof junkHint === "boolean" &&
    typeof hasChildren === "boolean"
  ) {
    return {
      id,
      name,
      path,
      sizeBytes,
      directBytes,
      fileCount,
      dirCount,
      access,
      opaque,
      system,
      junkHint,
      hasChildren,
    };
  }
  return null;
}

function decodeCrumb(raw: unknown): FolderCrumb | null {
  if (!isRecord(raw)) return null;
  const { id, name } = raw;
  return isCount(id) && typeof name === "string" && name.length > 0 ? { id, name } : null;
}

function decodeFolderPage(raw: unknown, command: string): FolderPage {
  if (isRecord(raw) && Array.isArray(raw["items"]) && Array.isArray(raw["breadcrumb"])) {
    const folder = decodeFolderEntry(raw["folder"]);
    const items = raw["items"].map(decodeFolderEntry);
    const crumbs = raw["breadcrumb"].map(decodeCrumb);
    const { scanId, total, remainderBytes } = raw;
    if (
      folder !== null &&
      isCount(scanId) &&
      isCount(total) &&
      isCount(remainderBytes) &&
      crumbs.length > 0 &&
      crumbs.every((crumb) => crumb !== null) &&
      items.every((item) => item !== null)
    ) {
      return {
        scanId,
        folder,
        breadcrumb: crumbs.filter((crumb): crumb is FolderCrumb => crumb !== null),
        total,
        items: items.filter((item): item is FolderEntry => item !== null),
        remainderBytes,
      };
    }
  }
  throw invalidResponse(command);
}

function decodeLargeFile(raw: unknown): LargeFileEntry | null {
  if (!isRecord(raw)) return null;
  const { id, name, path, folderId, sizeBytes, modifiedAt } = raw;
  if (
    isCount(id) &&
    typeof name === "string" &&
    name.length > 0 &&
    typeof path === "string" &&
    path.length > 0 &&
    isCount(folderId) &&
    isCount(sizeBytes) &&
    (modifiedAt === null || isCount(modifiedAt))
  ) {
    return { id, name, path, folderId, sizeBytes, modifiedAt };
  }
  return null;
}

function decodeLargeFilePage(raw: unknown, command: string): LargeFilePage {
  if (isRecord(raw) && Array.isArray(raw["items"])) {
    const items = raw["items"].map(decodeLargeFile);
    const { scanId, total } = raw;
    if (isCount(scanId) && isCount(total) && items.every((item) => item !== null)) {
      return { scanId, total, items: items.filter((item): item is LargeFileEntry => item !== null) };
    }
  }
  throw invalidResponse(command);
}

function decodePickResult(raw: unknown, command: string): PickResult {
  if (isRecord(raw)) {
    if (raw["status"] === "cancelled") return { status: "cancelled" };
    const { pickId, path } = raw;
    if (raw["status"] === "picked" && isCount(pickId) && typeof path === "string" && path.length > 0) {
      return { status: "picked", pickId, path };
    }
  }
  throw invalidResponse(command);
}

/**
 * Inicia un analisis de espacio en disco de solo lectura. La raiz viaja por tipo o por `pickId`
 * (nunca una ruta); el progreso llega por el canal de la invocacion (los eventos invalidos se
 * ignoran) y la promesa se resuelve con el resumen final (`done` o `cancelled`).
 */
export async function startFolderScan(
  root: FolderRootRequest,
  onEvent: (event: FolderScanEvent) => void,
): Promise<FolderScanSummary> {
  const channel = new Channel<unknown>((raw) => {
    const event = decodeFolderScanEvent(raw);
    if (event !== null) onEvent(event);
  });
  const raw = await call<unknown>("start_folder_scan", {
    root: root.kind === "picked" ? { kind: "picked", pickId: root.pickId } : { kind: "system_drive" },
    onProgress: channel,
  });
  return decodeFolderScanSummary(raw, "start_folder_scan");
}

/** Resumen del ultimo analisis terminado, o `null` si todavia no hay ninguno. */
export async function getLastFolderScan(): Promise<FolderScanSummary | null> {
  const raw = await call<unknown>("get_last_folder_scan");
  return raw === null ? null : decodeFolderScanSummary(raw, "get_last_folder_scan");
}

/** Hijos de una carpeta del ultimo analisis (`folderId = null` = la raiz); 1 a 200 por pagina. */
export async function listFolderChildren(
  scanId: number,
  folderId: number | null,
  sort: FolderSort,
  offset: number,
  limit: number,
): Promise<FolderPage> {
  return decodeFolderPage(
    await call<unknown>("list_folder_children", { scanId, folderId, sort, offset, limit }),
    "list_folder_children",
  );
}

/** Una pagina del top de archivos grandes del ultimo analisis (1 a 200 por pagina). */
export async function listLargeFiles(
  scanId: number,
  sort: LargeFileSort,
  offset: number,
  limit: number,
): Promise<LargeFilePage> {
  return decodeLargeFilePage(
    await call<unknown>("list_large_files", { scanId, sort, offset, limit }),
    "list_large_files",
  );
}

/** Abre el selector de carpeta del sistema; el backend valida la eleccion y devuelve su `pickId`. */
export async function pickFolderRoot(): Promise<PickResult> {
  return decodePickResult(await call<unknown>("pick_folder_root"), "pick_folder_root");
}

/** Muestra un elemento del ultimo analisis en el Explorador (lo selecciona, nunca lo abre). */
export async function revealInExplorer(scanId: number, target: RevealTarget): Promise<void> {
  await call<unknown>("reveal_in_explorer", { scanId, target: { kind: target.kind, id: target.id } });
}

// ---------------------------------------------------------------------------------------------
// Monitor en vivo (C-06)
// ---------------------------------------------------------------------------------------------

/** Nivel de una metrica (espejo de `commands::monitor::LevelDto`). */
export type MonitorLevel = "normal" | "high" | "very_high" | "unknown";
/** Nivel del espacio del volumen del sistema. */
export type MonitorSpaceLevel = "normal" | "almost_full" | "unknown";
/** Por que una metrica esta (o no) presente. */
export type MetricAvailability = "ok" | "warming_up" | "after_gap" | "source_error" | "not_supported";

/** Una muestra por segundo; cada metrica es `null` si esta ausente (nunca 0). */
export interface MonitorSample {
  seq: number;
  /** Milisegundos desde la epoca Unix. */
  at: number;
  gapBefore: boolean;
  cpuPct: number | null;
  memUsedBytes: number | null;
  memTotalBytes: number | null;
  volumeTotalBytes: number | null;
  volumeFreeBytes: number | null;
  diskActivePct: number | null;
  diskReadBps: number | null;
  diskWriteBps: number | null;
}

export interface MetricSummary {
  current: number | null;
  avg60: number | null;
  peak: number | null;
  level: MonitorLevel;
}

export interface MonitorAvailability {
  cpu: MetricAvailability;
  memory: MetricAvailability;
  volume: MetricAvailability;
  diskActive: MetricAvailability;
  diskIo: MetricAvailability;
}

export interface MonitorSummary {
  cpu: MetricSummary;
  memory: MetricSummary;
  diskActive: MetricSummary;
  volume: { usedPct: number | null; level: MonitorSpaceLevel };
  availability: MonitorAvailability;
  gapsInWindow: number;
  avgReadMicros: number | null;
}

export interface MonitorSnapshot {
  subscriptionId: number;
  intervalMs: number;
  windowMs: number;
  samples: MonitorSample[];
  summary: MonitorSummary | null;
}

/** Evento del flujo, etiquetado por `kind` (no `type` como los escaneos). */
export type MonitorEvent =
  | { kind: "sample"; sample: MonitorSample; summary: MonitorSummary }
  | { kind: "stopped"; reason: "sampler_failed" };

const MONITOR_LEVELS: readonly unknown[] = ["normal", "high", "very_high", "unknown"];
const MONITOR_SPACE_LEVELS: readonly unknown[] = ["normal", "almost_full", "unknown"];
const MONITOR_AVAILABILITY: readonly unknown[] = ["ok", "warming_up", "after_gap", "source_error", "not_supported"];

/** `undefined` = invalido; `null` = ausente. */
function nullable(value: unknown, valid: (v: unknown) => boolean): number | null | undefined {
  if (value === null) return null;
  return valid(value) ? (value as number) : undefined;
}

function isFiniteNumber(value: unknown): boolean {
  return typeof value === "number" && Number.isFinite(value);
}

function decodeMonitorSample(raw: unknown): MonitorSample | null {
  if (!isRecord(raw)) return null;
  const { seq, at, gapBefore } = raw;
  if (!isCount(seq) || typeof at !== "number" || !Number.isSafeInteger(at) || typeof gapBefore !== "boolean") {
    return null;
  }
  const cpuPct = nullable(raw["cpuPct"], isFiniteNumber);
  const diskActivePct = nullable(raw["diskActivePct"], isFiniteNumber);
  const memUsedBytes = nullable(raw["memUsedBytes"], isCount);
  const memTotalBytes = nullable(raw["memTotalBytes"], isCount);
  const volumeTotalBytes = nullable(raw["volumeTotalBytes"], isCount);
  const volumeFreeBytes = nullable(raw["volumeFreeBytes"], isCount);
  const diskReadBps = nullable(raw["diskReadBps"], isCount);
  const diskWriteBps = nullable(raw["diskWriteBps"], isCount);
  const fields = [
    cpuPct,
    diskActivePct,
    memUsedBytes,
    memTotalBytes,
    volumeTotalBytes,
    volumeFreeBytes,
    diskReadBps,
    diskWriteBps,
  ];
  if (fields.some((field) => field === undefined)) return null;
  return {
    seq,
    at,
    gapBefore,
    cpuPct: cpuPct ?? null,
    memUsedBytes: memUsedBytes ?? null,
    memTotalBytes: memTotalBytes ?? null,
    volumeTotalBytes: volumeTotalBytes ?? null,
    volumeFreeBytes: volumeFreeBytes ?? null,
    diskActivePct: diskActivePct ?? null,
    diskReadBps: diskReadBps ?? null,
    diskWriteBps: diskWriteBps ?? null,
  };
}

function decodeMetricSummary(raw: unknown): MetricSummary | null {
  if (!isRecord(raw)) return null;
  const current = nullable(raw["current"], isFiniteNumber);
  const avg60 = nullable(raw["avg60"], isFiniteNumber);
  const peak = nullable(raw["peak"], isFiniteNumber);
  const level = raw["level"];
  if (current === undefined || avg60 === undefined || peak === undefined || !MONITOR_LEVELS.includes(level)) {
    return null;
  }
  return { current, avg60, peak, level: level as MonitorLevel };
}

function decodeMonitorAvailability(raw: unknown): MonitorAvailability | null {
  if (!isRecord(raw)) return null;
  const keys = ["cpu", "memory", "volume", "diskActive", "diskIo"] as const;
  if (!keys.every((key) => MONITOR_AVAILABILITY.includes(raw[key]))) return null;
  return {
    cpu: raw["cpu"] as MetricAvailability,
    memory: raw["memory"] as MetricAvailability,
    volume: raw["volume"] as MetricAvailability,
    diskActive: raw["diskActive"] as MetricAvailability,
    diskIo: raw["diskIo"] as MetricAvailability,
  };
}

function decodeMonitorSummary(raw: unknown): MonitorSummary | null {
  if (!isRecord(raw)) return null;
  const cpu = decodeMetricSummary(raw["cpu"]);
  const memory = decodeMetricSummary(raw["memory"]);
  const diskActive = decodeMetricSummary(raw["diskActive"]);
  const availability = decodeMonitorAvailability(raw["availability"]);
  const volume = raw["volume"];
  const gapsInWindow = raw["gapsInWindow"];
  const avgReadMicros = nullable(raw["avgReadMicros"], isCount);
  if (!isRecord(volume) || cpu === null || memory === null || diskActive === null || availability === null) return null;
  const usedPct = nullable(volume["usedPct"], isFiniteNumber);
  if (
    usedPct === undefined ||
    !MONITOR_SPACE_LEVELS.includes(volume["level"]) ||
    !isCount(gapsInWindow) ||
    avgReadMicros === undefined
  ) {
    return null;
  }
  return {
    cpu,
    memory,
    diskActive,
    volume: { usedPct, level: volume["level"] as MonitorSpaceLevel },
    availability,
    gapsInWindow,
    avgReadMicros,
  };
}

function decodeMonitorEvent(raw: unknown): MonitorEvent | null {
  if (!isRecord(raw)) return null;
  if (raw["kind"] === "stopped") {
    return raw["reason"] === "sampler_failed" ? { kind: "stopped", reason: "sampler_failed" } : null;
  }
  if (raw["kind"] === "sample") {
    const sample = decodeMonitorSample(raw["sample"]);
    const summary = decodeMonitorSummary(raw["summary"]);
    return sample !== null && summary !== null ? { kind: "sample", sample, summary } : null;
  }
  return null;
}

function decodeMonitorSnapshot(raw: unknown, command: string): MonitorSnapshot {
  if (isRecord(raw) && Array.isArray(raw["samples"])) {
    const { subscriptionId, intervalMs, windowMs } = raw;
    const samples = raw["samples"].map(decodeMonitorSample);
    const summary = raw["summary"] === null ? null : decodeMonitorSummary(raw["summary"]);
    if (
      isCount(subscriptionId) &&
      isCount(intervalMs) &&
      isCount(windowMs) &&
      samples.every((s): s is MonitorSample => s !== null) &&
      (raw["summary"] === null || summary !== null)
    ) {
      return { subscriptionId, intervalMs, windowMs, samples, summary };
    }
  }
  throw invalidResponse(command);
}

/**
 * Se suscribe al monitor: devuelve la ventana actual y, por el canal, cada muestra nueva (los
 * eventos invalidos se ignoran). Una suscripcion nueva reemplaza a la anterior en el backend.
 */
export async function startMonitorStream(
  onEvent: (event: MonitorEvent) => void,
): Promise<{ snapshot: MonitorSnapshot }> {
  const channel = new Channel<unknown>((raw) => {
    const event = decodeMonitorEvent(raw);
    if (event !== null) onEvent(event);
  });
  const raw = await call<unknown>("start_monitor_stream", { onSample: channel });
  return { snapshot: decodeMonitorSnapshot(raw, "start_monitor_stream") };
}

/** Cancela la suscripcion; un id viejo no afecta a la vigente. */
export async function stopMonitorStream(subscriptionId: number): Promise<void> {
  await call<unknown>("stop_monitor_stream", { subscriptionId });
}
