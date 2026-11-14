import { useId } from "react";
import type { DuplicateFile, DuplicateGroup } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { Card } from "../../shared/ui/Card";
import { focusRing } from "../../shared/ui/focus";
import { RadioGroup, type RadioOption } from "../../shared/ui/RadioGroup";
import { folderName, groupName } from "./duplicatesText";

interface GroupCardProps {
  group: DuplicateGroup;
  advanced: boolean;
  /** Copias que el usuario eligio conservar (vacio = ninguna elegida). */
  keep: readonly number[];
  onChooseOne: (fileId: number) => void;
  /** Devuelve `false` si no se permitio (era la ultima copia conservada). */
  onToggleKeep: (fileId: number) => boolean;
}

function modifiedText(file: DuplicateFile): string {
  return file.modifiedAt === null
    ? es.duplicates.modifiedUnknown
    : format(es.duplicates.modified, { date: formatDateTime(file.modifiedAt) });
}

function placeText(file: DuplicateFile, advanced: boolean): string {
  return format(es.duplicates.copyPlace, {
    folder: folderName(file.root),
    path: advanced ? file.path : file.relativePath,
  });
}

/**
 * Un grupo de copias identicas. Nunca llega con una copia elegida ni recomendada: las opciones
 * solo muestran datos neutrales (carpeta, ruta y fecha) y "va a la Papelera" aparece despues de
 * que el usuario elige.
 */
export function GroupCard({ group, advanced, keep, onChooseOne, onToggleKeep }: GroupCardProps) {
  const helpId = useId();
  const resolved = group.resolvedAt !== null;
  const meta = format(es.duplicates.groupMeta, {
    count: String(group.files.length),
    size: formatBytes(group.sizeBytes),
    reclaim: formatBytes(group.reclaimableBytes),
  });
  const heading = format(es.duplicates.groupHeading, { name: groupName(group) });

  if (resolved) {
    const kept = group.files.filter((file) => file.kept);
    const where = kept.map((file) => placeText(file, advanced)).join(es.cleanup.metaSeparator);
    return (
      <Card heading={heading} level={3}>
        <p className="text-content-muted">{meta}</p>
        <p>{format(es.duplicates.resolved, { path: where })}</p>
      </Card>
    );
  }

  const decided = keep.length >= 1 && keep.length < group.files.length;
  const options: RadioOption<string>[] = group.files.map((file) => {
    const chosenElsewhere = keep.length > 0 && !keep.includes(file.fileId);
    const description = chosenElsewhere
      ? [modifiedText(file), es.duplicates.goesToTrash].join(es.cleanup.metaSeparator)
      : modifiedText(file);
    return { value: String(file.fileId), label: placeText(file, advanced), description };
  });
  const selected = keep.length === 1 ? String(keep[0]) : "";

  return (
    <Card heading={heading} level={3}>
      <p className="text-content-muted">{meta}</p>
      <p id={helpId} className="text-content-muted">
        {es.duplicates.help}
      </p>
      {advanced && (
        <p className="text-sm text-content-muted">
          <span>{es.duplicates.advanced.hashLabel} </span>
          <code className="break-all font-mono" title={group.blake3}>
            {format(es.duplicates.advanced.hashShort, { hash: group.blake3.slice(0, 16) })}
          </code>
          <span className="sr-only">{group.blake3}</span>
        </p>
      )}
      {advanced ? (
        <fieldset className="flex flex-col gap-1" aria-describedby={helpId}>
          <legend className="mb-1 font-medium">{es.duplicates.question}</legend>
          {group.files.map((file) => {
            const checked = keep.includes(file.fileId);
            const status = checked ? es.duplicates.kept : keep.length > 0 ? es.duplicates.goesToTrash : null;
            return (
              <label key={file.fileId} className="flex items-start gap-3">
                <input
                  type="checkbox"
                  className={`mt-1 h-5 w-5 ${focusRing}`}
                  checked={checked}
                  aria-label={format(es.duplicates.advanced.keepAria, { path: placeText(file, true) })}
                  onChange={() => onToggleKeep(file.fileId)}
                />
                <span className="flex flex-col">
                  <span className="break-all">{placeText(file, true)}</span>
                  <span className="text-sm text-content-muted">
                    {[modifiedText(file), status]
                      .filter((part): part is string => part !== null)
                      .join(es.cleanup.metaSeparator)}
                  </span>
                </span>
              </label>
            );
          })}
        </fieldset>
      ) : (
        <RadioGroup
          legend={es.duplicates.question}
          name={`group-${group.groupId}`}
          value={selected}
          options={options}
          onChange={(value) => onChooseOne(Number(value))}
        />
      )}
      {decided && advanced && keep.length > 0 && (
        <p className="text-sm text-content-muted">{es.duplicates.advanced.keepLastHint}</p>
      )}
    </Card>
  );
}
