import { createContext, type ReactNode, useCallback, useContext } from "react";
import { useSettings } from "../../app/providers/SettingsProvider";
import { type DuplicateCleanupApi, useDuplicateCleanupState } from "./useDuplicateCleanup";
import { type DuplicateGroupsApi, useDuplicateGroupsState } from "./useDuplicateGroups";
import { type DuplicateScanApi, useDuplicateScanState } from "./useDuplicateScan";

interface DuplicatesValue {
  scan: DuplicateScanApi;
  groups: DuplicateGroupsApi;
  cleanup: DuplicateCleanupApi;
}

const DuplicatesContext = createContext<DuplicatesValue | null>(null);

/**
 * Mantiene el escaneo de duplicados, sus grupos y la decision vivos mientras la app este abierta,
 * para que un escaneo o una limpieza sigan visibles si el usuario cambia de pantalla y vuelve.
 */
export function DuplicatesProvider({ children }: { children: ReactNode }) {
  const { settings } = useSettings();
  const scan = useDuplicateScanState(settings.uiMode === "advanced");
  const groups = useDuplicateGroupsState(scan.last);
  const { refresh } = scan;
  const { reload } = groups;
  const onResolved = useCallback(() => {
    void refresh();
    reload();
  }, [refresh, reload]);
  const cleanup = useDuplicateCleanupState(scan.last, groups.groups, onResolved);
  return <DuplicatesContext.Provider value={{ scan, groups, cleanup }}>{children}</DuplicatesContext.Provider>;
}

export function useDuplicates(): DuplicatesValue {
  const value = useContext(DuplicatesContext);
  if (value === null) throw new Error("useDuplicates debe usarse dentro de DuplicatesProvider");
  return value;
}
