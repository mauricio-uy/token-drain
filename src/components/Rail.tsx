import { useLayoutEffect, useRef, useState } from "react";
import "./Rail.css";
import { ProviderBadge } from "./ProviderBadge";
import { UsageCard } from "./UsageCard";
import { useNow } from "../lib/format";
import { useInteractiveRegions } from "../lib/interaction";
import type { ProviderView } from "../lib/usage";

/** Gap between the card's right edge and the rail. */
const CARD_GAP = 14;
/** Keep the card this far from the window edges. */
const EDGE_MARGIN = 8;

type Placement = { top: number; tailOffset: number };

/**
 * The rail: an opaque strip docked to the screen edge, carrying one badge per
 * provider, with a card that opens beside whichever badge the pointer is on.
 *
 * Everything outside the rail and the open card is transparent and
 * click-through, so both report their bounds as the regions that should receive
 * the mouse.
 */
export function Rail({ views }: { views: ProviderView[] }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const railRef = useRef<HTMLDivElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const badgeRefs = useRef(new Map<string, HTMLElement>());

  const [active, setActive] = useState<string | null>(null);
  const [placement, setPlacement] = useState<Placement>({ top: 0, tailOffset: 0 });
  const now = useNow();

  const activeView = views.find((view) => view.provider === active) ?? null;

  // Measured after layout, not during render: the card's height depends on how
  // many windows the provider reported, so it cannot be known in advance.
  useLayoutEffect(() => {
    if (!activeView) return;

    const root = rootRef.current;
    const badge = badgeRefs.current.get(activeView.provider);
    const card = cardRef.current;
    if (!root || !badge || !card) return;

    const rootBox = root.getBoundingClientRect();
    const badgeBox = badge.getBoundingClientRect();
    const cardHeight = card.offsetHeight;

    const badgeCentre = badgeBox.top + badgeBox.height / 2 - rootBox.top;

    // Centre on the badge, then keep the whole card on screen. Clamping the top
    // rather than the tail is what lets the tail keep pointing at the badge even
    // when the card has been pushed away from it.
    const lowest = rootBox.height - cardHeight - EDGE_MARGIN;
    const top = Math.min(Math.max(badgeCentre - cardHeight / 2, EDGE_MARGIN), Math.max(lowest, EDGE_MARGIN));

    setPlacement({ top, tailOffset: badgeCentre - top });
  }, [activeView, views]);

  useInteractiveRegions([railRef, cardRef]);

  return (
    <div ref={rootRef} className="rail-root" onMouseLeave={() => setActive(null)}>
      {activeView && (
        <div
          ref={cardRef}
          className="rail-card"
          style={{ top: placement.top, right: `calc(var(--rail-width) + ${CARD_GAP}px)` }}
        >
          <UsageCard view={activeView} now={now} tailOffset={placement.tailOffset} />
        </div>
      )}

      <div ref={railRef} className="rail">
        {views.map((view) => (
          <div
            key={view.provider}
            ref={(element) => {
              if (element) badgeRefs.current.set(view.provider, element);
              else badgeRefs.current.delete(view.provider);
            }}
          >
            <ProviderBadge
              view={view}
              active={active === view.provider}
              onEnter={setActive}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
