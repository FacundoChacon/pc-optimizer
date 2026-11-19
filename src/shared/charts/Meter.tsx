import { Icon, type IconName } from "../ui/Icon";

export type MeterLevel = "normal" | "high" | "very_high" | "unknown";

export interface MeterProps {
  label: string;
  /** Porcentaje 0..100 (se acota); `null` = ausente. */
  value: number | null;
  /** Valor ya formateado ("42 %"); lo arma quien conoce los textos. */
  valueText: string;
  level: MeterLevel;
  /** Nivel en palabras ("Normal"); acompana al icono, nunca solo el color. */
  levelText: string;
  /** Texto cuando no hay valor ("No disponible"). */
  unavailableText: string;
  /** Linea secundaria opcional (promedio y pico). */
  detail?: string | undefined;
}

const FILL: Record<Exclude<MeterLevel, "unknown">, string> = {
  normal: "fill-accent",
  high: "fill-warning",
  very_high: "fill-danger",
};

const LEVEL_ICON: Record<Exclude<MeterLevel, "unknown">, IconName> = {
  normal: "check",
  high: "warning",
  very_high: "error",
};

const LEVEL_TEXT: Record<Exclude<MeterLevel, "unknown">, string> = {
  normal: "text-content-muted",
  high: "text-warning",
  very_high: "text-danger",
};

function percentOf(value: number): number {
  if (Number.isNaN(value)) return 0;
  return Math.round(Math.min(100, Math.max(0, value)) * 100) / 100;
}

/**
 * Medidor lineal (skill dataviz): pista y relleno en un SVG con el ancho como atributo en
 * porcentaje (sin `style`, compatible con la CSP estricta). La severidad va en el color del
 * relleno Y en icono + texto del nivel, fuera del SVG.
 */
export function Meter({ label, value, valueText, level, levelText, unavailableText, detail }: MeterProps) {
  const known = value !== null && level !== "unknown";
  return (
    <div className="grid grid-cols-[8rem_1fr_auto] items-center gap-x-3 gap-y-1">
      <span className="text-content">{label}</span>
      <svg aria-hidden="true" focusable="false" viewBox="0 0 100 8" preserveAspectRatio="none" className="h-2 w-full">
        <rect x="0" y="0" width="100%" height="8" rx="2" className="fill-surface" />
        {known && (
          <rect data-meter="fill" x="0" y="0" width={`${percentOf(value)}%`} height="8" rx="2" className={FILL[level]} />
        )}
      </svg>
      <span className="flex items-center gap-2 text-content">
        {known ? (
          <>
            <span>{valueText}</span>
            <span className={`flex items-center gap-1 ${LEVEL_TEXT[level]}`}>
              <Icon name={LEVEL_ICON[level]} className="h-4 w-4" />
              <span>{levelText}</span>
            </span>
          </>
        ) : (
          <span className="text-content-muted">{unavailableText}</span>
        )}
      </span>
      {detail !== undefined && <span className="col-span-2 col-start-2 text-sm text-content-muted">{detail}</span>}
    </div>
  );
}
