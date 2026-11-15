import { createContext, type ReactNode, useContext } from "react";
import { useSettings } from "../../app/providers/SettingsProvider";
import { type FolderPageApi, useFolderPageState } from "./useFolderPage";
import { type FolderScanApi, useFolderScanState } from "./useFolderScan";
import { type LargeFilesApi, useLargeFilesState } from "./useLargeFiles";
import { type RevealApi, useRevealState } from "./useReveal";

interface DiskValue {
  scan: FolderScanApi;
  folders: FolderPageApi;
  files: LargeFilesApi;
  reveal: RevealApi;
}

const DiskContext = createContext<DiskValue | null>(null);

/**
 * Mantiene el analisis de disco vivo mientras la app este abierta, para que un analisis en curso
 * siga visible si el usuario cambia de pantalla y vuelve.
 */
export function DiskProvider({ children }: { children: ReactNode }) {
  const { settings } = useSettings();
  const advanced = settings.uiMode === "advanced";
  const scan = useFolderScanState(advanced);
  const folders = useFolderPageState(scan.last, advanced);
  const files = useLargeFilesState(scan.last, advanced);
  const reveal = useRevealState();
  return <DiskContext.Provider value={{ scan, folders, files, reveal }}>{children}</DiskContext.Provider>;
}

export function useDisk(): DiskValue {
  const value = useContext(DiskContext);
  if (value === null) throw new Error("useDisk debe usarse dentro de DiskProvider");
  return value;
}
