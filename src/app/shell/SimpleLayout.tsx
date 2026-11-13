import type { ReactNode } from "react";
import { AppIdentity } from "./AppIdentity";
import { NavList } from "./NavList";
import { SkipLink } from "./SkipLink";

export function SimpleLayout({ children }: { children: ReactNode }) {
  return (
    <div data-density="simple" className="min-h-screen bg-surface text-content">
      <SkipLink />
      <header className="flex flex-wrap items-center gap-6 border-b border-border-strong px-8 py-4">
        <AppIdentity />
        <NavList orientation="horizontal" />
      </header>
      <main id="main" tabIndex={-1} className="flex flex-col gap-6 p-8 focus:outline-none">
        {children}
      </main>
    </div>
  );
}
