import type { MetricAvailability, MetricSummary, MonitorSample, MonitorSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";
import { LevelBadge } from "./LevelBadge";
import { metricDisplay } from "./metricDisplay";
import { levelPhrase, type MonitorMetric, spaceLevelText } from "./monitorText";

function Missing({ fallback, note }: { fallback: string; note: string | null }) {
  return (
    <>
      <p className="text-2xl font-semibold text-content-muted">{fallback}</p>
      {note !== null && <p className="text-content-muted">{note}</p>}
    </>
  );
}

function MetricLine({
  metric,
  data,
  availability,
  detail,
  valueFormat,
}: {
  metric: MonitorMetric;
  data: MetricSummary;
  availability: MetricAvailability;
  detail?: string | null;
  valueFormat?: (pct: string) => string;
}) {
  const view = metricDisplay(data, availability);
  if (view.value === null) return <Missing fallback={view.fallback} note={view.note} />;
  const phrase = levelPhrase(metric, data.level);
  const level = view.level === "unknown" ? "normal" : view.level;
  return (
    <>
      <p className="flex flex-wrap items-baseline gap-3">
        <span className="text-3xl font-semibold">{valueFormat ? valueFormat(view.valueText) : view.valueText}</span>
        <LevelBadge level={level} text={view.levelText} />
      </p>
      {detail != null && <p className="text-content-muted">{detail}</p>}
      {phrase !== null && <p>{phrase}</p>}
    </>
  );
}

function SpaceLine({ summary, sample }: { summary: MonitorSummary; sample: MonitorSample | undefined }) {
  const { volume, availability } = summary;
  if (
    availability.volume !== "ok" ||
    volume.level === "unknown" ||
    sample?.volumeFreeBytes == null ||
    sample.volumeTotalBytes == null
  ) {
    return (
      <p className="text-content-muted">
        {format(es.monitor.labelValue, { label: es.monitor.metrics.space, value: es.monitor.unavailable })}
      </p>
    );
  }
  const almostFull = volume.level === "almost_full";
  return (
    <p className="flex flex-wrap items-center gap-3">
      <span>
        {format(es.monitor.tiles.spaceDetail, {
          free: formatBytes(sample.volumeFreeBytes),
          total: formatBytes(sample.volumeTotalBytes),
        })}
      </span>
      <LevelBadge
        level={almostFull ? "high" : "normal"}
        text={spaceLevelText(almostFull ? "almost_full" : "normal")}
      />
    </p>
  );
}

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
