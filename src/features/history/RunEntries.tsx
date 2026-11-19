import type { CleanupEntry, CleanupRunKey } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { entryCategoryText, entryDetailText, outcomeText } from "./historyText";
import { useRunEntries } from "./useRunEntries";

const text = es.history.advanced;

function withCode(label: string, code: string): string {
  return format(text.codeLabel, { text: label, code });
}

function detailCell(entry: CleanupEntry): string {
  if (entry.errorDetail === null) return text.noDetail;
  const label = entryDetailText(entry.outcome, entry.errorDetail);
  return withCode(label ?? entry.errorDetail, entry.errorDetail);
}

/** Tabla paginada (de a 100) de los items de una limpieza; solo existe mientras la limpieza esta desplegada. */
export function RunEntries({ runKey }: { runKey: CleanupRunKey }) {
  const entries = useRunEntries(runKey);
  return (
    <div className="flex flex-col gap-2">
      {entries.pathsHidden && <p className="text-warning">{text.pathsHiddenNotice}</p>}
      {entries.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(entries.error)}
        </p>
      )}
      {entries.loaded && entries.items.length > 0 && (
        <div className="overflow-x-auto">
          <table aria-label={text.entriesLabel} className="w-full text-left text-sm">
            <thead>
              <tr>
                <th scope="col" className="pr-3 font-semibold">{text.columns.time}</th>
                <th scope="col" className="pr-3 font-semibold">{text.columns.category}</th>
                <th scope="col" className="pr-3 font-semibold">{text.columns.path}</th>
                <th scope="col" className="pr-3 text-right font-semibold">{text.columns.size}</th>
                <th scope="col" className="pr-3 font-semibold">{text.columns.result}</th>
                <th scope="col" className="font-semibold">{text.columns.detail}</th>
              </tr>
            </thead>
            <tbody>
              {entries.items.map((entry) => (
                <tr key={entry.id} className="border-t border-border-strong align-top">
                  <td className="pr-3 py-1">{formatDateTime(entry.ts)}</td>
                  <td className="pr-3">{withCode(entryCategoryText(entry.category), entry.category)}</td>
                  <td className="break-all pr-3">{entry.path ?? text.pathHidden}</td>
                  <td className="pr-3 text-right">{formatBytes(entry.sizeBytes)}</td>
                  <td className="pr-3">{withCode(outcomeText(entry.outcome), entry.outcome)}</td>
                  <td>{detailCell(entry)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {entries.loaded && entries.items.length > 0 && (
        <p className="text-content-muted">
          {format(text.entriesCount, { shown: String(entries.items.length), total: String(entries.total) })}
        </p>
      )}
      {entries.items.length < entries.total && (
        <div>
          <Button disabled={entries.loading} onClick={entries.loadMore}>
            {text.loadMoreEntries}
          </Button>
        </div>
      )}
    </div>
  );
}
