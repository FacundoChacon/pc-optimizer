import { useAnnounce } from "../../app/providers/Announcer";
import { useSettings } from "../../app/providers/SettingsProvider";
import { useStoreStatus } from "../../app/providers/StoreStatusProvider";
import type { UiMode } from "../../lib/ipc";
import { errorMessage } from "../../shared/i18n/errors";
import { es } from "../../shared/i18n/es";
import { Card } from "../../shared/ui/Card";
import { RadioGroup, type RadioOption } from "../../shared/ui/RadioGroup";

const MODE_RADIO_NAME = "ui-mode";

const OPTIONS: readonly RadioOption<UiMode>[] = [
  {
    value: "simple",
    label: es.settings.appearance.simple,
    description: es.settings.appearance.simpleDescription,
  },
  {
    value: "advanced",
    label: es.settings.appearance.advanced,
    description: es.settings.appearance.advancedDescription,
  },
];

export function AppearanceSection() {
  const { settings, pending, lastError, setUiMode } = useSettings();
  const { status } = useStoreStatus();
  const announce = useAnnounce();
  const needsRecovery = status !== null && status.status === "unavailable";

  async function choose(mode: UiMode): Promise<void> {
    if (pending || needsRecovery) return;
    const saved = await setUiMode(mode);
    if (!saved) return;
    announce(
      mode === "advanced"
        ? es.settings.appearance.announceAdvanced
        : es.settings.appearance.announceSimple,
    );
    // El layout cambia y se vuelve a montar la seccion: se devuelve el foco a la opcion elegida.
    window.setTimeout(() => {
      document.querySelector<HTMLInputElement>(`input[name="${MODE_RADIO_NAME}"]:checked`)?.focus();
    }, 0);
  }

  return (
    <Card heading={es.settings.appearance.heading}>
      <RadioGroup
        legend={es.settings.appearance.modeLegend}
        name={MODE_RADIO_NAME}
        value={settings.uiMode}
        options={OPTIONS}
        onChange={(mode) => void choose(mode)}
        disabled={pending || needsRecovery}
        {...(needsRecovery ? { disabledReason: es.settings.appearance.needsRecovery } : {})}
      />
      {lastError !== null && (
        <p role="alert" className="text-danger">
          {errorMessage(lastError)}
        </p>
      )}
    </Card>
  );
}
