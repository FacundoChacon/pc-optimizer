import { useCallback, useEffect, useRef, useState } from "react";
import {
  type AppError,
  type FolderEntry,
  type FolderPage,
  type FolderScanSummary,
  type FolderSort,
  listFolderChildren,
  toAppError,
} from "../../lib/ipc";

/** Cuantas carpetas de la raiz se muestran en modo simple. */
export const SIMPLE_FOLDER_LIMIT = 5;
/** Tamano de pagina del modo avanzado ("Ver mas" pide la siguiente). */
export const ADVANCED_PAGE_SIZE = 50;

export interface FolderPageApi {
  /** La carpeta que se esta mirando (con migas de pan y resto) o `null` mientras carga. */
  page: FolderPage | null;
  /** Los hijos cargados hasta ahora (acumula las paginas pedidas con `loadMore`). */
  items: readonly FolderEntry[];
  total: number;
  loading: boolean;
  error: AppError | null;
  sort: FolderSort;
  setSort: (sort: FolderSort) => void;
  /** Entra en una carpeta (`null` = la raiz). Solo modo avanzado. */
  navigateTo: (folderId: number | null) => void;
  loadMore: () => void;
}

interface Position {
  scanId: number | null;
  folderId: number | null;
}

/**
 * Hijos de una carpeta del ultimo analisis. En simple, las 5 mas grandes de la raiz; en avanzado,
 * la carpeta actual (navegable) ordenada y paginada de a 50.
 */
export function useFolderPageState(last: FolderScanSummary | null, advanced: boolean): FolderPageApi {
  const scanId = last?.status === "done" ? last.scanId : null;
  const [position, setPosition] = useState<Position>({ scanId, folderId: null });
  const [sort, setSortState] = useState<FolderSort>("size");
  const [page, setPage] = useState<FolderPage | null>(null);
  const [items, setItems] = useState<readonly FolderEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const request = useRef(0);
  const itemCount = useRef(0);
  itemCount.current = items.length;
  // Un analisis nuevo vuelve a la raiz.
  const folderId = position.scanId === scanId ? position.folderId : null;
  const limit = advanced ? ADVANCED_PAGE_SIZE : SIMPLE_FOLDER_LIMIT;
  const order: FolderSort = advanced ? sort : "size";

  const fetchPage = useCallback(
    (offset: number) => {
      request.current += 1;
      const mine = request.current;
      if (scanId === null) {
        setPage(null);
        setItems([]);
        return;
      }
      setLoading(true);
      setError(null);
      listFolderChildren(scanId, folderId, order, offset, limit)
        .then((loaded) => {
          if (request.current !== mine) return;
          setPage(loaded);
          setItems((current) => (offset === 0 ? loaded.items : [...current, ...loaded.items]));
        })
        .catch((failure: unknown) => {
          if (request.current === mine) setError(toAppError(failure));
        })
        .finally(() => {
          if (request.current === mine) setLoading(false);
        });
    },
    [scanId, folderId, order, limit],
  );

  useEffect(() => {
    fetchPage(0);
    return () => {
      request.current += 1;
    };
  }, [fetchPage]);

  const setSort = useCallback((next: FolderSort) => setSortState(next), []);
  const navigateTo = useCallback(
    (target: number | null) => setPosition({ scanId, folderId: target }),
    [scanId],
  );
  const loadMore = useCallback(() => fetchPage(itemCount.current), [fetchPage]);

  return {
    page,
    items,
    total: page?.total ?? 0,
    loading,
    error,
    sort,
    setSort,
    navigateTo,
    loadMore,
  };
}
