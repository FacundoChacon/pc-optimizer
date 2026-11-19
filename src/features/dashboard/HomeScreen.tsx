import { useSettings } from "../../app/providers/SettingsProvider";
import { es } from "../../shared/i18n/es";
import { useJunkScan } from "../cleanup/JunkScanProvider";
import { useDisk } from "../disk/DiskProvider";
import { useMonitor } from "../monitor/MonitorProvider";
import { junkCardState, memoryCardState, spaceCardState } from "./cardState";
import { AdvancedSystem } from "./AdvancedSystem";
import { CleanNowAction } from "./CleanNowAction";
import { DiskSummaryBlock } from "./DiskSummaryBlock";
import { JunkCard } from "./JunkCard";
import { JunkTable } from "./JunkTable";
import { MemoryCard } from "./MemoryCard";
import { RecentCleanups } from "./RecentCleanups";
import { SpaceCard } from "./SpaceCard";

/** Inicio (C-12): el estado del equipo y una unica accion primaria, "Limpiar ahora". */
export function HomeScreen() {
  const { settings } = useSettings();
  const monitor = useMonitor();
  const junk = useJunkScan();
  const { scan: disk } = useDisk();
  const advanced = settings.uiMode === "advanced";
  const junkState = junkCardState({ loaded: junk.loaded, last: junk.last, error: junk.error });

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.home.title}</h1>
      <p className="text-content-muted">{es.home.intro}</p>
      <CleanNowAction />
      {advanced ? (
        <>
          <AdvancedSystem monitor={monitor} />
          <JunkTable state={junkState} last={junk.last} />
          <DiskSummaryBlock last={disk.last} />
          <RecentCleanups />
        </>
      ) : (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          <SpaceCard state={spaceCardState(monitor, disk.last)} advanced={false} />
          <JunkCard state={junkState} advanced={false} />
          <MemoryCard state={memoryCardState(monitor)} advanced={false} />
        </div>
      )}
    </div>
  );
}
