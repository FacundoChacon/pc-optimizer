/** Fraccion acotada a 0..1; un valor invalido se muestra como 0. */
function clamp(fraction: number): number {
  if (Number.isNaN(fraction)) return 0;
  return Math.min(1, Math.max(0, fraction));
}

/**
 * Barra proporcional decorativa (skill dataviz): un SVG con un `<rect>` cuyo ancho es un atributo
 * en porcentaje. No lleva estilos en linea (compatible con `style-src 'self'`), se oculta a las
 * tecnologias de asistencia y siempre va acompanada del tamano y el porcentaje en texto.
 */
export function ProportionBar({ fraction }: { fraction: number }) {
  const percent = Math.round(clamp(fraction) * 10_000) / 100;
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 100 8"
      preserveAspectRatio="none"
      className="h-2 w-full"
    >
      <rect x="0" y="0" width="100%" height="8" rx="2" className="fill-surface" />
      <rect data-bar="value" x="0" y="0" width={`${percent}%`} height="8" rx="2" className="fill-accent" />
    </svg>
  );
}
