import { describe, expect, it } from "vitest";
import { emptyDetailText, itemsText } from "../../features/cleanup/recycleBinText";
import { es } from "../../shared/i18n/es";

describe("itemsText", () => {
  it("usa el singular para 1 y el plural para el resto", () => {
    expect(itemsText(1)).toBe(es.recycleBin.itemsOne);
    expect(itemsText(12)).toBe("12 elementos");
    expect(itemsText(0)).toBe("0 elementos");
  });
});

describe("emptyDetailText", () => {
  it("traduce cada familia de detalle del backend a un texto llano", () => {
    expect(emptyDetailText("not_emptied")).toBe(es.recycleBin.details.not_emptied);
    expect(emptyDetailText("unverified")).toBe(es.recycleBin.details.unverified);
    expect(emptyDetailText("permission_denied")).toBe(es.recycleBin.details.permission_denied);
    expect(emptyDetailText("empty_unavailable")).toBe(es.recycleBin.details.empty_unavailable);
    expect(emptyDetailText("partial")).toBe(es.recycleBin.details.partial);
  });

  it("cae en el texto generico para un HRESULT o un codigo desconocido, sin mostrar el codigo", () => {
    for (const code of ["io_0x8000ffff", "algo_nuevo"]) {
      const text = emptyDetailText(code);
      expect(text).toBe(es.recycleBin.details.other);
      expect(text).not.toContain(code);
    }
  });

  it("no resuelve propiedades del prototipo como codigos", () => {
    expect(emptyDetailText("toString")).toBe(es.recycleBin.details.other);
    expect(emptyDetailText("constructor")).toBe(es.recycleBin.details.other);
  });

  it("sin detalle no hay texto", () => {
    expect(emptyDetailText(null)).toBeNull();
  });
});
