import type { ReactNode } from "react";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import type { JunkCardState } from "./cardState";
import { HomeCard, Loading, Unavailable } from "./HomeCard";
import { binNoteText } from "./homeText";

function binLine(binBytes: number | null): ReactNode {
  return binBytes === null ? null : <p className="text-content-muted">{binNoteText(binBytes)}</p>;
}

function scanLine(scannedAt: number): ReactNode {
  return (
    <p className="text-content-muted">{format(es.home.cards.junk.scanDate, { date: formatDateTime(scannedAt) })}</p>
  );
}

/** Basura del ultimo escaneo terminado (sin la Papelera, que se menciona aparte). */
export function JunkCard({ state, advanced }: { state: JunkCardState; advanced: boolean }) {
  const text = es.home.cards.junk;
  let body: ReactNode;
  switch (state.kind) {
    case "loading":
      body = <Loading />;
      break;
    case "unavailable":
      body = <Unavailable code={state.code} advanced={advanced} />;
      break;
    case "no_analysis":
      body = <p className="text-2xl font-semibold">{text.none}</p>;
      break;
    case "cleaned":
      body = (
        <>
          <p className="text-lg font-semibold">{format(text.cleaned, { date: formatDateTime(state.cleanedAt) })}</p>
          {binLine(state.binBytes)}
        </>
      );
      break;
    case "nothing":
      body = (
        <>
          <p className="text-lg font-semibold">{text.nothing}</p>
          {scanLine(state.scannedAt)}
          {binLine(state.binBytes)}
        </>
      );
      break;
    case "data":
      body = (
        <>
          <p className="text-2xl font-semibold">{format(text.value, { size: formatBytes(state.bytes) })}</p>
          {scanLine(state.scannedAt)}
          {binLine(state.binBytes)}
        </>
      );
      break;
  }
  return (
    <HomeCard title={text.title} target="cleanup">
      {body}
    </HomeCard>
  );
}
