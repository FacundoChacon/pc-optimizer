import { useCallback, useEffect, useRef, useState } from "react";
import { type AppError, type CleanupRun, type CleanupRunCursor, listCleanupRuns, toAppError } from "../../lib/ipc";

export interface CleanupRunsApi {
  items: readonly CleanupRun[];
  /** `false` hasta que llego la primera pagina (o fallo). */
  loaded: boolean;
  loading: boolean;
  error: AppError | null;
  /** Hay mas limpiezas despues de las cargadas. */
  hasMore: boolean;
  loadMore: () => void;
}

/**
 * Limpiezas del historial de la mas reciente a la mas antigua, paginadas por cursor. La clave y el
 * cursor se reenvian tal como llegaron del backend. Solo lectura: nunca escribe el registro.
 */
export function useCleanupRuns(pageSize: number): CleanupRunsApi {
  const [items, setItems] = useState<readonly CleanupRun[]>([]);
  const [cursor, setCursor] = useState<CleanupRunCursor | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const active = useRef(true);
  const inFlight = useRef(false);

  const fetchPage = useCallback(
    (before: CleanupRunCursor | null) => {
      if (inFlight.current) return;
      inFlight.current = true;
      setLoading(true);
      setError(null);
      listCleanupRuns(before, pageSize)
        .then((page) => {
          if (!active.current) return;
          setItems((current) => (before === null ? page.items : [...current, ...page.items]));
          setCursor(page.nextCursor);
          setHasMore(page.nextCursor !== null);
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
    [pageSize],
  );

  useEffect(() => {
    active.current = true;
    fetchPage(null);
    return () => {
      active.current = false;
      inFlight.current = false;
    };
  }, [fetchPage]);

  const loadMore = useCallback(() => {
    if (cursor !== null) fetchPage(cursor);
  }, [cursor, fetchPage]);

  return { items, loaded, loading, error, hasMore, loadMore };
}
