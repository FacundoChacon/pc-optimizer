import { useState } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import { useStoreStatus } from "../../app/providers/StoreStatusProvider";
import { toAppError } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { ConfirmDialog } from "../../shared/ui/ConfirmDialog";
import { StatusBadge } from "../../shared/ui/StatusBadge";
import { storeStatusText } from "./storeStatusText";

export function DatabaseSection() {
  const { status, recover } = useStoreStatus();
  const announce = useAnnounce();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const text = storeStatusText(status);

  async function confirm(): Promise<void> {
    setBusy(true);
    setFailure(null);
    try {
      const result = await recover();
      setDialogOpen(false);
      announce(format(es.settings.database.recovered, { file: result.backupFileName }));
      // La accion desaparece al quedar lista la base: el foco pasa al contenido principal.
      document.getElementById("main")?.focus();
    } catch (error: unknown) {
      const appError = toAppError(error);
      setDialogOpen(false);
      setFailure(
        appError.kind === "store_unavailable"
          ? es.settings.database.recoverRetry
          : errorMessage(appError),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card heading={es.settings.database.heading}>
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-content-muted">{es.settings.database.statusLabel}</span>
        <StatusBadge tone={text.tone}>{text.label}</StatusBadge>
      </div>
      {text.explanation !== undefined && <p className="text-content-muted">{text.explanation}</p>}
      {failure !== null && (
        <p role="alert" className="text-danger">
          {failure}
        </p>
      )}
      {text.canRecover && (
        <div>
          <Button onClick={() => setDialogOpen(true)}>{es.settings.database.recoverAction}</Button>
        </div>
      )}
      <ConfirmDialog
        open={dialogOpen}
        title={es.settings.database.confirmTitle}
        body={es.settings.database.confirmBody}
        confirmLabel={busy ? es.settings.database.working : es.settings.database.confirmAction}
        cancelLabel={es.settings.database.cancel}
        busy={busy}
        onConfirm={() => void confirm()}
        onCancel={() => setDialogOpen(false)}
      />
    </Card>
  );
}
