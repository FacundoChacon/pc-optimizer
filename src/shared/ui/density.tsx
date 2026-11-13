import { createContext, type ReactNode, useContext } from "react";

/** Densidad visual: coincide con el modo de interfaz. */
export type Density = "simple" | "advanced";

const DensityContext = createContext<Density>("simple");

export function DensityProvider({ value, children }: { value: Density; children: ReactNode }) {
  return <DensityContext.Provider value={value}>{children}</DensityContext.Provider>;
}

export function useDensity(): Density {
  return useContext(DensityContext);
}
