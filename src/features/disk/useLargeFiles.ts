import { useCallback, useEffect, useRef, useState } from "react";
import {
  type AppError,
  type FolderScanSummary,
  type LargeFileEntry,
  type LargeFileSort,
  listLargeFiles,
  toAppError,
} from "../../lib/ipc";
import { ADVANCED_PAGE_SIZE } from "./useFolderPage";

/** Cuantos archivos del top se muestran en modo simple. */
export const SIMPLE_FILE_LIMIT = 5;

export interface LargeFilesApi {
  items: readonly LargeFileEntry[];
  total: number;
  loading: boolean;
  error: AppError | null;
  sort: LargeFileSort;
  setSort: (sort: LargeFileSort) => void;
  loadMore: () => void;
}

/**
 * Los archivos mas grandes del ultimo analisis: en simple los 5 primeros por tamano; en avanzado
 * el top ordenable por tamano o fecha, paginado de a 50.
 */
export function useLargeFilesState(last: FolderScanSummary | null, advanced: boolean): LargeFilesApi {
  const [sort, setSortState] = useState<LargeFileSort>("size");
  const [items, setItems] = useState<readonly LargeFileEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const request = useRef(0);
  const itemCount = useRef(0);
  itemCount.current = items.length;
  const scanId = last?.status === "done" ? last.scanId : null;
  const limit = advanced ? ADVANCED_PAGE_SIZE : SIMPLE_FILE_LIMIT;
  const order: LargeFileSort = advanced ? sort : "size";

  const fetchPage = useCallback(
    (offset: number) => {
      request.current += 1;
      const mine = request.current;
      if (scanId === null) {
        setItems([]);
        setTotal(0);
        return;
      }
      setLoading(true);
      setError(null);
      listLargeFiles(scanId, order, offset, limit)
        .then((page) => {
          if (request.current !== mine) return;
          setTotal(page.total);
          setItems((current) => (offset === 0 ? page.items : [...current, ...page.items]));
        })
        .catch((failure: unknown) => {
          if (request.current === mine) setError(toAppError(failure));
        })
        .finally(() => {
          if (request.current === mine) setLoading(false);
        });
    },
    [scanId, order, limit],
  );

  useEffect(() => {
    fetchPage(0);
    return () => {
      request.current += 1;
    };
  }, [fetchPage]);

  const setSort = useCallback((next: LargeFileSort) => setSortState(next), []);
  const loadMore = useCallback(() => fetchPage(itemCount.current), [fetchPage]);

  return { items, total, loading, error, sort, setSort, loadMore };
}
