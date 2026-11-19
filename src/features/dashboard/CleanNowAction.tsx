import { useId } from "react";
import { useNavigation } from "../../app/providers/NavigationProvider";
import { useStoreStatus } from "../../app/providers/StoreStatusProvider";
import { es } from "../../shared/i18n/es";
import { Button } from "../../shared/ui/Button";
import { useCleanNow, useCleanup, useEmptyRecycleBin } from "../cleanup/CleanupProvider";
import { useJunkScan } from "../cleanup/JunkScanProvider";
import { useDisk } from "../disk/DiskProvider";
import { useDuplicates } from "../duplicates/DuplicatesProvider";
import { cleanNowPlan } from "./cleanNowPlan";
import { busyTask } from "./homeBusy";
import { busyReasonText } from "./homeText";

/**
 * "Limpiar ahora": la unica accion primaria de Inicio. NUNCA borra ni mueve nada: pide a Limpieza
 * abrir el preview (o buscar basura antes) y el preview es la unica confirmacion (D-02).
 * Se deshabilita, con el motivo visible, mientras haya otro trabajo en curso o la base no este lista (D-04);
 * el contexto `mismatch` no la deshabilita.
 */
export function CleanNowAction() {
  const junk = useJunkScan();
  const cleaning = useCleanup();
  const emptying = useEmptyRecycleBin();
  const { scan: disk } = useDisk();
  const duplicates = useDuplicates();
  const { status: store } = useStoreStatus();
  const cleanNow = useCleanNow();
  const { navigate } = useNavigation();
  const helpId = useId();
  const reasonId = useId();

  const task = busyTask({
    junkScanning: junk.progress !== null,
    cleaning: cleaning.progress !== null || cleaning.confirming,
    emptyingBin: emptying.phase === "preparing" || emptying.phase === "emptying",
    diskScanning: disk.progress !== null,
    duplicateScanning: duplicates.scan.progress !== null,
    duplicateCleaning: duplicates.cleanup.progress !== null || duplicates.cleanup.confirming,
  });
  const storeDown = store !== null && store.status !== "ready";
  const reason = task !== null ? busyReasonText(task) : storeDown ? es.home.busy.store : null;

  const run = () => {
    cleanNow.request(cleanNowPlan(junk.last, Date.now()));
    navigate("cleanup", { focus: "none" });
  };

  return (
    <div className="flex flex-col items-start gap-2">
      <Button
        variant="primary"
        disabled={reason !== null}
        aria-describedby={reason === null ? helpId : `${helpId} ${reasonId}`}
        onClick={run}
      >
        {es.home.cleanNow}
      </Button>
      <p id={helpId} className="text-content-muted">
        {es.home.cleanNowHelp}
      </p>
      {reason !== null && (
        <p id={reasonId} className="text-content-muted">
          {reason}
        </p>
      )}
    </div>
  );
}
