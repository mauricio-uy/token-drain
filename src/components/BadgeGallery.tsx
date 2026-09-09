import "./BadgeGallery.css";
import { ProviderBadge } from "./ProviderBadge";
import { UsageCard } from "./UsageCard";
import type { BadgeState, ProviderView } from "../lib/usage";

/**
 * Every badge appearance side by side.
 *
 * States that are hard to produce on demand — a rate-limited provider, a
 * changed response contract — otherwise only get looked at the day they happen,
 * which is the worst possible time to discover the label does not fit.
 *
 * Enabled with `VITE_DEBUG_BADGES=1`, so it costs nothing in a normal build.
 */
export const BADGE_GALLERY_ENABLED = import.meta.env.VITE_DEBUG_BADGES === "1";

function sample(
  provider: string,
  state: BadgeState,
  usedPercent: number | null,
  message?: string,
): ProviderView {
  const usage =
    usedPercent === null
      ? null
      : {
          provider,
          session: { usedPercent, windowMinutes: 300, resetsAt: null },
          weekly: { usedPercent: usedPercent / 2, windowMinutes: 10080, resetsAt: null },
          plan: "sample",
          fetchedAt: 0,
        };

  return {
    provider,
    state,
    usage,
    lastKnown: usage,
    remediation: message
      ? { message, command: null, resolvesItself: false }
      : null,
  };
}

const ROWS: { title: string; views: ProviderView[] }[] = [
  {
    title: "levels",
    views: [
      sample("claude", "ok", 0),
      sample("claude", "ok", 27),
      sample("claude", "ok", 73),
      sample("claude", "ok", 100),
    ],
  },
  {
    title: "states",
    views: [
      sample("codex", "pending", null),
      sample("codex", "stale", 64),
      sample("codex", "reauth", null, "Not signed in."),
      sample("codex", "unavailable", null, "Retrying later."),
      sample("codex", "error", null, "Needs an update."),
    ],
  },
];

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/**
 * A card for each reset format.
 *
 * Both branches of `formatReset` are exercised here, because the boundary
 * between them is a design decision rather than an implementation detail: under
 * an hour a countdown is the useful fact, beyond it a wall-clock time is.
 */
function cardSamples(now: number) {
  const soon = sample("claude", "ok", 73);
  if (soon.usage) {
    soon.usage.session = { usedPercent: 73, windowMinutes: 300, resetsAt: now + 51 * MINUTE };
    soon.usage.weekly = { usedPercent: 7, windowMinutes: 10080, resetsAt: now + 50 * HOUR };
  }

  const failing = sample("codex", "reauth", null, "Not signed in to Codex.");
  failing.lastKnown = {
    provider: "codex",
    session: { usedPercent: 48, windowMinutes: 300, resetsAt: now + 3 * HOUR },
    weekly: { usedPercent: 90, windowMinutes: 10080, resetsAt: now + 70 * HOUR },
    plan: "sample",
    fetchedAt: now - 2 * HOUR,
  };
  failing.remediation = { message: "Not signed in to Codex.", command: "codex", resolvesItself: false };

  return [soon, failing];
}

/**
 * A card for every non-ok state.
 *
 * The card has to say more than the badge can: what went wrong, whether the app
 * is already handling it, and the exact command to run when it is not. Seeing
 * them side by side is the only way to catch copy that reads fine alone but
 * contradicts its neighbour.
 */
function stateCards(now: number): ProviderView[] {
  const lastKnown = {
    provider: "claude",
    session: { usedPercent: 55, windowMinutes: 300, resetsAt: now + 2 * HOUR },
    weekly: { usedPercent: 62, windowMinutes: 10080, resetsAt: now + 60 * HOUR },
    plan: "sample",
    fetchedAt: now - 3 * HOUR,
  };

  const failing = (state: BadgeState, message: string, command: string | null, resolvesItself: boolean): ProviderView => ({
    provider: "claude",
    state,
    usage: null,
    lastKnown,
    remediation: { message, command, resolvesItself },
  });

  const stale = sample("claude", "stale", 55);
  if (stale.usage) {
    stale.usage.session = { usedPercent: 55, windowMinutes: 300, resetsAt: now + 2 * HOUR };
    stale.usage.weekly = { usedPercent: 62, windowMinutes: 10080, resetsAt: now + 60 * HOUR };
    // A realistic age. The fixture defaulted to the epoch, which rendered as
    // "updated 20701 days ago" and made the age line look broken.
    stale.usage.fetchedAt = now - 3 * HOUR;
  }

  return [
    { provider: "claude", state: "pending", usage: null, lastKnown: null, remediation: null },
    stale,
    failing("reauth", "Claude rejected the saved login. Sign in again.", "claude", false),
    failing("unavailable", "Could not reach Claude. Retrying later.", null, true),
    failing("error", "Claude changed its usage format. This app needs an update.", null, false),
  ];
}

/** Render local-only fixtures for visually reviewing every badge and card state. */
export function BadgeGallery() {
  const now = Date.now();

  return (
    <div className="gallery">
      <section className="gallery-row">
        <h2 className="gallery-title">state cards</h2>
        <div className="gallery-cards">
          {stateCards(now).map((view, index) => (
            <div key={index} className="gallery-card-labelled">
              <span className="gallery-card-tag">{view.state}</span>
              <div className="card-surface">
                <UsageCard view={view} now={now} />
              </div>
            </div>
          ))}
        </div>
      </section>

      {ROWS.map((row) => (
        <section key={row.title} className="gallery-row">
          <h2 className="gallery-title">{row.title}</h2>
          <div className="gallery-items">
            {row.views.map((view, index) => (
              <ProviderBadge
                key={`${row.title}-${index}`}
                view={view}
                active={false}
                onEnter={() => {}}
              />
            ))}
          </div>
        </section>
      ))}

      <section className="gallery-row">
        <h2 className="gallery-title">cards</h2>
        <div className="gallery-cards">
          {cardSamples(now).map((view, index) => (
            <div key={index} className="card-surface">
              <UsageCard view={view} now={now} />
            </div>
          ))}
        </div>
      </section>

    </div>
  );
}
