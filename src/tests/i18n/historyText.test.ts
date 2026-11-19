import { describe, expect, it } from "vitest";
import {
  entryCategoryText,
  entryDetailText,
  outcomeText,
  runPhrase,
  runTitle,
} from "../../features/history/historyText";
import { duplicatesRun, emptyBinRun, junkRun } from "../support/historyFixtures";

describe("runPhrase: limpieza de basura", () => {
  it("usa el plural con 340 archivos movidos y cuenta los 5 omitidos por familia", () => {
    const text = runPhrase(junkRun(1));

    expect(text).toContain("340 archivos");
    expect(text).toContain("1,2 GB");
    expect(text).toContain("movidos a la Papelera");
    expect(text).toContain("5 se omitieron");
    expect(text).toContain("3 en uso o modificados hace poco");
    expect(text).toContain("2 protegidos");
  });

  it("usa el singular con 1 archivo movido y 1 omitido", () => {
    const run = junkRun(2, {
      outcomes: { movedToTrash: 1, skippedLocked: 1, skippedProtected: 0, skippedCloud: 0, error: 0, recycleBinEmptied: 0 },
      movedBytes: 2_048,
    });

    const text = runPhrase(run);

    expect(text).toContain("1 archivo (2 KB) movido a la Papelera");
    expect(text).toContain("1 se omitió");
    expect(text).not.toContain("archivos");
  });

  it("sin omitidos no menciona omisiones, y con errores los cuenta", () => {
    const run = junkRun(3, {
      outcomes: { movedToTrash: 10, skippedLocked: 0, skippedProtected: 0, skippedCloud: 0, error: 2, recycleBinEmptied: 0 },
      movedBytes: 5_242_880,
    });

    const text = runPhrase(run);

    expect(text).toContain("10 archivos (5 MB) movidos a la Papelera");
    expect(text).not.toContain("omit");
    expect(text).toContain("2 no se pudieron mover");
  });

  it("cuenta los archivos en la nube y no muestra familias con cero", () => {
    const run = junkRun(4, {
      outcomes: { movedToTrash: 1, skippedLocked: 0, skippedProtected: 0, skippedCloud: 4, error: 1, recycleBinEmptied: 0 },
      movedBytes: 1_024,
    });

    const text = runPhrase(run);

    expect(text).toContain("4 se omitieron (4 en la nube)");
    expect(text).not.toContain("protegidos");
    expect(text).toContain("1 no se pudo mover");
  });

  it("sin archivos movidos lo dice con claridad", () => {
    const run = junkRun(5, {
      outcomes: { movedToTrash: 0, skippedLocked: 2, skippedProtected: 0, skippedCloud: 0, error: 0, recycleBinEmptied: 0 },
      movedBytes: 0,
    });

    expect(runPhrase(run)).toContain("no se movió ningún archivo");
  });
});

describe("runPhrase: duplicados y tipo desconocido", () => {
  it("una limpieza de duplicados usa su propio titulo y el tamano", () => {
    const text = runPhrase(duplicatesRun(6));

    expect(text).toBe("Copias duplicadas: 3 archivos (2 GB) movidos a la Papelera.");
  });

  it("el tipo `other` usa una frase generica y nunca el codigo", () => {
    const run = junkRun(7, { kind: "other", categories: ["custom_rule"] });

    const text = runPhrase(run);

    expect(text.startsWith("Limpieza: ")).toBe(true);
    expect(text).not.toContain("other");
    expect(runTitle("other")).toBe("Limpieza");
    expect(runTitle("junk")).toBe("Limpieza de basura");
    expect(runTitle("zzz" as never)).toBe("Limpieza");
  });
});

describe("runPhrase: vaciado de la Papelera", () => {
  it("un vaciado parcial dice cuanto se libero y que algo quedo", () => {
    const text = runPhrase(emptyBinRun(1));

    expect(text).toBe(
      "Papelera vaciada en parte: se liberaron 2 GB. Algunos elementos no se pudieron eliminar.",
    );
  });

  it("completo, sin verificar y fallido tienen cada uno su frase", () => {
    expect(runPhrase(emptyBinRun(2, { emptyDetail: null }))).toBe("Papelera vaciada: se liberaron 2 GB.");
    expect(runPhrase(emptyBinRun(3, { emptyDetail: "unverified" }))).toContain("no se pudo comprobar");
    const failed = emptyBinRun(4, {
      emptyDetail: null,
      emptiedBytes: 0,
      outcomes: { movedToTrash: 0, skippedLocked: 0, skippedProtected: 0, skippedCloud: 0, error: 1, recycleBinEmptied: 0 },
    });
    expect(runPhrase(failed)).toBe("No se pudo vaciar la Papelera. No se eliminó nada.");
  });
});

describe("textos de items", () => {
  it("el resultado conocido tiene su texto y el desconocido cae en el respaldo", () => {
    expect(outcomeText("moved_to_trash")).toBe("Movido a la Papelera");
    expect(outcomeText("skipped_cloud")).toBe("Omitido: en la nube");
    expect(outcomeText("algo_nuevo")).toBe("Resultado desconocido");
    expect(outcomeText("toString")).toBe("Resultado desconocido");
    expect(outcomeText("unknown")).toBe("Resultado desconocido");
  });

  it("las categorias de la whitelist, duplicados y desconocidas", () => {
    expect(entryCategoryText("temp_user")).toBe("Archivos temporales del usuario");
    expect(entryCategoryText("recycle_bin")).toBe("Papelera de reciclaje");
    expect(entryCategoryText("duplicates")).toBe("Copias duplicadas");
    expect(entryCategoryText("custom_rule")).toBe("Otra categoría");
    expect(entryCategoryText("toString")).toBe("Otra categoría");
  });

  it("el codigo de detalle se traduce segun el resultado y sin detalle no hay texto", () => {
    expect(entryDetailText("skipped_locked", "in_use")).toBe("En uso o modificados hace poco");
    expect(entryDetailText("error", "algo_nuevo")).toBe("No se pudieron mover");
    expect(entryDetailText("recycle_bin_emptied", "partial")).toBe("Algunos elementos seguían en uso.");
    expect(entryDetailText("moved_to_trash", "x")).toBeNull();
    expect(entryDetailText("error", null)).toBeNull();
  });
});
