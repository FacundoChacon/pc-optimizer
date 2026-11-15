import type { LargeFileEntry } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { ItemActions } from "./ItemActions";

/** Los archivos mas grandes con tamano y fecha (modo simple). */
export function LargeFileList({
  files,
  blocked,
  onReveal,
}: {
  files: readonly LargeFileEntry[];
  blocked: boolean;
  onReveal: (file: LargeFileEntry) => void;
}) {
  return (
    <ul className="flex flex-col gap-4">
      {files.map((file) => (
        <li key={file.id} className="flex flex-col gap-1">
          <div className="flex flex-wrap items-baseline justify-between gap-3">
            <span className="font-medium">{file.name}</span>
            <span className="text-content-muted">{formatBytes(file.sizeBytes)}</span>
          </div>
          <p className="text-sm text-content-muted">
            {file.modifiedAt === null ? es.disk.noDate : format(es.disk.modified, { date: formatDateTime(file.modifiedAt) })}
          </p>
          <ItemActions
            name={file.name}
            blocked={blocked}
            canReveal
            junkHint={false}
            onReveal={() => onReveal(file)}
          />
        </li>
      ))}
    </ul>
  );
}
