import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import {
  type AppError,
  cancelCleanup,
  type CleanupEvent,
  type CleanupPreview,
  type CleanupSummary,
  confirmCleanup,
  type JunkCategory,
  type PreviewCategory,
  previewCleanup,
  toAppError,
} from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { CLEANABLE_CATEGORIES, isCleanable } from "./cleanupText";
import type { JunkScanApi } from "./useJunkScan";

/** Avance visible de una limpieza en curso. */
export interface CleanupProgressState {
  processed: number;
  total: number;
}

export interface CleanupApi {
  dialogOpen: boolean;
  /** Preview para el dialogo: las filas de todas las categorias vistas y los totales del ultimo preview. */
  preview: CleanupPreview | null;
  selected: readonly JunkCategory[];
  loadingPreview: boolean;
  /** Se envio la confirmacion y todavia no empezo la ejecucion. */
  confirming: boolean;
  /** Aviso dentro del dialogo ("El resumen se actualizo"). */
  notice: string | null;
  /** No nulo mientras la limpieza corre. */
  progress: CleanupProgressState | null;
  summary: CleanupSummary | null;
  error: AppError | null;
  /** El resumen acaba de aparecer: su titulo debe recibir el foco una vez. */
  focusSummary: boolean;
  open: () => void;
  toggle: (category: JunkCategory) => void;
  confirm: () => void;
  closeDialog: () => void;
  cancel: () => void;
  /** Descarta resumen y error (al iniciar otro escaneo). */
  reset: () => void;
  consumeSummaryFocus: () => void;
}

/** Une las filas ya vistas con las del preview nuevo, conservando el orden y las categorias omitidas. */
function mergeRows(current: readonly PreviewCategory[], incoming: readonly PreviewCategory[]): PreviewCategory[] {
  const merged = current.map((row) => incoming.find((item) => item.category === row.category) ?? row);
  const extra = incoming.filter((item) => !current.some((row) => row.category === item.category));
  return [...merged, ...extra];
}

function inCleanableOrder(categories: readonly JunkCategory[]): JunkCategory[] {
  return CLEANABLE_CATEGORIES.filter((category) => categories.includes(category));
}

/**
 * Estado de la limpieza: preview, seleccion, ejecucion, cancelacion y resumen. Lo posee
 * `CleanupProvider` (vive toda la sesion) para que una limpieza siga visible si el usuario
 * cambia de pantalla y vuelve. La interfaz solo envia identificadores y categorias: nunca rutas.
 */
