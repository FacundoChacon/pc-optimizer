import { describe, expect, it } from "vitest";
import { linePath, niceMax, type SeriesPoint, timeDomain } from "../../shared/charts/scales";

const BOX = { width: 100, height: 50 };
const point = (t: number, v: number | null, gapBefore = false): SeriesPoint => ({ t, v, gapBefore });

describe("linePath", () => {
  it("un valor nulo en medio produce dos tramos", () => {
    const points = [point(0, 0), point(25, 100), point(50, null), point(75, 50), point(100, 100)];
    const d = linePath(points, { domain: [0, 100], yMax: 100, ...BOX });
    expect(d).toBe("M0 50L25 0M75 25L100 0");
  });

  it("un tramo de un solo punto se dibuja como punto (M seguido de L al mismo lugar)", () => {
    const points = [point(0, 10), point(50, null), point(100, 10)];
    const d = linePath(points, { domain: [0, 100], yMax: 100, ...BOX });
    expect(d.match(/M/g)).toHaveLength(2);
  });

  it("un hueco marcado corta aunque ambos valores existan", () => {
    const points = [point(0, 20), point(50, 20), point(75, 20, true), point(100, 20)];
    const d = linePath(points, { domain: [0, 100], yMax: 100, ...BOX });
    expect(d).toBe("M0 40L50 40M75 40L100 40");
  });

  it("descarta los puntos fuera del dominio de tiempo", () => {
    const points = [point(-10, 100), point(0, 100), point(100, 100), point(110, 100)];
    const d = linePath(points, { domain: [0, 100], yMax: 100, ...BOX });
    expect(d).toBe("M0 0L100 0");
  });

  it("serie vacia o solo nulos da un camino vacio", () => {
    expect(linePath([], { domain: [0, 100], yMax: 100, ...BOX })).toBe("");
    expect(linePath([point(0, null), point(10, null)], { domain: [0, 100], yMax: 100, ...BOX })).toBe("");
  });

  it("acota los valores al eje (un 130 % no sale del grafico)", () => {
    const d = linePath([point(0, 130), point(100, -5)], { domain: [0, 100], yMax: 100, ...BOX });
    expect(d).toBe("M0 0L100 50");
  });
});

describe("niceMax", () => {
  it.each([
    [37_000_000, 50_000_000],
    [1_200, 2_000],
    [100, 100],
    [0, 1],
    [Number.NaN, 1],
  ])("niceMax(%s) = %s", (peak, expected) => {
    expect(niceMax(peak)).toBe(expected);
  });

  it("siempre es redondo y al menos el pico", () => {
    for (const peak of [1, 7, 55, 999, 4_321, 12_345_678]) {
      expect(niceMax(peak)).toBeGreaterThanOrEqual(peak);
    }
  });
});

describe("timeDomain", () => {
  it("es [ultimo - ventana, ultimo]", () => {
    expect(timeDomain(1_000_000, 300_000)).toEqual([700_000, 1_000_000]);
  });
});
