import type { ReactNode } from "react";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { LevelBadge } from "../monitor/LevelBadge";
import { levelPhrase, levelText, percentText } from "../monitor/monitorText";
import type { MemoryCardState } from "./cardState";
import { HomeCard, Loading, Unavailable } from "./HomeCard";

/** Memoria en uso: valor actual y nivel del monitor, con la frase llana si esta alta. */
export function MemoryCard({ state, advanced }: { state: MemoryCardState; advanced: boolean }) {
  let body: ReactNode;
  if (state.kind === "loading") {
    body = <Loading />;
  } else if (state.kind === "measuring") {
    body = <p className="text-2xl font-semibold text-content-muted">{es.home.measuring}</p>;
  } else if (state.kind === "unavailable") {
    body = <Unavailable code={state.code} advanced={advanced} />;
  } else {
    const phrase = levelPhrase("memory", state.level);
    body = (
      <>
        <p className="text-2xl font-semibold">
          {state.usedBytes !== null && state.totalBytes !== null
            ? format(es.home.cards.memory.value, {
                pct: percentText(state.pct),
                used: formatBytes(state.usedBytes),
                total: formatBytes(state.totalBytes),
              })
            : percentText(state.pct)}
        </p>
        <p>
          <LevelBadge level={state.level} text={levelText(state.level)} />
        </p>
        {phrase !== null && <p>{phrase}</p>}
        {state.stale && <p className="text-warning">{es.home.noRecent}</p>}
      </>
    );
  }
  return (
    <HomeCard title={es.home.cards.memory.title} target="monitor">
      {body}
    </HomeCard>
  );
}
