import { es } from "../../shared/i18n/es";
import { focusRing } from "../../shared/ui/focus";

/** Primer elemento enfocable: lleva el foco al contenido principal. */
export function SkipLink() {
  return (
    <a
      href="#main"
      onClick={(event) => {
        event.preventDefault();
        document.getElementById("main")?.focus();
      }}
      className={`sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-10 focus:rounded focus:bg-accent focus:px-3 focus:py-2 focus:text-on-accent ${focusRing}`}
    >
      {es.a11y.skipToContent}
    </a>
  );
}
