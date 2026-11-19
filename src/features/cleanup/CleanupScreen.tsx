import { useEffect, useRef } from "react";
import { usePrivilege } from "../../app/providers/PrivilegeProvider";
import { useSettings } from "../../app/providers/SettingsProvider";
import type { CategorySummary } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { CategoryCard } from "./CategoryCard";
import { useAnnounce } from "../../app/providers/Announcer";
import { hasEligibleItems } from "../dashboard/cleanNowPlan";
import { useCleanNow, useCleanup, useEmptyRecycleBin } from "./CleanupProvider";
import { CleanupPreviewDialog } from "./CleanupPreviewDialog";
import { CleanupProgress } from "./CleanupProgress";
import { CleanupSummaryCard } from "./CleanupSummary";
import { EmptyRecycleBinDialog } from "./EmptyRecycleBinDialog";
import { EmptyRecycleBinResult } from "./EmptyRecycleBinResult";
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
  const cleanNow = useCleanNow();
  const announce = useAnnounce();
  const emptying = useEmptyRecycleBin();
  const privilege = usePrivilege();
  const advanced = settings.uiMode === "advanced";
  const scanning = progress !== null;
  const cleanRunning = cleaning.progress !== null;
  // Medir o vaciar la Papelera ocupa la pantalla: nada mas puede arrancar hasta que termine.
  const binBusy = emptying.phase === "preparing" || emptying.phase === "emptying";
  const scanButton = useRef<HTMLButtonElement>(null);
  const cleanButton = useRef<HTMLButtonElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const wasScanning = useRef(scanning);
  // "Limpiar ahora" (D-02/D-03): busqueda iniciada por el pedido y el escaneo previo, para saber si es nuevo.
  const awaitingScan = useRef<{ previousScanId: number | null; seenRunning: boolean } | null>(null);

  // Al terminar, el boton Cancelar desaparece: el foco pasa a Buscar basura para no perderse.
  useEffect(() => {
    if (wasScanning.current && !scanning) scanButton.current?.focus();
    wasScanning.current = scanning;
  }, [scanning]);

  const hasEligible = hasEligibleItems(last);
  const canClean =
    last !== null && hasEligible && last.cleanedAt === null && !scanning && !cleanRunning && !binBusy;
  const alreadyCleaned = last !== null && last.cleanedAt !== null && cleaning.summary === null;
  const showSummary = cleaning.summary !== null && !cleanRunning;
  const showEmpty = loaded && last === null && !scanning;

  const scanAgain = () => {
    cleaning.reset();
    emptying.dismiss();
    start();
  };

  // Pedido "Limpiar ahora" de Inicio: se consume al montar. Nunca ejecuta una limpieza: solo abre el preview.
  useEffect(() => {
    const plan = cleanNow.consume();
    if (plan === "preview") {
      cleaning.open();
    } else if (plan === "scan_then_preview") {
      awaitingScan.current = { previousScanId: last?.scanId ?? null, seenRunning: false };
      cleaning.reset();
      emptying.dismiss();
      start();
      // El boton de Inicio ya no existe y la navegacion no movio el foco: queda en el encabezado.
      titleRef.current?.focus();
    }
    // Al salir de Limpieza el pedido pendiente y la espera del escaneo se descartan.
    return () => {
      cleanNow.drop();
      awaitingScan.current = null;
    };
    // Solo al montar: el pedido se consume una vez.
  }, []);

  // Fin de la busqueda pedida: con un escaneo nuevo `done` y elegibles se abre el preview; si no, solo se anuncia.
  useEffect(() => {
    const waiting = awaitingScan.current;
    if (waiting === null) return;
    if (scanning) {
      waiting.seenRunning = true;
      return;
    }
    if (!waiting.seenRunning) return;
    awaitingScan.current = null;
    const fresh = last !== null && last.scanId !== waiting.previousScanId && last.status === "done";
    if (!fresh) return;
    if (hasEligibleItems(last)) cleaning.open();
    else announce(es.cleanup.cleanNowNothing);
  }, [scanning, last, cleaning, announce]);

  // "Vaciar Papelera…" (C-10): solo sin trabajos en curso y fuera de mismatch (la Papelera es por-usuario).
  const privilegeReady = privilege.phase === "ready";
  const binBlocked = privilegeReady && privilege.context.status === "mismatch";
  const canOfferEmpty =
    privilegeReady && !binBlocked && !scanning && !cleanRunning && emptying.phase !== "emptying";
  const startEmpty = canOfferEmpty ? emptying.start : undefined;
  const measuredBin = (summary: CategorySummary) =>
    summary.category === "recycle_bin" && emptying.measure !== null
      ? { ...summary, ...emptying.measure }
      : summary;

  return (
    <div className="flex flex-col gap-4">
      <h1 ref={titleRef} tabIndex={-1} className="text-2xl font-semibold">
        {es.cleanup.title}
      </h1>
      <div className="flex flex-wrap items-center gap-3">
        {canClean && (
          <Button ref={cleanButton} variant="primary" onClick={cleaning.open}>
            {es.cleanup.clean}
          </Button>
        )}
        <Button
          ref={scanButton}
          variant={canClean || showSummary ? "secondary" : "primary"}
          disabled={scanning || cleanRunning || binBusy}
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
      <EmptyRecycleBinResult
        phase={emptying.phase}
        result={emptying.result}
        error={emptying.error}
        focusRequested={emptying.focusResult}
        onFocused={emptying.consumeResultFocus}
        onReviewAgain={emptying.start}
        onDismiss={emptying.dismiss}
      />
      {showSummary && cleaning.summary !== null && (
        <CleanupSummaryCard
          summary={cleaning.summary}
          advanced={advanced}
          canClean={canClean}
          focusRequested={cleaning.focusSummary}
          onFocused={cleaning.consumeSummaryFocus}
          onScanAgain={scanAgain}
          onEmptyBin={startEmpty}
          binBlocked={binBlocked}
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
            {last.categories.map((original) => {
              const summary = measuredBin(original);
              const offer =
                summary.category === "recycle_bin" && summary.status === "done" && summary.itemCount > 0;
              return (
                <CategoryCard
                  key={summary.category}
                  summary={summary}
                  scanId={last.scanId}
                  advanced={advanced}
                  onEmptyBin={offer ? startEmpty : undefined}
                />
              );
            })}
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
        fallbackFocus={() => cleanButton.current ?? titleRef.current}
      />
      <EmptyRecycleBinDialog
        phase={emptying.phase}
        request={emptying.request}
        ack={emptying.ack}
        onContinue={emptying.acknowledge}
        onConfirm={emptying.confirm}
        onCancel={emptying.cancel}
      />
    </div>
  );
}
