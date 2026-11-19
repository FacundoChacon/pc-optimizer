import type { ReactNode } from "react";
import { useNavigation } from "../../app/providers/NavigationProvider";
import type { MonitorSample } from "../../lib/ipc";
import { type ChartLabels, TimeSeriesChart } from "../../shared/charts/TimeSeriesChart";
import { PERCENT_MAX, type SeriesPoint, timeDomain } from "../../shared/charts/scales";
import { es } from "../../shared/i18n/es";
import { formatClock } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { MeterList } from "../monitor/MeterList";
import type { MonitorState } from "../monitor/MonitorProvider";
import { percentText } from "../monitor/monitorText";
import { Loading, Unavailable } from "./HomeCard";

const LABELS: ChartLabels = { ...es.monitor.advanced.chart, unavailable: es.monitor.unavailable };

function memoryPct(sample: MonitorSample): number | null {
  if (sample.memUsedBytes === null || sample.memTotalBytes === null || sample.memTotalBytes === 0) return null;
  return (sample.memUsedBytes / sample.memTotalBytes) * 100;
}

function points(samples: readonly MonitorSample[], pick: (sample: MonitorSample) => number | null): SeriesPoint[] {
  return samples.map((sample) => ({ t: sample.at, v: pick(sample), gapBefore: sample.gapBefore }));
}

/** Modo avanzado: medidores de C-06 y un grafico en vivo de 5 min con CPU y memoria (un solo eje, 0-100 %). */
export function AdvancedSystem({ monitor }: { monitor: MonitorState }) {
  const { navigate } = useNavigation();
  const last = monitor.samples.at(-1);
  let body: ReactNode;
  if (monitor.status === "unavailable") {
    body = <Unavailable code={monitor.error?.kind ?? "monitor_unavailable"} advanced />;
  } else if (monitor.summary === null || last === undefined) {
    body = <Loading />;
  } else {
    body = (
      <>
        {monitor.stale && <p className="text-warning">{es.home.noRecent}</p>}
        <TimeSeriesChart
          title={es.home.advanced.chartLabel}
          series={[
            { id: "cpu", label: es.home.advanced.series.cpu, tone: "primary", points: points(monitor.samples, (s) => s.cpuPct) },
            { id: "memory", label: es.home.advanced.series.memory, tone: "secondary", points: points(monitor.samples, memoryPct) },
          ]}
          domain={timeDomain(last.at, monitor.windowMs)}
          yMax={PERCENT_MAX}
          formatValue={percentText}
          formatTime={formatClock}
          labels={LABELS}
        />
        <div>
          <Button onClick={() => navigate("monitor")}>{es.home.advanced.allCharts}</Button>
        </div>
      </>
    );
  }
  return (
    <>
      {monitor.summary !== null && monitor.status === "live" && <MeterList summary={monitor.summary} sample={last} />}
      <Card heading={es.home.advanced.systemTitle}>{body}</Card>
    </>
  );
}
