import { describe, expect, it } from "vitest";
import { optionsFromLastScan } from "../../features/duplicates/useDuplicateScan";
import { roots, scanSummary } from "../support/duplicateFixtures";

describe("optionsFromLastScan", () => {
  it("sin escaneo previo no hay nada que reflejar", () => {
    expect(optionsFromLastScan(null)).toBeNull();
  });

  it("toma las carpetas no 'not_selected', en el orden canonico, y el tamano minimo", () => {
    const summary = scanSummary({
      minSizeBytes: 104_857_600,
      roots: roots({
        desktop: { status: "not_selected" },
        pictures: { status: "not_selected" },
        music: { status: "unavailable", reason: "not_found" },
      }),
    });
    expect(optionsFromLastScan(summary)).toEqual({
      roots: ["documents", "downloads", "music", "videos"],
      minSizeBytes: 104_857_600,
    });
  });

  it("un tamano fuera de la lista cerrada vuelve a 1 MB sin perder las carpetas", () => {
    const summary = scanSummary({ minSizeBytes: 12_345, roots: roots({ videos: { status: "not_selected" } }) });
    expect(optionsFromLastScan(summary)).toEqual({
      roots: ["desktop", "documents", "downloads", "music", "pictures"],
      minSizeBytes: 1_048_576,
    });
  });

  it("sin ninguna carpeta elegida devuelve null", () => {
    const none = roots({
      desktop: { status: "not_selected" },
      documents: { status: "not_selected" },
      downloads: { status: "not_selected" },
      music: { status: "not_selected" },
      pictures: { status: "not_selected" },
      videos: { status: "not_selected" },
    });
    expect(optionsFromLastScan(scanSummary({ roots: none }))).toBeNull();
  });
});