export function useCleanupState(junk: JunkScanApi): CleanupApi {
  const announce = useAnnounce();
  const { last, refresh } = junk;
  const [dialogOpen, setDialogOpen] = useState(false);
  const [latest, setLatest] = useState<CleanupPreview | null>(null);
  const [rows, setRows] = useState<PreviewCategory[]>([]);
  const [selected, setSelected] = useState<readonly JunkCategory[]>([]);
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [progress, setProgress] = useState<CleanupProgressState | null>(null);
  const [summary, setSummary] = useState<CleanupSummary | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [focusSummary, setFocusSummary] = useState(false);
  const active = useRef(true);
  const previewSeq = useRef(0);
  const runningId = useRef<number | null>(null);
  const cancelRequested = useRef(false);

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);

  const requestPreview = useCallback(
    (scanId: number, categories: readonly JunkCategory[]) => {
      previewSeq.current += 1;
      const seq = previewSeq.current;
      setLoadingPreview(true);
      previewCleanup(scanId, categories)
        .then((received) => {
          if (!active.current || seq !== previewSeq.current) return;
          setLatest(received);
          setRows((current) => mergeRows(current, received.categories));
          setLoadingPreview(false);
          announce(
            format(es.cleanup.previewReady, {
              count: String(received.totalItems),
              size: formatBytes(received.totalBytes),
            }),
          );
        })
        .catch((failure: unknown) => {
          if (!active.current || seq !== previewSeq.current) return;
          setLoadingPreview(false);
          setDialogOpen(false);
          setError(toAppError(failure));
        });
    },
    [announce],
  );

  const open = useCallback(() => {
    if (last === null) return;
    const initial = inCleanableOrder(
      last.categories.filter((c) => isCleanable(c.category) && c.status !== "unavailable").map((c) => c.category),
    );
    setError(null);
    setSummary(null);
    setNotice(null);
    setLatest(null);
    setRows([]);
    setSelected(initial);
    setDialogOpen(true);
    requestPreview(last.scanId, initial);
  }, [last, requestPreview]);

  const toggle = useCallback(
    (category: JunkCategory) => {
      if (last === null) return;
      const next = inCleanableOrder(
        selected.includes(category) ? selected.filter((c) => c !== category) : [...selected, category],
      );
      setSelected(next);
      setNotice(null);
      const movable = next.some((c) =>
        rows.some((row) => row.category === c && row.status === "included" && row.itemCount > 0),
      );
      if (!movable) {
        // Nada que mover: no se pide otro preview; Confirmar queda deshabilitado.
        previewSeq.current += 1;
        setLoadingPreview(false);
        return;
      }
      requestPreview(last.scanId, next);
    },
    [last, selected, rows, requestPreview],
  );

  const closeDialog = useCallback(() => {
    previewSeq.current += 1;
    setLoadingPreview(false);
    setNotice(null);
    setDialogOpen(false);
  }, []);

  const handleEvent = useCallback((event: CleanupEvent) => {
    if (!active.current) return;
    if (event.type === "started") {
      setDialogOpen(false);
      setProgress({ processed: 0, total: event.total });
      if (cancelRequested.current) cancelCleanup(event.previewId).catch(() => undefined);
    } else {
      setProgress({ processed: event.processed, total: event.total });
    }
  }, []);

  const confirm = useCallback(() => {
    if (last === null || latest === null || loadingPreview || confirming) return;
    const id = latest.previewId;
    runningId.current = id;
    cancelRequested.current = false;
    setError(null);
    setConfirming(true);
    announce(es.cleanup.announceCleaningStarted);
    confirmCleanup(id, handleEvent)
      .then((result) => {
        runningId.current = null;
        announce(format(es.cleanup.announceCleaningDone, { count: String(result.movedCount) }));
        if (active.current) {
          setSummary(result);
          setFocusSummary(true);
          setDialogOpen(false);
          setProgress(null);
          setConfirming(false);
        }
        void refresh();
      })
      .catch((failure: unknown) => {
        runningId.current = null;
        if (!active.current) return;
        const failed = toAppError(failure);
        setConfirming(false);
        setProgress(null);
        if (failed.kind === "preview_expired") {
          // Nunca se confirma solo: se pide un preview nuevo y se avisa.
          setNotice(es.cleanup.previewUpdated);
          requestPreview(last.scanId, selected);
          return;
        }
        setDialogOpen(false);
        setError(failed);
        if (failed.kind === "scan_already_cleaned" || failed.kind === "preview_stale") void refresh();
      });
  }, [last, latest, loadingPreview, confirming, selected, announce, handleEvent, refresh, requestPreview]);

  const cancel = useCallback(() => {
    cancelRequested.current = true;
    if (runningId.current !== null) cancelCleanup(runningId.current).catch(() => undefined);
  }, []);

  const reset = useCallback(() => {
    setSummary(null);
    setError(null);
    setFocusSummary(false);
  }, []);

  const consumeSummaryFocus = useCallback(() => setFocusSummary(false), []);

  const preview = useMemo(
    () => (latest === null ? null : { ...latest, categories: rows.length > 0 ? rows : latest.categories }),
    [latest, rows],
  );

  return {
    dialogOpen,
    preview,
    selected,
    loadingPreview,
    confirming,
    notice,
    progress,
    summary,
    error,
    focusSummary,
    open,
    toggle,
    confirm,
    closeDialog,
    cancel,
    reset,
    consumeSummaryFocus,
  };
}
