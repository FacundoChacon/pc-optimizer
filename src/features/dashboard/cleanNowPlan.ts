import type { JunkScanSummary } from "../../lib/ipc";
import { isCleanable } from "../cleanup/cleanupText";

/** Un escaneo de basura mas viejo que esto ya no se usa para "Limpiar ahora": se busca de nuevo (D-02). */
export const CLEAN_NOW_MAX_AGE_MS = 60 * 60 * 1000;

/** Que hace "Limpiar ahora" antes de abrir el preview de la limpieza. */
export type CleanNowPlan = "preview" | "scan_then_preview";

/** `true` si el escaneo tiene alguna categoria limpiable, no bloqueada ni no disponible, con elementos. */
export function hasEligibleItems(last: JunkScanSummary | null): boolean {
  return (
    last !== null &&
    last.categories.some(
      (category) =>
        isCleanable(category.category) &&
        category.status !== "blocked" &&
        category.status !== "unavailable" &&
        category.itemCount > 0,
    )
  );
}

/** Decide si "Limpiar ahora" abre el preview ya (escaneo util y reciente) o busca basura primero. */
export function cleanNowPlan(last: JunkScanSummary | null, now: number): CleanNowPlan {
  if (last === null || last.status !== "done" || last.cleanedAt !== null || !hasEligibleItems(last)) {
    return "scan_then_preview";
  }
  const age = now - (last.finishedAt ?? last.startedAt);
  return age < CLEAN_NOW_MAX_AGE_MS ? "preview" : "scan_then_preview";
}
