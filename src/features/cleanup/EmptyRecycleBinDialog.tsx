import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import type { EmptyAck, EmptyRequest } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { focusRing } from "../../shared/ui/focus";
import { itemsText } from "./recycleBinText";
import type { EmptyPhase } from "./useEmptyRecycleBin";

interface StepDialogProps {
  open: boolean;
  title: string;
  body: string;
  onCancel: () => void;
  children: ReactNode;
}

/**
 * `<dialog>` modal nativo con foco inicial en Cancelar (la accion segura). Al cerrarse devuelve el
 * foco a quien lo tenia al abrirse (la accion "Vaciar Papelera…").
 */
function StepDialog({ open, title, body, onCancel, children }: StepDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const titleId = useId();
  const bodyId = useId();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) return;
    if (open && !dialog.open) {
      triggerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      dialog.showModal();
      dialog.querySelector<HTMLElement>("[data-initial-focus]")?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
      triggerRef.current?.focus();
      triggerRef.current = null;
    }
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      role="alertdialog"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
      className="m-auto max-h-[90vh] w-full max-w-md overflow-y-auto rounded-lg border border-border-strong bg-surface-raised p-6 text-content backdrop:bg-black/60"
    >
      <h2 id={titleId} className="text-lg font-semibold">
        {title}
      </h2>
      <p id={bodyId} className="mt-2 text-content-muted">
        {body}
      </p>
      {children}
    </dialog>
  );
}

interface EmptyRecycleBinDialogProps {
  phase: EmptyPhase;
  request: EmptyRequest | null;
  ack: EmptyAck | null;
  onContinue: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Doble confirmacion del vaciado (RN-LIM-06): DOS dialogos distintos, cada uno con su propio
 * titulo. El primero reconoce la solicitud; el segundo exige marcar una casilla y es el unico que
 * pide el vaciado. Cancelar, cerrar o Escape nunca reconocen ni vacian nada.
 */
export function EmptyRecycleBinDialog({
  phase,
  request,
  ack,
  onContinue,
  onConfirm,
  onCancel,
}: EmptyRecycleBinDialogProps) {
  const [understood, setUnderstood] = useState(false);
  const hintId = useId();
  const step1Open = phase === "step1" || phase === "acknowledging";
  const step2Open = phase === "step2";

  // La casilla nunca queda marcada de una vez para otra.
  useEffect(() => {
    if (!step2Open) setUnderstood(false);
  }, [step2Open]);

  const step1Values = {
    items: itemsText(request?.itemCount ?? 0),
    size: formatBytes(request?.sizeBytes ?? 0),
  };
  const step2Values = {
    items: itemsText(ack?.itemCount ?? 0),
    size: formatBytes(ack?.sizeBytes ?? 0),
  };
  const confirmLabel =
    ack?.itemCount === 1
      ? es.recycleBin.confirmEmptyOne
      : format(es.recycleBin.confirmEmpty, { count: String(ack?.itemCount ?? 0) });

  return (
    <>
      <StepDialog
        open={step1Open}
        title={es.recycleBin.step1Title}
        body={format(es.recycleBin.step1Body, step1Values)}
        onCancel={onCancel}
      >
        <div className="mt-6 flex flex-wrap justify-end gap-3">
          <Button data-initial-focus onClick={onCancel}>
            {es.recycleBin.cancel}
          </Button>
          <Button disabled={phase !== "step1"} onClick={onContinue}>
            {es.recycleBin.continue}
          </Button>
        </div>
      </StepDialog>
      <StepDialog
        open={step2Open}
        title={es.recycleBin.step2Title}
        body={format(es.recycleBin.step2Body, step2Values)}
        onCancel={onCancel}
      >
        <label className="mt-4 flex items-start gap-3">
          <input
            type="checkbox"
            className={`mt-1 h-5 w-5 ${focusRing}`}
            checked={understood}
            onChange={(event) => setUnderstood(event.target.checked)}
          />
          <span>{es.recycleBin.acknowledgeCheckbox}</span>
        </label>
        {!understood && (
          <p id={hintId} className="mt-2 text-content-muted">
            {es.recycleBin.acknowledgeHint}
          </p>
        )}
        <div className="mt-6 flex flex-wrap justify-end gap-3">
          <Button data-initial-focus onClick={onCancel}>
            {es.recycleBin.cancel}
          </Button>
          <Button
            variant="danger"
            disabled={!understood}
            aria-describedby={understood ? undefined : hintId}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
      </StepDialog>
    </>
  );
}
