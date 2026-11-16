import type { MetricAvailability, MonitorLevel, MonitorSpaceLevel } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";

export type MonitorMetric = "cpu" | "memory" | "diskActive";

type KnownLevel = Exclude<MonitorLevel, "unknown">;

/** Nivel en palabras. `unknown` no tiene texto propio: se muestra como no disponible. */
export function levelText(level: KnownLevel): string {
  return es.monitor.levels[level];
}

export function spaceLevelText(level: Exclude<MonitorSpaceLevel, "unknown">): string {
  return es.monitor.spaceLevels[level];
}

/** Lo que se ve en lugar del valor: "Midiendo…" mientras se calienta, si no "No disponible". */
export function availabilityText(availability: MetricAvailability): string {
  return availability === "warming_up" || availability === "after_gap" ? es.monitor.measuring : es.monitor.unavailable;
}

/** Explicacion breve del motivo; `null` si la medicion esta presente. */
export function availabilityNote(availability: MetricAvailability): string | null {
  return availability === "ok" ? null : es.monitor.availabilityNotes[availability];
}

/** Frase llana para un nivel alto o muy alto; `null` si no hay nada que avisar. */
export function levelPhrase(metric: MonitorMetric, level: MonitorLevel): string | null {
  return level === "high" || level === "very_high" ? es.monitor.phrases[metric][level] : null;
}

export function metricName(metric: keyof typeof es.monitor.metrics): string {
  return es.monitor.metrics[metric];
}

/** Porcentaje entero con espacio, como se lee en espanol: `23 %`. */
export function percentText(value: number): string {
  return `${Math.round(value)} %`;
}
