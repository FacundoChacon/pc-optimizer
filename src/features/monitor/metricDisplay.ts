import type { MetricAvailability, MetricSummary, MonitorLevel } from "../../lib/ipc";
import { availabilityNote, availabilityText, levelText, percentText } from "./monitorText";

/** Lo que se muestra de una metrica porcentual: el valor y su nivel, o por que no hay valor. */
export interface MetricDisplay {
  value: number | null;
  valueText: string;
  level: MonitorLevel;
  levelText: string;
  /** "Midiendo…" o "No disponible" (solo cuando no hay valor). */
  fallback: string;
  /** Explicacion del motivo (solo cuando no hay valor). */
  note: string | null;
}

/** Una metrica ausente nunca se convierte en 0: se muestra su motivo. */
export function metricDisplay(metric: MetricSummary, availability: MetricAvailability): MetricDisplay {
  if (metric.current === null) {
    return {
      value: null,
      valueText: "",
      level: "unknown",
      levelText: "",
      fallback: availabilityText(availability),
      note: availabilityNote(availability),
    };
  }
  const level = metric.level === "unknown" ? "normal" : metric.level;
  return {
    value: metric.current,
    valueText: percentText(metric.current),
    level,
    levelText: levelText(level),
    fallback: "",
    note: null,
  };
}
