import { describe, expect, it } from "vitest";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";

const KINDS = [
  "internal",
  "unknown",
  "invalid_response",
  "store_corrupt",
  "store_schema_too_new",
  "store_migration_failed",
  "store_unavailable",
  "store_not_recoverable",
  "setting_invalid",
  "setting_read_only",
] as const;

describe("errorMessage", () => {
  it.each(KINDS)("traduce el kind %s a su texto del catalogo", (kind) => {
    expect(errorMessage({ kind, message: "technical SELECT" })).toBe(es.errors[kind]);
  });

  it("cada kind tiene un texto distinto y no vacio", () => {
    const texts = KINDS.map((kind) => es.errors[kind]);
    expect(new Set(texts).size).toBe(KINDS.length);
    for (const text of texts) expect(text.length).toBeGreaterThan(0);
  });

  it("cae en el texto desconocido para un kind nuevo", () => {
    expect(errorMessage({ kind: "algo_nuevo", message: "x" })).toBe(es.errors.unknown);
  });

  it("no resuelve propiedades del prototipo como kinds", () => {
    expect(errorMessage({ kind: "toString", message: "x" })).toBe(es.errors.unknown);
    expect(errorMessage({ kind: "constructor", message: "x" })).toBe(es.errors.unknown);
  });

  it("nunca devuelve el message del backend", () => {
    const message = "database is corrupt: NotADatabase";
    for (const kind of [...KINDS, "otro"]) {
      expect(errorMessage({ kind, message })).not.toContain(message);
    }
  });
});
