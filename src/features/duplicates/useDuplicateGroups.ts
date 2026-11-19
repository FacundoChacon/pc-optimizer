import { useCallback, useEffect, useRef, useState } from "react";
import {
  type AppError,
  type DuplicateGroup,
  type DuplicateScanSummary,
  listDuplicateGroups,
  type RootKey,
  toAppError,
} from "../../lib/ipc";

/** Grupos que se piden por pagina ("Ver mas" pide la siguiente). */
export const PAGE_SIZE = 20;

export interface GroupFilters {
  root: RootKey | null;
  minSizeBytes: number | null;
}

export interface DuplicateGroupsApi {
  groups: readonly DuplicateGroup[];
  total: number;
  loading: boolean;
  error: AppError | null;
  filters: GroupFilters;
  setFilters: (filters: GroupFilters) => void;
  loadMore: () => void;
  /** Vuelve a pedir lo ya mostrado (tras resolver grupos). */
  reload: () => void;
}

/** Grupos del ultimo escaneo, paginados y filtrados en el backend. */
export function useDuplicateGroupsState(last: DuplicateScanSummary | null): DuplicateGroupsApi {
  const [groups, setGroups] = useState<readonly DuplicateGroup[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [filters, setFiltersState] = useState<GroupFilters>({ root: null, minSizeBytes: null });
  const [reloadTick, setReloadTick] = useState(0);
  const shown = useRef(PAGE_SIZE);
  const seq = useRef(0);
  const active = useRef(true);
  const scanId = last?.scanId ?? null;

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);

  // Cambia el escaneo o un filtro: se vuelve a la primera pagina.
  const key = `${scanId ?? "none"}/${filters.root ?? ""}/${filters.minSizeBytes ?? ""}`;
  const lastKey = useRef(key);
  useEffect(() => {
    if (lastKey.current !== key) {
      lastKey.current = key;
      shown.current = PAGE_SIZE;
    }
  }, [key]);

  const fetchGroups = useCallback(
    (limit: number) => {
      if (scanId === null) {
        setGroups([]);
        setTotal(0);
        return;
      }
      seq.current += 1;
      const mine = seq.current;
      setLoading(true);
      setError(null);
      const request = {
        ...(filters.root === null ? {} : { root: filters.root }),
        ...(filters.minSizeBytes === null ? {} : { minSizeBytes: filters.minSizeBytes }),
      };
      // Recarga lo ya mostrado en una sola consulta (el maximo del backend es 200 por pagina).
      listDuplicateGroups(scanId, 0, Math.min(limit, 200), request)
        .then((page) => {
          if (!active.current || mine !== seq.current) return;
          setGroups(page.groups);
          setTotal(page.total);
          setLoading(false);
        })
        .catch((failure: unknown) => {
          if (!active.current || mine !== seq.current) return;
          setError(toAppError(failure));
          setLoading(false);
        });
    },
    [scanId, filters.root, filters.minSizeBytes],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` y `reloadTick` disparan la carga.
  useEffect(() => {
    fetchGroups(shown.current);
  }, [fetchGroups, key, reloadTick]);

  const setFilters = useCallback((next: GroupFilters) => setFiltersState(next), []);

  const loadMore = useCallback(() => {
    if (scanId === null || loading) return;
    seq.current += 1;
    const mine = seq.current;
    setLoading(true);
    const request = {
      ...(filters.root === null ? {} : { root: filters.root }),
      ...(filters.minSizeBytes === null ? {} : { minSizeBytes: filters.minSizeBytes }),
    };
    listDuplicateGroups(scanId, groups.length, PAGE_SIZE, request)
      .then((page) => {
        if (!active.current || mine !== seq.current) return;
        shown.current = groups.length + page.groups.length;
        setGroups((current) => [...current, ...page.groups]);
        setTotal(page.total);
        setLoading(false);
      })
      .catch((failure: unknown) => {
        if (!active.current || mine !== seq.current) return;
        setError(toAppError(failure));
        setLoading(false);
      });
  }, [scanId, loading, groups.length, filters.root, filters.minSizeBytes]);

  const reload = useCallback(() => setReloadTick((tick) => tick + 1), []);

  return { groups, total, loading, error, filters, setFilters, loadMore, reload };
}
