import type { ComponentType } from "react";
import { HomeScreen } from "../features/dashboard/HomeScreen";
import { SettingsScreen } from "../features/settings/SettingsScreen";
import { es } from "../shared/i18n/es";
import type { IconName } from "../shared/ui/Icon";

export type ScreenId = "home" | "settings";

export interface ScreenDef {
  id: ScreenId;
  label: string;
  icon: IconName;
  Component: ComponentType;
}

/** Registro de pantallas implementadas: una pantalla nueva = una entrada aqui + su componente. */
export const SCREENS: readonly ScreenDef[] = [
  { id: "home", label: es.nav.home, icon: "home", Component: HomeScreen },
  { id: "settings", label: es.nav.settings, icon: "settings", Component: SettingsScreen },
];

export const DEFAULT_SCREEN: ScreenId = "home";
