import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { type AppError, getSettings, setSetting, type Settings, toAppError, type UiMode } from "../../lib/ipc";

export type SettingsPhase = "loading" | "ready" | "failed";

interface SettingsValue {
  phase: SettingsPhase;
  settings: Settings;
  /** Escritura en curso: el selector de modo se deshabilita. */
  pending: boolean;
  lastError: AppError | null;
  /** Devuelve `true` si el backend guardo el modo. */
  setUiMode: (mode: UiMode) => Promise<boolean>;
  reload: () => Promise<void>;
}

const DEFAULTS: Settings = { uiMode: "simple", autostartTray: false, language: "es" };

const SettingsContext = createContext<SettingsValue | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [phase, setPhase] = useState<SettingsPhase>("loading");
  const [settings, setSettings] = useState<Settings>(DEFAULTS);
  const [pending, setPending] = useState(false);
  const [lastError, setLastError] = useState<AppError | null>(null);

  const reload = useCallback(async () => {
    try {
      setSettings(await getSettings());
      setPhase("ready");
    } catch {
      setSettings(DEFAULTS);
      setPhase("failed");
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const setUiMode = useCallback(async (mode: UiMode): Promise<boolean> => {
    setPending(true);
    setLastError(null);
    try {
      setSettings(await setSetting("ui_mode", mode));
      return true;
    } catch (error: unknown) {
      setLastError(toAppError(error));
      return false;
    } finally {
      setPending(false);
    }
  }, []);

  const value = useMemo(
    () => ({ phase, settings, pending, lastError, setUiMode, reload }),
    [phase, settings, pending, lastError, setUiMode, reload],
  );
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): SettingsValue {
  const value = useContext(SettingsContext);
  if (value === null) throw new Error("useSettings debe usarse dentro de SettingsProvider");
  return value;
}
