import { useId } from "react";
import {
  type DuplicateScanSummary,
  MIN_SIZE_OPTIONS,
  ROOT_KEYS,
  type RootKey,
} from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { focusRing } from "../../shared/ui/focus";
import { folderName, rootStatusText } from "./duplicatesText";
import type { GroupFilters } from "./useDuplicateGroups";

const CELL = "border-b border-border-strong px-2 py-1 text-left";
const SELECT = `rounded border border-border-strong bg-surface-raised px-2 py-1 ${focusRing}`;

function sizeLabel(bytes: number): string {
  const key = String(bytes) as keyof typeof es.duplicates.advanced.minSizeOptions;
  return es.duplicates.advanced.minSizeOptions[key];
}

interface ScanOptionsProps {
  roots: readonly RootKey[];
  minSizeBytes: number;
  disabled: boolean;
  onToggleRoot: (root: RootKey) => void;
  onMinSize: (bytes: number) => void;
}

/** Antes de buscar: carpetas (al menos una) y tamano minimo de la lista cerrada. */
export function ScanOptions({ roots, minSizeBytes, disabled, onToggleRoot, onMinSize }: ScanOptionsProps) {
  const selectId = useId();
  const hintId = useId();
  return (
    <div className="flex flex-col gap-3">
      <fieldset className="flex flex-wrap gap-x-4 gap-y-1" aria-describedby={hintId}>
        <legend className="mb-1 font-medium">{es.duplicates.advanced.foldersLegend}</legend>
        {ROOT_KEYS.map((root) => (
          <label key={root} className="flex items-center gap-2">
            <input
              type="checkbox"
              className={`h-5 w-5 ${focusRing}`}
              checked={roots.includes(root)}
              disabled={disabled}
              onChange={() => onToggleRoot(root)}
            />
            <span>{folderName(root)}</span>
          </label>
        ))}
        <p id={hintId} className="w-full text-sm text-content-muted">
          {es.duplicates.advanced.foldersMinimum}
        </p>
      </fieldset>
      <div className="flex items-center gap-2">
        <label htmlFor={selectId}>{es.duplicates.advanced.minSizeLabel}</label>
        <select
          id={selectId}
          className={SELECT}
          value={minSizeBytes}
          disabled={disabled}
          onChange={(event) => onMinSize(Number(event.target.value))}
        >
          {MIN_SIZE_OPTIONS.map((bytes) => (
            <option key={bytes} value={bytes}>
              {sizeLabel(bytes)}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}

interface FiltersProps {
  filters: GroupFilters;
  onChange: (filters: GroupFilters) => void;
}

/** Filtros de resultados: por carpeta y por tamano minimo (se consultan en el backend). */
export function ResultFilters({ filters, onChange }: FiltersProps) {
  const folderId = useId();
  const sizeId = useId();
  return (
    <div className="flex flex-wrap items-center gap-4">
      <div className="flex items-center gap-2">
        <label htmlFor={folderId}>{es.duplicates.advanced.filterFolder}</label>
        <select
          id={folderId}
          className={SELECT}
          value={filters.root ?? ""}
          onChange={(event) => {
            const value = event.target.value;
            const root = ROOT_KEYS.find((key) => key === value) ?? null;
            onChange({ ...filters, root });
          }}
        >
          <option value="">{es.duplicates.advanced.filterFolderAll}</option>
          {ROOT_KEYS.map((root) => (
            <option key={root} value={root}>
              {folderName(root)}
            </option>
          ))}
        </select>
      </div>
      <div className="flex items-center gap-2">
        <label htmlFor={sizeId}>{es.duplicates.advanced.filterMinSize}</label>
        <select
          id={sizeId}
          className={SELECT}
          value={filters.minSizeBytes ?? ""}
          onChange={(event) => {
            const bytes = MIN_SIZE_OPTIONS.find((option) => String(option) === event.target.value) ?? null;
            onChange({ ...filters, minSizeBytes: bytes });
          }}
        >
          <option value="">{es.duplicates.advanced.filterMinSizeAll}</option>
          {MIN_SIZE_OPTIONS.map((bytes) => (
            <option key={bytes} value={bytes}>
              {sizeLabel(bytes)}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}

/** Estado por carpeta (con el codigo crudo) y omitidos por motivo del ultimo escaneo. */
export function ScanDetails({ summary }: { summary: DuplicateScanSummary }) {
  const a = es.duplicates.advanced;
  const skipped: Array<[string, number]> = [
    [a.belowMinSize, summary.belowMinSize],
    [a.cloudCount, summary.cloudCount],
    [a.protectedCount, summary.protectedCount],
    [a.hardLinkedCount, summary.hardLinkedCount],
    [a.changedCount, summary.changedCount],
    [a.unreadableCount, summary.unreadableCount],
    [a.inaccessibleDirs, summary.inaccessibleDirs],
    [a.excludedDirs, summary.excludedDirs],
  ];
  return (
    <div className="flex flex-col gap-4">
      <table className="w-full text-sm">
        <caption className="py-1 text-left font-medium">{a.rootsCaption}</caption>
        <thead>
          <tr>
            <th scope="col" className={CELL}>
              {a.folderColumn}
            </th>
            <th scope="col" className={CELL}>
              {a.statusColumn}
            </th>
            <th scope="col" className={CELL}>
              {a.codeColumn}
            </th>
            <th scope="col" className={CELL}>
              {a.filesColumn}
            </th>
          </tr>
        </thead>
        <tbody>
          {summary.roots.map((root) => (
            <tr key={root.root}>
              <th scope="row" className={CELL}>
                {folderName(root.root)}
              </th>
              <td className={CELL}>
                {rootStatusText(root)}
              </td>
              <td className={`${CELL} font-mono`}>{root.reason ?? root.status}</td>
              <td className={CELL}>{root.filesSeen}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <table className="w-full text-sm">
        <caption className="py-1 text-left font-medium">{a.skippedCaption}</caption>
        <thead>
          <tr>
            <th scope="col" className={CELL}>
              {a.reasonColumn}
            </th>
            <th scope="col" className={CELL}>
              {a.countColumn}
            </th>
          </tr>
        </thead>
        <tbody>
          {skipped.map(([label, count]) => (
            <tr key={label}>
              <th scope="row" className={CELL}>
                {label}
              </th>
              <td className={CELL}>{count}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
