/** Reemplaza `{nombre}` por `params.nombre`. Un parametro faltante deja el marcador visible. */
export function format(template: string, params: Readonly<Record<string, string>>): string {
  return template.replace(/\{(\w+)\}/g, (marker: string, name: string) =>
    Object.hasOwn(params, name) ? (params[name] ?? marker) : marker,
  );
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

const SIZE_NUMBER = new Intl.NumberFormat("es", { maximumFractionDigits: 1, useGrouping: false });

/** Tamano legible (base 1024, como el Explorador): `1,5 KB`. Un valor invalido se muestra como 0 B. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return `0 ${BYTE_UNITS[0]}`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${SIZE_NUMBER.format(value)} ${BYTE_UNITS[unit]}`;
}

const DATE_TIME = new Intl.DateTimeFormat("es", { dateStyle: "medium", timeStyle: "short" });

/** Fecha y hora locales en espanol a partir de milisegundos desde la epoca Unix. */
export function formatDateTime(epochMillis: number): string {
  return DATE_TIME.format(new Date(epochMillis));
}
