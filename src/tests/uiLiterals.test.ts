import { describe, expect, it } from "vitest";
import { findUiLiterals } from "./support/uiLiterals";

const sources = import.meta.glob<string>("/src/{app,features,shared/ui}/**/*.tsx", {
  query: "?raw",
  import: "default",
  eager: true,
});

describe("guardia de literales en componentes reales", () => {
  it("revisa al menos App.tsx", () => {
    expect(Object.keys(sources)).toContain("/src/app/App.tsx");
  });

  it("no encuentra literales de UI ni movimiento sin motion-safe", () => {
    const findings = Object.entries(sources).flatMap(([file, source]) =>
      findUiLiterals(file, source),
    );
    const report = findings.map((f) => `${f.file}:${f.line} ${f.kind} ${f.text}`);
    expect(report).toEqual([]);
  });
});

function kinds(source: string): string[] {
  return findUiLiterals("Sample.tsx", source).map((finding) => finding.kind);
}

describe("findUiLiterals (detector)", () => {
  it("reporta texto JSX directo con archivo y linea", () => {
    const findings = findUiLiterals("A.tsx", "export const A = () => (\n  <p>Hola</p>\n);");
    expect(findings).toHaveLength(1);
    expect(findings[0]).toMatchObject({ file: "A.tsx", line: 2, kind: "text", text: "Hola" });
  });

  it("reporta una cadena como hijo de expresion", () => {
    expect(kinds('const A = () => <p>{"Hola"}</p>;')).toEqual(["child-string"]);
    expect(kinds("const A = () => <p>{`Hola`}</p>;")).toEqual(["child-string"]);
  });

  it("reporta atributos de texto accesible con literal", () => {
    expect(kinds('const A = () => <button aria-label="Cerrar" />;')).toEqual(["attr:aria-label"]);
    expect(kinds('const A = () => <div title="x" />;')).toEqual(["attr:title"]);
    expect(kinds('const A = () => <input placeholder="x" />;')).toEqual(["attr:placeholder"]);
    expect(kinds('const A = () => <img alt="x" />;')).toEqual(["attr:alt"]);
    expect(kinds('const A = () => <b title={"x"} />;')).toEqual(["attr:title"]);
  });

  it("reporta transiciones y animaciones sin motion-safe", () => {
    expect(kinds('const A = () => <p className="p-4 transition" />;')).toEqual(["motion"]);
    expect(kinds('const A = () => <p className="animate-spin" />;')).toEqual(["motion"]);
    expect(kinds('const A = () => <p className="hover:transition-colors" />;')).toEqual(["motion"]);
    expect(kinds('const A = () => <p className={cond ? "transition" : "p-2"} />;')).toEqual([
      "motion",
    ]);
  });

  it("no reporta contenido que viene del catalogo ni clases inocuas", () => {
    const clean = [
      'const A = () => <p className="p-4 flex" />;',
      'const A = () => <p role="switch" />;',
      "const A = () => <p>{es.app.title}</p>;",
      "const A = () => <p aria-label={es.a11y.x} />;",
      'const A = () => <p className="motion-safe:transition-colors" />;',
      'const A = () => <p className="md:motion-safe:animate-pulse" />;',
      "const A = () => <p>{format(es.a, { x: v })}</p>;",
      "const A = () => <>\n  <p>{es.a}</p>\n</>;",
    ];
    for (const source of clean) expect(kinds(source), source).toEqual([]);
  });
});
