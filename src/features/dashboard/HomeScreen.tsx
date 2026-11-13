import { es } from "../../shared/i18n/es";
import { Card } from "../../shared/ui/Card";

/** Inicio: estado vacio sin accion primaria hasta que existan analisis (C-06/C-12). */
export function HomeScreen() {
  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">{es.home.title}</h1>
      <Card heading={es.home.emptyTitle} level={2}>
        <p className="text-content-muted">{es.home.emptyBody}</p>
      </Card>
    </div>
  );
}
