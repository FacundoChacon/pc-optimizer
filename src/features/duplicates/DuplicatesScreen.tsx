import { useEffect, useId, useRef } from "react";
import { usePrivilege } from "../../app/providers/PrivilegeProvider";
import { useSettings } from "../../app/providers/SettingsProvider";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { ResultFilters, ScanDetails, ScanOptions } from "./AdvancedPanels";
import { MoveProgressPanel, ScanProgressPanel } from "./DuplicateProgress";
import { DuplicateSummaryCard } from "./DuplicateSummary";
import { useDuplicates } from "./DuplicatesProvider";
import { folderName, rootReasonText } from "./duplicatesText";
import { GroupCard } from "./GroupCard";
import { ReviewDialog } from "./ReviewDialog";

/**
 * Duplicados (C-11): busca archivos repetidos en las carpetas de contenido del usuario y deja que
 * el usuario elija, grupo por grupo, cual conservar. Nunca elige por el usuario ni borra de forma
 * permanente: lo que no se conserva va a la Papelera tras una revision.
 */
export function DuplicatesScreen() {
  const { settings } = useSettings();
  const privilege = usePrivilege();
  const { scan, groups, cleanup } = useDuplicates();
  const advanced = settings.uiMode === "advanced";
  const { last } = scan;
  const scanning = scan.progress !== null;
  const moving = cleanup.progress !== null;
  const busy = scanning || moving;
  const blockedReason =
    privilege.phase === "ready" && privilege.context.status === "mismatch" ? privilege.context.reason : null;
  const scanButton = useRef<HTMLButtonElement>(null);
  const wasScanning = useRef(scanning);
  const reasonId = useId();

  // Al terminar, el boton Cancelar desaparece: el foco pasa a Buscar para no perderse.
  useEffect(() => {
    if (wasScanning.current && !scanning) scanButton.current?.focus();
    wasScanning.current = scanning;
  }, [scanning]);

  const hasOpen = last !== null && last.openGroupCount > 0;
  const canReview = hasOpen && cleanup.copiesToMove > 0 && !busy && cleanup.summary === null;
  const reviewVisible = hasOpen && !moving;
  const reviewLabel =
    cleanup.copiesToMove === 0
      ? es.duplicates.reviewNone
      : cleanup.copiesToMove === 1
        ? es.duplicates.reviewOne
        : format(es.duplicates.review, { count: String(cleanup.copiesToMove) });
  const scanAgain = () => {
    cleanup.dismissSummary();
    scan.start();
  };
  const shown = groups.groups;
  const intro = format(es.duplicates.introTemplate, { size: formatBytes(scan.minSizeBytes) });

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.duplicates.title}</h1>
      {last === null && scan.loaded && !scanning && <p className="text-content-muted">{intro}</p>}
      {advanced && !scanning && blockedReason === null && (
        <ScanOptions
          roots={scan.roots}
          minSizeBytes={scan.minSizeBytes}
          disabled={busy}
          onToggleRoot={scan.toggleRoot}
          onMinSize={scan.setMinSize}
        />
      )}
      {blockedReason !== null ? (
        <Card heading={es.duplicates.blockedTitle}>
          <p>{es.duplicates.blockedReasons[blockedReason]}</p>
        </Card>
      ) : (
        <div className="sticky top-0 z-10 flex flex-wrap items-center gap-3 bg-surface py-2">
          {reviewVisible && (
            <Button
              variant="primary"
              disabled={!canReview}
              aria-describedby={canReview ? undefined : reasonId}
              onClick={cleanup.openReview}
            >
              {reviewLabel}
            </Button>
          )}
          <Button
            ref={scanButton}
            variant={reviewVisible ? "secondary" : "primary"}
            disabled={busy}
            onClick={scanAgain}
          >
            {last === null ? es.duplicates.scan : es.duplicates.scanAgain}
          </Button>
          {last !== null && (
            <span className="text-content-muted">
              {format(es.duplicates.lastScan, { date: formatDateTime(last.finishedAt ?? last.startedAt) })}
            </span>
          )}
        </div>
      )}
      {reviewVisible && !canReview && cleanup.summary === null && (
        <p id={reasonId} className="text-content-muted">
          {es.duplicates.reviewDisabledReason}
        </p>
      )}
      {scan.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(scan.error)}
        </p>
      )}
      {cleanup.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(cleanup.error)}
        </p>
      )}
      {groups.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(groups.error)}
        </p>
      )}
      {scan.notice !== null && <p className="text-content-muted">{scan.notice}</p>}
      {scan.progress !== null && <ScanProgressPanel progress={scan.progress} onCancel={scan.cancel} />}
      {cleanup.progress !== null && <MoveProgressPanel progress={cleanup.progress} onCancel={cleanup.cancel} />}
      {cleanup.summary !== null && !moving && (
        <DuplicateSummaryCard
          summary={cleanup.summary}
          advanced={advanced}
          focusRequested={cleanup.focusSummary}
          onFocused={cleanup.consumeSummaryFocus}
          onScanAgain={scanAgain}
        />
      )}
      {last !== null && (
        <>
          <Card heading={es.duplicates.resultTitle}>
            <p className="text-xl font-semibold">
              {format(last.groupCount === 1 ? es.duplicates.resultSummaryOne : es.duplicates.resultSummary, {
                count: String(last.groupCount),
                size: formatBytes(last.reclaimableBytes),
              })}
            </p>
            {last.groupCount === 0 && <p className="text-content-muted">{es.duplicates.noGroups}</p>}
            {last.groupCount > 0 && last.openGroupCount === 0 && (
              <p className="text-content-muted">{es.duplicates.allResolved}</p>
            )}
            <SkippedLines summary={last} />
          </Card>
          {advanced && <ScanDetails summary={last} />}
          {advanced && last.groupCount > 0 && (
            <ResultFilters filters={groups.filters} onChange={groups.setFilters} />
          )}
          <div className="flex flex-col gap-4">
            {shown.map((group) => (
              <GroupCard
                key={group.groupId}
                group={group}
                advanced={advanced}
                keep={cleanup.choices[group.groupId] ?? []}
                onChooseOne={(fileId) => cleanup.chooseOne(group.groupId, fileId)}
                onToggleKeep={(fileId) => cleanup.toggleKeep(group.groupId, fileId)}
              />
            ))}
          </div>
          {groups.loading && <p className="text-content-muted">{es.duplicates.loadingGroups}</p>}
          {groups.total > 0 && (
            <p className="text-content-muted">
              {format(es.duplicates.shown, { shown: String(shown.length), total: String(groups.total) })}
            </p>
          )}
          {shown.length < groups.total && (
            <div>
              <Button disabled={groups.loading} onClick={groups.loadMore}>
                {es.duplicates.loadMore}
              </Button>
            </div>
          )}
        </>
      )}
      <ReviewDialog
        open={cleanup.dialogOpen}
        preview={cleanup.preview}
        loading={cleanup.loadingPreview}
        busy={cleanup.confirming}
        notice={cleanup.notice}
        onConfirm={cleanup.confirm}
        onCancel={cleanup.closeReview}
      />
    </div>
  );
}

