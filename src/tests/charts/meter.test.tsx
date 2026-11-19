import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Meter, type MeterProps } from "../../shared/charts/Meter";

afterEach(cleanup);

const base: MeterProps = {
  label: "Procesador",
  value: 42,
  valueText: "42 %",
  level: "normal",
  levelText: "Normal",
  unavailableText: "No disponible",
};

function fill(container: HTMLElement): SVGRectElement | null {
  return container.querySelector<SVGRectElement>('rect[data-meter="fill"]');
}

describe("Meter", () => {
  it("42 % normal: relleno con width en porcentaje, clase de acento, valor en texto y sin style", () => {
    const { container } = render(<Meter {...base} />);
    const rect = fill(container);
    expect(rect).toHaveAttribute("width", "42%");
    expect(rect?.getAttribute("class")).toContain("fill-accent");
    expect(screen.getByText("42 %")).toBeInTheDocument();
    expect(screen.getByText("Normal")).toBeInTheDocument();
    expect(container.querySelectorAll("[style]")).toHaveLength(0);
  });

  it("un 55 % alto usa la clase de advertencia y un 12 % la de acento (otro valor, otra anchura)", () => {
    const high = render(<Meter {...base} value={55} valueText="55 %" level="high" levelText="Alto" />);
    expect(fill(high.container)).toHaveAttribute("width", "55%");
    expect(fill(high.container)?.getAttribute("class")).toContain("fill-warning");
    high.unmount();
    const low = render(<Meter {...base} value={12} valueText="12 %" />);
    expect(fill(low.container)).toHaveAttribute("width", "12%");
  });

  it("130 % se acota a 100 % y NaN a 0 %", () => {
    const over = render(<Meter {...base} value={130} valueText="130 %" />);
    expect(fill(over.container)).toHaveAttribute("width", "100%");
    over.unmount();
    const nan = render(<Meter {...base} value={Number.NaN} />);
    expect(fill(nan.container)).toHaveAttribute("width", "0%");
  });

  it("muy alto: clase de peligro y el nivel en texto fuera del SVG", () => {
    const { container } = render(<Meter {...base} value={95} valueText="95 %" level="very_high" levelText="Muy alto" />);
    expect(fill(container)?.getAttribute("class")).toContain("fill-danger");
    const level = screen.getByText("Muy alto");
    expect(level.closest("svg")).toBeNull();
  });

  it("desconocido o nulo: sin relleno y 'No disponible'", () => {
    const unknown = render(<Meter {...base} value={null} level="unknown" levelText="" />);
    expect(fill(unknown.container)).toBeNull();
    expect(screen.getByText("No disponible")).toBeInTheDocument();
    unknown.unmount();
    const nullValue = render(<Meter {...base} value={null} level="normal" />);
    expect(fill(nullValue.container)).toBeNull();
  });
});
