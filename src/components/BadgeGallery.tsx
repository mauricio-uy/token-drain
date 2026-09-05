import "./BadgeGallery.css";
import { ProviderBadge } from "./ProviderBadge";
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

export function BadgeGallery() {
  return (
    <div className="gallery">
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
    </div>
  );
}
