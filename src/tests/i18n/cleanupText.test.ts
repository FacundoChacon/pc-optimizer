import { describe, expect, it } from "vitest";
import {
  abortText,
  CLEANABLE_CATEGORIES,
  cleanupReasonText,
  isCleanable,
} from "../../features/cleanup/cleanupText";
import type { CleanupReasonOutcome } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";

const reasons = es.cleanup.cleanupReasons;

describe("cleanupReasonText", () => {
  it.each<[CleanupReasonOutcome, string, string]>([
    ["skipped_locked", "in_use", reasons.locked],
    ["skipped_locked", "unresolvable_in_use", reasons.locked],
    ["skipped_locked", "changed_since_scan", reasons.locked],
    ["skipped_locked", "recently_modified", reasons.locked],
    ["skipped_protected", "identity_changed", reasons.changed],
    ["skipped_protected", "unresolvable_not_found", reasons.missing],
    ["error", "not_found", reasons.missing],
    ["skipped_protected", "blocked_different_user", reasons.other_account],
    ["skipped_protected", "blocked_undetermined", reasons.other_account],
    ["skipped_protected", "other_user_profile", reasons.other_account],
    ["skipped_protected", "reparse_point", reasons.links],
    ["skipped_protected", "multiple_hard_links", reasons.links],
    ["skipped_cloud", "skipped_cloud", reasons.cloud],
    ["error", "recycle_bin_unavailable", reasons.bin_unavailable],
    ["error", "trash_unavailable", reasons.bin_unavailable],
    ["error", "recycle_bin_too_small", reasons.too_large],
    ["error", "path_too_long", reasons.path_too_long],
    ["error", "permission_denied", reasons.permission_denied],
    ["error", "not_moved", reasons.error],
    ["error", "io", reasons.error],
    ["error", "io_0x80270021", reasons.error],
  ])("%s / %s se agrupa en su familia", (outcome, detail, expected) => {
    expect(cleanupReasonText(outcome, detail)).toBe(expected);
  });

  it("un codigo desconocido cae en la familia del resultado, nunca vacio ni el codigo crudo", () => {
    const fallback: Array<[CleanupReasonOutcome, string]> = [
      ["skipped_locked", reasons.locked],
      ["skipped_protected", reasons.protected],
      ["skipped_cloud", reasons.cloud],
      ["error", reasons.error],
    ];
    for (const [outcome, expected] of fallback) {
      for (const detail of ["algo_nuevo", "", "constructor", "toString"]) {
        expect(cleanupReasonText(outcome, detail)).toBe(expected);
      }
    }
  });

  it("todos los textos de familia existen y son distintos entre si", () => {
    const texts = Object.values(reasons);
    for (const text of texts) expect(text.length).toBeGreaterThan(0);
    expect(new Set(texts).size).toBe(texts.length);
  });
});

describe("abortText", () => {
  it("cada motivo de aborto tiene su texto del catalogo", () => {
    expect(abortText("log_unavailable")).toBe(es.cleanup.summaryAborted.log_unavailable);
    expect(abortText("recycle_bin_unavailable")).toBe(es.cleanup.summaryAborted.recycle_bin_unavailable);
    expect(abortText("trash_unavailable")).toBe(es.cleanup.summaryAborted.trash_unavailable);
  });
});

describe("categorias limpiables", () => {
  it("la Papelera y los registros antiguos no se limpian; el resto si", () => {
    expect(isCleanable("recycle_bin")).toBe(false);
    expect(isCleanable("old_logs")).toBe(false);
    expect(isCleanable("temp_user")).toBe(true);
    expect(isCleanable("crash_dumps")).toBe(true);
    expect(CLEANABLE_CATEGORIES).toHaveLength(9);
    expect(CLEANABLE_CATEGORIES).not.toContain("recycle_bin");
    expect(CLEANABLE_CATEGORIES).not.toContain("old_logs");
  });
});
