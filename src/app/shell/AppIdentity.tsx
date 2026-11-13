import { es } from "../../shared/i18n/es";
import { useAppInfo } from "../providers/AppInfoProvider";

/** Nombre de la app y version devuelta por el backend (o el error generico del catalogo). */
export function AppIdentity() {
  const info = useAppInfo();
  return (
    <div className="flex flex-col">
      <span className="font-semibold">{es.app.title}</span>
      {info.status === "ready" && (
        <span className="text-sm text-content-muted">
          {es.app.versionLabel} {info.info.version}
        </span>
      )}
      {info.status === "error" && (
        <span role="alert" className="text-sm text-danger">
          {es.app.ipcError}
        </span>
      )}
    </div>
  );
}
