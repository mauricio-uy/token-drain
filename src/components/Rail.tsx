import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import "./Rail.css";
import { ProviderBadge } from "./ProviderBadge";
import { UsageCard } from "./UsageCard";
import { useNow } from "../lib/format";
import { useInteractiveRegions } from "../lib/interaction";
import type { ProviderView } from "../lib/usage";
import type { RailSide } from "../lib/settings";

/** Keep the card this far from the window edges. */
const EDGE_MARGIN = 8;

/**
 * Motion for the card as it follows the pointer between badges.
 *
 * A spring rather than an ease: the card is being dragged along by the cursor,
 * and a duration-based curve always finishes late or early relative to where the
 * pointer actually went. Damped hard enough not to overshoot, since a card that
 * bounces past its badge and settles back reads as sloppy rather than lively.
 */
const FOLLOW_SPRING = { type: "spring", stiffness: 460, damping: 40, mass: 0.9 } as const;

/** How long the outgoing contents take to give way to the incoming ones. */
const CROSSFADE = { duration: 0.13, ease: "easeOut" } as const;

type Placement = { top: number; tailOffset: number };

/**
 * Counts how many times the card element has been mounted, for the dev overlay.
 *
 * The number must not change while the pointer moves between badges. If it does,
 * the card is being torn down and rebuilt rather than moved, which is exactly
 * the implementation this design rules out.
 */
let cardMountCount = 0;

const SHOW_MOUNT_PROBE = import.meta.env.VITE_DEBUG_RAIL === "1";

function CardMountProbe() {
  const [mount] = useState(() => ++cardMountCount);
  return <span className="rail-debug">card mounts: {mount}</span>;
}

/** Only the open card needs a clock; the idle rail has no countdown to update. */
function LiveUsageCard({ view }: { view: ProviderView }) {
  return <UsageCard view={view} now={useNow()} />;
}

/**
 * The rail, with a card that follows the pointer between badges.
 *
 * **The card is one element for the life of the app.** It is rendered
 * unconditionally and hidden by opacity rather than being mounted and unmounted,
 * so moving from one badge to another slides a single card instead of tearing
 * one down and building another. Only its contents are replaced, and those
 * cross-fade.
 */
export function Rail({ views, side = "right" }: { views: ProviderView[]; side?: RailSide }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const railRef = useRef<HTMLDivElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const bridgeRef = useRef<HTMLSpanElement>(null);
  const interactiveRefs = useMemo(() => [railRef, cardRef, bridgeRef], []);
  const badgeRefs = useRef(new Map<string, HTMLElement>());

  const [active, setActive] = useState<string | null>(null);
  const [placement, setPlacement] = useState<Placement>({ top: 0, tailOffset: 0 });

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
    const lowest = Math.max(rootBox.height - cardHeight - EDGE_MARGIN, EDGE_MARGIN);
    const top = Math.min(Math.max(badgeCentre - cardHeight / 2, EDGE_MARGIN), lowest);

    setPlacement({ top, tailOffset: badgeCentre - top });
  }, [activeView, views]);

  useInteractiveRegions(interactiveRefs, side);

  return (
    <div ref={rootRef} className="rail-root" data-side={side} onMouseLeave={() => setActive(null)}>
      <motion.div
        ref={cardRef}
        className="rail-card card-surface"
        animate={{
          y: placement.top,
          opacity: activeView ? 1 : 0,
          scale: activeView ? 1 : 0.97,
        }}
        transition={{ ...FOLLOW_SPRING, opacity: CROSSFADE, scale: CROSSFADE }}
        // Hidden from the pointer and from assistive technology when closed;
        // it is still in the tree, just not participating.
        aria-hidden={!activeView}
        inert={!activeView}
      >
        <AnimatePresence mode="popLayout" initial={false}>
          {activeView && (
            <motion.div
              key={activeView.provider}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={CROSSFADE}
            >
              <LiveUsageCard view={activeView} />
            </motion.div>
          )}
        </AnimatePresence>

        {SHOW_MOUNT_PROBE && <CardMountProbe />}

        <span ref={bridgeRef} className="rail-card-bridge" />

        <motion.span
          className="card-tail"
          aria-hidden="true"
          animate={{ y: placement.tailOffset - 10 }}
          transition={FOLLOW_SPRING}
        />
      </motion.div>

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
