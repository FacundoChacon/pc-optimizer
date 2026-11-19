import { describe, expect, it } from "vitest";
import {
  accessText,
  issueReasonText,
  opaqueLabel,
  revealUnavailableText,
  rootRejectedText,
  shareText,
  sharePercent,
} from "../../features/disk/diskText";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { GB, MB } from "../support/diskFixtures";

describe("catalogo de espacio en disco", () => {
  it("nombra las tres carpetas opacas con textos distintos y sin rutas", () => {
    const labels = (["other_user_profile", "recycle_bin", "system_volume_information"] as const).map(opaqueLabel);
    expect(labels).toEqual(["Perfil de otro usuario", "Papelera", "Información del sistema"]);
    expect(new Set(labels).size).toBe(3);
  });

  it("explica el estado de acceso de una carpeta y calla cuando esta completa", () => {
    expect(accessText("complete")).toBeNull();
    expect(accessText("incomplete")).toBe(es.disk.access.incomplete);
    expect(accessText("inaccessible")).toBe(es.disk.access.inaccessible);
    expect(es.disk.access.incomplete).not.toBe(es.disk.access.inaccessible);
  });

  it("traduce el motivo de cada carpeta sin acceso", () => {
    expect(issueReasonText("access_denied")).toBe(es.disk.issueReasons.access_denied);
    expect(issueReasonText("not_found")).toBe(es.disk.issueReasons.not_found);
    expect(issueReasonText("io_error")).toBe(es.disk.issueReasons.io_error);
    expect(new Set(Object.values(es.disk.issueReasons)).size).toBe(3);
  });

  it("los seis errores nuevos tienen texto propio y distinto del generico", () => {
    const kinds = [
      "folder_root_rejected",
      "folder_pick_not_found",
      "folder_picker_busy",
      "folder_picker_unavailable",
      "folder_not_found",
      "reveal_unavailable",
    ];
    const texts = kinds.map((kind) => errorMessage({ kind, message: "x" }));
    for (const text of texts) {
      expect(text).not.toBe(es.errors.unknown);
      expect(text.length).toBeGreaterThan(10);
    }
    expect(new Set(texts).size).toBe(kinds.length);
  });
});

describe("rootRejectedText", () => {
  it("usa el motivo que viaja al final del mensaje", () => {
    const text = (reason: string) =>
      rootRejectedText({ kind: "folder_root_rejected", message: `folder root rejected: ${reason}` });
    expect(text("reparse_point")).toBe(es.disk.rootRejectedReasons.reparse_point);
    expect(text("other_user_profile")).toBe(es.disk.rootRejectedReasons.other_user_profile);
    expect(text("unsupported_volume")).toBe(es.disk.rootRejectedReasons.unsupported_volume);
    expect(text("not_a_directory")).toBe(es.disk.rootRejectedReasons.not_a_directory);
  });

  it("cae en el texto general con un motivo desconocido o un mensaje sin motivo", () => {
    expect(rootRejectedText({ kind: "folder_root_rejected", message: "folder root rejected: algo_nuevo" })).toBe(
      es.errors.folder_root_rejected,
    );
    expect(rootRejectedText({ kind: "folder_root_rejected", message: "sin motivo" })).toBe(
      es.errors.folder_root_rejected,
    );
  });

  it("cada motivo conocido tiene un texto distinto", () => {
    const texts = Object.values(es.disk.rootRejectedReasons);
    expect(new Set(texts).size).toBe(texts.length);
  });
});

describe("revealUnavailableText", () => {
  it("traduce cada motivo del Explorador", () => {
    const text = (reason: string) =>
      revealUnavailableText({ kind: "reveal_unavailable", message: `reveal unavailable: ${reason}` });
    for (const reason of ["target_missing", "opaque", "different_user", "undetermined", "shell_error"] as const) {
      expect(text(reason)).toBe(es.disk.revealReasons[reason]);
    }
    expect(text("otro")).toBe(es.errors.reveal_unavailable);
    expect(new Set(Object.values(es.disk.revealReasons)).size).toBe(5);
  });
});

describe("tamano y porcentaje", () => {
  it("muestra el tamano y el porcentaje del padre en texto", () => {
    expect(shareText(31 * GB, 100 * GB)).toBe("31 GB · 31 %");
    expect(shareText(12.4 * GB, 40 * GB)).toBe("12,4 GB · 31 %");
  });

  it("redondea, muestra menos de 1 % y acota a 100 %", () => {
    expect(sharePercent(1, 3)).toBe(33);
    expect(sharePercent(1, 1000)).toBe(0.1);
    expect(shareText(MB, 1000 * GB)).toBe("1 MB · menos de 1 %");
    expect(sharePercent(5, 3)).toBe(100);
    expect(sharePercent(0, 10)).toBe(0);
  });

  it("con un padre vacio el porcentaje es 0", () => {
    expect(sharePercent(10, 0)).toBe(0);
    expect(shareText(0, 0)).toBe("0 B · 0 %");
  });
});
