import { useState } from "react";
import type { CategorySummary } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { StatusBadge } from "../../shared/ui/StatusBadge";
import { categoryName, categoryStatusText } from "./categoryText";
import { FindingsList } from "./FindingsList";

interface CategoryCardProps {
  summary: CategorySummary;
  scanId: number;
  advanced: boolean;
}

/** Una categoria: estado (icono + texto), tamano y, en modo avanzado, sus rutas. */
export function CategoryCard({ summary, scanId, advanced }: CategoryCardProps) {
  const [showPaths, setShowPaths] = useState(false);
  const text = categoryStatusText(summary);
  const isBin = summary.category === "recycle_bin";
  const hasPaths = advanced && text.showSize && !isBin && summary.itemCount + summary.skippedCount > 0;
  return (
    <Card heading={categoryName(summary.category)} level={3}>
      <StatusBadge tone={text.tone}>{text.label}</StatusBadge>
      {text.note !== undefined && <p className="text-content-muted">{text.note}</p>}
      {text.showSize && <p className="text-xl font-semibold">{formatBytes(summary.sizeBytes)}</p>}
      {text.showSize && isBin && (
        <p className="text-content-muted">
          {format(es.cleanup.recycleBinItems, { count: String(summary.itemCount) })}
        </p>
      )}
      {text.showSize && !isBin && advanced && (
        <p className="text-content-muted">
          {format(es.cleanup.itemsLine, { count: String(summary.itemCount) })}
        </p>
      )}
      {advanced && summary.skippedCount > 0 && (
        <p className="text-content-muted">
          {format(es.cleanup.skippedCount, { count: String(summary.skippedCount) })}
        </p>
      )}
      {advanced && summary.inaccessibleCount > 0 && (
        <p className="text-content-muted">
          {format(es.cleanup.inaccessibleCount, { count: String(summary.inaccessibleCount) })}
        </p>
      )}
      {hasPaths && (
        <div>
          <Button aria-expanded={showPaths} onClick={() => setShowPaths((open) => !open)}>
            {showPaths ? es.cleanup.hidePaths : es.cleanup.viewPaths}
          </Button>
        </div>
      )}
      {hasPaths && showPaths && <FindingsList scanId={scanId} category={summary.category} />}
    </Card>
  );
}
