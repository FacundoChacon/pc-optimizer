import type { AppError } from "../../lib/ipc";
import { es } from "./es";

type ErrorKind = keyof typeof es.errors;

function isKnownKind(kind: string): kind is ErrorKind {
  return Object.hasOwn(es.errors, kind);
}

/** Texto en espanol para un error del backend. El `message` tecnico nunca se muestra. */
export function errorMessage(error: AppError): string {
  return isKnownKind(error.kind) ? es.errors[error.kind] : es.errors.unknown;
}
