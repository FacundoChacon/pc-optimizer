import type { StoreStatus } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import type { StatusTone } from "../../shared/ui/StatusBadge";

export interface StoreStatusText {
  tone: StatusTone;
  label: string;
  explanation?: string;
  canRecover: boolean;
}

/** Estado de la base (o `null` si aun se desconoce) -> textos del catalogo. Puro. */
export function storeStatusText(status: StoreStatus | null): StoreStatusText {
  if (status === null) return { tone: "neutral", label: es.app.loading, canRecover: false };
  if (status.status === "ready") {
    return { tone: "success", label: es.settings.database.ready, canRecover: false };
  }
  return {
    tone: "danger",
    label: es.settings.database.unavailable,
    explanation: es.store.reasons[status.reason],
    canRecover: status.recoverable,
  };
}
