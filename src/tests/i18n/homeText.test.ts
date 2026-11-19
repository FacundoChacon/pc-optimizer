import { describe, expect, it } from "vitest";
import { binNoteText, busyReasonText, codeText } from "../../features/dashboard/homeText";
import { es } from "../../shared/i18n/es";

describe("busyReasonText", () => {
  it("nombra el trabajo que hay que esperar", () => {
    expect(busyReasonText("duplicateScan")).toBe("Esperá a que termine la búsqueda de duplicados para limpiar.");
    expect(busyReasonText("junkScan")).toBe("Esperá a que termine la búsqueda de basura para limpiar.");
    expect(busyReasonText("emptyBin")).toBe("Esperá a que termine el vaciado de la Papelera para limpiar.");
  });

  it("cada trabajo del catalogo tiene un motivo distinto", () => {
    const reasons = (Object.keys(es.home.busy.tasks) as Array<keyof typeof es.home.busy.tasks>).map(busyReasonText);

    expect(new Set(reasons).size).toBe(reasons.length);
  });
});

describe("binNoteText", () => {
  it("menciona el tamano de la Papelera y remite a Limpieza", () => {
    expect(binNoteText(2 * 1024 ** 3)).toBe("La Papelera ocupa 2 GB. Podés vaciarla desde Limpieza.");
    expect(binNoteText(512 * 1024 ** 2)).toBe("La Papelera ocupa 512 MB. Podés vaciarla desde Limpieza.");
  });
});

describe("codeText", () => {
  it("muestra el codigo crudo con su etiqueta", () => {
    expect(codeText("store_unavailable")).toBe("Código: store_unavailable");
    expect(codeText("not_supported")).toBe("Código: not_supported");
  });
});
