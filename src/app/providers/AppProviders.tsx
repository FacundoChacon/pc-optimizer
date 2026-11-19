import type { ReactNode } from "react";
import { CleanupProvider } from "../../features/cleanup/CleanupProvider";
import { JunkScanProvider } from "../../features/cleanup/JunkScanProvider";
import { DiskProvider } from "../../features/disk/DiskProvider";
import { DuplicatesProvider } from "../../features/duplicates/DuplicatesProvider";
import { MonitorProvider } from "../../features/monitor/MonitorProvider";
import { Announcer } from "./Announcer";
import { AppInfoProvider } from "./AppInfoProvider";
import { NavigationProvider } from "./NavigationProvider";
import { PrivilegeProvider } from "./PrivilegeProvider";
import { SettingsProvider, useSettings } from "./SettingsProvider";
import { StoreStatusProvider } from "./StoreStatusProvider";

/** Tras recuperar la base, la configuracion se vuelve a leer sin reiniciar. */
function StoreStatusWithSettings({ children }: { children: ReactNode }) {
  const { reload } = useSettings();
  return <StoreStatusProvider onRecovered={() => void reload()}>{children}</StoreStatusProvider>;
}

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <Announcer>
      <AppInfoProvider>
        <PrivilegeProvider>
          <SettingsProvider>
            <StoreStatusWithSettings>
              <JunkScanProvider>
                <CleanupProvider>
                  <DuplicatesProvider>
                    <DiskProvider>
                      <MonitorProvider>
                        <NavigationProvider>{children}</NavigationProvider>
                      </MonitorProvider>
                    </DiskProvider>
                  </DuplicatesProvider>
                </CleanupProvider>
              </JunkScanProvider>
            </StoreStatusWithSettings>
          </SettingsProvider>
        </PrivilegeProvider>
      </AppInfoProvider>
    </Announcer>
  );
}
