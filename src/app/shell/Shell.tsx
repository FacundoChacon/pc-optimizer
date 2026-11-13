import { es } from "../../shared/i18n/es";
import { DensityProvider } from "../../shared/ui/density";
import { SCREENS } from "../navigation";
import { useNavigation } from "../providers/NavigationProvider";
import { useSettings } from "../providers/SettingsProvider";
import { AdvancedLayout } from "./AdvancedLayout";
import { SimpleLayout } from "./SimpleLayout";
import { PrivilegeBanner } from "./PrivilegeBanner";
import { StoreBanner } from "./StoreBanner";

export function Shell() {
  const { phase, settings } = useSettings();
  const { current } = useNavigation();

  // No se muestra ningun layout hasta conocer el modo (evita el parpadeo simple -> avanzado).
  if (phase === "loading") {
    return (
      <div className="min-h-screen bg-surface p-8 text-content">
        <p className="text-content-muted">{es.app.loading}</p>
      </div>
    );
  }

  const Layout = settings.uiMode === "advanced" ? AdvancedLayout : SimpleLayout;
  const Screen = (SCREENS.find((screen) => screen.id === current) ?? SCREENS[0])?.Component;
  return (
    <DensityProvider value={settings.uiMode}>
      <Layout>
        <PrivilegeBanner />
        <StoreBanner />
        {Screen !== undefined && <Screen />}
      </Layout>
    </DensityProvider>
  );
}
