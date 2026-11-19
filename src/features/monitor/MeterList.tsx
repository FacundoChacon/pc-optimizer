import type { MetricAvailability, MetricSummary, MonitorSample, MonitorSummary } from "../../lib/ipc";
import { Meter } from "../../shared/charts/Meter";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";
import { metricDisplay } from "./metricDisplay";
import { levelText, percentText, spaceLevelText } from "./monitorText";

function PercentMeter({
  label,
  data,
  availability,
}: {
  label: string;
  data: MetricSummary;
  availability: MetricAvailability;
}) {
  const view = metricDisplay(data, availability);
  const detail =
    data.avg60 !== null && data.peak !== null
      ? format(es.monitor.advanced.meterDetail, { avg: percentText(data.avg60), peak: percentText(data.peak) })
      : (view.note ?? undefined);
  return (
    <Meter
      label={label}
      value={view.value}
      valueText={view.valueText}
      level={view.level}
      levelText={view.levelText}
      unavailableText={view.fallback}
      detail={detail}
    />
  );
}

function SpaceMeter({ summary, sample }: { summary: MonitorSummary; sample: MonitorSample | undefined }) {
  const { volume, availability } = summary;
  const known = availability.volume === "ok" && volume.usedPct !== null && volume.level !== "unknown";
  const almostFull = volume.level === "almost_full";
  const detail =
    sample?.volumeFreeBytes != null && sample.volumeTotalBytes != null
      ? format(es.monitor.tiles.spaceDetail, {
          free: formatBytes(sample.volumeFreeBytes),
          total: formatBytes(sample.volumeTotalBytes),
        })
      : undefined;
  return (
    <Meter
      label={es.monitor.metrics.spaceUsed}
      value={known ? volume.usedPct : null}
      valueText={volume.usedPct === null ? "" : percentText(volume.usedPct)}
      level={known ? (almostFull ? "high" : "normal") : "unknown"}
      levelText={known ? spaceLevelText(almostFull ? "almost_full" : "normal") : levelText("normal")}
      unavailableText={es.monitor.unavailable}
      detail={detail}
    />
  );
}

/** Modo avanzado: una fila por medidor con el valor actual, el promedio de 60 s y el pico de 5 min. */
export function MeterList({ summary, sample }: { summary: MonitorSummary; sample: MonitorSample | undefined }) {
  const { availability } = summary;
  return (
    <Card heading={es.monitor.advanced.metersTitle}>
      <div className="flex flex-col gap-3">
        <PercentMeter label={es.monitor.metrics.cpu} data={summary.cpu} availability={availability.cpu} />
        <PercentMeter label={es.monitor.metrics.memory} data={summary.memory} availability={availability.memory} />
        <PercentMeter
          label={es.monitor.metrics.diskActive}
          data={summary.diskActive}
          availability={availability.diskActive}
        />
        <SpaceMeter summary={summary} sample={sample} />
      </div>
    </Card>
  );
}
