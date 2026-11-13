import type { ReactNode } from "react";
import { AppIdentity } from "./AppIdentity";
import { NavList } from "./NavList";
import { SkipLink } from "./SkipLink";

export function AdvancedLayout({ children }: { children: ReactNode }) {
  return (
    <div data-density="advanced" className="min-h-screen bg-surface text-content">
      <SkipLink />
      <div className="flex min-h-screen flex-col md:flex-row">
        <aside className="flex flex-col gap-4 border-b border-border-strong p-3 md:w-56 md:border-b-0 md:border-r">
          <AppIdentity />
          <NavList orientation="vertical" />
        </aside>
        <main id="main" tabIndex={-1} className="flex min-w-0 flex-1 flex-col gap-3 p-4 focus:outline-none">
          {children}
        </main>
      </div>
    </div>
  );
}
