import { useEffect, useId, useRef } from "react";
import type { CleanupPreview, JunkCategory, PreviewCategory } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { focusRing } from "../../shared/ui/focus";
import { StatusBadge } from "../../shared/ui/StatusBadge";
import { categoryName } from "./categoryText";

interface CleanupPreviewDialogProps {
  open: boolean;
  /** `null` mientras llega el primer preview. */
  preview: CleanupPreview | null;
  /** Se esta pidiendo un preview nuevo: no se puede confirmar hasta recibirlo. */
  loading: boolean;
  /** Se esta confirmando: Confirmar queda deshabilitado. */
  busy: boolean;
  selected: readonly JunkCategory[];
  advanced: boolean;
  /** Aviso visible dentro del dialogo (por ejemplo, "El resumen se actualizo"). */
  notice: string | null;
  onToggle: (category: JunkCategory) => void;
  onConfirm: () => void;
  onCancel: () => void;
  /**
   * Destino del foco al cerrar cuando el elemento que lo tenia al abrir ya no existe (por ejemplo,
   * el dialogo se abrio al llegar desde Inicio). Si falta o devuelve `null`, no se mueve el foco.
   */
  fallbackFocus?: () => HTMLElement | null;
  /**
   * Si devuelve `true` al cerrar, el foco va SIEMPRE a `fallbackFocus`, aunque el elemento que lo tenia al
   * abrir siga conectado (por ejemplo `<main>` o Buscar basura al llegar desde Inicio con "Limpiar ahora").
   */
  preferFallbackFocus?: () => boolean;
}

/** Se muestra una categoria si esta bloqueada o tiene algo que mover o que se omitio por reciente. */
function isVisible(category: PreviewCategory): boolean {
  return category.status === "blocked" || category.itemCount > 0 || category.recentCount > 0;
}

/**
 * Preview de la limpieza: ES la confirmacion (un solo paso). `<dialog>` nativo con foco inicial
 * en la accion segura (Cancelar); cerrarlo nunca ejecuta nada.
 */
export function CleanupPreviewDialog({
  open,
  preview,
  loading,
  busy,
  selected,
  advanced,
  notice,
  onToggle,
  onConfirm,
  onCancel,
  fallbackFocus,
  preferFallbackFocus,
}: CleanupPreviewDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const titleId = useId();
  const bodyId = useId();
  const hintId = useId();
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
      const trigger = triggerRef.current;
      const usable =
        preferFallbackFocus?.() !== true && trigger !== null && trigger.isConnected && trigger !== document.body;
      (usable ? trigger : (fallbackFocus?.() ?? null))?.focus();
      triggerRef.current = null;
    }
  }, [open]);

  const categories = preview?.categories ?? [];
  const chosen = categories.filter((category) => category.status === "included" && selected.includes(category.category));
  const count = preview === null || chosen.length === 0 ? 0 : preview.totalItems;
  const bytes = preview === null || chosen.length === 0 ? 0 : preview.totalBytes;
  const recentCount = chosen.reduce((total, category) => total + category.recentCount, 0);
  const recentBytes = chosen.reduce((total, category) => total + category.recentBytes, 0);
  const binDisabled = preview !== null && !preview.recycleBinAvailable;
  const nothingToMove = preview !== null && !loading && count === 0;
  const canConfirm = preview !== null && !loading && !busy && !binDisabled && count > 0;
  const confirmLabel =
    count === 1 ? es.cleanup.confirmMoveOne : format(es.cleanup.confirmMove, { count: String(count) });
  const describedBy = binDisabled ? alertId : nothingToMove ? hintId : undefined;
  const liveText = loading ? es.cleanup.previewLoading : (notice ?? "");

  return (
    <dialog
      ref={dialogRef}
      role="alertdialog"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      onCancel={(event) => {
        event.preventDefault();
        // Con la confirmacion en vuelo no se cierra: la ejecucion ya fue pedida.
        if (!busy) onCancel();
      }}
      className="m-auto max-h-[90vh] w-full max-w-xl overflow-y-auto rounded-lg border border-border-strong bg-surface-raised p-6 text-content backdrop:bg-black/60"
    >
      <h2 id={titleId} className="text-lg font-semibold">
        {es.cleanup.previewTitle}
      </h2>
      <p id={bodyId} className="mt-2 text-content-muted">
        {es.cleanup.previewIntro}
      </p>
      <p aria-live="polite" className="mt-2 min-h-6 text-content-muted">
        {liveText}
      </p>
      {binDisabled && (
        <div id={alertId} role="alert" className="mt-2">
          <StatusBadge tone="danger">{es.cleanup.recycleBinDisabled}</StatusBadge>
        </div>
      )}
      <fieldset className="mt-4 flex flex-col gap-3">
        <legend className="mb-1 font-medium">{es.cleanup.previewCategoriesLabel}</legend>
        {categories.filter(isVisible).map((category) => {
          const blocked = category.status === "blocked";
          return (
            <label key={category.category} className="flex items-start gap-3">
              <input
                type="checkbox"
                className={`mt-1 h-5 w-5 ${focusRing}`}
                checked={!blocked && selected.includes(category.category)}
                disabled={blocked}
                onChange={() => onToggle(category.category)}
              />
              <span className="flex flex-col">
                <span className="font-medium">{categoryName(category.category)}</span>
                {blocked ? (
                  <span className="text-content-muted">
                    {category.blockReason === undefined ? "" : es.cleanup.blockReasons[category.blockReason]}
                  </span>
                ) : (
                  <span className="text-content-muted">
                    {format(es.cleanup.previewItemLine, {
                      count: String(category.itemCount),
                      size: formatBytes(category.sizeBytes),
                    })}
                  </span>
                )}
                {advanced && !blocked && (
                  <span className="text-content-muted">
                    {format(es.cleanup.previewRecentColumn, {
                      count: String(category.recentCount),
                      size: formatBytes(category.recentBytes),
                    })}
                  </span>
                )}
              </span>
            </label>
          );
        })}
      </fieldset>
      <div className="mt-4">
        <p className="text-content-muted">{es.cleanup.previewTotalLabel}</p>
        <p className="text-2xl font-semibold">
          {format(es.cleanup.previewTotal, { count: String(count), size: formatBytes(bytes) })}
        </p>
      </div>
      <p className="mt-2 text-content-muted">{es.cleanup.previewRecycleNote}</p>
      {recentCount > 0 && (
        <p className="mt-1 text-content-muted">
          {format(es.cleanup.previewRecentNote, {
            count: String(recentCount),
            size: formatBytes(recentBytes),
          })}
        </p>
      )}
      {nothingToMove && !binDisabled && (
        <p id={hintId} className="mt-2 text-content-muted">
          {es.cleanup.previewNothingSelected}
        </p>
      )}
      <div className="mt-6 flex flex-wrap justify-end gap-3">
        <Button ref={cancelRef} disabled={busy} onClick={onCancel}>
          {es.cleanup.cancel}
        </Button>
        <Button
          variant="danger"
          disabled={!canConfirm}
          aria-describedby={describedBy}
          onClick={onConfirm}
        >
          {confirmLabel}
        </Button>
      </div>
    </dialog>
  );
}
