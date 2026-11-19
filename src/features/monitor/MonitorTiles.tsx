import type { MonitorSample, MonitorSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";
import { MetricLine, SpaceLine } from "./MetricLines";

/** Modo simple: tres tarjetas de cifra con el nivel en palabras. Sin graficos ni acciones. */
export function MonitorTiles({ summary, sample }: { summary: MonitorSummary; sample: MonitorSample | undefined }) {
  const { availability } = summary;
  const memoryDetail =
    sample?.memUsedBytes != null && sample.memTotalBytes != null
      ? format(es.monitor.tiles.memoryDetail, {
          used: formatBytes(sample.memUsedBytes),
          total: formatBytes(sample.memTotalBytes),
        })
      : null;
  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
      <Card heading={es.monitor.metrics.cpu}>
        <MetricLine metric="cpu" data={summary.cpu} availability={availability.cpu} />
      </Card>
      <Card heading={es.monitor.metrics.memory}>
        <MetricLine metric="memory" data={summary.memory} availability={availability.memory} detail={memoryDetail} />
      </Card>
      <Card heading={es.monitor.metrics.disk}>
        <MetricLine
          metric="diskActive"
          data={summary.diskActive}
          availability={availability.diskActive}
          valueFormat={(pct) => format(es.monitor.tiles.diskActive, { pct })}
        />
        <SpaceLine summary={summary} sample={sample} />
      </Card>
    </div>
  );
}
