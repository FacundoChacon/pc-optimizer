import { useCallback, useEffect, useRef, useState } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import {
  type AppError,
  cancelScan,
  DEFAULT_MIN_SIZE,
  type DuplicateScanEvent,
  type DuplicateScanSummary,
  getLastDuplicateScan,
  ROOT_KEYS,
  type RootKey,
  startDuplicateScan,
  toAppError,
} from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";

/** Avance visible del escaneo, por etapa. */
export interface DuplicateScanProgress {
  scanId: number | null;
  stage: "starting" | "walking" | "partial" | "full";
  filesSeen: number;
  done: number;
  total: number;
}

export interface DuplicateScanApi {
  /** `false` hasta que se leyo el ultimo escaneo guardado. */
  loaded: boolean;
  last: DuplicateScanSummary | null;
  progress: DuplicateScanProgress | null;
  error: AppError | null;
  notice: string | null;
  /** Carpetas elegidas para el proximo escaneo (modo avanzado; por defecto las seis). */
  roots: readonly RootKey[];
  minSizeBytes: number;
  start: () => void;
  cancel: () => void;
  toggleRoot: (root: RootKey) => void;
  setMinSize: (bytes: number) => void;
  /** Vuelve a leer el ultimo escaneo (por ejemplo tras resolver grupos). */
  refresh: () => Promise<void>;
}

const STARTING: DuplicateScanProgress = { scanId: null, stage: "starting", filesSeen: 0, done: 0, total: 0 };

/**
 * Estado del escaneo de duplicados: ultimo resultado, progreso por etapa, cancelacion y opciones
 * (carpetas y tamano minimo, que solo viajan como claves y valores de una lista cerrada).
 */
export function useDuplicateScanState(advanced: boolean): DuplicateScanApi {
  const announce = useAnnounce();
  const [loaded, setLoaded] = useState(false);
  const [last, setLast] = useState<DuplicateScanSummary | null>(null);
  const [progress, setProgress] = useState<DuplicateScanProgress | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [roots, setRoots] = useState<readonly RootKey[]>(ROOT_KEYS);
  const [minSizeBytes, setMinSizeBytes] = useState(DEFAULT_MIN_SIZE);
  const active = useRef(true);
  const scanId = useRef<number | null>(null);
  const cancelRequested = useRef(false);
  const running = useRef(false);

  useEffect(() => {
    active.current = true;
    getLastDuplicateScan()
      .then((summary) => {
        if (!active.current) return;
        setLast((current) => current ?? summary);
        setLoaded(true);
      })
      .catch((failure: unknown) => {
        if (!active.current) return;
        setError(toAppError(failure));
        setLoaded(true);
      });
    return () => {
      active.current = false;
    };
  }, []);

  const requestCancel = useCallback((id: number) => {
    cancelScan(id).catch(() => undefined);
  }, []);

  const handleEvent = useCallback(
    (event: DuplicateScanEvent) => {
      if (!active.current) return;
      setProgress((current) => {
        const base = current ?? STARTING;
        switch (event.type) {
          case "started":
            return { ...base, scanId: event.scanId };
          case "walking":
            return { ...base, stage: "walking", filesSeen: event.filesSeen };
          case "partial":
            return { ...base, stage: "partial", done: event.done, total: event.total };
          case "full":
            return { ...base, stage: "full", done: event.doneBytes, total: event.totalBytes };
        }
      });
      if (event.type === "started") {
        scanId.current = event.scanId;
        if (cancelRequested.current) requestCancel(event.scanId);
      }
    },
    [requestCancel],
  );

  const start = useCallback(() => {
    if (running.current) return;
    running.current = true;
    scanId.current = null;
    cancelRequested.current = false;
    setError(null);
    setNotice(null);
    setProgress(STARTING);
    announce(es.duplicates.announceStarted);
    // Solo en avanzado se eligen carpetas y tamano: en simple se buscan las seis con 1 MB.
    const options = advanced ? { roots, minSizeBytes } : {};
    startDuplicateScan(options, handleEvent)
      .then((summary) => {
        if (summary.status === "done") {
          if (active.current) setLast(summary);
          announce(format(es.duplicates.announceDone, { count: String(summary.groupCount) }));
        } else {
          if (active.current) setNotice(es.duplicates.cancelledNotice);
          announce(es.duplicates.announceCancelled);
        }
      })
      .catch((failure: unknown) => {
        if (active.current) setError(toAppError(failure));
      })
      .finally(() => {
        running.current = false;
        if (active.current) setProgress(null);
      });
  }, [advanced, roots, minSizeBytes, announce, handleEvent]);

  const cancel = useCallback(() => {
    cancelRequested.current = true;
    if (scanId.current !== null) requestCancel(scanId.current);
  }, [requestCancel]);

  const toggleRoot = useCallback((root: RootKey) => {
    setRoots((current) => {
      if (!current.includes(root)) return ROOT_KEYS.filter((key) => key === root || current.includes(key));
      // Al menos una carpeta tiene que quedar elegida.
      return current.length === 1 ? current : current.filter((key) => key !== root);
    });
  }, []);

  const refresh = useCallback(async () => {
    try {
      const summary = await getLastDuplicateScan();
      if (active.current) setLast(summary);
    } catch (failure: unknown) {
      if (active.current) setError(toAppError(failure));
    }
  }, []);

  return {
    loaded,
    last,
    progress,
    error,
    notice,
    roots,
    minSizeBytes,
    start,
    cancel,
    toggleRoot,
    setMinSize: setMinSizeBytes,
    refresh,
  };
}
