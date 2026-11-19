import { useCallback, useEffect, useRef, useState } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import {
  type AppError,
  cancelScan,
  type FolderRootRequest,
  type FolderScanEvent,
  type FolderScanSummary,
  getLastFolderScan,
  pickFolderRoot,
  startFolderScan,
  toAppError,
} from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";

/** Avance visible del analisis: solo conteos (el backend nunca manda rutas). */
export interface FolderScanProgress {
  scanId: number | null;
  filesSeen: number;
  dirsSeen: number;
  bytesSeen: number;
  saving: boolean;
}

/** Que se va a analizar (solo en modo avanzado se puede elegir una carpeta). */
export type RootChoice = "disk" | "folder";

/** La carpeta elegida con el selector: la ruta es solo para mostrarla, el analisis usa el `pickId`. */
export interface PickedFolder {
  pickId: number;
  path: string;
}

export interface FolderScanApi {
  /** `false` hasta que se leyo el ultimo analisis guardado. */
  loaded: boolean;
  last: FolderScanSummary | null;
  progress: FolderScanProgress | null;
  error: AppError | null;
  notice: string | null;
  rootChoice: RootChoice;
  picked: PickedFolder | null;
  pickError: AppError | null;
  pickNotice: string | null;
  picking: boolean;
  /** Hay una raiz valida para analizar (con "Una carpeta..." hace falta haber elegido una). */
  canStart: boolean;
  start: () => void;
  cancel: () => void;
  setRootChoice: (choice: RootChoice) => void;
  pickFolder: () => void;
}

const STARTING: FolderScanProgress = { scanId: null, filesSeen: 0, dirsSeen: 0, bytesSeen: 0, saving: false };

/** Estado del analisis de disco: ultimo resultado, progreso, cancelacion, raiz elegida y errores. */
export function useFolderScanState(advanced: boolean): FolderScanApi {
  const announce = useAnnounce();
  const [loaded, setLoaded] = useState(false);
  const [last, setLast] = useState<FolderScanSummary | null>(null);
  const [progress, setProgress] = useState<FolderScanProgress | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [rootChoice, setRootChoiceState] = useState<RootChoice>("disk");
  const [picked, setPicked] = useState<PickedFolder | null>(null);
  const [pickError, setPickError] = useState<AppError | null>(null);
  const [pickNotice, setPickNotice] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const active = useRef(true);
  const scanId = useRef<number | null>(null);
  const cancelRequested = useRef(false);
  const running = useRef(false);
  const lastRef = useRef<FolderScanSummary | null>(null);
  lastRef.current = last;

  useEffect(() => {
    active.current = true;
    getLastFolderScan()
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
    (event: FolderScanEvent) => {
      if (!active.current) return;
      setProgress((current) => {
        const base = current ?? STARTING;
        switch (event.type) {
          case "started":
            return { ...base, scanId: event.scanId };
          case "walking":
            return {
              ...base,
              filesSeen: event.filesSeen,
              dirsSeen: event.dirsSeen,
              bytesSeen: event.bytesSeen,
            };
          case "saving":
            return { ...base, saving: true };
        }
      });
      if (event.type === "started") {
        scanId.current = event.scanId;
        if (cancelRequested.current) requestCancel(event.scanId);
      }
    },
    [requestCancel],
  );

  const folderMode = advanced && rootChoice === "folder";
  const canStart = !folderMode || picked !== null;

  const start = useCallback(() => {
    if (running.current) return;
    // En simple siempre se analiza el disco; en avanzado, la raiz elegida (sin seleccion no hay nada que hacer).
    let root: FolderRootRequest = { kind: "system_drive" };
    if (folderMode) {
      if (picked === null) return;
      root = { kind: "picked", pickId: picked.pickId };
    }
    running.current = true;
    scanId.current = null;
    cancelRequested.current = false;
    setError(null);
    setNotice(null);
    setProgress(STARTING);
    announce(es.disk.announceStarted);
    startFolderScan(root, handleEvent)
      .then((summary) => {
        if (summary.status === "done") {
          if (active.current) setLast(summary);
          announce(format(es.disk.announceDone, { size: formatBytes(summary.totalBytes) }));
        } else {
          // Con un analisis previo, el aviso dice que se conserva; sin uno, solo que se cancelo.
          const text = lastRef.current === null ? es.disk.cancelledNoPrevious : es.disk.cancelledNotice;
          if (active.current) setNotice(text);
          announce(text);
        }
      })
      .catch((failure: unknown) => {
        if (!active.current) return;
        const failed = toAppError(failure);
        setError(failed);
        // La seleccion se perdio en el backend: hay que elegir la carpeta de nuevo.
        if (failed.kind === "folder_pick_not_found") setPicked(null);
      })
      .finally(() => {
        running.current = false;
        if (active.current) setProgress(null);
      });
  }, [announce, handleEvent, folderMode, picked]);

  const cancel = useCallback(() => {
    cancelRequested.current = true;
    if (scanId.current !== null) requestCancel(scanId.current);
  }, [requestCancel]);

  const setRootChoice = useCallback((choice: RootChoice) => {
    setRootChoiceState(choice);
    setPickError(null);
    setPickNotice(null);
  }, []);

  const pickFolder = useCallback(() => {
    setPickError(null);
    setPickNotice(null);
    setPicking(true);
    pickFolderRoot()
      .then((result) => {
        if (!active.current) return;
        if (result.status === "picked") setPicked({ pickId: result.pickId, path: result.path });
        // Cancelar el dialogo conserva la carpeta anterior, si habia una.
        else setPickNotice(es.disk.pickCancelled);
      })
      .catch((failure: unknown) => {
        if (active.current) setPickError(toAppError(failure));
      })
      .finally(() => {
        if (active.current) setPicking(false);
      });
  }, []);

  return {
    loaded,
    last,
    progress,
    error,
    notice,
    rootChoice,
    picked,
    pickError,
    pickNotice,
    picking,
    canStart,
    start,
    cancel,
    setRootChoice,
    pickFolder,
  };
}
