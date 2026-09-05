import "./UsageCard.css";
import { ProviderLogo } from "./ProviderLogo";
import { severityOf } from "./ProviderBadge";
import { formatReset } from "../lib/format";
import type { ProviderView, UsageWindow } from "../lib/usage";

const PROVIDER_TITLES: Record<string, string> = {
  claude: "Claude Usage",
  codex: "Codex Usage",
};

function titleFor(provider: string): string {
  return PROVIDER_TITLES[provider] ?? `${provider} Usage`;
}

/** One window: its name, how much is gone, and when it comes back. */
function WindowRow({
  label,
  window,
  now,
}: {
  label: string;
  window: UsageWindow;
  now: number;
}) {
  const severity = severityOf(window.usedPercent);
  const reset = formatReset(window.resetsAt, now);

  return (
    <div className="card-row">
      <div className="card-row-head">
        <span className="card-row-label">{label}</span>
        {reset && <span className="card-row-reset">{reset}</span>}
      </div>
      <div className="card-bar">
        <div
          className={`card-bar-fill card-bar-fill--${severity}`}
          style={{ width: `${Math.min(Math.max(window.usedPercent, 0), 100)}%` }}
        />
      </div>
      <div className="card-row-value">{Math.round(window.usedPercent)}% Used</div>
    </div>
  );
}

/**
 * The hover card: what the badge's ring cannot say on its own.
 *
 * The ring carries one number. This carries both windows, what each is called,
 * and when each comes back — which is the part that actually decides whether to
 * keep working or stop.
 */
export function UsageCard({
  view,
  now,
  tailOffset,
}: {
  view: ProviderView;
  now: number;
  /** Distance from the card's top to the centre of the tail, in pixels. */
  tailOffset: number;
}) {
  // Falls back to the last successful snapshot so a failing provider still
  // shows what it knew, clearly labelled, instead of an empty card.
  const usage = view.usage ?? view.lastKnown;
  const showingLastKnown = view.usage === null && view.lastKnown !== null;

  return (
    <div className="card" style={{ "--tail-offset": `${tailOffset}px` } as React.CSSProperties}>
      <div className="card-head">
        <span className="card-head-mark">
          <ProviderLogo provider={view.provider} size={16} />
        </span>
        <span className="card-head-title">{titleFor(view.provider)}</span>
      </div>

      {view.remediation && (
        <p className="card-note">
          {view.remediation.message}
          {view.remediation.command && (
            <code className="card-command">{view.remediation.command}</code>
          )}
        </p>
      )}

      {showingLastKnown && usage && (
        <p className="card-stale">Last known figures</p>
      )}

      {usage?.session && (
        <WindowRow label="Current session" window={usage.session} now={now} />
      )}
      {usage?.weekly && <WindowRow label="All models" window={usage.weekly} now={now} />}

      {!usage?.session && !usage?.weekly && !view.remediation && (
        <p className="card-note">No limits reported.</p>
      )}

      <span className="card-tail" aria-hidden="true" />
    </div>
  );
}
