import { describe, expect, it } from "vitest";
import {
  availabilityNote,
  availabilityText,
  levelPhrase,
  levelText,
  metricName,
  percentText,
  spaceLevelText,
} from "../../features/monitor/monitorText";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { formatRate } from "../../shared/i18n/format";

describe("levelText", () => {
  it.each([
    ["normal", es.monitor.levels.normal],
    ["high", es.monitor.levels.high],
    ["very_high", es.monitor.levels.very_high],
  ] as const)("%s tiene su texto en palabras", (level, text) => {
    expect(levelText(level)).toBe(text);
  });

  it("los tres niveles tienen textos distintos y sin el codigo tecnico", () => {
    const texts = (["normal", "high", "very_high"] as const).map(levelText);
    expect(new Set(texts).size).toBe(3);
    for (const text of texts) expect(text).not.toContain("_");
  });
});

describe("spaceLevelText", () => {
  it("casi lleno y normal", () => {
    expect(spaceLevelText("almost_full")).toBe(es.monitor.spaceLevels.almost_full);
    expect(spaceLevelText("normal")).toBe(es.monitor.spaceLevels.normal);
  });
});

describe("availabilityText y availabilityNote", () => {
  it("calentando y tras un hueco dicen 'Midiendo…'; el resto 'No disponible'", () => {
    expect(availabilityText("warming_up")).toBe(es.monitor.measuring);
    expect(availabilityText("after_gap")).toBe(es.monitor.measuring);
    expect(availabilityText("not_supported")).toBe(es.monitor.unavailable);
    expect(availabilityText("source_error")).toBe(es.monitor.unavailable);
  });

  it("cada motivo tiene una explicacion propia y ok no tiene", () => {
    const notes = (["warming_up", "after_gap", "source_error", "not_supported"] as const).map(availabilityNote);
    expect(new Set(notes).size).toBe(4);
    for (const note of notes) expect(note).toBeTruthy();
    expect(availabilityNote("ok")).toBeNull();
  });
});

describe("levelPhrase", () => {
  it("solo hay frase llana para alto y muy alto, distinta por metrica", () => {
    expect(levelPhrase("cpu", "normal")).toBeNull();
    expect(levelPhrase("cpu", "very_high")).toBe(es.monitor.phrases.cpu.very_high);
    expect(levelPhrase("memory", "high")).toBe(es.monitor.phrases.memory.high);
    expect(levelPhrase("cpu", "high")).not.toBe(levelPhrase("memory", "high"));
  });
});

describe("percentText y metricName", () => {
  it("redondea al entero y formatea con espacio", () => {
    expect(percentText(23.4)).toBe("23 %");
    expect(percentText(99.6)).toBe("100 %");
    expect(metricName("memory")).toBe(es.monitor.metrics.memory);
  });
});

describe("formatRate", () => {
  it.each([
    [0, "0 B/s"],
    [1536, "1,5 KB/s"],
    [37_000_000, "35,3 MB/s"],
    [512, "512 B/s"],
  ])("formatRate(%s) = %s", (bps, expected) => {
    expect(formatRate(bps)).toBe(expected);
  });
});

describe("error monitor_unavailable", () => {
  it("tiene mensaje propio en espanol", () => {
    expect(errorMessage({ kind: "monitor_unavailable", message: "x" })).toBe(es.errors.monitor_unavailable);
    expect(es.errors.monitor_unavailable).not.toBe(es.errors.unknown);
  });
});
