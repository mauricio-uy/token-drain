import "./ProviderBadge.css";
import { ProviderLogo } from "./ProviderLogo";
import type { BadgeState, ProviderView } from "../lib/usage";

const RING_SIZE = 56;
const RING_STROKE = 4;
const RING_RADIUS = (RING_SIZE - RING_STROKE) / 2;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

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

/**
 * One provider: a progress ring around its mark, with the session figure below.
 *
 * The ring shows the **session** window, because that is the number that
 * changes while you work. The weekly figure is one hover away in the card.
 */
export function ProviderBadge({
  view,
  active,
  onEnter,
}: {
  view: ProviderView;
  active: boolean;
  onEnter: (provider: string) => void;
}) {
  const session = view.usage?.session ?? null;
  const percent = session?.usedPercent ?? null;

  // A failure has no figure, so it draws an empty track and says what is wrong
  // instead. Rendering 0% here would be indistinguishable from a fresh quota.
  const severity = percent === null ? null : severityOf(percent);
  const offset =
    percent === null
      ? RING_CIRCUMFERENCE
      : RING_CIRCUMFERENCE * (1 - Math.min(Math.max(percent, 0), 100) / 100);

  return (
    <button
      type="button"
      className={`badge badge--${view.state} ${active ? "is-active" : ""}`}
      onMouseEnter={() => onEnter(view.provider)}
      onFocus={() => onEnter(view.provider)}
      aria-label={`${view.provider}: ${percent === null ? view.state : `${Math.round(percent)}% used`}`}
    >
      <span className="badge-ring">
        <svg width={RING_SIZE} height={RING_SIZE} viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}>
          <circle
            className="badge-ring-track"
            cx={RING_SIZE / 2}
            cy={RING_SIZE / 2}
            r={RING_RADIUS}
            strokeWidth={RING_STROKE}
            fill="none"
          />
          {severity && (
            <circle
              className={`badge-ring-value badge-ring-value--${severity}`}
              cx={RING_SIZE / 2}
              cy={RING_SIZE / 2}
              r={RING_RADIUS}
              strokeWidth={RING_STROKE}
              fill="none"
              strokeLinecap="round"
              strokeDasharray={RING_CIRCUMFERENCE}
              strokeDashoffset={offset}
              /* Start the arc at twelve o'clock rather than three, so a nearly
                 empty ring reads as a gauge instead of a stray tick. */
              transform={`rotate(-90 ${RING_SIZE / 2} ${RING_SIZE / 2})`}
            />
          )}
        </svg>
        <span className="badge-mark">
          <ProviderLogo provider={view.provider} size={22} />
        </span>
      </span>
      <span className="badge-label">
        {percent === null ? labelFor(view.state) : `${Math.round(percent)}%`}
      </span>
    </button>
  );
}
