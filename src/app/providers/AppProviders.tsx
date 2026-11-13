import type { ReactNode } from "react";
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
              <NavigationProvider>{children}</NavigationProvider>
            </StoreStatusWithSettings>
          </SettingsProvider>
        </PrivilegeProvider>
      </AppInfoProvider>
    </Announcer>
  );
}
