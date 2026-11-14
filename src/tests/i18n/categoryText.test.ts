import { describe, expect, it } from "vitest";
import {
  categoryName,
  categoryStatusText,
  skipReasonText,
} from "../../features/cleanup/categoryText";
import { es } from "../../shared/i18n/es";
import { ALL_CATEGORIES, category } from "../support/junkFixtures";

describe("categoryName", () => {
  it("cada categoria tiene un nombre llano distinto y sin el identificador tecnico", () => {
    const names = ALL_CATEGORIES.map(categoryName);
    expect(new Set(names).size).toBe(ALL_CATEGORIES.length);
    for (const [index, name] of names.entries()) {
      expect(name).toBe(es.cleanup.categoryNames[ALL_CATEGORIES[index] ?? "temp_user"]);
      expect(name).not.toContain("_");
    }
  });
});

describe("categoryStatusText", () => {
  it("done: exito, con tamano y sin nota", () => {
    expect(categoryStatusText(category("temp_windows"))).toEqual({
      tone: "success",
      label: es.cleanup.status.done,
      showSize: true,
    });
  });

  it("blocked: advertencia con el texto del motivo, sin tamano", () => {
    const text = categoryStatusText(
      category("temp_user", { scope: "per_user", status: "blocked", blockReason: "different_user" }),
    );
    expect(text).toEqual({
      tone: "warning",
      label: es.cleanup.status.blocked,
      note: es.cleanup.blockReasons.different_user,
      showSize: false,
    });
    const undetermined = categoryStatusText(
      category("temp_user", { scope: "per_user", status: "blocked", blockReason: "undetermined" }),
    );
    expect(undetermined.note).toBe(es.cleanup.blockReasons.undetermined);
  });

  it("partial: advertencia con tamano y nota de motivo mas aclaracion", () => {
    const text = categoryStatusText(
      category("crash_dumps", { scope: "mixed", status: "partial", blockReason: "different_user" }),
    );
    expect(text.tone).toBe("warning");
    expect(text.label).toBe(es.cleanup.status.partial);
    expect(text.showSize).toBe(true);
    expect(text.note).toContain(es.cleanup.blockReasons.different_user);
    expect(text.note).toContain(es.cleanup.partialNote);
  });

  it("unavailable: old_logs dice que no esta en esta version; el resto, que no se pudo revisar", () => {
    const logs = categoryStatusText(category("old_logs", { status: "unavailable" }));
    expect(logs).toEqual({
      tone: "neutral",
      label: es.cleanup.status.notInThisVersion,
      showSize: false,
    });
    const other = categoryStatusText(category("wu_cache", { status: "unavailable" }));
    expect(other.label).toBe(es.cleanup.status.unavailable);
    expect(other.showSize).toBe(false);
  });
});

describe("skipReasonText", () => {
  it.each([
    ["windows_dir", es.cleanup.skipReasons.protected_system],
    ["program_files", es.cleanup.skipReasons.protected_system],
    ["other_user_profile", es.cleanup.skipReasons.other_profile],
    ["browser_profile", es.cleanup.skipReasons.browser_profile],
    ["reparse_point", es.cleanup.skipReasons.link],
    ["reparse_point_in_path", es.cleanup.skipReasons.link],
    ["multiple_hard_links", es.cleanup.skipReasons.hard_links],
    ["skipped_cloud", es.cleanup.skipReasons.cloud],
    ["unresolvable_permission_denied", es.cleanup.skipReasons.unreadable],
    ["unresolvable_in_use", es.cleanup.skipReasons.unreadable],
    ["escapes_root", es.cleanup.skipReasons.invalid_path],
  ])("el codigo %s se agrupa en su familia", (code, expected) => {
    expect(skipReasonText(code)).toBe(expected);
  });

  it("un codigo desconocido cae en el texto generico, nunca vacio ni el codigo crudo", () => {
    for (const code of ["algo_nuevo", "", "constructor"]) {
      expect(skipReasonText(code)).toBe(es.cleanup.skipReasons.generic);
    }
  });

  it("un hallazgo elegible (null) se muestra como limpiable", () => {
    expect(skipReasonText(null)).toBe(es.cleanup.eligible);
  });
});
