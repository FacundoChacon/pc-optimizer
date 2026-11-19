import type { HomeBusyTask } from "./homeText";

/** Trabajos en curso que impiden iniciar "Limpiar ahora" (D-04). */
export interface BusyFlags {
  junkScanning: boolean;
  cleaning: boolean;
  emptyingBin: boolean;
  diskScanning: boolean;
  duplicateScanning: boolean;
  duplicateCleaning: boolean;
}

/** Primer trabajo en curso en un orden estable, o `null` si no hay ninguno. */
export function busyTask(flags: BusyFlags): HomeBusyTask | null {
  if (flags.junkScanning) return "junkScan";
  if (flags.cleaning) return "cleanup";
  if (flags.emptyingBin) return "emptyBin";
  if (flags.diskScanning) return "diskScan";
  if (flags.duplicateScanning) return "duplicateScan";
  if (flags.duplicateCleaning) return "duplicateCleanup";
  return null;
}
