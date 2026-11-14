import { useEffect, useRef } from "react";
import { useSettings } from "../../app/providers/SettingsProvider";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { CategoryCard } from "./CategoryCard";
import { ScanProgress } from "./ScanProgress";
import { useJunkScan } from "./JunkScanProvider";

/** Limpieza (C-08): busca basura y muestra el resultado. Solo detecta: no borra nada. */
export function CleanupScreen() {
  const { settings } = useSettings();
  const { loaded, last, progress, error, notice, start, cancel } = useJunkScan();
  const advanced = settings.uiMode === "advanced";
  const scanning = progress !== null;
  const scanButton = useRef<HTMLButtonElement>(null);
  const wasScanning = useRef(scanning);

  // Al terminar, el boton Cancelar desaparece: el foco pasa a Buscar basura para no perderse.
  useEffect(() => {
    if (wasScanning.current && !scanning) scanButton.current?.focus();
    wasScanning.current = scanning;
  }, [scanning]);

  const showEmpty = loaded && last === null && !scanning;
  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.cleanup.title}</h1>
      <div className="flex flex-wrap items-center gap-3">
        <Button ref={scanButton} variant="primary" disabled={scanning} onClick={start}>
          {es.cleanup.scan}
        </Button>
        {last !== null && (
          <span className="text-content-muted">
            {format(es.cleanup.lastScan, { date: formatDateTime(last.finishedAt ?? last.startedAt) })}
          </span>
        )}
      </div>
      {error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(error)}
        </p>
      )}
      {notice !== null && <p className="text-content-muted">{notice}</p>}
      {scanning && <ScanProgress progress={progress} onCancel={cancel} />}
      {showEmpty && (
        <Card heading={es.cleanup.emptyTitle}>
          <p className="text-content-muted">{es.cleanup.emptyBody}</p>
        </Card>
      )}
      {last !== null && (
        <>
          <Card heading={es.cleanup.resultsTitle}>
            <p className="text-content-muted">{es.cleanup.totalLabel}</p>
            <p className="text-3xl font-semibold">{formatBytes(last.totalBytes)}</p>
            <p className="text-content-muted">{es.cleanup.inUseNotice}</p>
          </Card>
          <h2 className="text-lg font-semibold">{es.cleanup.categoriesHeading}</h2>
          <div className="grid gap-4 md:grid-cols-2">
            {last.categories.map((summary) => (
              <CategoryCard key={summary.category} summary={summary} scanId={last.scanId} advanced={advanced} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
