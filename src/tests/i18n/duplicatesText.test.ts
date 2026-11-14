import { describe, expect, it } from "vitest";
import { cleanupReasonText } from "../../features/cleanup/cleanupText";
import {
  copiesToMove,
  folderName,
  groupName,
  isDecided,
  rootReasonText,
  rootStatusText,
} from "../../features/duplicates/duplicatesText";
import { MIN_SIZE_OPTIONS, ROOT_KEYS } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { file, group, roots, twoGroups } from "../support/duplicateFixtures";

describe("catalogo de duplicados", () => {
  it("nombra cada carpeta de contenido en espanol", () => {
    expect(ROOT_KEYS.map(folderName)).toEqual([
      "Escritorio",
      "Documentos",
      "Descargas",
      "Música",
      "Imágenes",
      "Videos",
    ]);
  });

  it("tiene una etiqueta para cada opcion de tamano minimo", () => {
    for (const bytes of MIN_SIZE_OPTIONS) {
      expect(es.duplicates.advanced.minSizeOptions[String(bytes) as keyof typeof es.duplicates.advanced.minSizeOptions]).toBeTruthy();
    }
  });

  it("los 4 errores nuevos tienen texto propio, no el generico", () => {
    for (const kind of ["duplicates_blocked", "duplicate_group_not_found", "duplicate_group_resolved", "duplicate_keep_required"]) {
      const text = errorMessage({ kind, message: "x" });
      expect(text).not.toBe(es.errors.unknown);
      expect(text.length).toBeGreaterThan(10);
    }
  });

  it("los codigos nuevos de la ejecucion caen en sus familias", () => {
    expect(cleanupReasonText("skipped_locked", "content_changed")).toBe(es.cleanup.cleanupReasons.changed);
    expect(cleanupReasonText("skipped_locked", "content_unreadable")).toBe(es.cleanup.cleanupReasons.unreadable);
    expect(cleanupReasonText("skipped_protected", "kept_copy_unavailable")).toBe(es.cleanup.cleanupReasons.kept_missing);
    expect(cleanupReasonText("skipped_locked", "in_use")).toBe(es.cleanup.cleanupReasons.locked);
  });
});

describe("rootReasonText y rootStatusText", () => {
  it("traduce los motivos conocidos y usa un respaldo generico para los desconocidos", () => {
    expect(rootReasonText("unsupported_volume")).toBe(es.duplicates.rootReasons.unsupported_volume);
    expect(rootReasonText("cloud")).toBe(es.duplicates.rootReasons.cloud);
    for (const raw of ["algo_nuevo", null, "constructor"]) expect(rootReasonText(raw)).toBe(es.duplicates.rootReasons.generic);
  });

  it("el estado de una carpeta no disponible incluye su motivo y el resto no", () => {
    const [desktop] = roots({ desktop: { status: "unavailable", reason: "cloud" } });
    expect(desktop && rootStatusText(desktop)).toBe("No disponible: está en la nube");
    const [done] = roots();
    expect(done && rootStatusText(done)).toBe(es.duplicates.rootStatus.done);
  });
});

describe("groupName, isDecided y copiesToMove", () => {
  it("el nombre del grupo es el del archivo, sin carpetas", () => {
    const [photo, video] = twoGroups();
    expect(photo && groupName(photo)).toBe("foto.jpg");
    expect(video && groupName(video)).toBe("video.mp4");
    expect(groupName(group(3, [file(1, "downloads", "Viajes\\2019\\playa.jpg"), file(2, "pictures", "x.jpg")]))).toBe("playa.jpg");
  });

  it("un grupo esta decidido con al menos una conservada y al menos una a mover", () => {
    const [photo, video] = twoGroups();
    if (photo === undefined || video === undefined) throw new Error("fixture");
    expect(isDecided(photo, undefined)).toBe(false);
    expect(isDecided(photo, [])).toBe(false);
    expect(isDecided(photo, [11])).toBe(true);
    expect(isDecided(photo, [11, 12])).toBe(false);
    expect(isDecided(video, [21, 22])).toBe(true);
  });

  it("copiesToMove suma solo los grupos abiertos y decididos", () => {
    const [photo, video] = twoGroups();
    if (photo === undefined || video === undefined) throw new Error("fixture");
    expect(copiesToMove([photo, video], {})).toBe(0);
    expect(copiesToMove([photo, video], { 1: [11] })).toBe(1);
    expect(copiesToMove([photo, video], { 1: [11], 2: [22] })).toBe(3);
    expect(copiesToMove([photo, video], { 2: [21, 22] })).toBe(1);
    expect(copiesToMove([{ ...photo, resolvedAt: 5 }, video], { 1: [11], 2: [22] })).toBe(2);
  });
});
