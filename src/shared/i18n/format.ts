/** Reemplaza `{nombre}` por `params.nombre`. Un parametro faltante deja el marcador visible. */
export function format(template: string, params: Readonly<Record<string, string>>): string {
  return template.replace(/\{(\w+)\}/g, (marker: string, name: string) =>
    Object.hasOwn(params, name) ? (params[name] ?? marker) : marker,
  );
}
