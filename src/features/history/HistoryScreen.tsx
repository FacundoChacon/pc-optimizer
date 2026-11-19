import { useSettings } from "../../app/providers/SettingsProvider";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { Button } from "../../shared/ui/Button";
import { RunList } from "./RunList";
import { useCleanupRuns } from "./useCleanupRuns";

/** Cuantas limpiezas trae cada pagina del historial. */
const PAGE_SIZE = 20;

/** Historial de limpiezas (C-12): consulta de solo lectura del registro; sin accion primaria. */
export function HistoryScreen() {
  const { settings } = useSettings();
  const runs = useCleanupRuns(PAGE_SIZE);
  const advanced = settings.uiMode === "advanced";
  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.history.title}</h1>
      <p className="text-content-muted">{es.history.intro}</p>
      {runs.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(runs.error)}
        </p>
      )}
      {!runs.loaded && runs.error === null && <p className="text-content-muted">{es.history.loading}</p>}
      {runs.loaded && runs.items.length === 0 && runs.error === null && (
        <p className="text-content-muted">{es.history.empty}</p>
      )}
      {runs.items.length > 0 && <RunList runs={runs.items} advanced={advanced} />}
      {runs.hasMore && (
        <div>
          <Button disabled={runs.loading} onClick={runs.loadMore}>
            {runs.loading ? es.history.loadingMore : es.history.loadMore}
          </Button>
        </div>
      )}
    </div>
  );
}
