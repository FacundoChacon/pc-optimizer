import { useId } from "react";
import type { LargeFileEntry, LargeFileSort } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { focusRing } from "../../shared/ui/focus";
import { ItemActions } from "./ItemActions";

const CELL = "px-2 py-1 align-top";

function isFileSort(value: string): value is LargeFileSort {
  return value === "size" || value === "newest" || value === "oldest";
}

/** Top de archivos grandes (modo avanzado): ruta completa, ordenable por tamano o fecha y paginado. */
export function FileTable({
  files,
  total,
  loading,
  sort,
  blocked,
  onSort,
  onLoadMore,
  onReveal,
}: {
  files: readonly LargeFileEntry[];
  total: number;
  loading: boolean;
  sort: LargeFileSort;
  blocked: boolean;
  onSort: (sort: LargeFileSort) => void;
  onLoadMore: () => void;
  onReveal: (file: LargeFileEntry) => void;
}) {
  const sortId = useId();
  const section = es.disk.topFilesTitle;
  return (
    <div className="flex flex-col gap-3">
      <p className="text-content-muted">{es.disk.topFilesNote}</p>
      <label htmlFor={sortId} className="flex items-center gap-2">
        <span>{es.disk.sortLabel}</span>
        <select
          id={sortId}
          aria-label={format(es.disk.sortFor, { label: es.disk.sortLabel, section })}
          value={sort}
          onChange={(event) => {
            if (isFileSort(event.target.value)) onSort(event.target.value);
          }}
          className={`rounded border border-border-strong bg-surface-raised px-2 py-1 ${focusRing}`}
        >
          <option value="size">{es.disk.sortFiles.size}</option>
          <option value="newest">{es.disk.sortFiles.newest}</option>
          <option value="oldest">{es.disk.sortFiles.oldest}</option>
        </select>
      </label>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-left">
          <caption className="sr-only">{section}</caption>
          <thead>
            <tr className="border-b border-border-strong">
              <th scope="col" className={CELL}>
                {es.disk.columns.name}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.path}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.size}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.modified}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.actions}
              </th>
            </tr>
          </thead>
          <tbody>
            {files.map((file) => (
              <tr key={file.id} className="border-b border-border-strong">
                <th scope="row" className={`${CELL} font-medium`}>
                  {file.name}
                </th>
                <td className={`${CELL} break-all text-content-muted`}>{file.path}</td>
                <td className={CELL}>{formatBytes(file.sizeBytes)}</td>
                <td className={CELL}>{file.modifiedAt === null ? es.disk.noDate : formatDateTime(file.modifiedAt)}</td>
                <td className={CELL}>
                  <ItemActions
                    name={file.name}
                    blocked={blocked}
                    canReveal
                    junkHint={false}
                    onReveal={() => onReveal(file)}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {total > 0 && (
        <p className="text-content-muted">
          {format(es.disk.shown, { shown: String(files.length), total: String(total) })}
        </p>
      )}
      {files.length < total && (
        <div>
          <Button
            disabled={loading}
            aria-label={format(es.disk.loadMoreFor, { label: es.disk.loadMore, section })}
            onClick={onLoadMore}
          >
            {es.disk.loadMore}
          </Button>
        </div>
      )}
    </div>
  );
}
