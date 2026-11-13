import { es } from "../../shared/i18n/es";
import { AboutSection } from "./AboutSection";
import { AppearanceSection } from "./AppearanceSection";
import { AutostartSection } from "./AutostartSection";
import { DatabaseSection } from "./DatabaseSection";

export function SettingsScreen() {
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">{es.settings.title}</h1>
      <AppearanceSection />
      <AutostartSection />
      <DatabaseSection />
      <AboutSection />
    </div>
  );
}
