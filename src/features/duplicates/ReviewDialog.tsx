import { useEffect, useId, useRef } from "react";
import type { DuplicateCleanupPreview } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { StatusBadge } from "../../shared/ui/StatusBadge";

interface ReviewDialogProps {
  open: boolean;
  /** `null` mientras llega el preview. */
  preview: DuplicateCleanupPreview | null;
  loading: boolean;
  /** Se envio la confirmacion: Confirmar queda deshabilitado y el dialogo no se cierra. */
  busy: boolean;
  notice: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Revision antes de mover (un solo paso, como el preview de la limpieza): `<dialog>` nativo con
 * foco inicial en la accion segura (Cancelar). Cerrarlo nunca ejecuta nada.
 */
export function ReviewDialog({ open, preview, loading, busy, notice, onConfirm, onCancel }: ReviewDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const titleId = useId();
  const bodyId = useId();
  const alertId = useId();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) return;
    if (open && !dialog.open) {
      triggerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      dialog.showModal();
      cancelRef.current?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
      triggerRef.current?.focus();
      triggerRef.current = null;
    }
  }, [open]);

  const count = preview?.totalItems ?? 0;
  const binDisabled = preview !== null && !preview.recycleBinAvailable;
  const canConfirm = preview !== null && !loading && !busy && !binDisabled && count > 0;
  const confirmLabel =
    count === 1 ? es.duplicates.confirmMoveOne : format(es.duplicates.confirmMove, { count: String(count) });
  const liveText = loading ? es.duplicates.reviewLoading : (notice ?? "");

  return (
    <dialog
      ref={dialogRef}
      role="alertdialog"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onCancel();
      }}
      className="m-auto max-h-[90vh] w-full max-w-xl overflow-y-auto rounded-lg border border-border-strong bg-surface-raised p-6 text-content backdrop:bg-black/60"
    >
      <h2 id={titleId} className="text-lg font-semibold">
        {es.duplicates.reviewTitle}
      </h2>
      <p id={bodyId} className="mt-2 text-content-muted">
        {es.duplicates.reviewIntro}
      </p>
      <p aria-live="polite" className="mt-2 min-h-6 text-content-muted">
        {liveText}
      </p>
      {binDisabled && (
        <div id={alertId} role="alert" className="mt-2">
          <StatusBadge tone="danger">{es.duplicates.recycleBinDisabled}</StatusBadge>
        </div>
      )}
      <div className="mt-4">
        <p className="text-content-muted">{es.duplicates.reviewTotalLabel}</p>
        <p className="text-2xl font-semibold">
          {format(es.duplicates.reviewTotal, {
            count: String(count),
            groups: String(preview?.groupCount ?? 0),
            size: formatBytes(preview?.totalBytes ?? 0),
          })}
        </p>
      </div>
      <p className="mt-2 text-content-muted">{es.duplicates.reviewKeepNote}</p>
      {preview !== null && preview.recentCount > 0 && (
        <p className="mt-1 text-content-muted">
          {format(es.duplicates.reviewRecentNote, {
            count: String(preview.recentCount),
            size: formatBytes(preview.recentBytes),
          })}
        </p>
      )}
      <div className="mt-6 flex flex-wrap justify-end gap-3">
        <Button ref={cancelRef} disabled={busy} onClick={onCancel}>
          {es.duplicates.cancel}
        </Button>
        <Button
          variant="danger"
          disabled={!canConfirm}
          aria-describedby={binDisabled ? alertId : undefined}
          onClick={onConfirm}
        >
          {confirmLabel}
        </Button>
      </div>
    </dialog>
  );
}
