import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ProportionBar } from "../../features/disk/ProportionBar";

afterEach(() => {
  cleanup();
});

function widthOf(fraction: number): string | null {
  const { container } = render(<ProportionBar fraction={fraction} />);
  const width = container.querySelector('rect[data-bar="value"]')?.getAttribute("width") ?? null;
  cleanup();
  return width;
}

describe("ProportionBar", () => {
  it("dibuja el ancho como atributo en porcentaje", () => {
    expect(widthOf(0.31)).toBe("31%");
    expect(widthOf(0.5)).toBe("50%");
    expect(widthOf(0.0625)).toBe("6.25%");
  });

  it("0 % y 100 % llegan a los bordes", () => {
    expect(widthOf(0)).toBe("0%");
    expect(widthOf(1)).toBe("100%");
  });

  it("acota un valor fuera de rango o invalido", () => {
    expect(widthOf(1.7)).toBe("100%");
    expect(widthOf(-0.4)).toBe("0%");
    expect(widthOf(Number.NaN)).toBe("0%");
    expect(widthOf(Number.POSITIVE_INFINITY)).toBe("100%");
  });

  it("es decorativa: oculta a las tecnologias de asistencia y sin rol propio", () => {
    const { container } = render(<ProportionBar fraction={0.4} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(svg).toHaveAttribute("focusable", "false");
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("ningun elemento de la barra usa el atributo style (compatible con la CSP de produccion)", () => {
    const { container } = render(<ProportionBar fraction={0.4} />);
    expect(container.querySelectorAll("[style]")).toHaveLength(0);
    for (const element of container.querySelectorAll("*")) {
      expect(element.hasAttribute("style")).toBe(false);
    }
  });
});
