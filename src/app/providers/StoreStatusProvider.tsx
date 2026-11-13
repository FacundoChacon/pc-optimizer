import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { getStoreStatus, recoverStore, type RecoveryResult, type StoreStatus } from "../../lib/ipc";

interface StoreStatusValue {
  /** `null` hasta conocer el estado (o si la consulta fallo). */
  status: StoreStatus | null;
  refresh: () => Promise<void>;
  /** Respalda y recrea la base; las llamadas simultaneas comparten una sola invocacion. */
  recover: () => Promise<RecoveryResult>;
}

const StoreStatusContext = createContext<StoreStatusValue | null>(null);

interface Props {
  children: ReactNode;
  /** Se invoca tras una recuperacion exitosa (p. ej. para recargar la configuracion). */
  onRecovered: () => void;
}

export function StoreStatusProvider({ children, onRecovered }: Props) {
  const [status, setStatus] = useState<StoreStatus | null>(null);
  const inFlight = useRef<Promise<RecoveryResult> | null>(null);
  const onRecoveredRef = useRef(onRecovered);
  onRecoveredRef.current = onRecovered;

  const refresh = useCallback(async () => {
    try {
      setStatus(await getStoreStatus());
    } catch {
      // Sin estado conocido: no se muestra aviso; los errores de datos los reporta cada pantalla.
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const recover = useCallback((): Promise<RecoveryResult> => {
    if (inFlight.current !== null) return inFlight.current;
    const run = (async () => {
      try {
        const result = await recoverStore();
        await refresh();
        onRecoveredRef.current();
        return result;
      } finally {
        inFlight.current = null;
      }
    })();
    inFlight.current = run;
    return run;
  }, [refresh]);

  const value = useMemo(() => ({ status, refresh, recover }), [status, refresh, recover]);
  return <StoreStatusContext.Provider value={value}>{children}</StoreStatusContext.Provider>;
}

export function useStoreStatus(): StoreStatusValue {
  const value = useContext(StoreStatusContext);
  if (value === null) throw new Error("useStoreStatus debe usarse dentro de StoreStatusProvider");
  return value;
}
