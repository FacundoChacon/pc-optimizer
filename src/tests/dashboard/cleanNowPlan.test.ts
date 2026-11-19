import { describe, expect, it } from "vitest";
import { CLEAN_NOW_MAX_AGE_MS, cleanNowPlan, hasEligibleItems } from "../../features/dashboard/cleanNowPlan";
import { summary } from "../support/junkFixtures";

const NOW = 1_800_000_000_000;
const MIN = 60_000;
const withTemp = { temp_user: { scope: "per_user" as const, itemCount: 4, sizeBytes: 4096 } };

/** Escaneo `done` con elegibles que termino hace `ageMs`. */
const finishedAgo = (ageMs: number, overrides = {}) =>
  summary(withTemp, { finishedAt: NOW - ageMs, startedAt: NOW - ageMs - 5000, ...overrides });

describe("cleanNowPlan", () => {
  it("escaneo done, no limpiado y con elegibles de hace 10 min -> preview", () => {
    expect(cleanNowPlan(finishedAgo(10 * MIN), NOW)).toBe("preview");
  });

  it("59 min 59 s sigue siendo preview y exactamente 60 min ya busca de nuevo", () => {
    expect(cleanNowPlan(finishedAgo(60 * MIN - 1000), NOW)).toBe("preview");
    expect(CLEAN_NOW_MAX_AGE_MS).toBe(60 * MIN);
    expect(cleanNowPlan(finishedAgo(60 * MIN), NOW)).toBe("scan_then_preview");
    expect(cleanNowPlan(finishedAgo(120 * MIN), NOW)).toBe("scan_then_preview");
  });

  it("sin escaneo -> scan_then_preview", () => {
    expect(cleanNowPlan(null, NOW)).toBe("scan_then_preview");
  });

  it("escaneo cancelado -> scan_then_preview", () => {
    expect(cleanNowPlan(finishedAgo(MIN, { status: "cancelled", categories: [] }), NOW)).toBe("scan_then_preview");
  });

  it("escaneo ya limpiado -> scan_then_preview", () => {
    expect(cleanNowPlan(finishedAgo(MIN, { cleanedAt: NOW - 30_000 }), NOW)).toBe("scan_then_preview");
  });

  it("sin categorias limpiables con elementos -> scan_then_preview", () => {
    const blocked = summary({ temp_user: { scope: "per_user", status: "blocked", itemCount: 9 } });
    const unavailable = summary({ temp_windows: { status: "unavailable", itemCount: 9 } });
    const empty = summary({ temp_user: { itemCount: 0 } });
    const onlyBin = summary({ recycle_bin: { scope: "per_user", itemCount: 12, sizeBytes: 999 } });
    for (const last of [blocked, unavailable, empty, onlyBin]) {
      expect(cleanNowPlan({ ...last, finishedAt: NOW - MIN }, NOW)).toBe("scan_then_preview");
    }
  });

  it("usa startedAt cuando finishedAt es nulo", () => {
    expect(cleanNowPlan(finishedAgo(10 * MIN, { finishedAt: null, startedAt: NOW - 10 * MIN }), NOW)).toBe("preview");
    expect(cleanNowPlan(finishedAgo(10 * MIN, { finishedAt: null, startedAt: NOW - 90 * MIN }), NOW)).toBe(
      "scan_then_preview",
    );
  });
});

describe("hasEligibleItems", () => {
  it("cuenta una categoria limpiable no bloqueada con elementos y no la Papelera", () => {
    expect(hasEligibleItems(summary(withTemp))).toBe(true);
    expect(hasEligibleItems(summary({ recycle_bin: { itemCount: 3 } }))).toBe(false);
    expect(hasEligibleItems(null)).toBe(false);
  });
});
