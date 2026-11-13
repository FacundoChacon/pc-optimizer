import { useSettings } from "../../app/providers/SettingsProvider";
import { es } from "../../shared/i18n/es";
import { Card } from "../../shared/ui/Card";
import { Switch } from "../../shared/ui/Switch";

/** Visible pero no disponible hasta C-22: nunca llama al backend. */
export function AutostartSection() {
  const { settings } = useSettings();
  return (
    <Card heading={es.settings.autostart.heading}>
      <Switch
        label={es.settings.autostart.label}
        checked={settings.autostartTray}
        onChange={() => {}}
        unavailable
        description={es.settings.autostart.unavailable}
      />
    </Card>
  );
}
