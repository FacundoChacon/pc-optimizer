import { useCallback, useEffect, useRef, useState } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import {
  type AppError,
  cancelScan,
  getLastJunkScan,
  type JunkCategory,
  type JunkScanSummary,
  type ScanEvent,
  startJunkScan,
  toAppError,
} from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";

/** Avance visible de un escaneo en curso. */
export interface ScanProgressState {
  category: JunkCategory | null;
  filesSeen: number;
  scanId: number | null;
}

export interface JunkScanApi {
  /** `false` hasta que se leyo el ultimo escaneo guardado. */
  loaded: boolean;
  last: JunkScanSummary | null;
  progress: ScanProgressState | null;
  error: AppError | null;
  /** Aviso visible del ultimo escaneo (hoy solo el cancelado); se limpia al iniciar otro. */
  notice: string | null;
  start: () => void;
  cancel: () => void;
}

const STARTING: ScanProgressState = { category: null, filesSeen: 0, scanId: null };

/**
 * Estado del escaneo de basura: ultimo resultado, escaneo en curso, cancelacion y errores.
 * Lo posee `JunkScanProvider` (vive toda la sesion) para que un escaneo siga visible si el
 * usuario cambia de pantalla y vuelve.
 */
export function useJunkScanState(): JunkScanApi {
  const announce = useAnnounce();
  const [loaded, setLoaded] = useState(false);
  const [last, setLast] = useState<JunkScanSummary | null>(null);
  const [progress, setProgress] = useState<ScanProgressState | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const active = useRef(true);
  const scanId = useRef<number | null>(null);
  const cancelRequested = useRef(false);
  const running = useRef(false);
  const hadPrevious = useRef(false);

  useEffect(() => {
    active.current = true;
    getLastJunkScan()
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
    (event: ScanEvent) => {
      if (!active.current) return;
      if (event.type === "started") {
        scanId.current = event.scanId;
        setProgress((current) => ({ ...(current ?? STARTING), scanId: event.scanId }));
        if (cancelRequested.current) requestCancel(event.scanId);
      } else if (event.type === "category") {
        setProgress((current) => ({ ...(current ?? STARTING), category: event.category }));
      } else {
        setProgress((current) => ({
          ...(current ?? STARTING),
          category: event.category,
          filesSeen: event.filesSeen,
        }));
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
    announce(es.cleanup.announceStarted);
    startJunkScan(handleEvent)
      .then((summary) => {
        if (summary.status === "done") {
          if (active.current) setLast(summary);
          announce(format(es.cleanup.announceDone, { size: formatBytes(summary.totalBytes) }));
        } else {
          const text = hadPrevious.current ? es.cleanup.announceCancelled : es.cleanup.cancelledNoPrevious;
          if (active.current) setNotice(text);
          announce(text);
        }
      })
      .catch((failure: unknown) => {
        if (active.current) setError(toAppError(failure));
      })
      .finally(() => {
        running.current = false;
        if (active.current) setProgress(null);
      });
  }, [announce, handleEvent]);

  // El anuncio de cancelado depende de si habia un resultado previo que se conserva.
  hadPrevious.current = last !== null;

  const cancel = useCallback(() => {
    cancelRequested.current = true;
    if (scanId.current !== null) requestCancel(scanId.current);
  }, [requestCancel]);

  return { loaded, last, progress, error, notice, start, cancel };
}
