import { useCallback, useEffect, useRef, useState } from "react";
import { type AppError, type CleanupEntry, type CleanupRunKey, listCleanupEntries, toAppError } from "../../lib/ipc";

/** Items por pagina de la tabla del historial en modo avanzado. */
export const ENTRIES_PAGE_SIZE = 100;

export interface RunEntriesApi {
  items: readonly CleanupEntry[];
  total: number;
  /** Las rutas llegan nulas porque la app corre con otra cuenta (decision del backend). */
  pathsHidden: boolean;
  loaded: boolean;
  loading: boolean;
  error: AppError | null;
  loadMore: () => void;
}

/** Items de una limpieza en paginas de 100. La clave se reenvia tal cual; nunca se envian rutas. */
export function useRunEntries(runKey: CleanupRunKey): RunEntriesApi {
  const [items, setItems] = useState<readonly CleanupEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [pathsHidden, setPathsHidden] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const active = useRef(true);
  const inFlight = useRef(false);
  const count = useRef(0);
  count.current = items.length;

  const fetchPage = useCallback(
    (offset: number) => {
      if (inFlight.current) return;
      inFlight.current = true;
      setLoading(true);
      setError(null);
      listCleanupEntries(runKey, offset, ENTRIES_PAGE_SIZE)
        .then((page) => {
          if (!active.current) return;
          setTotal(page.total);
          setPathsHidden(page.pathsHidden);
          setItems((current) => (offset === 0 ? page.items : [...current, ...page.items]));
        })
        .catch((failure: unknown) => {
          if (active.current) setError(toAppError(failure));
        })
        .finally(() => {
          inFlight.current = false;
          if (!active.current) return;
          setLoading(false);
          setLoaded(true);
        });
    },
    [runKey],
  );

  useEffect(() => {
    active.current = true;
    fetchPage(0);
    return () => {
      active.current = false;
      inFlight.current = false;
    };
  }, [fetchPage]);

  const loadMore = useCallback(() => fetchPage(count.current), [fetchPage]);

  return { items, total, pathsHidden, loaded, loading, error, loadMore };
}
