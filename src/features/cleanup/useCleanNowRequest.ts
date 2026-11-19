import { useCallback, useMemo, useRef, useState } from "react";
import type { CleanNowPlan } from "../dashboard/cleanNowPlan";

/** Pedido de sesion "Limpiar ahora": Inicio lo deja y la pantalla Limpieza lo consume al montarse (D-03). */
export interface CleanNowApi {
  /** Plan pedido y todavia no consumido. */
  pending: CleanNowPlan | null;
  request: (plan: CleanNowPlan) => void;
  /** Devuelve el plan pendiente y lo limpia (`null` si no hay). */
  consume: () => CleanNowPlan | null;
  /** Descarta el pedido sin ejecutarlo (por ejemplo, al salir de Limpieza). */
  drop: () => void;
}

export function useCleanNowRequestState(): CleanNowApi {
  const [pending, setPending] = useState<CleanNowPlan | null>(null);
  const latest = useRef<CleanNowPlan | null>(null);

  const request = useCallback((plan: CleanNowPlan) => {
    latest.current = plan;
    setPending(plan);
  }, []);

  const consume = useCallback(() => {
    const plan = latest.current;
    latest.current = null;
    setPending(null);
    return plan;
  }, []);

  const drop = useCallback(() => {
    latest.current = null;
    setPending(null);
  }, []);

  return useMemo(() => ({ pending, request, consume, drop }), [pending, request, consume, drop]);
}
