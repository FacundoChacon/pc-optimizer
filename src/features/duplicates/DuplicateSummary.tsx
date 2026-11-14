import { useEffect, useId, useRef } from "react";
import type { DuplicateCleanupSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { cleanupReasonText } from "../cleanup/cleanupText";

interface DuplicateSummaryProps {
  summary: DuplicateCleanupSummary;
  advanced: boolean;
  /** El resumen acaba de aparecer: su titulo recibe el foco una sola vez. */
  focusRequested: boolean;
  onFocused: () => void;
  onScanAgain: () => void;
}

const CELL = "border-b border-border-strong px-2 py-1 text-left";

/** Resumen de la ejecucion en lenguaje llano; en avanzado, con el codigo crudo de cada motivo. */
export function DuplicateSummaryCard({
  summary,
  advanced,
  focusRequested,
  onFocused,
  onScanAgain,
}: DuplicateSummaryProps) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (!focusRequested) return;
    heading.current?.focus();
    onFocused();
  }, [focusRequested, onFocused]);

  const skipped = summary.skippedLocked + summary.skippedProtected + summary.skippedCloud + summary.errors;
  return (
    <section
      aria-labelledby={headingId}
      className="flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-6"
    >
      <h2 id={headingId} ref={heading} tabIndex={-1} className="text-lg font-semibold">
        {es.duplicates.summaryTitle}
      </h2>
      {summary.abortReason !== null && <p role="alert">{es.duplicates.summaryAborted[summary.abortReason]}</p>}
      {summary.status === "cancelled" && (
        <p>{format(es.duplicates.summaryCancelled, { count: String(summary.notProcessed) })}</p>
      )}
      {(summary.movedCount > 0 || summary.status === "completed") && (
        <p className="text-xl font-semibold">
          {format(es.duplicates.summaryMoved, {
            count: String(summary.movedCount),
            size: formatBytes(summary.movedBytes),
          })}
        </p>
      )}
      {summary.movedCount > 0 && <p className="text-content-muted">{es.duplicates.summarySpaceNote}</p>}
      {summary.reasons.map((reason) => (
        <p key={`${reason.outcome}/${reason.detail}`}>
          {format(es.duplicates.summarySkipped, {
            count: String(reason.count),
            reason: cleanupReasonText(reason.outcome, reason.detail).toLowerCase(),
          })}
        </p>
      ))}
      {skipped > 0 && <p className="text-content-muted">{es.duplicates.summaryRetry}</p>}
      {!summary.groupsResolved && <p role="alert">{es.duplicates.summaryNotResolved}</p>}
      {advanced && summary.reasons.length > 0 && (
        <table className="w-full text-sm">
          <caption className="py-1 text-left font-medium">{es.duplicates.advanced.reasonsCaption}</caption>
          <thead>
            <tr>
              <th scope="col" className={CELL}>
                {es.duplicates.advanced.reasonTextColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.duplicates.advanced.reasonCodeColumn}
              </th>
              <th scope="col" className={CELL}>
                {es.duplicates.advanced.countColumn}
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
        <Button variant="secondary" onClick={onScanAgain}>
          {es.duplicates.scanAgain}
        </Button>
      </div>
    </section>
  );
}
