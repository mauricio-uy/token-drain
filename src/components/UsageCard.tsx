import "./UsageCard.css";
import { ProviderLogo } from "./ProviderLogo";
import { severityOf } from "./ProviderBadge";
import { formatAge, formatReset, formatResetDate } from "../lib/format";
import type { ProviderView, UsageWindow } from "../lib/usage";
import { formatUsd } from "../lib/money";

const PROVIDER_TITLES: Record<string, string> = {
  claude: "Claude Usage",
  codex: "Codex Usage",
  "opencode-go": "OpenCode Go Usage",
  "opencode-zen": "OpenCode Zen Billing",
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
        {reset && <time className="card-row-reset" dateTime={new Date(window.resetsAt!).toISOString()} title={formatResetDate(window.resetsAt) ?? undefined}>{reset}</time>}
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
 * The contents of the hover card: what the badge's ring cannot say on its own.
 *
 * The rings carry percentages. This carries the windows, what each is called,
 * and when each comes back — which is the part that actually decides whether to
 * keep working or stop.
 *
 * Deliberately renders no surface of its own. The card's background, radius,
 * shadow and tail belong to the persistent container in `Rail`, so they survive
 * a change of provider while only this content cross-fades.
 */
export function UsageCard({ view, now }: { view: ProviderView; now: number }) {
  // Falls back to the last successful snapshot so a failing provider still
  // shows what it knew, clearly labelled, instead of an empty card.
  const usage = view.usage ?? view.lastKnown;
  const showingLastKnown = view.usage === null && view.lastKnown !== null;

  // Stale figures are real, just not current. The age is what decides whether to
  // trust them, so the card says it: "55% used" means something quite different
  // an hour old than three days old.
  const staleAge =
    view.state === "stale" && usage ? formatAge(usage.fetchedAt, now) : null;

  return (
    <div className="card-content">
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
        <p className="card-stale">
          Last known figures{usage.fetchedAt ? ` · ${formatAge(usage.fetchedAt, now)}` : ""}
        </p>
      )}

      {staleAge && <p className="card-stale">Updated {staleAge}</p>}

      {usage?.session && (
        <WindowRow label="5-hour usage" window={usage.session} now={now} />
      )}
      {usage?.weekly && <WindowRow label="7-day usage" window={usage.weekly} now={now} />}
      {usage?.monthly && <WindowRow label="Monthly usage" window={usage.monthly} now={now} />}

      {usage?.billing && <>
        <dl className="card-billing">
          <div><dt>Balance</dt><dd>{formatUsd(usage.billing.balanceUsd)}</dd></div>
          <div><dt>Reported monthly spend</dt><dd>{formatUsd(usage.billing.monthlySpendUsd)}</dd></div>
          <div><dt>Monthly spending limit</dt><dd>{usage.billing.monthlyLimitUsd === null ? "Not set" : formatUsd(usage.billing.monthlyLimitUsd)}</dd></div>
        </dl>
        <p className="card-note">USD · Pay as you go. No 5-hour or 7-day quota.</p>
        <p className="card-note">{usage.billing.spendUpdatedAt
          ? `Spend last updated ${formatResetDate(usage.billing.spendUpdatedAt)}.`
          : "Spend period not reported by the console."}</p>
      </>}

      {!usage?.session && !usage?.weekly && !usage?.monthly && !usage?.billing && !view.remediation && (
        <p className="card-note">
          {/* Nothing has been fetched yet, which is not the same as a provider
              that answered and reported no limits. Saying the latter would be a
              small lie the user has no way to check. */}
          {view.state === "pending" ? "Checking…" : "No limits reported."}
        </p>
      )}

    </div>
  );
}
