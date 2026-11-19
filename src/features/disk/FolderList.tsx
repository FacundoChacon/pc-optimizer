import type { FolderEntry } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { accessText, opaqueLabel, shareText } from "./diskText";
import { ItemActions } from "./ItemActions";
import { ProportionBar } from "./ProportionBar";

/** Nombre a mostrar: una carpeta opaca se muestra por su etiqueta, nunca por su nombre. */
export function displayName(folder: FolderEntry): string {
  return folder.opaque === null ? folder.name : opaqueLabel(folder.opaque);
}

/** Lista de carpetas con barra proporcional y el tamano en texto (modo simple). */
export function FolderList({
  folders,
  parentBytes,
  blocked,
  onReveal,
}: {
  folders: readonly FolderEntry[];
  parentBytes: number;
  blocked: boolean;
  onReveal: (folder: FolderEntry) => void;
}) {
  return (
    <ul className="flex flex-col gap-4">
      {folders.map((folder) => {
        const label = displayName(folder);
        const access = accessText(folder.access);
        return (
          <li key={folder.id} className="flex flex-col gap-1">
            <div className="flex flex-wrap items-baseline justify-between gap-3">
              <span className="font-medium">{label}</span>
              <span className="text-content-muted">{shareText(folder.sizeBytes, parentBytes)}</span>
            </div>
            <ProportionBar fraction={parentBytes > 0 ? folder.sizeBytes / parentBytes : 0} />
            {folder.system && (
              <p className="text-sm text-content-muted">
                <span className="font-medium">{es.disk.systemLabel}</span> {es.disk.systemHint}
              </p>
            )}
            {folder.opaque !== null && <p className="text-sm text-content-muted">{es.disk.opaqueHint}</p>}
            {access !== null && <p className="text-sm text-content-muted">{access}</p>}
            <ItemActions
              name={label}
              blocked={blocked}
              canReveal={folder.opaque === null}
              junkHint={folder.junkHint}
              onReveal={() => onReveal(folder)}
            />
          </li>
        );
      })}
    </ul>
  );
}
