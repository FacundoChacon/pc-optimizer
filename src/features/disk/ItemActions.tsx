import { useNavigation } from "../../app/providers/NavigationProvider";
import { usePrivilege } from "../../app/providers/PrivilegeProvider";
import type { PrivilegeMismatchReason } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";

/** Motivo por el que el Explorador esta bloqueado (over-the-shoulder), o `null` si se puede usar. */
export function useRevealBlocked(): PrivilegeMismatchReason | null {
  const privilege = usePrivilege();
  return privilege.phase === "ready" && privilege.context.status === "mismatch" ? privilege.context.reason : null;
}

/** Explicacion que reemplaza a los botones cuando "Mostrar en el Explorador" esta bloqueado. */
export function RevealBlockedNote({ reason }: { reason: PrivilegeMismatchReason }) {
  return <p className="text-content-muted">{es.disk.revealBlocked[reason]}</p>;
}

/** Acciones de un elemento: mostrarlo en el Explorador y, si es basura conocida, revisarla en Limpieza. */
export function ItemActions({
  name,
  blocked,
  canReveal,
  junkHint,
  onReveal,
}: {
  name: string;
  /** El Explorador esta bloqueado: la explicacion va una vez por tarjeta, no por fila. */
  blocked: boolean;
  canReveal: boolean;
  junkHint: boolean;
  onReveal: () => void;
}) {
  const { navigate } = useNavigation();
  if ((blocked || !canReveal) && !junkHint) return null;
  return (
    <div className="flex flex-wrap items-center gap-2">
      {canReveal && !blocked && (
        <Button aria-label={format(es.disk.revealLabel, { name })} onClick={onReveal}>
          {es.disk.reveal}
        </Button>
      )}
      {junkHint && (
        <Button aria-label={format(es.disk.reviewInCleanupLabel, { name })} onClick={() => navigate("cleanup")}>
          {es.disk.reviewInCleanup}
        </Button>
      )}
    </div>
  );
}
