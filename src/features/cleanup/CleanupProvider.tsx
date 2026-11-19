import { createContext, type ReactNode, useContext } from "react";
import { useJunkScan } from "./JunkScanProvider";
import { type CleanupApi, useCleanupState } from "./useCleanup";
import { type CleanNowApi, useCleanNowRequestState } from "./useCleanNowRequest";
import { type EmptyRecycleBinApi, useEmptyRecycleBinState } from "./useEmptyRecycleBin";

const CleanupContext = createContext<CleanupApi | null>(null);
const EmptyRecycleBinContext = createContext<EmptyRecycleBinApi | null>(null);
const CleanNowContext = createContext<CleanNowApi | null>(null);

/**
 * Mantiene la limpieza (preview, ejecucion y resumen) y el vaciado de la Papelera (C-10) vivos
 * mientras la app este abierta, para que la tarjeta y el resumen compartan el mismo flujo.
 */
export function CleanupProvider({ children }: { children: ReactNode }) {
  const junk = useJunkScan();
  const cleaning = useCleanupState(junk);
  const emptying = useEmptyRecycleBinState(junk);
  const cleanNow = useCleanNowRequestState();
  return (
    <CleanupContext.Provider value={cleaning}>
      <EmptyRecycleBinContext.Provider value={emptying}>
        <CleanNowContext.Provider value={cleanNow}>{children}</CleanNowContext.Provider>
      </EmptyRecycleBinContext.Provider>
    </CleanupContext.Provider>
  );
}

export function useCleanup(): CleanupApi {
  const value = useContext(CleanupContext);
  if (value === null) throw new Error("useCleanup debe usarse dentro de CleanupProvider");
  return value;
}

export function useEmptyRecycleBin(): EmptyRecycleBinApi {
  const value = useContext(EmptyRecycleBinContext);
  if (value === null) throw new Error("useEmptyRecycleBin debe usarse dentro de CleanupProvider");
  return value;
}

export function useCleanNow(): CleanNowApi {
  const value = useContext(CleanNowContext);
  if (value === null) throw new Error("useCleanNow debe usarse dentro de CleanupProvider");
  return value;
}
