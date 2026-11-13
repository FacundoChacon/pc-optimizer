import { describe, expect, it } from "vitest";
import { format } from "../../shared/i18n/format";

describe("format", () => {
  it("reemplaza un marcador", () => {
    expect(format("Copia: {file}", { file: "a.bak" })).toBe("Copia: a.bak");
  });

  it("reemplaza dos marcadores distintos", () => {
    expect(format("{a} y {b}", { a: "uno", b: "dos" })).toBe("uno y dos");
  });

  it("reemplaza un marcador repetido en todas sus apariciones", () => {
    expect(format("{x}-{x}", { x: "7" })).toBe("7-7");
  });

  it("deja intacto el marcador sin parametro", () => {
    expect(format("Hola {name}", {})).toBe("Hola {name}");
    expect(format("{a} {b}", { a: "1" })).toBe("1 {b}");
  });

  it("devuelve igual una plantilla sin marcadores", () => {
    expect(format("sin marcadores", { a: "1" })).toBe("sin marcadores");
  });

  it("no interpreta claves heredadas del prototipo como parametros", () => {
    expect(format("{toString}", {})).toBe("{toString}");
  });
});
