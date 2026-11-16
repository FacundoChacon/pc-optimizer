import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import {
  type AppError,
  type MonitorEvent,
  type MonitorSample,
  type MonitorSummary,
  startMonitorStream,
  stopMonitorStream,
  toAppError,
} from "../../lib/ipc";

export type MonitorStatus = "idle" | "loading" | "live" | "unavailable";

export interface MonitorState {
  status: MonitorStatus;
  /** Ventana de muestras por `seq` creciente (acotada a `windowMs / intervalMs`). */
  samples: readonly MonitorSample[];
  summary: MonitorSummary | null;
  intervalMs: number;
  windowMs: number;
  /** No llegaron muestras por 3 intervalos. */
  stale: boolean;
  error: AppError | null;
}

const DEFAULT_INTERVAL_MS = 1000;
const DEFAULT_WINDOW_MS = 300_000;
const STALE_INTERVALS = 3;
const STALE_CHECK_MS = 1000;

const INITIAL: MonitorState = {
  status: "idle",
  samples: [],
  summary: null,
  intervalMs: DEFAULT_INTERVAL_MS,
  windowMs: DEFAULT_WINDOW_MS,
  stale: false,
  error: null,
};

const UNAVAILABLE: AppError = { kind: "monitor_unavailable", message: "monitor stopped" };

interface MonitorContextValue {
  state: MonitorState;
  /** Registra un consumidor; devuelve la funcion que lo libera. La suscripcion es una por proveedor. */
  acquire: () => () => void;
}

const MonitorContext = createContext<MonitorContextValue | null>(null);

/** Agrega la muestra al final si es mas nueva; acota la ventana. Devuelve la misma lista si no cambia. */
function appendSample(
  samples: readonly MonitorSample[],
  sample: MonitorSample,
  capacity: number,
): readonly MonitorSample[] {
  const last = samples.at(-1);
  if (last !== undefined && sample.seq <= last.seq) return samples;
  const next = [...samples, sample];
  return next.length > capacity ? next.slice(next.length - capacity) : next;
}

/**
 * Suscripcion unica al monitor (D-04): se abre al montar el primer consumidor de `useMonitor` y se
 * cierra al desmontar el ultimo. Con la ventana oculta acumula sin renderizar.
 */
export function MonitorProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<MonitorState>(INITIAL);
  const data = useRef<MonitorState>(INITIAL);
  const lastAt = useRef(0);
  const consumers = useRef(0);
  const generation = useRef(0);
  const subscriptionId = useRef<number | null>(null);
  const pending = useRef<MonitorEvent[]>([]);
  const staleTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  const mounted = useRef(true);

  const publish = useCallback((next: MonitorState): void => {
    data.current = next;
    if (mounted.current && !document.hidden) setState(next);
  }, []);

  const capacity = (): number => Math.max(1, Math.floor(data.current.windowMs / data.current.intervalMs));

  const applyEvent = useCallback(
    (event: MonitorEvent): void => {
      if (event.kind === "stopped") {
        publish({ ...data.current, status: "unavailable", stale: false, error: UNAVAILABLE });
        return;
      }
      lastAt.current = Date.now();
      const samples = appendSample(data.current.samples, event.sample, capacity());
      publish({ ...data.current, samples, summary: event.summary, stale: false });
    },
    [publish],
  );

  const stop = useCallback((): void => {
    generation.current += 1;
    if (staleTimer.current !== null) clearInterval(staleTimer.current);
    staleTimer.current = null;
    pending.current = [];
    const id = subscriptionId.current;
    subscriptionId.current = null;
    if (id !== null) stopMonitorStream(id).catch(() => undefined);
    publish(INITIAL);
  }, [publish]);

  const start = useCallback((): void => {
    generation.current += 1;
    const mine = generation.current;
    pending.current = [];
    publish({ ...INITIAL, status: "loading" });
    startMonitorStream((event) => {
      if (mine !== generation.current) return;
      if (data.current.status === "loading") pending.current.push(event);
      else applyEvent(event);
    })
      .then(({ snapshot }) => {
        if (mine !== generation.current) {
          stopMonitorStream(snapshot.subscriptionId).catch(() => undefined);
          return;
        }
        subscriptionId.current = snapshot.subscriptionId;
        lastAt.current = Date.now();
        publish({
          status: "live",
          samples: snapshot.samples,
          summary: snapshot.summary,
          intervalMs: snapshot.intervalMs,
          windowMs: snapshot.windowMs,
          stale: false,
          error: null,
        });
        const buffered = pending.current;
        pending.current = [];
        for (const event of buffered) applyEvent(event);
        staleTimer.current = setInterval(() => {
          const current = data.current;
          if (current.status !== "live") return;
          const stale = Date.now() - lastAt.current > STALE_INTERVALS * current.intervalMs;
          if (stale !== current.stale) publish({ ...current, stale });
        }, STALE_CHECK_MS);
      })
      .catch((error: unknown) => {
        if (mine !== generation.current) return;
        publish({ ...INITIAL, status: "unavailable", error: toAppError(error) });
      });
  }, [applyEvent, publish]);

  const acquire = useCallback((): (() => void) => {
    consumers.current += 1;
    if (consumers.current === 1) start();
    let released = false;
    return () => {
      if (released) return;
      released = true;
      consumers.current -= 1;
      if (consumers.current === 0) stop();
    };
  }, [start, stop]);

  useEffect(() => {
    mounted.current = true;
    const onVisibility = (): void => {
      if (!document.hidden) setState(data.current);
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      mounted.current = false;
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, []);

  const value = useMemo(() => ({ state, acquire }), [state, acquire]);
  return <MonitorContext.Provider value={value}>{children}</MonitorContext.Provider>;
}

/** Estado en vivo del monitor; usarlo abre la suscripcion compartida mientras el componente exista. */
export function useMonitor(): MonitorState {
  const context = useContext(MonitorContext);
  if (context === null) throw new Error("useMonitor debe usarse dentro de MonitorProvider");
  const { acquire } = context;
  useEffect(() => acquire(), [acquire]);
  return context.state;
}
