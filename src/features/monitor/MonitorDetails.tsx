import type { MonitorSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";

const MS = new Intl.NumberFormat("es", { maximumFractionDigits: 1 });

/** Modo avanzado: como esta midiendo el monitor y por que falta cada medicion (codigo crudo). */
export function MonitorDetails({
  summary,
  intervalMs,
  sampleCount,
}: {
  summary: MonitorSummary;
  intervalMs: number;
  sampleCount: number;
}) {
  const rows: Array<[string, string]> = [
    [es.monitor.details.interval, format(es.monitor.details.intervalValue, { seconds: MS.format(intervalMs / 1000) })],
    [es.monitor.details.samples, String(sampleCount)],
    [es.monitor.details.gaps, String(summary.gapsInWindow)],
    [
      es.monitor.details.readCost,
      summary.avgReadMicros === null
        ? es.monitor.details.none
        : format(es.monitor.details.readCostValue, { ms: MS.format(summary.avgReadMicros / 1000) }),
    ],
  ];
  const availability: Array<[string, string]> = [
    [es.monitor.metrics.cpu, summary.availability.cpu],
    [es.monitor.metrics.memory, summary.availability.memory],
    [es.monitor.metrics.space, summary.availability.volume],
    [es.monitor.metrics.diskActive, summary.availability.diskActive],
    [es.monitor.metrics.diskIo, summary.availability.diskIo],
  ];
  return (
    <Card heading={es.monitor.details.title}>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        {rows.map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-content-muted">{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      <h3 className="font-semibold">{es.monitor.details.availability}</h3>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        {availability.map(([label, code]) => (
          <div key={label} className="contents">
            <dt className="text-content-muted">{label}</dt>
            <dd>
              <code>{code}</code>
            </dd>
          </div>
        ))}
      </dl>
    </Card>
  );
}
