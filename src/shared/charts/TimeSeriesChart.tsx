import { type KeyboardEvent, useId, useState } from "react";
import { linePath, type SeriesPoint, xOf, yOf } from "./scales";

export interface ChartSeries {
  id: string;
  label: string;
  /** `primary` = acento; `secondary` = `--color-series-2`. */
  tone: "primary" | "secondary";
  points: SeriesPoint[];
}

export interface ChartLabels {
  showTable: string;
  hideTable: string;
  timeColumn: string;
  unavailable: string;
  hint: string;
  axisStart: string;
  axisEnd: string;
}

export interface TimeSeriesChartProps {
  title: string;
  series: readonly ChartSeries[];
  domain: readonly [number, number];
  yMax: number;
  formatValue: (value: number) => string;
  formatTime: (t: number) => string;
  labels: ChartLabels;
}

const WIDTH = 600;
const HEIGHT = 140;
const TABLE_ROWS = 60;

const STROKE = { primary: "stroke-accent", secondary: "stroke-series-2" } as const;
const SWATCH = { primary: "bg-accent", secondary: "bg-series-2" } as const;

function timeline(series: readonly ChartSeries[], domain: readonly [number, number]): number[] {
  const all = new Set<number>();
  for (const s of series) for (const p of s.points) if (p.t >= domain[0] && p.t <= domain[1]) all.add(p.t);
  return [...all].sort((a, b) => a - b);
}

function valueAt(s: ChartSeries, t: number): number | null {
  return s.points.find((p) => p.t === t)?.v ?? null;
}

/** Indice destino del cursor segun la tecla; `undefined` si la tecla no mueve nada. */
function nextIndex(key: string, current: number, count: number): number | undefined {
  const last = count - 1;
  switch (key) {
    case "ArrowLeft":
      return current < 0 ? last : Math.max(0, current - 1);
    case "ArrowRight":
      return current < 0 ? last : Math.min(last, current + 1);
    case "Home":
      return 0;
    case "End":
      return last;
    default:
      return undefined;
  }
}

/**
 * Serie temporal SVG (skill dataviz): un `path` por serie, un solo eje Y de 0 a `yMax`, eje X fijo.
 * Sin `style` ni transiciones. El grafico es enfocable: las flechas mueven un cursor cuya lectura
 * se dibuja con `transform`; "Ver tabla" ofrece las ultimas 60 filas como alternativa textual.
 */
export function TimeSeriesChart({ title, series, domain, yMax, formatValue, formatTime, labels }: TimeSeriesChartProps) {
  const hintId = useId();
  const [cursor, setCursor] = useState<number | null>(null);
  const [table, setTable] = useState(false);
  const times = timeline(series, domain);
  const cursorT = cursor !== null && times.includes(cursor) ? cursor : null;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    if (event.key === "Escape") {
      setCursor(null);
      return;
    }
    if (times.length === 0) return;
    const next = nextIndex(event.key, cursorT === null ? -1 : times.indexOf(cursorT), times.length);
    if (next === undefined) return;
    event.preventDefault();
    setCursor(times[next] ?? null);
  };

  const cursorX = cursorT === null ? 0 : xOf(cursorT, domain, WIDTH);
  const leftSide = cursorX > WIDTH / 2;
  const anchor = leftSide ? "end" : "start";
  const rows = times.slice(-TABLE_ROWS).reverse();
  const options = { domain, yMax, width: WIDTH, height: HEIGHT };

  return (
    <div className="flex flex-col gap-2">
      <div
        role="group"
        tabIndex={0}
        aria-label={title}
        aria-describedby={hintId}
        onKeyDown={onKeyDown}
        className="rounded-md focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
      >
        <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} focusable="false" aria-hidden="true" className="h-auto w-full">
          <rect x="0" y="0" width={WIDTH} height={HEIGHT} className="fill-surface" />
          {[0, 0.5, 1].map((fraction) => (
            <line
              key={fraction}
              x1="0"
              x2={WIDTH}
              y1={fraction * HEIGHT}
              y2={fraction * HEIGHT}
              strokeWidth="1"
              className="stroke-border-strong opacity-40"
            />
          ))}
          {series.map((s) => (
            <path
              key={s.id}
              data-series={s.id}
              d={linePath(s.points, options)}
              fill="none"
              strokeWidth="2"
              strokeLinejoin="round"
              strokeLinecap="round"
              className={STROKE[s.tone]}
            />
          ))}
          {cursorT !== null && (
            <g data-cursor="" transform={`translate(${cursorX} 0)`}>
              <line x1="0" x2="0" y1="0" y2={HEIGHT} strokeWidth="1" className="stroke-content" />
              {series.map((s) => {
                const v = valueAt(s, cursorT);
                return v === null ? null : (
                  <circle
                    key={s.id}
                    cx="0"
                    cy={yOf(v, yMax, HEIGHT)}
                    r="4"
                    strokeWidth="2"
                    className={`fill-surface ${STROKE[s.tone]}`}
                  />
                );
              })}
              <g transform={`translate(${leftSide ? -8 : 8} 0)`}>
                <text y="20" textAnchor={anchor} paintOrder="stroke" strokeWidth="4" className="fill-content stroke-surface text-[16px]">
                  {formatTime(cursorT)}
                </text>
                {series.map((s, i) => {
                  const v = valueAt(s, cursorT);
                  return (
                    <text key={s.id} y={42 + i * 22} textAnchor={anchor} paintOrder="stroke" strokeWidth="4" className="fill-content stroke-surface text-[16px]">
                      {`${s.label}: ${v === null ? labels.unavailable : formatValue(v)}`}
                    </text>
                  );
                })}
              </g>
            </g>
          )}
        </svg>
        <div className="flex justify-between text-sm text-content-muted">
          <span>{labels.axisStart}</span>
          <span>{`0 – ${formatValue(yMax)}`}</span>
          <span>{labels.axisEnd}</span>
        </div>
      </div>
      <p id={hintId} className="text-sm text-content-muted">
        {labels.hint}
      </p>
      {series.length > 1 && (
        <ul aria-label={title} className="flex flex-wrap gap-4 text-sm text-content">
          {series.map((s) => (
            <li key={s.id} className="flex items-center gap-2">
              <span aria-hidden="true" className={`inline-block h-1 w-4 rounded ${SWATCH[s.tone]}`} />
              {s.label}
            </li>
          ))}
        </ul>
      )}
      <button
        type="button"
        aria-expanded={table}
        onClick={() => setTable((value) => !value)}
        className="self-start rounded-md px-2 py-1 text-sm text-accent underline focus-visible:outline-2 focus-visible:outline-focus"
      >
        {table ? labels.hideTable : labels.showTable}
      </button>
      {table && (
        <table className="w-full text-left text-sm">
          <caption className="sr-only">{title}</caption>
          <thead>
            <tr>
              <th scope="col" className="py-1 pr-4 font-medium text-content-muted">
                {labels.timeColumn}
              </th>
              {series.map((s) => (
                <th key={s.id} scope="col" className="py-1 pr-4 font-medium text-content-muted">
                  {s.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((t) => (
              <tr key={t}>
                <th scope="row" className="py-1 pr-4 font-normal text-content">
                  {formatTime(t)}
                </th>
                {series.map((s) => {
                  const v = valueAt(s, t);
                  return (
                    <td key={s.id} className="py-1 pr-4 text-content">
                      {v === null ? labels.unavailable : formatValue(v)}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
