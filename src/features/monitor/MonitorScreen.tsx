import type { ReactNode } from "react";
import { useSettings } from "../../app/providers/SettingsProvider";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { LiveCharts } from "./LiveCharts";
import { MeterList } from "./MeterList";
import { MonitorDetails } from "./MonitorDetails";
import { MonitorTiles } from "./MonitorTiles";
import { useMonitor } from "./MonitorProvider";
import { useLevelAnnouncements } from "./useLevelAnnouncements";

/** Monitor en vivo (C-06): metricas de la maquina, solo lectura. */
export function MonitorScreen() {
  const { settings } = useSettings();
  const monitor = useMonitor();
  useLevelAnnouncements(monitor.summary);
  const last = monitor.samples.at(-1);
  const advanced = settings.uiMode === "advanced";

  let body: ReactNode;
  if (monitor.status === "unavailable") {
    body = (
      <p role="alert" className="text-danger">
        {errorMessage(monitor.error ?? { kind: "monitor_unavailable", message: "" })}
      </p>
    );
  } else if (monitor.summary === null) {
    body = <p className="text-content-muted">{es.monitor.waiting}</p>;
  } else {
    body = (
      <>
        {monitor.stale && <p className="text-warning">{es.monitor.noRecent}</p>}
        {advanced ? (
          <>
            <MeterList summary={monitor.summary} sample={last} />
            <LiveCharts samples={monitor.samples} windowMs={monitor.windowMs} />
            <MonitorDetails
              summary={monitor.summary}
              intervalMs={monitor.intervalMs}
              sampleCount={monitor.samples.length}
            />
          </>
        ) : (
          <MonitorTiles summary={monitor.summary} sample={last} />
        )}
      </>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.monitor.title}</h1>
      <p className="text-content-muted">{es.monitor.intro}</p>
      {body}
    </div>
  );
}
