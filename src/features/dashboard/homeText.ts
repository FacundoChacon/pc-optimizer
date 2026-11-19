import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";

/** Trabajos en curso que deshabilitan "Limpiar ahora" (claves de `es.home.busy.tasks`). */
export type HomeBusyTask = keyof typeof es.home.busy.tasks;

/** Motivo visible de "Limpiar ahora" deshabilitado por un trabajo en curso. */
export function busyReasonText(task: HomeBusyTask): string {
  return format(es.home.busy.reason, { task: es.home.busy.tasks[task] });
}

/** Linea aparte de la tarjeta Basura cuando la Papelera ocupa espacio (Inicio no la vacia). */
export function binNoteText(binBytes: number): string {
  return format(es.home.cards.junk.bin, { size: formatBytes(binBytes) });
}

/** Codigo crudo de un estado no disponible (solo en modo avanzado). */
export function codeText(code: string): string {
  return format(es.home.codeLabel, { code });
}

/** Explicacion breve de un estado no disponible: motivo del monitor o mensaje del catalogo por `kind`. */
export function unavailableNote(code: string): string {
  const notes: Readonly<Record<string, string>> = es.monitor.availabilityNotes;
  return Object.hasOwn(notes, code) ? (notes[code] ?? "") : errorMessage({ kind: code, message: "" });
}
