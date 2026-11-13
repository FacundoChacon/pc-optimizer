export type IconName = "home" | "settings" | "check" | "warning" | "error" | "info";

const PATHS: Record<IconName, string> = {
  home: "M3 11l9-8 9 8M5 10v10h5v-6h4v6h5V10",
  settings: "M4 6h16M4 12h16M4 18h16M9 4v4M15 10v4M8 16v4",
  check: "M5 13l4 4L19 7",
  warning: "M12 3L2 21h20L12 3zM12 10v5M12 18v.5",
  error: "M12 3a9 9 0 100 18 9 9 0 000-18zM9 9l6 6M15 9l-6 6",
  info: "M12 3a9 9 0 100 18 9 9 0 000-18zM12 11v6M12 7.5v.5",
};

/** Icono decorativo (siempre oculto a lectores de pantalla; el texto lo acompana). */
export function Icon({ name, className }: { name: IconName; className?: string }) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className ?? "h-5 w-5"}
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
