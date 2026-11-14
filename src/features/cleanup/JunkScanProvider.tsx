import { createContext, type ReactNode, useContext } from "react";
import { type JunkScanApi, useJunkScanState } from "./useJunkScan";

const JunkScanContext = createContext<JunkScanApi | null>(null);

/** Mantiene el escaneo de basura (estado y resultado) vivo mientras la app este abierta. */
export function JunkScanProvider({ children }: { children: ReactNode }) {
  const value = useJunkScanState();
  return <JunkScanContext.Provider value={value}>{children}</JunkScanContext.Provider>;
}

export function useJunkScan(): JunkScanApi {
  const value = useContext(JunkScanContext);
  if (value === null) throw new Error("useJunkScan debe usarse dentro de JunkScanProvider");
  return value;
}
