import { es } from "../../shared/i18n/es";
import { Button } from "../../shared/ui/Button";
import { StatusBadge } from "../../shared/ui/StatusBadge";
import { useNavigation } from "../providers/NavigationProvider";
import { useStoreStatus } from "../providers/StoreStatusProvider";

/** Aviso global mientras la base local no este disponible. */
export function StoreBanner() {
  const { status } = useStoreStatus();
  const { navigate } = useNavigation();
  if (status === null || status.status === "ready") return null;
  return (
    <div
      role="alert"
      className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-danger bg-surface-raised p-4"
    >
      <div className="flex flex-col gap-1">
        <StatusBadge tone="danger">{es.store.bannerTitle}</StatusBadge>
        <p className="text-content-muted">{es.store.reasons[status.reason]}</p>
      </div>
      <Button onClick={() => navigate("settings")}>{es.store.bannerAction}</Button>
    </div>
  );
}
