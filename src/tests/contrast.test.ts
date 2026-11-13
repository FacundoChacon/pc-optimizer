import { describe, expect, it } from "vitest";
import css from "../styles.css?raw";
import { contrastRatio, parseColorTokens } from "./support/contrast";

describe("contrastRatio", () => {
  it("blanco sobre negro es 21 y un par igual es 1", () => {
    expect(contrastRatio("#ffffff", "#000000")).toBeCloseTo(21, 5);
    expect(contrastRatio("#4f9cf9", "#4f9cf9")).toBeCloseTo(1, 5);
  });

  it("es simetrico y rechaza colores invalidos", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(() => contrastRatio("rojo", "#000000")).toThrow();
  });
});

describe("tokens del tema (styles.css)", () => {
  const tokens = parseColorTokens(css);
  const color = (name: string): string => {
    const value = tokens[name];
    if (value === undefined) throw new Error(`Falta el token --color-${name}`);
    return value;
  };
  const backgrounds = ["surface", "surface-raised"];

  it("el CSS crudo se lee y define los tokens requeridos", () => {
    for (const name of [
      "surface", "surface-raised", "content", "content-muted", "accent",
      "focus", "success", "warning", "danger", "on-accent", "border-strong",
    ]) {
      expect(tokens, name).toHaveProperty(name);
    }
  });

  it.each(["content", "content-muted", "success", "warning", "danger"])(
    "texto %s tiene al menos 4.5:1 sobre ambas superficies",
    (name) => {
      for (const bg of backgrounds) {
        expect(contrastRatio(color(name), color(bg)), `${name} / ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    },
  );

  it.each(["accent", "danger"])("on-accent tiene al menos 4.5:1 sobre %s", (name) => {
    expect(contrastRatio(color("on-accent"), color(name))).toBeGreaterThanOrEqual(4.5);
  });

  it.each(["focus", "border-strong"])("%s tiene al menos 3:1 sobre ambas superficies", (name) => {
    for (const bg of backgrounds) {
      expect(contrastRatio(color(name), color(bg)), `${name} / ${bg}`).toBeGreaterThanOrEqual(3);
    }
  });
});

describe("movimiento reducido", () => {
  it("styles.css anula transiciones y animaciones con prefers-reduced-motion", () => {
    const block = /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?\n\})/.exec(css);
    expect(block).not.toBeNull();
    const body = block?.[1] ?? "";
    expect(body).toMatch(/animation-duration:\s*0\.01ms\s*!important/);
    expect(body).toMatch(/transition-duration:\s*0\.01ms\s*!important/);
    expect(body).toMatch(/animation-iteration-count:\s*1\s*!important/);
  });
});
