import type { AppError, FolderScanSummary, JunkScanSummary, MonitorLevel } from "../../lib/ipc";
import { isCleanable } from "../cleanup/cleanupText";
import type { MonitorState } from "../monitor/MonitorProvider";

/** Que ve cada tarjeta de Inicio (D-06): nunca un 0 inventado ni un bloque vacio cuando falta un dato. */

export type SpaceCardState =
  | { kind: "loading" }
  | {
      kind: "data";
      freeBytes: number;
      totalBytes: number;
      /** `null` cuando el dato viene de un analisis de disco (respaldo) y no del monitor. */
      level: "normal" | "almost_full" | null;
      /** Fecha del analisis de respaldo; `null` si el dato es del monitor. */
      analyzedAt: number | null;
      stale: boolean;
    }
  | { kind: "unavailable"; code: string };

export type MemoryCardState =
  | { kind: "loading" }
  | { kind: "measuring" }
  | {
      kind: "data";
      pct: number;
      level: Exclude<MonitorLevel, "unknown">;
      usedBytes: number | null;
      totalBytes: number | null;
      stale: boolean;
    }
  | { kind: "unavailable"; code: string };

export type JunkCardState =
  | { kind: "loading" }
  | { kind: "no_analysis" }
  | { kind: "unavailable"; code: string }
  | { kind: "data"; bytes: number; scannedAt: number; binBytes: number | null }
  | { kind: "nothing"; scannedAt: number; binBytes: number | null }
  | { kind: "cleaned"; cleanedAt: number; binBytes: number | null };

const MONITOR_DOWN = "monitor_unavailable";

function monitorCode(monitor: MonitorState): string {
  return monitor.error?.kind ?? MONITOR_DOWN;
}

export function spaceCardState(monitor: MonitorState, disk: FolderScanSummary | null): SpaceCardState {
  const { summary } = monitor;
  if (monitor.status === "idle" || monitor.status === "loading" || (monitor.status === "live" && summary === null)) {
    return { kind: "loading" };
  }
  const sample = monitor.samples.at(-1);
  if (
    monitor.status === "live" &&
    summary !== null &&
    summary.availability.volume === "ok" &&
    summary.volume.level !== "unknown" &&
    sample?.volumeFreeBytes != null &&
    sample.volumeTotalBytes != null
  ) {
    return {
      kind: "data",
      freeBytes: sample.volumeFreeBytes,
      totalBytes: sample.volumeTotalBytes,
      level: summary.volume.level,
      analyzedAt: null,
      stale: monitor.stale,
    };
  }
  if (disk !== null && disk.status === "done" && disk.volume !== null) {
    return {
      kind: "data",
      freeBytes: disk.volume.freeBytes,
      totalBytes: disk.volume.totalBytes,
      level: null,
      analyzedAt: disk.finishedAt ?? disk.startedAt,
      stale: false,
    };
  }
  return { kind: "unavailable", code: summary === null ? monitorCode(monitor) : summary.availability.volume };
}

export function memoryCardState(monitor: MonitorState): MemoryCardState {
  const { summary } = monitor;
  if (monitor.status === "unavailable") return { kind: "unavailable", code: monitorCode(monitor) };
  if (monitor.status !== "live" || summary === null) return { kind: "loading" };
  const availability = summary.availability.memory;
  if (availability === "warming_up" || availability === "after_gap") return { kind: "measuring" };
  if (availability !== "ok" || summary.memory.current === null) return { kind: "unavailable", code: availability };
  const sample = monitor.samples.at(-1);
  return {
    kind: "data",
    pct: summary.memory.current,
    level: summary.memory.level === "unknown" ? "normal" : summary.memory.level,
    usedBytes: sample?.memUsedBytes ?? null,
    totalBytes: sample?.memTotalBytes ?? null,
    stale: monitor.stale,
  };
}

interface JunkInput {
  loaded: boolean;
  last: JunkScanSummary | null;
  error: AppError | null;
}

export function junkCardState({ loaded, last, error }: JunkInput): JunkCardState {
  if (!loaded) return { kind: "loading" };
  if (last === null || last.status !== "done") {
    return error === null ? { kind: "no_analysis" } : { kind: "unavailable", code: error.kind };
  }
  const bin = last.categories.find((category) => category.category === "recycle_bin");
  const binBytes = bin !== undefined && bin.status === "done" && bin.itemCount > 0 && bin.sizeBytes > 0 ? bin.sizeBytes : null;
  if (last.cleanedAt !== null) return { kind: "cleaned", cleanedAt: last.cleanedAt, binBytes };
  const scannedAt = last.finishedAt ?? last.startedAt;
  const cleanable = last.categories.filter(
    (category) => isCleanable(category.category) && (category.status === "done" || category.status === "partial"),
  );
  const bytes = cleanable.reduce((total, category) => total + category.sizeBytes, 0);
  const items = cleanable.reduce((total, category) => total + category.itemCount, 0);
  return items === 0 ? { kind: "nothing", scannedAt, binBytes } : { kind: "data", bytes, scannedAt, binBytes };
}
