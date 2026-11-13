import { createElement, type ReactNode, useId } from "react";
import { useDensity } from "./density";

interface CardProps {
  heading: string;
  level?: 2 | 3 | 4;
  children: ReactNode;
}

export function Card({ heading, level = 2, children }: CardProps) {
  const density = useDensity();
  const headingId = useId();
  const spacing = density === "advanced" ? "gap-2 p-3" : "gap-4 p-6";
  return (
    <section
      aria-labelledby={headingId}
      data-density={density}
      className={`flex flex-col rounded-lg border border-border-strong bg-surface-raised ${spacing}`}
    >
      {createElement(`h${level}`, { id: headingId, className: "text-lg font-semibold" }, heading)}
      {children}
    </section>
  );
}
