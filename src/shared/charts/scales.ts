/** Escalas puras de los graficos (sin DOM): tiempo fijo en X, un solo eje Y. */

/** Un punto de una serie: `v = null` es una metrica ausente; `gapBefore` corta la linea. */
export interface SeriesPoint {
  t: number;
  v: number | null;
  gapBefore?: boolean;
}

export interface LineOptions {
  /** Dominio de tiempo `[desde, hasta]` en la unidad de `t`. */
  domain: readonly [number, number];
  /** Maximo del eje Y (el minimo es siempre 0). */
  yMax: number;
  width: number;
  height: number;
}

/** Eje fijo de porcentajes. */
export const PERCENT_MAX = 100;

/** Dominio de tiempo fijo: la ventana que termina en la ultima muestra. */
export function timeDomain(last: number, windowMs: number): [number, number] {
  return [last - windowMs, last];
}

function round(value: number): number {
  return Math.round(value * 100) / 100;
}

/** Maximo "redondo" (1, 2, 5 x 10^n) que no queda por debajo del pico; 1 si no hay pico valido. */
export function niceMax(peak: number): number {
  if (!Number.isFinite(peak) || peak <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(peak));
  for (const step of [1, 2, 5, 10]) {
    if (step * magnitude >= peak) return step * magnitude;
  }
  return 10 * magnitude;
}

/** Coordenada X de un instante en el dominio (puede salir de 0..width si `t` esta fuera). */
export function xOf(t: number, domain: readonly [number, number], width: number): number {
  const span = domain[1] - domain[0];
  return span <= 0 ? 0 : round(((t - domain[0]) / span) * width);
}

/** Coordenada Y de un valor, acotado al eje. */
export function yOf(value: number, yMax: number, height: number): number {
  const fraction = yMax <= 0 ? 0 : Math.min(1, Math.max(0, value / yMax));
  return round(height - fraction * height);
}

/**
 * Atributo `d` de la serie: un tramo por cada racha de valores presentes; los nulos y los huecos
 * marcados cortan la linea (nunca se interpola sobre lo ausente).
 */
export function linePath(points: readonly SeriesPoint[], options: LineOptions): string {
  const { domain, yMax, width, height } = options;
  let d = "";
  let open = false;
  for (const p of points) {
    if (p.t < domain[0] || p.t > domain[1]) continue;
    if (p.v === null || !Number.isFinite(p.v)) {
      open = false;
      continue;
    }
    const x = xOf(p.t, domain, width);
    const y = yOf(p.v, yMax, height);
    if (open && p.gapBefore !== true) {
      d += `L${x} ${y}`;
    } else {
      d += `M${x} ${y}`;
      open = true;
    }
  }
  return d;
}
