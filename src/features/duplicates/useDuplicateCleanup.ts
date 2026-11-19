import { useCallback, useEffect, useRef, useState } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import {
  type AppError,
  cancelCleanup,
  type CleanupEvent,
  confirmDuplicateCleanup,
  type DuplicateCleanupPreview,
  type DuplicateCleanupSummary,
  type DuplicateDecision,
  type DuplicateGroup,
  type DuplicateScanSummary,
  previewDuplicateCleanup,
  toAppError,
} from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { formatPlural } from "../../shared/i18n/format";
import { copiesToMove, isDecided } from "./duplicatesText";

/** Avance visible de la ejecucion. */
export interface MoveProgressState {
  processed: number;
  total: number;
}

export interface DuplicateCleanupApi {
  /** Copias a conservar elegidas por el usuario, por grupo. Nada viene elegido de antemano. */
  choices: Readonly<Record<number, number[]>>;
  /** Cantidad de copias que se moverian con las elecciones actuales. */
  copiesToMove: number;
  /** Simple: conserva exactamente esa copia. */
  chooseOne: (groupId: number, fileId: number) => void;
  /** Avanzado: marca o desmarca una copia; desmarcar la ultima no se permite y devuelve `false`. */
  toggleKeep: (groupId: number, fileId: number) => boolean;
  dialogOpen: boolean;
  preview: DuplicateCleanupPreview | null;
  loadingPreview: boolean;
  confirming: boolean;
  notice: string | null;
  progress: MoveProgressState | null;
  summary: DuplicateCleanupSummary | null;
  error: AppError | null;
  focusSummary: boolean;
  openReview: () => void;
  closeReview: () => void;
  confirm: () => void;
  cancel: () => void;
  dismissSummary: () => void;
  consumeSummaryFocus: () => void;
}

/**
 * Estado de la decision y de la ejecucion. Las elecciones viven solo aca (nunca se persisten:
 * un clic no es una decision confirmada); al backend viajan unicamente al pedir el preview y son
 * ids de grupo y de las copias que se conservan.
 */
export function useDuplicateCleanupState(
  last: DuplicateScanSummary | null,
  groups: readonly DuplicateGroup[],
  onResolved: () => void,
): DuplicateCleanupApi {
  const announce = useAnnounce();
  const [choices, setChoices] = useState<Readonly<Record<number, number[]>>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [preview, setPreview] = useState<DuplicateCleanupPreview | null>(null);
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [progress, setProgress] = useState<MoveProgressState | null>(null);
  const [summary, setSummary] = useState<DuplicateCleanupSummary | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [focusSummary, setFocusSummary] = useState(false);
  const active = useRef(true);
  const runningId = useRef<number | null>(null);
  const cancelRequested = useRef(false);
  const inFlight = useRef(false);
  const previewSeq = useRef(0);
  const scanId = last?.scanId ?? null;

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);

  // Un escaneo nuevo descarta las elecciones del anterior.
  useEffect(() => {
    if (scanId !== null) setChoices({});
  }, [scanId]);

  const chooseOne = useCallback((groupId: number, fileId: number) => {
    setChoices((current) => ({ ...current, [groupId]: [fileId] }));
  }, []);

  const toggleKeep = useCallback(
    (groupId: number, fileId: number): boolean => {
      const current = choices[groupId] ?? [];
      if (current.includes(fileId) && current.length === 1) {
        announce(es.duplicates.advanced.keepLastHint);
        return false;
      }
      const next = current.includes(fileId) ? current.filter((id) => id !== fileId) : [...current, fileId];
      setChoices((all) => ({ ...all, [groupId]: next }));
      return true;
    },
    [choices, announce],
  );

  const decisions = useCallback((): DuplicateDecision[] => {
    return groups
      .filter((group) => group.resolvedAt === null && isDecided(group, choices[group.groupId]))
      .map((group) => ({ groupId: group.groupId, keepFileIds: [...(choices[group.groupId] ?? [])] }));
  }, [groups, choices]);

  const requestPreview = useCallback(
    (id: number) => {
      previewSeq.current += 1;
      const seq = previewSeq.current;
      setLoadingPreview(true);
      previewDuplicateCleanup(id, decisions())
        .then((received) => {
          if (!active.current || seq !== previewSeq.current) return;
          setPreview(received);
          setLoadingPreview(false);
        })
        .catch((failure: unknown) => {
          if (!active.current || seq !== previewSeq.current) return;
          setLoadingPreview(false);
          setDialogOpen(false);
          setError(toAppError(failure));
        });
    },
    [decisions],
  );

  const openReview = useCallback(() => {
    if (scanId === null) return;
    setError(null);
    setSummary(null);
    setNotice(null);
    setPreview(null);
    setDialogOpen(true);
    requestPreview(scanId);
  }, [scanId, requestPreview]);

  const closeReview = useCallback(() => {
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
    // Un doble clic confirma una sola vez.
    if (preview === null || scanId === null || loadingPreview || inFlight.current) return;
    inFlight.current = true;
    const id = preview.previewId;
    runningId.current = id;
    cancelRequested.current = false;
    setError(null);
    setConfirming(true);
    announce(es.duplicates.announceMovingStarted);
    confirmDuplicateCleanup(id, handleEvent)
      .then((result) => {
        runningId.current = null;
        inFlight.current = false;
        announce(formatPlural(es.duplicates.announceMovingDone, result.movedCount));
        if (active.current) {
          setSummary(result);
          setFocusSummary(true);
          setDialogOpen(false);
          setProgress(null);
          setConfirming(false);
          setChoices({});
        }
        onResolved();
      })
      .catch((failure: unknown) => {
        runningId.current = null;
        inFlight.current = false;
        if (!active.current) return;
        const failed = toAppError(failure);
        setConfirming(false);
        setProgress(null);
        if (failed.kind === "preview_expired") {
          // Nunca se confirma solo: se pide un preview nuevo y se avisa.
          setNotice(es.duplicates.previewUpdated);
          requestPreview(scanId);
          return;
        }
        setDialogOpen(false);
        setError(failed);
        if (failed.kind === "preview_stale" || failed.kind === "duplicate_group_resolved") onResolved();
      });
  }, [preview, scanId, loadingPreview, announce, handleEvent, onResolved, requestPreview]);

  const cancel = useCallback(() => {
    cancelRequested.current = true;
    if (runningId.current !== null) cancelCleanup(runningId.current).catch(() => undefined);
  }, []);

  const dismissSummary = useCallback(() => {
    setSummary(null);
    setError(null);
    setFocusSummary(false);
  }, []);

  const consumeSummaryFocus = useCallback(() => setFocusSummary(false), []);

  return {
    choices,
    copiesToMove: copiesToMove(groups, choices),
    chooseOne,
    toggleKeep,
    dialogOpen,
    preview,
    loadingPreview,
    confirming,
    notice,
    progress,
    summary,
    error,
    focusSummary,
    openReview,
    closeReview,
    confirm,
    cancel,
    dismissSummary,
    consumeSummaryFocus,
  };
}
