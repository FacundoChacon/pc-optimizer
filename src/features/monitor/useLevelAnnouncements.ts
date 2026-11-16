import { useEffect, useRef } from "react";
import { useAnnounce } from "../../app/providers/Announcer";
import type { MonitorLevel, MonitorSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { levelText } from "./monitorText";

type Tracked = "cpu" | "memory" | "diskActive";
const TRACKED: readonly Tracked[] = ["cpu", "memory", "diskActive"];

/**
 * Anuncia (region `polite`) solo cuando cambia el nivel de una metrica; los valores de 1 Hz no se
 * anuncian. El primer nivel observado no se anuncia: no es un cambio.
 */
export function useLevelAnnouncements(summary: MonitorSummary | null): void {
  const announce = useAnnounce();
  const previous = useRef<Partial<Record<Tracked, MonitorLevel>>>({});
  useEffect(() => {
    if (summary === null) return;
    for (const key of TRACKED) {
      const level = summary[key].level;
      const before = previous.current[key];
      previous.current[key] = level;
      if (before === undefined || before === level || level === "unknown") continue;
      announce(format(es.monitor.announce, { metric: es.monitor.metrics[key], level: levelText(level).toLowerCase() }));
    }
  }, [summary, announce]);
}
