import { useCallback, useEffect, useRef, useState } from "react";
import {
  type AppError,
  type JunkCategory,
  type JunkFinding,
  listJunkFindings,
  toAppError,
} from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { categoryName, skipReasonText } from "./categoryText";

const PAGE_SIZE = 100;

interface FindingsState {
  items: JunkFinding[];
  total: number;
  loading: boolean;
  error: AppError | null;
}

/** Tamano, fecha y estado de un hallazgo en una sola linea (el codigo crudo, solo si fue omitido). */
function findingMeta(finding: JunkFinding): string {
  const reason = skipReasonText(finding.skipReason);
  const parts = [
    formatBytes(finding.sizeBytes),
    ...(finding.modifiedAt === null ? [] : [formatDateTime(finding.modifiedAt)]),
    finding.skipReason === null
      ? reason
      : `${reason} ${format(es.cleanup.reasonCode, { code: finding.skipReason })}`,
  ];
  return parts.join(es.cleanup.metaSeparator);
}

/** Rutas de una categoria (solo modo avanzado): paginadas de a 100, mayor tamano primero. */
export function FindingsList({ scanId, category }: { scanId: number; category: JunkCategory }) {
  const [state, setState] = useState<FindingsState>({ items: [], total: 0, loading: true, error: null });
  const requested = useRef(0);

  const load = useCallback(
    (offset: number) => {
      requested.current = offset;
      setState((current) => ({ ...current, loading: true, error: null }));
      listJunkFindings(scanId, category, offset, PAGE_SIZE)
        .then((page) => {
          if (requested.current !== offset) return;
          setState((current) => ({
            items: offset === 0 ? page.items : [...current.items, ...page.items],
            total: page.total,
            loading: false,
            error: null,
          }));
        })
        .catch((failure: unknown) => {
          setState((current) => ({ ...current, loading: false, error: toAppError(failure) }));
        });
    },
    [scanId, category],
  );

  useEffect(() => {
    load(0);
  }, [load]);

  const { items, total, loading, error } = state;
  return (
    <div className="flex flex-col gap-2">
      {error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(error)}
        </p>
      )}
      {items.length > 0 && (
        <ul
          aria-label={format(es.cleanup.pathsListLabel, { category: categoryName(category) })}
          className="flex flex-col gap-1 text-sm"
        >
          {items.map((finding) => (
            <li key={finding.id} className="flex flex-col gap-0.5 border-b border-border-strong pb-1">
              <span className="break-all font-mono">{finding.path}</span>
              <span className="text-content-muted">{findingMeta(finding)}</span>
            </li>
          ))}
        </ul>
      )}
      {loading && <p className="text-content-muted">{es.cleanup.loadingPaths}</p>}
      {!loading && error === null && items.length === 0 && (
        <p className="text-content-muted">{es.cleanup.pathsEmpty}</p>
      )}
      {items.length > 0 && (
        <p className="text-content-muted">
          {format(es.cleanup.pathsShown, { shown: String(items.length), total: String(total) })}
        </p>
      )}
      {!loading && items.length < total && (
        <div>
          <Button onClick={() => load(items.length)}>{es.cleanup.loadMore}</Button>
        </div>
      )}
    </div>
  );
}
