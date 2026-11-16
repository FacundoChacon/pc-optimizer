import type { ComponentProps } from "react";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { type ChartLabels, type ChartSeries, TimeSeriesChart } from "../../shared/charts/TimeSeriesChart";

afterEach(cleanup);

const LABELS: ChartLabels = {
  showTable: "Ver tabla",
  hideTable: "Ocultar tabla",
  timeColumn: "Hora",
  unavailable: "No disponible",
  hint: "Usá las flechas para recorrer el gráfico.",
  axisStart: "hace 5 min",
  axisEnd: "ahora",
};

const WINDOW = 300_000;

function series(count: number, start = 0, id = "cpu", label = "CPU", scale = 1): ChartSeries {
  return {
    id,
    label,
    tone: "primary",
    points: Array.from({ length: count }, (_, i) => ({ t: (start + i) * 1000, v: ((i * 7) % 100) * scale })),
  };
}

function renderChart(items: ChartSeries[], extra: Partial<ComponentProps<typeof TimeSeriesChart>> = {}) {
  const last = Math.max(...items.flatMap((s) => s.points.map((p) => p.t)));
  return render(
    <TimeSeriesChart
      title="Procesador"
      series={items}
      domain={[last - WINDOW, last]}
      yMax={100}
      formatValue={(v) => `${v} %`}
      formatTime={(t) => `t${t / 1000}`}
      labels={LABELS}
      {...extra}
    />,
  );
}

describe("TimeSeriesChart", () => {
  it("10 muestras: un path por serie, nombre accesible y ningun atributo style", () => {
    const { container } = renderChart([series(10)]);
    expect(container.querySelectorAll('path[data-series]')).toHaveLength(1);
    expect(screen.getByRole("group", { name: "Procesador" })).toBeInTheDocument();
    expect(container.querySelectorAll("[style]")).toHaveLength(0);
    expect(container.querySelector("[class*='transition']")).toBeNull();
  });

  it("dos series: un path por serie, leyenda con ambas y colores distintos", () => {
    const { container } = renderChart([series(10, 0, "r", "Lectura"), { ...series(10, 0, "w", "Escritura"), tone: "secondary" }]);
    const paths = container.querySelectorAll("path[data-series]");
    expect(paths).toHaveLength(2);
    expect(paths[0]?.getAttribute("class")).not.toBe(paths[1]?.getAttribute("class"));
    const legend = screen.getByRole("list", { name: "Procesador" });
    expect(within(legend).getAllByRole("listitem")).toHaveLength(2);
  });

  it("flechas mueven el cursor y la lectura muestra hora y valor", () => {
    renderChart([series(10)]);
    const chart = screen.getByRole("group", { name: "Procesador" });
    chart.focus();
    fireEvent.keyDown(chart, { key: "ArrowLeft" });
    expect(screen.getAllByText(/t9\b/).length).toBeGreaterThan(0);
    fireEvent.keyDown(chart, { key: "ArrowLeft" });
    expect(screen.getAllByText(/t8\b/).length).toBeGreaterThan(0);
    fireEvent.keyDown(chart, { key: "ArrowRight" });
    expect(screen.getAllByText(/t9\b/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/\bt8\b/)).toBeNull();
  });

  it("la lectura del cursor se dibuja con transform y sin style", () => {
    const { container } = renderChart([series(10)]);
    const chart = screen.getByRole("group", { name: "Procesador" });
    fireEvent.keyDown(chart, { key: "ArrowLeft" });
    expect(container.querySelector("g[data-cursor][transform]")).not.toBeNull();
    expect(container.querySelectorAll("[style]")).toHaveLength(0);
  });

  it("Ver tabla muestra como maximo las ultimas 60 filas y 'No disponible' en los ausentes", () => {
    const data = series(80);
    data.points[79] = { t: 79_000, v: null };
    renderChart([data]);
    fireEvent.click(screen.getByRole("button", { name: "Ver tabla" }));
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("row")).toHaveLength(61);
    expect(within(table).getByText("No disponible")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Ocultar tabla" }));
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("una muestra nueva desplaza el eje: el punto mas viejo sale del camino", () => {
    const first = renderChart([series(10)]);
    const before = first.container.querySelector("path[data-series]")?.getAttribute("d");
    first.unmount();
    const next = renderChart([series(11)]);
    const after = next.container.querySelector("path[data-series]")?.getAttribute("d");
    expect(after).not.toBe(before);
  });

  it("un hueco corta la linea en dos tramos", () => {
    const data = series(6);
    data.points[3] = { t: 3000, v: 50, gapBefore: true };
    const { container } = renderChart([data]);
    const d = container.querySelector("path[data-series]")?.getAttribute("d") ?? "";
    expect(d.match(/M/g)).toHaveLength(2);
  });

  it("la lectura del cursor es legible: texto grande con halo de la superficie (UX 7.7)", () => {
    const { container } = renderChart([series(10)]);
    fireEvent.keyDown(screen.getByRole("group", { name: "Procesador" }), { key: "ArrowLeft" });
    const texts = container.querySelectorAll("g[data-cursor] text");
    expect(texts.length).toBeGreaterThan(1);
    for (const text of texts) {
      expect(text.getAttribute("paint-order")).toBe("stroke");
      expect(text.getAttribute("class")).toContain("text-[16px]");
    }
  });
});
