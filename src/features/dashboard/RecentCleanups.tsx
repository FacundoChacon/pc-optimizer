import { useNavigation } from "../../app/providers/NavigationProvider";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { runPhrase } from "../history/historyText";
import { useCleanupRuns } from "../history/useCleanupRuns";
import { Loading } from "./HomeCard";

/** Cuantas limpiezas recientes muestra Inicio en modo avanzado. */
const RECENT_COUNT = 3;

/** Modo avanzado: las ultimas limpiezas en una linea cada una, con enlace a Historial. */
export function RecentCleanups() {
  const { navigate } = useNavigation();
  const runs = useCleanupRuns(RECENT_COUNT);
  const text = es.home.advanced;
  let body;
  if (runs.error !== null) {
    body = <p className="text-danger">{errorMessage(runs.error)}</p>;
  } else if (!runs.loaded) {
    body = <Loading />;
  } else if (runs.items.length === 0) {
    body = <p className="text-content-muted">{text.recentEmpty}</p>;
  } else {
    body = (
      <ul className="flex flex-col gap-1">
        {runs.items.map((run) => (
          <li key={`${run.key.kind}-${run.finishedAt}-${JSON.stringify(run.key)}`}>
            <span className="text-content-muted">{formatDateTime(run.finishedAt)}</span> {runPhrase(run)}
          </li>
        ))}
      </ul>
    );
  }
  return (
    <Card heading={text.recentTitle}>
      {body}
      <div>
        <Button onClick={() => navigate("history")}>{text.recentLink}</Button>
      </div>
    </Card>
  );
}
