import type { ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

export type StatusTone = "success" | "warning" | "danger" | "neutral";

const TONES: Record<StatusTone, { icon: IconName; color: string }> = {
  success: { icon: "check", color: "text-success" },
  warning: { icon: "warning", color: "text-warning" },
  danger: { icon: "error", color: "text-danger" },
  neutral: { icon: "info", color: "text-content-muted" },
};

/** Estado como icono + texto: nunca solo color ni solo icono. */
export function StatusBadge({ tone, children }: { tone: StatusTone; children: ReactNode }) {
  const { icon, color } = TONES[tone];
  return (
    <span data-tone={tone} className={`inline-flex items-center gap-2 font-medium ${color}`}>
      <Icon name={icon} />
      <span>{children}</span>
    </span>
  );
}
