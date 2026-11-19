import type { ReactNode } from "react";
import { es } from "../../shared/i18n/es";
import { format, formatBytes, formatDateTime } from "../../shared/i18n/format";
import { LevelBadge } from "../monitor/LevelBadge";
import { spaceLevelText } from "../monitor/monitorText";
import type { SpaceCardState } from "./cardState";
import { HomeCard, Loading, Unavailable } from "./HomeCard";

/** Espacio libre del volumen del sistema: del monitor o, si no esta, del ultimo analisis de disco. */
export function SpaceCard({ state, advanced }: { state: SpaceCardState; advanced: boolean }) {
  let body: ReactNode;
  if (state.kind === "loading") {
    body = <Loading />;
  } else if (state.kind === "unavailable") {
    body = <Unavailable code={state.code} note={es.home.cards.space.noData} advanced={advanced} />;
  } else {
    const params = { free: formatBytes(state.freeBytes), total: formatBytes(state.totalBytes) };
    body = (
      <>
        <p className="text-2xl font-semibold">
          {state.analyzedAt === null
            ? format(es.home.cards.space.value, params)
            : format(es.home.cards.space.fallback, { ...params, date: formatDateTime(state.analyzedAt) })}
        </p>
        {state.level !== null && (
          <p>
            <LevelBadge level={state.level === "almost_full" ? "high" : "normal"} text={spaceLevelText(state.level)} />
          </p>
        )}
        {state.stale && <p className="text-warning">{es.home.noRecent}</p>}
      </>
    );
  }
  return (
    <HomeCard title={es.home.cards.space.title} target="disk">
      {body}
    </HomeCard>
  );
}
