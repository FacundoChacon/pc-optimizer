import { useEffect, useId, useRef } from "react";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import type { CleanupProgressState } from "./useCleanup";

/** Avance de la limpieza anunciado a lectores de pantalla (`aria-live="polite"`) y boton para cancelar. */
export function CleanupProgress({
  progress,
  onCancel,
}: {
  progress: CleanupProgressState;
  onCancel: () => void;
}) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);

  // El dialogo se cierra al empezar: el foco pasa al titulo del avance para no perderse.
  useEffect(() => {
    heading.current?.focus();
  }, []);

  return (
    <section
      aria-labelledby={headingId}
      aria-live="polite"
      className="flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-4"
    >
      <h2 id={headingId} ref={heading} tabIndex={-1} className="text-lg font-semibold">
        {es.cleanup.cleaningTitle}
      </h2>
      <p>
        {format(es.cleanup.cleaningProgress, {
          done: String(progress.processed),
          total: String(progress.total),
        })}
      </p>
      <div>
        <Button onClick={onCancel}>{es.cleanup.cancelCleaning}</Button>
      </div>
    </section>
  );
}
