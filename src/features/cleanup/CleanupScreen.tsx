import { useEffect, useRef } from "react";
import { useSettings } from "../../app/providers/SettingsProvider";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { CategoryCard } from "./CategoryCard";
import { useCleanup } from "./CleanupProvider";
import { CleanupPreviewDialog } from "./CleanupPreviewDialog";
import { CleanupProgress } from "./CleanupProgress";
import { CleanupSummaryCard } from "./CleanupSummary";
import { isCleanable } from "./cleanupText";
import { useJunkScan } from "./JunkScanProvider";
import { ScanProgress } from "./ScanProgress";

/**
 * Limpieza (C-08 + C-09): busca basura y, con un preview que ES la confirmacion, mueve a la
 * Papelera lo que el usuario eligio. Nunca borra de forma permanente.
 */
export function CleanupScreen() {
  const { settings } = useSettings();
  const { loaded, last, progress, error, notice, start, cancel } = useJunkScan();
  const cleaning = useCleanup();
  const advanced = settings.uiMode === "advanced";
  const scanning = progress !== null;
  const cleanRunning = cleaning.progress !== null;
  const scanButton = useRef<HTMLButtonElement>(null);
  const wasScanning = useRef(scanning);

  // Al terminar, el boton Cancelar desaparece: el foco pasa a Buscar basura para no perderse.
  useEffect(() => {
    if (wasScanning.current && !scanning) scanButton.current?.focus();
    wasScanning.current = scanning;
  }, [scanning]);

  const hasEligible =
    last !== null &&
    last.categories.some(
      (category) =>
        isCleanable(category.category) &&
        category.status !== "blocked" &&
        category.status !== "unavailable" &&
        category.itemCount > 0,
    );
  const canClean = last !== null && hasEligible && last.cleanedAt === null && !scanning && !cleanRunning;
  const alreadyCleaned = last !== null && last.cleanedAt !== null && cleaning.summary === null;
  const showSummary = cleaning.summary !== null && !cleanRunning;
  const showEmpty = loaded && last === null && !scanning;

  const scanAgain = () => {
    cleaning.reset();
    start();
  };

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.cleanup.title}</h1>
      <div className="flex flex-wrap items-center gap-3">
        {canClean && (
          <Button variant="primary" onClick={cleaning.open}>
            {es.cleanup.clean}
          </Button>
        )}
        <Button
          ref={scanButton}
          variant={canClean || showSummary ? "secondary" : "primary"}
          disabled={scanning || cleanRunning}
          onClick={scanAgain}
        >
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
      {cleaning.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(cleaning.error)}
        </p>
      )}
      {notice !== null && <p className="text-content-muted">{notice}</p>}
      {alreadyCleaned && last.cleanedAt !== null && (
        <p className="text-content-muted">
          {format(es.cleanup.alreadyCleaned, { date: formatDateTime(last.cleanedAt) })}
        </p>
      )}
      {scanning && <ScanProgress progress={progress} onCancel={cancel} />}
      {cleaning.progress !== null && <CleanupProgress progress={cleaning.progress} onCancel={cleaning.cancel} />}
      {showSummary && cleaning.summary !== null && (
        <CleanupSummaryCard
          summary={cleaning.summary}
          advanced={advanced}
          canClean={canClean}
          focusRequested={cleaning.focusSummary}
          onFocused={cleaning.consumeSummaryFocus}
          onScanAgain={scanAgain}
        />
      )}
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
      <CleanupPreviewDialog
        open={cleaning.dialogOpen}
        preview={cleaning.preview}
        loading={cleaning.loadingPreview}
        busy={cleaning.confirming}
        selected={cleaning.selected}
        advanced={advanced}
        notice={cleaning.notice}
        onToggle={cleaning.toggle}
        onConfirm={cleaning.confirm}
        onCancel={cleaning.closeDialog}
      />
    </div>
  );
}
