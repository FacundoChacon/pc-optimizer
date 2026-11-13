import type { ButtonHTMLAttributes, Ref } from "react";
import { useDensity } from "./density";
import { focusRing } from "./focus";

export type ButtonVariant = "primary" | "secondary" | "danger";

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary: "bg-accent text-on-accent",
  secondary: "border border-border-strong bg-surface-raised text-content",
  danger: "bg-danger text-on-accent",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  ref?: Ref<HTMLButtonElement>;
}

export function Button({ variant = "secondary", type = "button", className, ...rest }: ButtonProps) {
  const density = useDensity();
  const size = density === "advanced" ? "px-2 py-1 text-sm" : "px-4 py-2 text-base";
  const classes = [
    "rounded font-medium motion-safe:transition-colors disabled:opacity-50",
    size,
    VARIANT_CLASSES[variant],
    focusRing,
    className ?? "",
  ]
    .filter((part) => part.length > 0)
    .join(" ");
  return <button {...rest} type={type} data-variant={variant} className={classes} />;
}
