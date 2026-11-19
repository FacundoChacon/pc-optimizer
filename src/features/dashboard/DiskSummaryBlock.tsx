import { useNavigation } from "../../app/providers/NavigationProvider";
import type { FolderScanSummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-wrap gap-2">
      <dt className="text-content-muted">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

/** Modo avanzado: ultimo analisis de disco terminado con la ruta completa de su raiz. */
export function DiskSummaryBlock({ last }: { last: FolderScanSummary | null }) {
  const { navigate } = useNavigation();
  const text = es.home.advanced;
  const done = last !== null && last.status === "done" ? last : null;
  return (
    <Card heading={text.diskTitle}>
      {done === null ? (
        <p className="text-content-muted">{text.diskNone}</p>
      ) : (
        <>
          <dl className="grid gap-1">
            <Row label={text.diskRoot} value={done.root.path} />
            <Row label={es.home.title} value={format(text.diskDate, { date: formatDateTime(done.finishedAt ?? done.startedAt) })} />
            <Row label={text.diskTotal} value={formatBytes(done.totalBytes)} />
            {done.unaccountedBytes !== null && <Row label={text.diskUnaccounted} value={formatBytes(done.unaccountedBytes)} />}
            <Row label={text.diskInaccessible} value={String(done.inaccessibleDirs)} />
            <Row label={text.diskCloud} value={String(done.cloudFiles)} />
          </dl>
        </>
      )}
      <div>
        <Button onClick={() => navigate("disk")}>{text.diskLink}</Button>
      </div>
    </Card>
  );
}
