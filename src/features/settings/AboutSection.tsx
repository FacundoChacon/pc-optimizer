import { useAppInfo } from "../../app/providers/AppInfoProvider";
import { es } from "../../shared/i18n/es";
import { Card } from "../../shared/ui/Card";

export function AboutSection() {
  const info = useAppInfo();
  return (
    <Card heading={es.settings.about.heading}>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
        <dt className="text-content-muted">{es.settings.about.nameLabel}</dt>
        <dd>{es.app.title}</dd>
        <dt className="text-content-muted">{es.settings.about.versionLabel}</dt>
        <dd>{info.status === "ready" ? info.info.version : es.app.ipcError}</dd>
      </dl>
    </Card>
  );
}
