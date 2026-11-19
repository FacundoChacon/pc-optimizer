import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { RadioGroup } from "../../shared/ui/RadioGroup";
import { pickRejectedText } from "./diskText";
import type { FolderScanApi, RootChoice } from "./useFolderScan";

/**
 * Elegir que analizar (solo modo avanzado): el disco del sistema o una carpeta con el selector de
 * Windows. La interfaz nunca escribe una ruta: el backend valida la eleccion y devuelve un `pickId`.
 */
export function RootPicker({ scan }: { scan: FolderScanApi }) {
  const options = [
    { value: "disk" as const, label: es.disk.rootDisk },
    { value: "folder" as const, label: es.disk.rootFolder },
  ];
  return (
    <div className="flex flex-col gap-3">
      <RadioGroup<RootChoice>
        legend={es.disk.rootLegend}
        name="disk-root"
        value={scan.rootChoice}
        options={options}
        onChange={scan.setRootChoice}
      />
      {scan.rootChoice === "folder" && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap items-center gap-3">
            <Button disabled={scan.picking} onClick={scan.pickFolder}>
              {es.disk.pickButton}
            </Button>
            <span className="text-content-muted">
              {scan.picked === null ? es.disk.pickNone : format(es.disk.pickedPath, { path: scan.picked.path })}
            </span>
          </div>
          {scan.pickError !== null && (
            <p role="alert" className="text-danger">
              {pickRejectedText(scan.pickError)}
            </p>
          )}
          {scan.pickNotice !== null && <p className="text-content-muted">{scan.pickNotice}</p>}
        </div>
      )}
    </div>
  );
}
