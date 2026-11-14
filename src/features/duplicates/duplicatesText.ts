import type { DuplicateGroup, RootKey, RootResult } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";

/** Nombre en espanol de una carpeta de contenido. */
export function folderName(root: RootKey): string {
  return es.duplicates.folderNames[root];
}

type RootReasonKey = keyof typeof es.duplicates.rootReasons;

/** Motivo de una carpeta no disponible; uno desconocido cae en el texto generico. */
export function rootReasonText(reason: string | null): string {
  const key = reason !== null && Object.hasOwn(es.duplicates.rootReasons, reason) ? reason : "generic";
  return es.duplicates.rootReasons[key as RootReasonKey];
}

/** Nombre del grupo: el del archivo de su primera copia (sin la carpeta). */
export function groupName(group: DuplicateGroup): string {
  const first = group.files[0];
  if (first === undefined) return "";
  const segments = first.relativePath.split("\\").filter((segment) => segment.length > 0);
  return segments[segments.length - 1] ?? first.relativePath;
}

/** Un grupo cuenta como decidido con al menos una copia conservada y al menos una a mover. */
export function isDecided(group: DuplicateGroup, keep: readonly number[] | undefined): boolean {
  const kept = keep ?? [];
  return kept.length >= 1 && kept.length < group.files.length;
}

/** Copias que se moverian segun las elecciones (solo grupos abiertos y decididos). */
export function copiesToMove(groups: readonly DuplicateGroup[], choices: Readonly<Record<number, number[]>>): number {
  return groups
    .filter((group) => group.resolvedAt === null && isDecided(group, choices[group.groupId]))
    .reduce((total, group) => total + group.files.length - (choices[group.groupId]?.length ?? 0), 0);
}

/** Estado de una carpeta en lenguaje llano; si no estuvo disponible, con su motivo. */
export function rootStatusText(root: RootResult): string {
  const status = es.duplicates.rootStatus[root.status];
  return root.status === "unavailable"
    ? format(es.duplicates.statusWithReason, { status, reason: rootReasonText(root.reason) })
    : status;
}
