import { useCallback, useEffect, useRef, useState } from "react";
import { type AppError, type RevealTarget, revealInExplorer, toAppError } from "../../lib/ipc";

export interface RevealApi {
  error: AppError | null;
  /** Muestra un elemento del ultimo analisis en el Explorador (por id, nunca por ruta). */
  reveal: (scanId: number, target: RevealTarget) => void;
}

/** "Mostrar en el Explorador": solo selecciona el elemento; un fallo queda para informar. */
export function useRevealState(): RevealApi {
  const [error, setError] = useState<AppError | null>(null);
  const active = useRef(true);

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);

  const reveal = useCallback((scanId: number, target: RevealTarget) => {
    setError(null);
    revealInExplorer(scanId, target).catch((failure: unknown) => {
      if (active.current) setError(toAppError(failure));
    });
  }, []);

  return { error, reveal };
}
