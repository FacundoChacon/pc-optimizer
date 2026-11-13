import { useId } from "react";
import { focusRing } from "./focus";

interface SwitchProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** No disponible: sigue enfocable (aria-disabled), no cambia y explica el motivo. */
  unavailable?: boolean;
  description?: string;
}

export function Switch({ label, checked, onChange, unavailable = false, description }: SwitchProps) {
  const descriptionId = useId();
  const labelId = useId();
  const knob = checked ? "translate-x-5" : "translate-x-0";
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-3">
        <button
          type="button"
          role="switch"
          aria-labelledby={labelId}
          aria-checked={checked}
          aria-disabled={unavailable ? "true" : undefined}
          aria-describedby={description !== undefined ? descriptionId : undefined}
          onClick={() => {
            if (!unavailable) onChange(!checked);
          }}
          className={`relative inline-flex h-6 w-11 items-center rounded-full border border-border-strong aria-disabled:opacity-60 ${
            checked ? "bg-accent" : "bg-surface"
          } ${focusRing}`}
        >
          <span
            aria-hidden="true"
            className={`ml-1 inline-block h-4 w-4 rounded-full bg-content motion-safe:transition-transform ${knob}`}
          />
        </button>
        <span id={labelId}>{label}</span>
      </div>
      {description !== undefined && (
        <p id={descriptionId} className="text-sm text-content-muted">
          {description}
        </p>
      )}
    </div>
  );
}
