import { createContext, type ReactNode, useContext } from "react";
import { useJunkScan } from "./JunkScanProvider";
import { type CleanupApi, useCleanupState } from "./useCleanup";

const CleanupContext = createContext<CleanupApi | null>(null);

/** Mantiene la limpieza (preview, ejecucion y resumen) viva mientras la app este abierta. */
export function CleanupProvider({ children }: { children: ReactNode }) {
  const value = useCleanupState(useJunkScan());
  return <CleanupContext.Provider value={value}>{children}</CleanupContext.Provider>;
}

export function useCleanup(): CleanupApi {
  const value = useContext(CleanupContext);
  if (value === null) throw new Error("useCleanup debe usarse dentro de CleanupProvider");
  return value;
}
