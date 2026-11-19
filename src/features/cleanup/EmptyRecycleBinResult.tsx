import { useEffect, useId, useRef } from "react";
import type { AppError, EmptyResult } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { Button } from "../../shared/ui/Button";
import { emptyDetailText, emptyResultText } from "./recycleBinText";
import type { EmptyPhase } from "./useEmptyRecycleBin";

interface EmptyRecycleBinResultProps {
  phase: EmptyPhase;
  result: EmptyResult | null;
  error: AppError | null;
  /** El titulo acaba de aparecer: recibe el foco una sola vez. */
  focusRequested: boolean;
  onFocused: () => void;
  onReviewAgain: () => void;
  onDismiss: () => void;
}

/** Errores tras los cuales tiene sentido volver a medir y empezar desde el primer paso. */
const RETRYABLE = new Set(["recycle_bin_changed", "empty_request_expired", "empty_request_not_found"]);

/**
 * Estado del vaciado fuera de los dialogos: mientras se mide o se vacia, una region `aria-live`
 * educada sin controles; al terminar, el resultado en lenguaje llano o el error.
 */
export function EmptyRecycleBinResult({
  phase,
  result,
  error,
  focusRequested,
  onFocused,
  onReviewAgain,
  onDismiss,
}: EmptyRecycleBinResultProps) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const showing = (phase === "result" && result !== null) || (phase === "error" && error !== null);

  useEffect(() => {
    if (!focusRequested || !showing) return;
    heading.current?.focus();
    onFocused();
  }, [focusRequested, showing, onFocused]);

  if (phase === "preparing" || phase === "emptying") {
    return (
      <section aria-live="polite" className="rounded-lg border border-border-strong bg-surface-raised p-4">
        <p className="text-lg font-semibold">
          {phase === "emptying" ? es.recycleBin.emptying : es.recycleBin.preparing}
        </p>
      </section>
    );
  }
  if (!showing) return null;

  const detail = result === null ? null : emptyDetailText(result.status === "failed" ? result.detail : null);
  return (
    <section
      aria-labelledby={headingId}
      className="flex flex-col gap-2 rounded-lg border border-border-strong bg-surface-raised p-6"
    >
      <h2 id={headingId} ref={heading} tabIndex={-1} className="text-lg font-semibold">
        {es.recycleBin.resultTitle}
      </h2>
      {phase === "result" && result !== null && (
        <>
          <p className="text-xl font-semibold">{emptyResultText(result)}</p>
          {detail !== null && <p className="text-content-muted">{detail}</p>}
          {!result.logged && <p role="alert">{es.recycleBin.notLogged}</p>}
        </>
      )}
      {phase === "error" && error !== null && <p role="alert">{errorMessage(error)}</p>}
      <div className="flex flex-wrap gap-3">
        {phase === "error" && error !== null && RETRYABLE.has(error.kind) && (
          <Button variant="primary" onClick={onReviewAgain}>
            {es.recycleBin.reviewAgain}
          </Button>
        )}
        <Button onClick={onDismiss}>{es.recycleBin.close}</Button>
      </div>
    </section>
  );
}
