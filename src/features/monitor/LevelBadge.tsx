import { Icon, type IconName } from "../../shared/ui/Icon";

export type BadgeLevel = "normal" | "high" | "very_high";

const ICON: Record<BadgeLevel, IconName> = { normal: "check", high: "warning", very_high: "error" };
const COLOR: Record<BadgeLevel, string> = {
  normal: "text-success",
  high: "text-warning",
  very_high: "text-danger",
};

/** Nivel como icono + texto: nunca solo color. */
export function LevelBadge({ level, text }: { level: BadgeLevel; text: string }) {
  return (
    <span data-level={level} className={`inline-flex items-center gap-1 font-medium ${COLOR[level]}`}>
      <Icon name={ICON[level]} className="h-4 w-4" />
      <span>{text}</span>
    </span>
  );
}
