import { useEffect, useState } from "react";
import { type AppInfo, getAppInfo } from "../lib/ipc";
import { es } from "../shared/i18n/es";

type State =
  | { status: "loading" }
  | { status: "ready"; info: AppInfo }
  | { status: "error" };

export function App() {
  const [state, setState] = useState<State>({ status: "loading" });

  useEffect(() => {
    let active = true;
    getAppInfo().then(
      (info) => {
        if (active) setState({ status: "ready", info });
      },
      () => {
        if (active) setState({ status: "error" });
      },
    );
    return () => {
      active = false;
    };
  }, []);

  return (
    <main className="min-h-screen bg-surface p-8 text-content">
      <h1 className="text-2xl font-semibold">{es.app.title}</h1>
      {state.status === "loading" && <p className="text-content-muted">{es.app.loading}</p>}
      {state.status === "ready" && (
        <p className="text-content-muted">
          {es.app.versionLabel} {state.info.version}
        </p>
      )}
      {state.status === "error" && <p role="alert">{es.app.ipcError}</p>}
    </main>
  );
}
