import type { CleanupOutcome, CleanupReasonOutcome, CleanupRun, JunkCategory } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatPlural } from "../../shared/i18n/format";
import { categoryName } from "../cleanup/categoryText";
import { cleanupReasonText } from "../cleanup/cleanupText";
import { emptyDetailText } from "../cleanup/recycleBinText";

const JUNK_CATEGORIES: readonly string[] = Object.keys(es.cleanup.categoryNames);

function isJunkCategory(category: string): category is JunkCategory {
  return JUNK_CATEGORIES.includes(category);
}

/** Cuantos items se omitieron (en uso, protegidos o en la nube). */
function skippedCount(run: CleanupRun): number {
  const { skippedLocked, skippedProtected, skippedCloud } = run.outcomes;
  return skippedLocked + skippedProtected + skippedCloud;
}

/** "5 se omitieron (3 en uso o modificados hace poco, 2 protegidos)." o `null` si no se omitio nada. */
function omittedSentence(run: CleanupRun): string | null {
  const total = skippedCount(run);
  if (total === 0) return null;
  const { families } = es.history.phrases;
  const { skippedLocked, skippedProtected, skippedCloud } = run.outcomes;
  const parts: string[] = [];
  if (skippedLocked > 0) parts.push(format(families.locked, { count: String(skippedLocked) }));
  if (skippedProtected > 0) parts.push(format(families.protected, { count: String(skippedProtected) }));
  if (skippedCloud > 0) parts.push(format(families.cloud, { count: String(skippedCloud) }));
  const omitted = formatPlural(es.history.phrases.omitted, total);
  return format(es.history.phrases.omittedWithFamilies, { omitted, families: parts.join(", ") });
}

function movedSentence(run: CleanupRun): string {
  const { moved, none } = es.history.phrases;
  const family = run.kind === "junk" || run.kind === "duplicates" ? run.kind : "other";
  if (run.outcomes.movedToTrash === 0) return none[family];
  return formatPlural(moved[family], run.outcomes.movedToTrash, { size: formatBytes(run.movedBytes) });
}

function emptySentence(run: CleanupRun): string {
  const { phrases } = es.history;
  if (run.outcomes.recycleBinEmptied === 0) return phrases.emptyFailed;
  const size = formatBytes(run.emptiedBytes);
  if (run.emptyDetail === "partial") return format(phrases.emptiedPartial, { size });
  if (run.emptyDetail === "unverified") return format(phrases.emptiedUnverified, { size });
  return format(phrases.emptied, { size });
}

/**
 * Frase llana de una limpieza (modo simple): que se movio, cuantos se omitieron por familia y
 * cuantos fallaron. Sin rutas ni codigos; un tipo desconocido cae en la frase generica.
 */
export function runPhrase(run: CleanupRun): string {
  if (run.kind === "recycle_bin_empty") return emptySentence(run);
  const parts = [movedSentence(run), omittedSentence(run)];
  if (run.outcomes.error > 0) parts.push(formatPlural(es.history.phrases.errors, run.outcomes.error));
  return parts.filter((part): part is string => part !== null).join(" ");
}

/** Titulo corto de una limpieza segun su tipo. */
export function runTitle(kind: CleanupRun["kind"]): string {
  return Object.hasOwn(es.history.kindTitles, kind) ? es.history.kindTitles[kind] : es.history.kindTitles.other;
}

/** Resultado de un item en texto llano; un resultado desconocido cae en el respaldo. */
export function outcomeText(outcome: string): string {
  return Object.hasOwn(es.history.outcomes, outcome) && outcome !== "unknown"
    ? es.history.outcomes[outcome as CleanupOutcome]
    : es.history.outcomes.unknown;
}

/** Nombre llano de la categoria de un item; las que no son de la whitelist caen en el respaldo. */
export function entryCategoryText(category: string): string {
  if (isJunkCategory(category)) return categoryName(category);
  if (category === "duplicates") return es.history.categories.duplicates;
  return es.history.categories.unknown;
}

function isReasonOutcome(outcome: string): outcome is CleanupReasonOutcome {
  return (
    outcome === "skipped_locked" ||
    outcome === "skipped_protected" ||
    outcome === "skipped_cloud" ||
    outcome === "error"
  );
}

/** Codigo de detalle de un item -> texto llano, o `null` si no hay detalle. */
export function entryDetailText(outcome: string, detail: string | null): string | null {
  if (detail === null) return null;
  if (outcome === "recycle_bin_emptied") return emptyDetailText(detail);
  return isReasonOutcome(outcome) ? cleanupReasonText(outcome, detail) : null;
}
