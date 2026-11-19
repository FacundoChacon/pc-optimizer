import { usePrivilege } from "../../app/providers/PrivilegeProvider";
import type { FolderScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatPlural } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";

function summaryLine(summary: FolderScanSummary): string {
  const analyzed = formatBytes(summary.totalBytes);
  const files = formatPlural(es.disk.files, summary.fileCount);
  if (summary.root.kind === "system_drive" && summary.volume !== null) {
    return format(es.disk.summaryDrive, {
      used: formatBytes(Math.max(0, summary.volume.totalBytes - summary.volume.freeBytes)),
      total: formatBytes(summary.volume.totalBytes),
      analyzed,
      files,
    });
  }
  return format(es.disk.summaryFolder, { path: summary.root.path, analyzed, files });
}

/** Resumen del ultimo analisis y sus avisos, en lenguaje llano. */
export function DiskSummaryCard({ summary }: { summary: FolderScanSummary }) {
  const privilege = usePrivilege();
  const mismatch = privilege.phase === "ready" && privilege.context.status === "mismatch";
  return (
    <Card heading={es.disk.summaryTitle}>
      <p className="text-xl font-semibold">{summaryLine(summary)}</p>
      {summary.unaccountedBytes !== null && summary.unaccountedBytes > 0 && (
        <>
          <p>{format(es.disk.unaccounted, { size: formatBytes(summary.unaccountedBytes) })}</p>
          <p className="text-content-muted">{es.disk.unaccountedHelp}</p>
        </>
      )}
      {summary.overCounted && <p className="text-content-muted">{es.disk.overCounted}</p>}
      {summary.inaccessibleDirs > 0 && (
        <p className="text-content-muted">{formatPlural(es.disk.warnInaccessible, summary.inaccessibleDirs)}</p>
      )}
      {summary.cloudFiles > 0 && (
        <p className="text-content-muted">{formatPlural(es.disk.warnCloud, summary.cloudFiles)}</p>
      )}
      {mismatch && <p className="text-content-muted">{es.disk.mismatchNote}</p>}
    </Card>
  );
}
