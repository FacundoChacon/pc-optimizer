import { useId } from "react";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { categoryName } from "./categoryText";
import type { ScanProgressState } from "./useJunkScan";

/** Progreso anunciado a lectores de pantalla (`aria-live="polite"`) y boton para cancelar. */
export function ScanProgress({ progress, onCancel }: { progress: ScanProgressState; onCancel: () => void }) {
  const headingId = useId();
  return (
    <section
      aria-labelledby={headingId}
      aria-live="polite"
      className="flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-4"
    >
      <h2 id={headingId} className="text-lg font-semibold">
        {es.cleanup.progressTitle}
      </h2>
      <p>
        {progress.category === null
          ? es.cleanup.progressStarting
          : format(es.cleanup.progressCategory, { category: categoryName(progress.category) })}
      </p>
      <p className="text-content-muted">
        {format(es.cleanup.progressFiles, { count: String(progress.filesSeen) })}
      </p>
      <div>
        <Button onClick={onCancel}>{es.cleanup.cancel}</Button>
      </div>
    </section>
  );
}