/** Lo que no se miro, en lenguaje llano: en la nube, protegido, enlaces, cambiados y carpetas no disponibles. */
function SkippedLines({ summary }: { summary: NonNullable<ReturnType<typeof useDuplicates>["scan"]["last"]> }) {
  const lines: Array<[number, string]> = [
    [summary.cloudCount, es.duplicates.skippedCloud],
    [summary.protectedCount, es.duplicates.skippedProtected],
    [summary.hardLinkedCount, es.duplicates.skippedLinks],
    [summary.changedCount, es.duplicates.skippedChanged],
    [summary.unreadableCount, es.duplicates.skippedUnreadable],
    [summary.inaccessibleDirs, es.duplicates.skippedFolders],
  ];
  const unavailable = summary.roots.filter((root) => root.status === "unavailable");
  return (
    <>
      {lines
        .filter(([count]) => count > 0)
        .map(([count, template]) => (
          <p key={template} className="text-content-muted">
            {format(template, { count: String(count) })}
          </p>
        ))}
      {unavailable.map((root) => (
        <p key={root.root} className="text-content-muted">
          {format(es.duplicates.unavailableFolder, {
            folder: folderName(root.root),
            reason: rootReasonText(root.reason),
          })}
        </p>
      ))}
    </>
  );
}
