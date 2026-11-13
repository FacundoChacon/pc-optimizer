import { es } from "../../shared/i18n/es";
import { useDensity } from "../../shared/ui/density";
import { focusRing } from "../../shared/ui/focus";
import { Icon } from "../../shared/ui/Icon";
import { SCREENS } from "../navigation";
import { useNavigation } from "../providers/NavigationProvider";

/** Entradas del registro de pantallas; compartida por ambos layouts. */
export function NavList({ orientation }: { orientation: "horizontal" | "vertical" }) {
  const { current, navigate } = useNavigation();
  const density = useDensity();
  const direction = orientation === "vertical" ? "flex-col" : "flex-row";
  const padding = density === "advanced" ? "px-2 py-1 text-sm" : "px-4 py-2";
  return (
    <nav aria-label={es.nav.label}>
      <ul className={`flex gap-1 ${direction}`}>
        {SCREENS.map((screen) => (
          <li key={screen.id}>
            <button
              type="button"
              aria-current={screen.id === current ? "page" : undefined}
              onClick={() => navigate(screen.id)}
              className={`flex w-full items-center gap-2 rounded text-left aria-[current=page]:bg-surface-raised aria-[current=page]:font-semibold motion-safe:transition-colors ${padding} ${focusRing}`}
            >
              <Icon name={screen.icon} />
              <span>{screen.label}</span>
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
