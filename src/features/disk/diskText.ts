import type { AppError, FolderAccess, FolderIssueReason, OpaqueReason } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";

/** Etiqueta llana de una carpeta opaca (su contenido nunca se muestra). */
export function opaqueLabel(reason: OpaqueReason): string {
  return es.disk.opaque[reason];
}

/** Estado de lectura de una carpeta; `null` si se leyo completa (no hay nada que avisar). */
export function accessText(access: FolderAccess): string | null {
  return access === "complete" ? null : es.disk.access[access];
}

export function issueReasonText(reason: FolderIssueReason): string {
  return es.disk.issueReasons[reason];
}

/** Motivo que el backend deja al final del mensaje (`folder root rejected: reparse_point`). */
function reasonCode(error: AppError): string | null {
  const index = error.message.lastIndexOf(": ");
  return index < 0 ? null : error.message.slice(index + 2).trim();
}

function reasonText(error: AppError, reasons: Readonly<Record<string, string>>): string {
  const code = reasonCode(error);
  return code !== null && Object.hasOwn(reasons, code) ? (reasons[code] ?? errorMessage(error)) : errorMessage(error);
}

/** Por que no se puede analizar la carpeta elegida; un motivo desconocido cae en el texto general. */
export function rootRejectedText(error: AppError): string {
  return reasonText(error, es.disk.rootRejectedReasons);
}

/** Por que no se puede mostrar un elemento en el Explorador. */
export function revealUnavailableText(error: AppError): string {
  return reasonText(error, es.disk.revealReasons);
}

/** Porcentaje de `size` sobre `parent`, acotado a 0..100. Redondea salvo por debajo de 1 %, que se informa aparte. */
export function sharePercent(size: number, parent: number): number {
  if (!(parent > 0) || !(size > 0)) return 0;
  const raw = Math.min(100, (size / parent) * 100);
  return raw >= 1 ? Math.round(raw) : raw;
}

/** "12,4 GB · 31 %": el tamano y el porcentaje del padre, siempre en texto. */
export function shareText(size: number, parent: number): string {
  const percent = sharePercent(size, parent);
  const text = percent > 0 && percent < 1 ? es.disk.shareLessThanOne : `${percent} %`;
  return format(es.disk.share, { size: formatBytes(size), percent: text });
}

/** Texto de una carpeta elegida que el backend rechazo: con su motivo cuando se conoce. */
export function pickRejectedText(error: AppError): string {
  if (error.kind !== "folder_root_rejected") return errorMessage(error);
  const reason = rootRejectedText(error);
  return reason === es.errors.folder_root_rejected ? reason : format(es.disk.pickRejected, { reason });
}

/** Un texto con su codigo estable al lado (modo avanzado): `Sin acceso (inaccessible)`. */
export function withCode(text: string, code: string): string {
  return format(es.disk.withCode, { text, code });
}
