import { useEffect, useId, useRef } from "react";
import type { CleanupSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { categoryName } from "./categoryText";
import { abortText, cleanupReasonText } from "./cleanupText";

interface CleanupSummaryCardProps {
  summary: CleanupSummary;
  advanced: boolean;
  /** `true` si la limpieza puede repetirse (queda algo elegible): entonces Buscar de nuevo no es primaria. */
  canClean: boolean;
  /** El resumen acaba de aparecer: su titulo recibe el foco una sola vez. */
  focusRequested: boolean;
  onFocused: () => void;
  onScanAgain: () => void;
  /** "Vaciar Papelera…" junto a la nota del espacio; `undefined` = no se ofrece. */
  onEmptyBin?: (() => void) | undefined;
  /** La Papelera es de otra cuenta (mismatch): se explica en vez de ofrecer vaciarla. */
  binBlocked?: boolean;
}

const CELL = "border-b border-border-strong px-2 py-1 text-left";

/** Resumen en lenguaje llano (D-23): movido a la Papelera, omitidos por familia, cancelacion o aborto. */
export function CleanupSummaryCard({
  summary,
  advanced,
  canClean,
  focusRequested,
  onFocused,
  onScanAgain,
  onEmptyBin,
  binBlocked = false,
}: CleanupSummaryCardProps) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (!focusRequested) return;
    heading.current?.focus();
    onFocused();
  }, [focusRequested, onFocused]);

  const count = (value: number) => String(value);
  const lines: Array<[number, string]> = [
    [summary.skippedLocked, es.cleanup.summaryLocked],
    [summary.skippedProtected, es.cleanup.summaryProtected],
    [summary.skippedCloud, es.cleanup.summaryCloud],
    [summary.errors, es.cleanup.summaryErrors],
  ];
  return (
    <section
      aria-labelledby={headingId}
      className="flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-6"
    >
      <h2 id={headingId} ref={heading} tabIndex={-1} className="text-lg font-semibold">
        {es.cleanup.summaryTitle}
      </h2>
      {summary.abortReason !== null && <p role="alert">{abortText(summary.abortReason)}</p>}
      {summary.status === "cancelled" && (
        <p>{format(es.cleanup.summaryCancelled, { count: count(summary.notProcessed) })}</p>
      )}
      {(summary.movedCount > 0 || summary.status === "completed") && (
        <p className="text-xl font-semibold">
          {format(es.cleanup.summaryMoved, {
            count: count(summary.movedCount),
            size: formatBytes(summary.movedBytes),
          })}
        </p>
      )}
      {summary.movedCount > 0 && <p className="text-content-muted">{es.cleanup.summarySpaceNote}</p>}
      {summary.movedCount > 0 && onEmptyBin !== undefined && (
        <div>
          <Button onClick={onEmptyBin}>{es.recycleBin.emptyAction}</Button>
        </div>
      )}
      {summary.movedCount > 0 && binBlocked && (
        <p className="text-content-muted">{es.recycleBin.blockedMismatch}</p>
      )}
      {lines
        .filter(([value]) => value > 0)
        .map(([value, template]) => (
          <p key={template}>{format(template, { count: count(value) })}</p>
        ))}
      {advanced && summary.categories.length > 0 && (
        <table className="w-full text-sm">
          <caption className="py-1 text-left font-medium">{es.cleanup.summaryByCategory}</caption>
          <thead>
            <tr>
              <th scope="col" className={CELL}>
                {es.cleanup.categoryColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.cleanup.movedColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.cleanup.sizeColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.cleanup.skippedColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.cleanup.errorsColumn}
              </th>
            </tr>
          </thead>
          <tbody>
            {summary.categories.map((row) => (
              <tr key={row.category}>
                <th scope="row" className={CELL}>
                  {categoryName(row.category)}
                </th>
                <td className={CELL}>{row.movedCount}</td>
                <td className={CELL}>{formatBytes(row.movedBytes)}</td>
                <td className={CELL}>{row.skippedCount}</td>
                <td className={CELL}>{row.errorCount}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {advanced && summary.reasons.length > 0 && (
        <table className="w-full text-sm">
          <caption className="py-1 text-left font-medium">{es.cleanup.summaryByReason}</caption>
          <thead>
            <tr>
              <th scope="col" className={CELL}>
                {es.cleanup.reasonTextColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.cleanup.reasonCodeColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.cleanup.countColumn}
              </th>
            </tr>
          </thead>
          <tbody>
            {summary.reasons.map((row) => (
              <tr key={`${row.outcome}/${row.detail}`}>
                <th scope="row" className={CELL}>
                  {cleanupReasonText(row.outcome, row.detail)}
                </th>
                <td className={`${CELL} font-mono`}>{row.detail}</td>
                <td className={CELL}>{row.count}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div>
        <Button variant={canClean ? "secondary" : "primary"} onClick={onScanAgain}>
          {es.cleanup.scanAgain}
        </Button>
      </div>
    </section>
  );
}
