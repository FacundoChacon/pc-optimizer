import type { CleanupAbortReason, CleanupReasonOutcome, JunkCategory } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";

/** Categorias que la limpieza puede mover: todas menos la Papelera (solo se mide) y los registros antiguos. */
export const CLEANABLE_CATEGORIES: readonly JunkCategory[] = [
  "temp_user",
  "temp_windows",
  "browser_cache_chrome",
  "browser_cache_edge",
  "browser_cache_firefox",
  "browser_cache_brave",
  "wu_cache",
  "thumbnail_cache",
  "crash_dumps",
];

/** `true` si la categoria se puede limpiar (espejo de `is_cleanable` del backend). */
export function isCleanable(category: JunkCategory): boolean {
  return CLEANABLE_CATEGORIES.includes(category);
}

type ReasonFamily = keyof typeof es.cleanup.cleanupReasons;

/** Codigo de detalle estable del backend -> familia con texto llano (D-06). */
const DETAIL_FAMILIES: Readonly<Record<string, ReasonFamily>> = {
  in_use: "locked",
  unresolvable_in_use: "locked",
  changed_since_scan: "locked",
  recently_modified: "locked",
  identity_changed: "changed",
  unresolvable_not_found: "missing",
  not_found: "missing",
  blocked_different_user: "other_account",
  blocked_undetermined: "other_account",
  other_user_profile: "other_account",
  reparse_point: "links",
  reparse_point_in_path: "links",
  multiple_hard_links: "links",
  skipped_cloud: "cloud",
  recycle_bin_unavailable: "bin_unavailable",
  trash_unavailable: "bin_unavailable",
  recycle_bin_too_small: "too_large",
  path_too_long: "path_too_long",
  permission_denied: "permission_denied",
  not_moved: "error",
  io: "error",
  content_changed: "changed",
  content_unreadable: "unreadable",
  kept_copy_unavailable: "kept_missing",
};

const OUTCOME_FAMILIES: Readonly<Record<CleanupReasonOutcome, ReasonFamily>> = {
  skipped_locked: "locked",
  skipped_protected: "protected",
  skipped_cloud: "cloud",
  error: "error",
};

/**
 * Resultado + codigo de detalle -> texto llano. Un codigo desconocido cae en la familia del
 * resultado: nunca queda vacio ni muestra el codigo crudo.
 */
export function cleanupReasonText(outcome: CleanupReasonOutcome, detail: string): string {
  const known = Object.hasOwn(DETAIL_FAMILIES, detail) ? DETAIL_FAMILIES[detail] : undefined;
  const hresult = detail.startsWith("io_") ? "error" : undefined;
  return es.cleanup.cleanupReasons[known ?? hresult ?? OUTCOME_FAMILIES[outcome]];
}

/** Motivo de aborto -> texto del catalogo. */
export function abortText(reason: CleanupAbortReason): string {
  return es.cleanup.summaryAborted[reason];
}
