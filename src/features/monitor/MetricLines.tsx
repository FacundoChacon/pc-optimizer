import type { MetricAvailability, MetricSummary, MonitorSample, MonitorSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { LevelBadge } from "./LevelBadge";
import { metricDisplay } from "./metricDisplay";
import { levelPhrase, type MonitorMetric, spaceLevelText } from "./monitorText";

/** Piezas de cifra y nivel del modo simple, compartidas por Monitor (MonitorTiles) e Inicio (C-12). */

export function Missing({ fallback, note }: { fallback: string; note: string | null }) {
  return (
    <>
      <p className="text-2xl font-semibold text-content-muted">{fallback}</p>
      {note !== null && <p className="text-content-muted">{note}</p>}
    </>
  );
}

export function MetricLine({
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

export function SpaceLine({ summary, sample }: { summary: MonitorSummary; sample: MonitorSample | undefined }) {
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
