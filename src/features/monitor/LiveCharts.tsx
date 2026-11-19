import type { ReactNode } from "react";
import type { MonitorSample } from "../../lib/ipc";
import { type ChartLabels, type ChartSeries, TimeSeriesChart } from "../../shared/charts/TimeSeriesChart";
import { niceMax, PERCENT_MAX, type SeriesPoint, timeDomain } from "../../shared/charts/scales";
import { es } from "../../shared/i18n/es";
import { formatClock, formatRate } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";
import { percentText } from "./monitorText";

const LABELS: ChartLabels = {
  ...es.monitor.advanced.chart,
  unavailable: es.monitor.unavailable,
};

function memoryPct(sample: MonitorSample): number | null {
  if (sample.memUsedBytes === null || sample.memTotalBytes === null || sample.memTotalBytes === 0) return null;
  return (sample.memUsedBytes / sample.memTotalBytes) * 100;
}

function points(samples: readonly MonitorSample[], pick: (s: MonitorSample) => number | null): SeriesPoint[] {
  return samples.map((s) => ({ t: s.at, v: pick(s), gapBefore: s.gapBefore }));
}

function ChartBlock({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h3 className="font-semibold">{title}</h3>
      {children}
    </div>
  );
}

/** Modo avanzado: un grafico por metrica (un solo eje cada uno), con eje X fijo de la ventana. */
export function LiveCharts({ samples, windowMs }: { samples: readonly MonitorSample[]; windowMs: number }) {
  const last = samples.at(-1);
  if (last === undefined) return null;
  const domain = timeDomain(last.at, windowMs);
  const percentChart = (title: string, series: ChartSeries[]) => (
    <ChartBlock title={title}>
      <TimeSeriesChart
        title={title}
        series={series}
        domain={domain}
        yMax={PERCENT_MAX}
        formatValue={percentText}
        formatTime={formatClock}
        labels={LABELS}
      />
    </ChartBlock>
  );
  const read = points(samples, (s) => s.diskReadBps);
  const write = points(samples, (s) => s.diskWriteBps);
  const peak = Math.max(0, ...[...read, ...write].map((p) => p.v ?? 0));
  const ioTitle = es.monitor.metrics.diskIo;
  return (
    <Card heading={es.monitor.advanced.chartsTitle}>
      <div className="grid grid-cols-1 gap-6 xl:grid-cols-2">
        {percentChart(es.monitor.metrics.cpu, [
          { id: "cpu", label: es.monitor.metrics.cpu, tone: "primary", points: points(samples, (s) => s.cpuPct) },
        ])}
        {percentChart(es.monitor.metrics.memory, [
          { id: "memory", label: es.monitor.metrics.memory, tone: "primary", points: points(samples, memoryPct) },
        ])}
        {percentChart(es.monitor.metrics.diskActive, [
          {
            id: "disk-active",
            label: es.monitor.metrics.diskActive,
            tone: "primary",
            points: points(samples, (s) => s.diskActivePct),
          },
        ])}
        <ChartBlock title={ioTitle}>
          <TimeSeriesChart
            title={ioTitle}
            series={[
              { id: "read", label: es.monitor.advanced.series.read, tone: "primary", points: read },
              { id: "write", label: es.monitor.advanced.series.write, tone: "secondary", points: write },
            ]}
            domain={domain}
            yMax={niceMax(peak)}
            formatValue={formatRate}
            formatTime={formatClock}
            labels={LABELS}
          />
        </ChartBlock>
      </div>
    </Card>
  );
}
