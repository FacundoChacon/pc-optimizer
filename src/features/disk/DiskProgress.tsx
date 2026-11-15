import { useId } from "react";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatPlural } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import type { FolderScanProgress } from "./useFolderScan";

const BOX = "flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-4";

function progressText(progress: FolderScanProgress): string {
  if (progress.saving) return es.disk.progressSaving;
  if (progress.filesSeen === 0 && progress.dirsSeen === 0) return es.disk.progressStarting;
  return format(es.disk.progressRunning, {
    files: formatPlural(es.disk.files, progress.filesSeen),
    size: formatBytes(progress.bytesSeen),
  });
}

/** Progreso del analisis anunciado a lectores de pantalla (`aria-live="polite"`). */
export function DiskProgressPanel({
  progress,
  onCancel,
}: {
  progress: FolderScanProgress;
  onCancel: () => void;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} aria-live="polite" className={BOX}>
      <h2 id={headingId} className="text-lg font-semibold">
        {es.disk.progressTitle}
      </h2>
      <p>{progressText(progress)}</p>
      <div>
        <Button onClick={onCancel}>{es.disk.cancel}</Button>
      </div>
    </section>
  );
}
