import { useEffect, useId, useRef } from "react";
import { useSettings } from "../../app/providers/SettingsProvider";
import type { AppError, FolderScanSummary } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { DiskDetails } from "./DiskDetails";
import { DiskProgressPanel } from "./DiskProgress";
import { useDisk } from "./DiskProvider";
import { DiskSummaryCard } from "./DiskSummary";
import { revealUnavailableText } from "./diskText";
import { FileTable } from "./FileTable";
import { FolderList } from "./FolderList";
import { FolderTable } from "./FolderTable";
import { RevealBlockedNote, useRevealBlocked } from "./ItemActions";
import { LargeFileList } from "./LargeFileList";
import { RootPicker } from "./RootPicker";

function revealErrorText(error: AppError): string {
  return error.kind === "reveal_unavailable" ? revealUnavailableText(error) : errorMessage(error);
}

/** Ruta comparable: sin prefijo `\\?\`, sin barras finales y sin distinguir mayusculas (Windows). */
function normalizePath(path: string): string {
  return path
    .replace(/^\\\\\?\\/, "")
    .replace(/[\\/]+$/, "")
    .toLowerCase();
}

/** La raiz elegida es la del ultimo analisis: solo entonces se ofrece "Analizar de nuevo". */
function sameRoot(last: FolderScanSummary | null, folderMode: boolean, pickedPath: string | null): boolean {
  if (last === null) return false;
  if (!folderMode) return last.root.kind === "system_drive";
  return (
    last.root.kind === "picked" && pickedPath !== null && normalizePath(last.root.path) === normalizePath(pickedPath)
  );
}

/**
 * Espacio en disco (C-07): que carpetas y archivos ocupan el disco. Solo lee: nunca borra ni
 * mueve nada; revisar basura es ir a la pantalla Limpieza.
 */
export function DiskScreen() {
  const { settings } = useSettings();
  const { scan, folders, files, reveal } = useDisk();
  const blocked = useRevealBlocked();
  const advanced = settings.uiMode === "advanced";
  const { last } = scan;
  const scanning = scan.progress !== null;
  const scanButton = useRef<HTMLButtonElement>(null);
  const wasScanning = useRef(scanning);
  const reasonId = useId();

  // Al terminar, el boton Cancelar desaparece: el foco pasa a Analizar para no perderse.
  useEffect(() => {
    if (wasScanning.current && !scanning) scanButton.current?.focus();
    wasScanning.current = scanning;
  }, [scanning]);

  const scanId = last?.scanId ?? 0;
  const parentBytes = folders.page?.folder.sizeBytes ?? 0;
  const folderMode = advanced && scan.rootChoice === "folder";
  const scanLabel = sameRoot(last, folderMode, scan.picked?.path ?? null)
    ? es.disk.scanAgain
    : folderMode
      ? es.disk.scanFolder
      : es.disk.scanDisk;

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.disk.title}</h1>
      {last === null && scan.loaded && !scanning && <p className="text-content-muted">{es.disk.intro}</p>}
      {advanced && !scanning && <RootPicker scan={scan} />}
      {!scanning && (
        <div className="flex flex-wrap items-center gap-3">
          <Button
            ref={scanButton}
            variant={last === null ? "primary" : "secondary"}
            disabled={!scan.canStart}
            aria-describedby={scan.canStart ? undefined : reasonId}
            onClick={scan.start}
          >
            {scanLabel}
          </Button>
          {!scan.canStart && (
            <span id={reasonId} className="text-content-muted">
              {es.disk.scanDisabledReason}
            </span>
          )}
        </div>
      )}
      {scan.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(scan.error)}
        </p>
      )}
      {folders.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(folders.error)}
        </p>
      )}
      {files.error !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(files.error)}
        </p>
      )}
      {reveal.error !== null && (
        <p role="alert" className="text-danger">
          {revealErrorText(reveal.error)}
        </p>
      )}
      {scan.notice !== null && <p className="text-content-muted">{scan.notice}</p>}
      {scan.progress !== null && <DiskProgressPanel progress={scan.progress} onCancel={scan.cancel} />}
      {last !== null && (
        <>
          <DiskSummaryCard summary={last} />
          <Card heading={es.disk.topFoldersTitle}>
            {blocked !== null && <RevealBlockedNote reason={blocked} />}
            {folders.page !== null && folders.items.length === 0 && (!advanced || (folders.page.remainderBytes === 0 && folders.page.folder.directBytes === 0)) && (
              <p className="text-content-muted">{es.disk.noFolders}</p>
            )}
            {advanced && folders.page !== null ? (
              <FolderTable
                page={folders.page}
                items={folders.items}
                total={folders.total}
                loading={folders.loading}
                sort={folders.sort}
                blocked={blocked !== null}
                onSort={folders.setSort}
                onNavigate={folders.navigateTo}
                onLoadMore={folders.loadMore}
                onReveal={(folder) => reveal.reveal(scanId, { kind: "folder", id: folder.id })}
              />
            ) : (
              <FolderList
                folders={folders.items}
                parentBytes={parentBytes}
                blocked={blocked !== null}
                onReveal={(folder) => reveal.reveal(scanId, { kind: "folder", id: folder.id })}
              />
            )}
          </Card>
          <Card heading={es.disk.topFilesTitle}>
            {blocked !== null && <RevealBlockedNote reason={blocked} />}
            {files.items.length === 0 && !files.loading && <p className="text-content-muted">{es.disk.noFiles}</p>}
            {advanced ? (
              <FileTable
                files={files.items}
                total={files.total}
                loading={files.loading}
                sort={files.sort}
                blocked={blocked !== null}
                onSort={files.setSort}
                onLoadMore={files.loadMore}
                onReveal={(file) => reveal.reveal(scanId, { kind: "file", id: file.id })}
              />
            ) : (
              <LargeFileList
                files={files.items}
                blocked={blocked !== null}
                onReveal={(file) => reveal.reveal(scanId, { kind: "file", id: file.id })}
              />
            )}
          </Card>
          {advanced && <DiskDetails summary={last} />}
        </>
      )}
    </div>
  );
}
