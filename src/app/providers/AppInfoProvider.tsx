import { createContext, type ReactNode, useContext, useEffect, useRef, useState } from "react";
import { type AppInfo, getAppInfo } from "../../lib/ipc";

export type AppInfoState =
  | { status: "loading" }
  | { status: "ready"; info: AppInfo }
  | { status: "error" };

const AppInfoContext = createContext<AppInfoState | null>(null);

export function AppInfoProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AppInfoState>({ status: "loading" });
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    getAppInfo().then(
      (info) => setState({ status: "ready", info }),
      () => setState({ status: "error" }),
    );
  }, []);

  return <AppInfoContext.Provider value={state}>{children}</AppInfoContext.Provider>;
}

export function useAppInfo(): AppInfoState {
  const value = useContext(AppInfoContext);
  if (value === null) throw new Error("useAppInfo debe usarse dentro de AppInfoProvider");
  return value;
}
