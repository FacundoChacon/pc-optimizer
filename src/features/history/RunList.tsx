import { useId, useState } from "react";
import type { CleanupRun, CleanupRunOutcomes } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { runPhrase, outcomeText } from "./historyText";
import { RunEntries } from "./RunEntries";

const OUTCOME_CODES: ReadonlyArray<readonly [keyof CleanupRunOutcomes, string]> = [
  ["movedToTrash", "moved_to_trash"],
  ["skippedLocked", "skipped_locked"],
  ["skippedProtected", "skipped_protected"],
  ["skippedCloud", "skipped_cloud"],
  ["error", "error"],
  ["recycleBinEmptied", "recycle_bin_emptied"],
];

function runId(run: CleanupRun): string {
  return run.key.kind === "scan" ? `scan-${run.key.scanId}` : `entry-${run.key.entryId}`;
}

function OutcomeCounts({ run }: { run: CleanupRun }) {
  const present = OUTCOME_CODES.filter(([field]) => run.outcomes[field] > 0);
  if (present.length === 0) return null;
  return (
    <p className="text-sm text-content-muted">
      {format(es.history.advanced.outcomesLine, {
        title: es.history.advanced.outcomesTitle,
        counts: present
          .map(([field, code]) =>
            format(es.history.advanced.outcomeCount, {
              label: format(es.history.advanced.codeLabel, { text: outcomeText(code), code }),
              count: String(run.outcomes[field]),
            }),
          )
          .join(" · "),
      })}
    </p>
  );
}

function RunItem({ run, advanced }: { run: CleanupRun; advanced: boolean }) {
  const [open, setOpen] = useState(false);
  const summaryId = useId();
  return (
    <li className="flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-4">
      <div id={summaryId} className="flex flex-col gap-2">
        <p className="text-sm text-content-muted">{formatDateTime(run.finishedAt)}</p>
        <p>{runPhrase(run)}</p>
      </div>
      {advanced && (
        <>
          <OutcomeCounts run={run} />
          {run.kind === "duplicates" && (
            <p className="text-sm text-content-muted">
              {format(es.history.duplicatesNote, { date: formatDateTime(run.finishedAt) })}
            </p>
          )}
          <div>
            <Button aria-expanded={open} aria-describedby={summaryId} onClick={() => setOpen((value) => !value)}>
              {open ? es.history.advanced.hideFiles : es.history.advanced.viewFiles}
            </Button>
          </div>
          {open && <RunEntries runKey={run.key} />}
        </>
      )}
    </li>
  );
}

/** Lista de limpiezas, de la mas reciente a la mas antigua. */
export function RunList({ runs, advanced }: { runs: readonly CleanupRun[]; advanced: boolean }) {
  return (
    <ul aria-label={es.history.listLabel} className="flex flex-col gap-3">
      {runs.map((run) => (
        <RunItem key={runId(run)} run={run} advanced={advanced} />
      ))}
    </ul>
  );
}
