import "./ProviderBadge.css";
import { ProviderLogo } from "./ProviderLogo";
import type { BadgeState, ProviderView } from "../lib/usage";

const RING_SIZE = 64;
const RING_STROKE = 4;

/**
 * Severity of a usage figure.
 *
 * The thresholds are about attention, not arithmetic: below half there is
 * nothing to think about, past 80% a decision is coming, and the band between
 * them is worth noticing without being alarming.
 */
export function severityOf(usedPercent: number): "ok" | "warn" | "high" {
  if (usedPercent >= 80) return "high";
  if (usedPercent >= 50) return "warn";
  return "ok";
}

/** Short text shown in place of a percentage when there is no figure to show. */
function labelFor(state: BadgeState): string {
  switch (state) {
    case "pending":
      return "···";
    case "reauth":
      return "sign in";
    case "unavailable":
      return "offline";
    case "error":
      return "error";
    default:
      return "";
  }
}

function QuotaRing({ radius, percent, window }: {
  radius: number;
  percent: number | null;
  window: "5h" | "7d";
}) {
  return (
    <g data-window={window}>
      <circle className="badge-ring-track" cx="32" cy="32" r={radius}
        strokeWidth={RING_STROKE} fill="none" />
      {percent !== null && percent > 0 && (
        <circle
          className={`badge-ring-value badge-ring-value--${severityOf(percent)}`}
          cx="32" cy="32" r={radius} strokeWidth={RING_STROKE} fill="none"
          strokeLinecap="round" pathLength="100" strokeDasharray="100"
          strokeDashoffset={100 - Math.min(Math.max(percent, 0), 100)}
          transform="rotate(-90 32 32)"
        />
      )}
    </g>
  );
}

function percentLabel(percent: number | null): string {
  return percent === null ? "—" : `${Math.round(percent)}%`;
}

/** Outer ring: five-hour quota. Inner ring: seven-day quota. One shared logo. */
export function ProviderBadge({
  view,
  active,
  onEnter,
  cardId,
}: {
  view: ProviderView;
  active: boolean;
  onEnter: (provider: string) => void;
  cardId?: string;
}) {
  const session = view.usage?.session ?? null;
  const percent = session?.usedPercent ?? null;
  const weeklyPercent = view.usage?.weekly?.usedPercent ?? null;
  const description = view.usage
    ? `5h ${percent === null ? "not reported" : `${Math.round(percent)}% used`}, 7d ${weeklyPercent === null ? "not reported" : `${Math.round(weeklyPercent)}% used`}`
    : view.state;

  return (
    <button
      type="button"
      className={`badge badge--${view.state} ${active ? "is-active" : ""}`}
      onMouseEnter={() => onEnter(view.provider)}
      onFocus={() => onEnter(view.provider)}
      aria-expanded={cardId ? active : undefined}
      aria-controls={cardId}
      aria-label={`${view.provider}: ${description}${view.state === "stale" ? ", stale" : ""}`}
    >
      <span className="badge-ring">
        <svg aria-hidden="true" width={RING_SIZE} height={RING_SIZE} viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}>
          <QuotaRing radius={30} percent={percent} window="5h" />
          <QuotaRing radius={23} percent={weeklyPercent} window="7d" />
        </svg>
        <span className="badge-mark">
          <ProviderLogo provider={view.provider} size="calc(22px * var(--ui-scale))" />
        </span>
      </span>
      {view.usage ? (
        <span className="badge-quotas" aria-hidden="true">
          <span className="badge-quota" title="Outer ring · 5 hours">
            <span className="badge-window-label">5h</span>
            <span className="badge-label">{percentLabel(percent)}</span>
          </span>
          <span className="badge-quota" title="Inner ring · 7 days">
            <span className="badge-window-label">Wk</span>
            <span className="badge-label">{percentLabel(weeklyPercent)}</span>
          </span>
        </span>
      ) : <span className="badge-label badge-status">{labelFor(view.state)}</span>}
    </button>
  );
}
