import { useEffect, useId, useRef } from "react";
import type { FolderEntry, FolderPage, FolderSort } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { focusRing } from "../../shared/ui/focus";
import { accessText, shareText, withCode } from "./diskText";
import { displayName } from "./FolderList";
import { ItemActions } from "./ItemActions";
import { ProportionBar } from "./ProportionBar";

const CELL = "px-2 py-1 align-top";

function SizeCell({ bytes, parentBytes }: { bytes: number; parentBytes: number }) {
  return (
    <td className={CELL}>
      <div className="flex min-w-32 flex-col gap-1">
        <span>{shareText(bytes, parentBytes)}</span>
        <ProportionBar fraction={parentBytes > 0 ? bytes / parentBytes : 0} />
      </div>
    </td>
  );
}

/** Migas de pan de la raiz a la carpeta actual; la ultima es la carpeta en la que se esta. */
function Breadcrumb({
  page,
  onNavigate,
}: {
  page: FolderPage;
  onNavigate: (folderId: number | null) => void;
}) {
  const lastIndex = page.breadcrumb.length - 1;
  const current = useRef<HTMLSpanElement>(null);
  const shownFolder = useRef(page.folder.id);
  // Al entrar en otra carpeta el boton pulsado desaparece: el foco pasa a la miga actual.
  useEffect(() => {
    if (shownFolder.current === page.folder.id) return;
    shownFolder.current = page.folder.id;
    current.current?.focus();
  }, [page.folder.id]);
  return (
    <nav aria-label={es.disk.breadcrumbLabel}>
      <ol className="flex flex-wrap items-center gap-1">
        {page.breadcrumb.map((crumb, index) => (
          <li
            key={crumb.id}
            className="flex items-center gap-1 after:text-content-muted after:content-['/'] last:after:content-none"
          >
            {index === lastIndex ? (
              <span ref={current} tabIndex={-1} aria-current="page" className="font-semibold">
                {crumb.name}
              </span>
            ) : (
              <button
                type="button"
                onClick={() => onNavigate(index === 0 ? null : crumb.id)}
                className={`rounded underline ${focusRing}`}
              >
                {crumb.name}
              </button>
            )}
          </li>
        ))}
      </ol>
    </nav>
  );
}

/** Lista completa de carpetas (modo avanzado): navegable, ordenable y paginada. */
export function FolderTable({
  page,
  items,
  total,
  loading,
  sort,
  blocked,
  onSort,
  onNavigate,
  onLoadMore,
  onReveal,
}: {
  page: FolderPage;
  items: readonly FolderEntry[];
  total: number;
  loading: boolean;
  sort: FolderSort;
  blocked: boolean;
  onSort: (sort: FolderSort) => void;
  onNavigate: (folderId: number | null) => void;
  onLoadMore: () => void;
  onReveal: (folder: FolderEntry) => void;
}) {
  const sortId = useId();
  const parentBytes = page.folder.sizeBytes;
  const section = es.disk.topFoldersTitle;
  const hasMore = items.length < total;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Breadcrumb page={page} onNavigate={onNavigate} />
        <label htmlFor={sortId} className="flex items-center gap-2">
          <span>{es.disk.sortLabel}</span>
          <select
            id={sortId}
            aria-label={format(es.disk.sortFor, { label: es.disk.sortLabel, section })}
            value={sort}
            onChange={(event) => onSort(event.target.value === "name" ? "name" : "size")}
            className={`rounded border border-border-strong bg-surface-raised px-2 py-1 ${focusRing}`}
          >
            <option value="size">{es.disk.sortFolders.size}</option>
            <option value="name">{es.disk.sortFolders.name}</option>
          </select>
        </label>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-left">
          <caption className="sr-only">{section}</caption>
          <thead>
            <tr className="border-b border-border-strong">
              <th scope="col" className={CELL}>
                {es.disk.columns.name}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.size}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.files}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.folders}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.state}
              </th>
              <th scope="col" className={CELL}>
                {es.disk.columns.actions}
              </th>
            </tr>
          </thead>
          <tbody>
            {items.map((folder) => {
              const label = displayName(folder);
              const access = accessText(folder.access);
              return (
                <tr key={folder.id} className="border-b border-border-strong">
                  <th scope="row" className={`${CELL} font-medium`}>
                    {folder.hasChildren && folder.opaque === null ? (
                      <button
                        type="button"
                        aria-label={format(es.disk.open, { name: label })}
                        onClick={() => onNavigate(folder.id)}
                        className={`rounded underline ${focusRing}`}
                      >
                        {label}
                      </button>
                    ) : (
                      label
                    )}
                  </th>
                  <SizeCell bytes={folder.sizeBytes} parentBytes={parentBytes} />
                  <td className={CELL}>{folder.opaque === null ? folder.fileCount : null}</td>
                  <td className={CELL}>{folder.opaque === null ? folder.dirCount : null}</td>
                  <td className={CELL}>
                    <div className="flex flex-col gap-1 text-sm">
                      {folder.system && <span>{es.disk.systemLabel}</span>}
                      {folder.opaque !== null && <span>{es.disk.opaqueHint}</span>}
                      {access !== null && <span>{withCode(access, folder.access)}</span>}
                    </div>
                  </td>
                  <td className={CELL}>
                    <ItemActions
                      name={label}
                      blocked={blocked}
                      canReveal={folder.opaque === null}
                      junkHint={folder.junkHint}
                      onReveal={() => onReveal(folder)}
                    />
                  </td>
                </tr>
              );
            })}
            {page.remainderBytes > 0 && (
              <tr className="border-b border-border-strong text-content-muted">
                <th scope="row" className={`${CELL} font-medium`}>
                  {es.disk.smallerFolders}
                </th>
                <SizeCell bytes={page.remainderBytes} parentBytes={parentBytes} />
                <td className={CELL} />
                <td className={CELL} />
                <td className={CELL} />
                <td className={CELL} />
              </tr>
            )}
            {page.folder.directBytes > 0 && (
              <tr className="border-b border-border-strong text-content-muted">
                <th scope="row" className={`${CELL} font-medium`}>
                  {es.disk.looseFiles}
                </th>
                <SizeCell bytes={page.folder.directBytes} parentBytes={parentBytes} />
                <td className={CELL} />
                <td className={CELL} />
                <td className={CELL} />
                <td className={CELL} />
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {total > 0 && (
        <p className="text-content-muted">
          {format(es.disk.shown, { shown: String(items.length), total: String(total) })}
        </p>
      )}
      {hasMore && (
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
