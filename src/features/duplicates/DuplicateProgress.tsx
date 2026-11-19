import { useEffect, useId, useRef } from "react";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatPlural } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import type { MoveProgressState } from "./useDuplicateCleanup";
import type { DuplicateScanProgress } from "./useDuplicateScan";

const BOX = "flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-4";

function stageText(progress: DuplicateScanProgress): string {
  switch (progress.stage) {
    case "starting":
      return es.duplicates.progressStarting;
    case "walking":
      return formatPlural(es.duplicates.progressWalking, progress.filesSeen);
    case "partial":
      return format(es.duplicates.progressPartial, {
        done: String(progress.done),
        total: String(progress.total),
      });
    case "full":
      return format(es.duplicates.progressFull, {
        done: formatBytes(progress.done),
        total: formatBytes(progress.total),
      });
  }
}

/** Progreso del escaneo por etapa, anunciado a lectores de pantalla (`aria-live="polite"`). */
export function ScanProgressPanel({
  progress,
  onCancel,
}: {
  progress: DuplicateScanProgress;
  onCancel: () => void;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} aria-live="polite" className={BOX}>
      <h2 id={headingId} className="text-lg font-semibold">
        {es.duplicates.progressTitle}
      </h2>
      <p>{stageText(progress)}</p>
      <div>
        <Button onClick={onCancel}>{es.duplicates.cancel}</Button>
      </div>
    </section>
  );
}

/** Progreso de la ejecucion (copias movidas a la Papelera) con su boton para cancelar. */
export function MoveProgressPanel({
  progress,
  onCancel,
}: {
  progress: MoveProgressState;
  onCancel: () => void;
}) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);

  // El dialogo se cierra al empezar: el foco pasa al titulo del avance para no perderse.
  useEffect(() => {
    heading.current?.focus();
  }, []);

  return (
    <section aria-labelledby={headingId} aria-live="polite" className={BOX}>
      <h2 id={headingId} ref={heading} tabIndex={-1} className="text-lg font-semibold">
        {es.duplicates.movingTitle}
      </h2>
      <p>
        {format(es.duplicates.movingProgress, {
          done: String(progress.processed),
          total: String(progress.total),
        })}
      </p>
      <div>
        <Button onClick={onCancel}>{es.duplicates.cancelMoving}</Button>
      </div>
    </section>
  );
}
