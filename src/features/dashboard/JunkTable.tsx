import type { ReactNode } from "react";
import type { JunkScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";
import { categoryName } from "../cleanup/categoryText";
import type { JunkCardState } from "./cardState";
import { Loading, Unavailable } from "./HomeCard";

const columns = es.home.advanced.junkColumns;

function withCode(text: string, code: string): string {
  return format(es.history.advanced.codeLabel, { text, code });
}

/** Modo avanzado: tabla por categoria del ultimo escaneo, con codigos y conteos (sin rutas por item). */
export function JunkTable({ state, last }: { state: JunkCardState; last: JunkScanSummary | null }) {
  let body: ReactNode;
  if (state.kind === "loading") {
    body = <Loading />;
  } else if (state.kind === "unavailable") {
    body = <Unavailable code={state.code} advanced />;
  } else if (state.kind === "no_analysis" || last === null) {
    body = <p className="text-content-muted">{es.home.cards.junk.none}</p>;
  } else {
    body = (
      <>
        <p className="text-content-muted">
          {format(es.home.advanced.junkScanDate, { date: formatDateTime(last.finishedAt ?? last.startedAt) })}
        </p>
        {last.cleanedAt !== null && (
          <p className="text-content-muted">
            {format(es.home.advanced.junkCleanedAt, { date: formatDateTime(last.cleanedAt) })}
          </p>
        )}
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr>
                <th scope="col" className="pr-3 font-semibold">{columns.category}</th>
                <th scope="col" className="pr-3 font-semibold">{columns.status}</th>
                <th scope="col" className="pr-3 text-right font-semibold">{columns.items}</th>
                <th scope="col" className="pr-3 text-right font-semibold">{columns.size}</th>
                <th scope="col" className="pr-3 text-right font-semibold">{columns.skipped}</th>
                <th scope="col" className="text-right font-semibold">{columns.inaccessible}</th>
              </tr>
            </thead>
            <tbody>
              {last.categories.map((category) => (
                <tr key={category.category} className="border-t border-border-strong">
                  <th scope="row" className="pr-3 py-1 font-normal">
                    {withCode(categoryName(category.category), category.category)}
                  </th>
                  <td className="pr-3">{withCode(es.cleanup.status[category.status], category.status)}</td>
                  <td className="pr-3 text-right">{category.itemCount}</td>
                  <td className="pr-3 text-right">{formatBytes(category.sizeBytes)}</td>
                  <td className="pr-3 text-right">{category.skippedCount}</td>
                  <td className="text-right">{category.inaccessibleCount}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </>
    );
  }
  return <Card heading={es.home.advanced.junkTitle}>{body}</Card>;
}
