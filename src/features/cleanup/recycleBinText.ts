import type { EmptyResult } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";

type DetailFamily = keyof typeof es.recycleBin.details;

function isDetailFamily(detail: string): detail is DetailFamily {
  return Object.hasOwn(es.recycleBin.details, detail) && detail !== "other";
}

/** "1 elemento" / "12 elementos". */
export function itemsText(count: number): string {
  return count === 1 ? es.recycleBin.itemsOne : format(es.recycleBin.itemsMany, { count: String(count) });
}

/**
 * Codigo de detalle estable del backend -> texto llano. Un codigo desconocido o un `io_0x...` cae
 * en el texto generico: nunca se muestra el codigo crudo. Sin detalle (`emptied`), sin texto.
 */
export function emptyDetailText(detail: string | null): string | null {
  if (detail === null) return null;
  return es.recycleBin.details[isDetailFamily(detail) ? detail : "other"];
}

/** Resultado del vaciado en lenguaje llano, segun su estado. */
export function emptyResultText(result: EmptyResult): string {
  if (result.status === "emptied") {
    return format(es.recycleBin.result.emptied, { size: formatBytes(result.freedBytes) });
  }
  if (result.status === "partial") {
    return format(es.recycleBin.result.partial, {
      size: formatBytes(result.freedBytes),
      items: itemsText(result.itemsAfter ?? 0),
    });
  }
  return es.recycleBin.result[result.status];
}
