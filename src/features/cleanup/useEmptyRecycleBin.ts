import { useCallback, useEffect, useRef, useState } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import {
  acknowledgeEmptyRecycleBin,
  type AppError,
  type EmptyAck,
  type EmptyRequest,
  type EmptyResult,
  emptyRecycleBin,
  prepareEmptyRecycleBin,
  toAppError,
} from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { emptyResultText } from "./recycleBinText";
import type { JunkScanApi } from "./useJunkScan";

/**
 * Maquina del vaciado: `idle -> preparing -> step1 -> acknowledging -> step2 -> emptying ->
 * result | error`. Cada paso de la interfaz es un comando distinto del backend, que rechaza todo
 * lo que llegue fuera de orden: un doble clic o un estado desincronizado no pueden saltar un paso.
 */
export type EmptyPhase =
  | "idle"
  | "preparing"
  | "step1"
  | "acknowledging"
  | "step2"
  | "emptying"
  | "result"
  | "error";

/** Ultima medicion conocida de la Papelera, posterior al escaneo (tras vaciar o ver que ya estaba vacia). */
export interface BinMeasure {
  itemCount: number;
  sizeBytes: number;
}

export interface EmptyRecycleBinApi {
  phase: EmptyPhase;
  request: EmptyRequest | null;
  ack: EmptyAck | null;
  result: EmptyResult | null;
  error: AppError | null;
  /** Medicion posterior al vaciado; `null` hasta que haya una (se descarta con cada escaneo nuevo). */
  measure: BinMeasure | null;
  /** El resultado o el error acaba de aparecer: su titulo debe recibir el foco una vez. */
  focusResult: boolean;
  /** Paso 1: mide la Papelera y abre el primer dialogo (tambien reintenta tras un error). */
  start: () => void;
  /** Primera confirmacion (solo desde `step1`; una sola vez aunque se pida dos). */
  acknowledge: () => void;
  /** Segunda confirmacion (solo desde `step2`, con el token recibido). */
  confirm: () => void;
  /** Cierra los dialogos sin pedir nada mas (la solicitud vence sola). */
  cancel: () => void;
  /** Descarta el resultado o el error. */
  dismiss: () => void;
  consumeResultFocus: () => void;
}

const STARTABLE: readonly EmptyPhase[] = ["idle", "result", "error"];
const CANCELLABLE: readonly EmptyPhase[] = ["step1", "acknowledging", "step2"];

/** Estado del vaciado de la Papelera; lo posee `CleanupProvider` junto a la limpieza. */
export function useEmptyRecycleBinState(junk: JunkScanApi): EmptyRecycleBinApi {
  const announce = useAnnounce();
  const { last } = junk;
  const [phase, setPhaseState] = useState<EmptyPhase>("idle");
  const [request, setRequest] = useState<EmptyRequest | null>(null);
  const [ack, setAck] = useState<EmptyAck | null>(null);
  const [result, setResult] = useState<EmptyResult | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [measure, setMeasure] = useState<BinMeasure | null>(null);
  const [focusResult, setFocusResult] = useState(false);
  const active = useRef(true);
  // El valor sincrono de la fase: dos clics seguidos ven el cambio antes del siguiente render.
  const phaseRef = useRef<EmptyPhase>("idle");
  // Cada paso o cancelacion invalida las respuestas pendientes de los pasos anteriores.
  const seq = useRef(0);

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);

  // Un escaneo nuevo trae su propia medicion de la Papelera: la posterior al vaciado ya no vale.
  useEffect(() => {
    setMeasure(null);
  }, [last]);

  const setPhase = useCallback((next: EmptyPhase) => {
    phaseRef.current = next;
    setPhaseState(next);
  }, []);

  const fail = useCallback(
    (failure: unknown) => {
      const failed = toAppError(failure);
      if (failed.kind === "recycle_bin_already_empty") setMeasure({ itemCount: 0, sizeBytes: 0 });
      setError(failed);
      setFocusResult(true);
      setPhase("error");
    },
    [setPhase],
  );

  const start = useCallback(() => {
    if (!STARTABLE.includes(phaseRef.current)) return;
    seq.current += 1;
    const mine = seq.current;
    setError(null);
    setResult(null);
    setRequest(null);
    setAck(null);
    setFocusResult(false);
    setPhase("preparing");
    announce(es.recycleBin.preparing);
    prepareEmptyRecycleBin()
      .then((received) => {
        if (!active.current || mine !== seq.current) return;
        setRequest(received);
        setPhase("step1");
      })
      .catch((failure: unknown) => {
        if (!active.current || mine !== seq.current) return;
        fail(failure);
      });
  }, [announce, fail, setPhase]);

  const acknowledge = useCallback(() => {
    if (phaseRef.current !== "step1" || request === null) return;
    seq.current += 1;
    const mine = seq.current;
    setPhase("acknowledging");
    acknowledgeEmptyRecycleBin(request.requestId)
      .then((received) => {
        if (!active.current || mine !== seq.current) return;
        setAck(received);
        setPhase("step2");
      })
      .catch((failure: unknown) => {
        if (!active.current || mine !== seq.current) return;
        fail(failure);
      });
  }, [request, fail, setPhase]);

  const confirm = useCallback(() => {
    if (phaseRef.current !== "step2" || ack === null) return;
    seq.current += 1;
    const mine = seq.current;
    setPhase("emptying");
    announce(es.recycleBin.announceEmptying);
    emptyRecycleBin(ack.requestId, ack.confirmToken)
      .then((received) => {
        if (!active.current || mine !== seq.current) return;
        setResult(received);
        if (received.itemsAfter !== null && received.bytesAfter !== null) {
          setMeasure({ itemCount: received.itemsAfter, sizeBytes: received.bytesAfter });
        }
        setFocusResult(true);
        setPhase("result");
        announce(emptyResultText(received));
      })
      .catch((failure: unknown) => {
        if (!active.current || mine !== seq.current) return;
        fail(failure);
      });
  }, [ack, announce, fail, setPhase]);

  const cancel = useCallback(() => {
    if (!CANCELLABLE.includes(phaseRef.current)) return;
    seq.current += 1;
    setRequest(null);
    setAck(null);
    setPhase("idle");
  }, [setPhase]);

  const dismiss = useCallback(() => {
    if (phaseRef.current !== "result" && phaseRef.current !== "error") return;
    setResult(null);
    setError(null);
    setFocusResult(false);
    setPhase("idle");
  }, [setPhase]);

  const consumeResultFocus = useCallback(() => setFocusResult(false), []);

  return {
    phase,
    request,
    ack,
    result,
    error,
    measure,
    focusResult,
    start,
    acknowledge,
    confirm,
    cancel,
    dismiss,
    consumeResultFocus,
  };
}
