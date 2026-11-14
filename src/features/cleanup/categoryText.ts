import type { CategorySummary, JunkCategory } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import type { StatusTone } from "../../shared/ui/StatusBadge";

export interface CategoryStatusText {
  tone: StatusTone;
  label: string;
  /** Explicacion bajo el estado (motivo de bloqueo). */
  note?: string;
  /** `false` cuando no hay un tamano confiable que mostrar (nunca "0 B" engañoso). */
  showSize: boolean;
}

/** Nombre llano de una categoria. */
export function categoryName(category: JunkCategory): string {
  return es.cleanup.categoryNames[category];
}

/** Estado de una categoria -> textos del catalogo. Puro. */
export function categoryStatusText(summary: CategorySummary): CategoryStatusText {
  const reason = summary.blockReason === undefined ? undefined : es.cleanup.blockReasons[summary.blockReason];
  switch (summary.status) {
    case "done":
      return { tone: "success", label: es.cleanup.status.done, showSize: true };
    case "partial":
      return {
        tone: "warning",
        label: es.cleanup.status.partial,
        note: [reason, es.cleanup.partialNote].filter((part) => part !== undefined).join(" "),
        showSize: true,
      };
    case "blocked":
      return {
        tone: "warning",
        label: es.cleanup.status.blocked,
        ...(reason === undefined ? {} : { note: reason }),
        showSize: false,
      };
    case "unavailable":
      return {
        tone: "neutral",
        label:
          summary.category === "old_logs" ? es.cleanup.status.notInThisVersion : es.cleanup.status.unavailable,
        showSize: false,
      };
  }
}

type SkipFamily = keyof typeof es.cleanup.skipReasons;

const SKIP_FAMILIES: Readonly<Record<string, SkipFamily>> = {
  context_unavailable: "protected_system",
  unsupported_volume: "protected_system",
  volume_root: "protected_system",
  volume_system_entry: "protected_system",
  anchor: "protected_system",
  windows_dir: "protected_system",
  program_files: "protected_system",
  program_data: "protected_system",
  app_dir: "protected_system",
  other_user_profile: "other_profile",
  browser_profile: "browser_profile",
  reparse_point: "link",
  reparse_point_in_path: "link",
  multiple_hard_links: "hard_links",
  skipped_cloud: "cloud",
  unresolvable_not_found: "unreadable",
  unresolvable_permission_denied: "unreadable",
  unresolvable_in_use: "unreadable",
  unresolvable_unsupported: "unreadable",
  unresolvable_io: "unreadable",
  unresolvable_not_a_directory: "unreadable",
  unresolvable_missing_identity: "unreadable",
  unresolvable_pin_failed: "unreadable",
  empty_path: "invalid_path",
  non_unicode_path: "invalid_path",
  not_absolute: "invalid_path",
  network_path: "invalid_path",
  device_path: "invalid_path",
  alternate_data_stream: "invalid_path",
  invalid_character: "invalid_path",
  reserved_device_name: "invalid_path",
  trailing_dot_or_space: "invalid_path",
  escapes_root: "invalid_path",
  verbatim_not_normalized: "invalid_path",
  resolved_path_mismatch: "invalid_path",
  identity_changed: "invalid_path",
};

/** Motivo de omision (codigo estable del backend) -> familia con texto llano; `null` = elegible. */
export function skipReasonText(code: string | null): string {
  if (code === null) return es.cleanup.eligible;
  const family = Object.hasOwn(SKIP_FAMILIES, code) ? SKIP_FAMILIES[code] : undefined;
  return es.cleanup.skipReasons[family ?? "generic"];
}
