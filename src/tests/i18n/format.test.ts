import { describe, expect, it } from "vitest";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";

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

describe("formatBytes", () => {
  it.each([
    [0, "0 B"],
    [512, "512 B"],
    [1024, "1 KB"],
    [1536, "1,5 KB"],
    [1_048_576, "1 MB"],
    [340_000_000, "324,2 MB"],
    [5 * 1024 ** 3, "5 GB"],
    [2 * 1024 ** 4, "2 TB"],
  ])("formatea %d bytes como %s", (bytes, expected) => {
    expect(formatBytes(bytes)).toBe(expected);
  });

  it("no pasa de TB ni inventa unidades para valores enormes", () => {
    expect(formatBytes(2048 * 1024 ** 4)).toBe("2048 TB");
  });

  it("trata un valor negativo o no finito como 0 B en vez de mostrar basura", () => {
    expect(formatBytes(-5)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });
});

describe("formatDateTime", () => {
  it("formatea en espanol una fecha conocida (mediodia UTC: el dia no depende de la zona)", () => {
    const text = formatDateTime(Date.UTC(2027, 0, 15, 12, 0));
    expect(text).toContain("2027");
    expect(text).toContain("ene");
  });

  it("dos instantes distintos dan textos distintos", () => {
    expect(formatDateTime(Date.UTC(2027, 0, 15, 12, 0))).not.toBe(
      formatDateTime(Date.UTC(2027, 5, 15, 12, 0)),
    );
  });
});
