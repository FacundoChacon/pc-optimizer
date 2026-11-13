import { useId } from "react";
import { useDensity } from "./density";
import { focusRing } from "./focus";

export interface RadioOption<V extends string> {
  value: V;
  label: string;
  description?: string;
}

interface RadioGroupProps<V extends string> {
  legend: string;
  name: string;
  value: V;
  options: readonly RadioOption<V>[];
  onChange: (value: V) => void;
  disabled?: boolean;
  /** Explicacion asociada (aria-describedby) a cada opcion cuando esta deshabilitado. */
  disabledReason?: string;
}

export function RadioGroup<V extends string>({
  legend,
  name,
  value,
  options,
  onChange,
  disabled = false,
  disabledReason,
}: RadioGroupProps<V>) {
  const density = useDensity();
  const reasonId = useId();
  const gap = density === "advanced" ? "gap-1" : "gap-3";
  const showReason = disabled && disabledReason !== undefined;
  return (
    <fieldset className={`flex flex-col ${gap}`}>
      <legend className="mb-2 font-medium">{legend}</legend>
      {options.map((option) => (
        <label key={option.value} className="flex items-start gap-3">
          <input
            type="radio"
            name={name}
            value={option.value}
            checked={option.value === value}
            disabled={disabled}
            aria-describedby={showReason ? reasonId : undefined}
            onChange={() => onChange(option.value)}
            className={`mt-1 accent-accent ${focusRing}`}
          />
          <span className="flex flex-col">
            <span>{option.label}</span>
            {option.description !== undefined && (
              <span className="text-sm text-content-muted">{option.description}</span>
            )}
          </span>
        </label>
      ))}
      {showReason && (
        <p id={reasonId} className="text-sm text-content-muted">
          {disabledReason}
        </p>
      )}
    </fieldset>
  );
}
