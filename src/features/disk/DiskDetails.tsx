import { useId } from "react";
import type { FolderScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";
import { issueReasonText, withCode } from "./diskText";

/** Detalles del analisis (modo avanzado): contadores, truncado y la muestra de carpetas sin acceso. */
export function DiskDetails({ summary }: { summary: FolderScanSummary }) {
  const issuesId = useId();
  const counters: Array<[string, number]> = [
    [es.disk.detailsCounters.inaccessible, summary.inaccessibleDirs],
    [es.disk.detailsCounters.reparse, summary.reparseSkipped],
    [es.disk.detailsCounters.cloud, summary.cloudFiles],
    [es.disk.detailsCounters.opaque, summary.opaqueDirs],
    [es.disk.detailsCounters.stored, summary.foldersStored],
  ];
  return (
    <Card heading={es.disk.detailsTitle}>
      <ul className="flex flex-col gap-1">
        {counters.map(([label, value]) => (
          <li key={label}>{format(es.disk.counterLine, { label, value: String(value) })}</li>
        ))}
      </ul>
      {summary.foldersTruncated && (
        <p className="text-content-muted">{format(es.disk.truncated, { count: String(summary.foldersStored) })}</p>
      )}
      {summary.issues.length > 0 && (
        <section aria-labelledby={issuesId} className="flex flex-col gap-1">
          <h3 id={issuesId} className="font-semibold">{es.disk.issuesTitle}</h3>
          <ul className="flex flex-col gap-1">
            {summary.issues.map((issue) => (
              <li key={issue.path} className="break-all">
                <span>{issue.path}</span> <span className="text-content-muted">{withCode(issueReasonText(issue.reason), issue.reason)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </Card>
  );
}
