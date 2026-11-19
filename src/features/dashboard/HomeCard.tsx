import type { ReactNode } from "react";
import type { ScreenId } from "../../app/navigation";
import { useNavigation } from "../../app/providers/NavigationProvider";
import { es } from "../../shared/i18n/es";
import { format } from "../../shared/i18n/format";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { codeText, unavailableNote } from "./homeText";

interface HomeCardProps {
  title: string;
  /** Pantalla a la que lleva "Ver detalle". */
  target: ScreenId;
  children: ReactNode;
}

/** Tarjeta de Inicio: titulo, cuerpo y un enlace secundario "Ver detalle" a su pantalla. */
export function HomeCard({ title, target, children }: HomeCardProps) {
  const { navigate } = useNavigation();
  return (
    <Card heading={title}>
      {children}
      <div>
        <Button aria-label={format(es.home.viewDetailOf, { name: title })} onClick={() => navigate(target)}>
          {es.home.viewDetail}
        </Button>
      </div>
    </Card>
  );
}

/** "No disponible" + explicacion; en modo avanzado agrega el codigo crudo. Nunca un 0 ni un bloque vacio. */
export function Unavailable({ code, note, advanced }: { code: string; note?: string; advanced: boolean }) {
  return (
    <>
      <p className="text-2xl font-semibold text-content-muted">{es.home.unavailable}</p>
      <p className="text-content-muted">{note ?? unavailableNote(code)}</p>
      {advanced && <p className="text-content-muted">{codeText(code)}</p>}
    </>
  );
}

export function Loading() {
  return <p className="text-content-muted">{es.home.loading}</p>;
}
