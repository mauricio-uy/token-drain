import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import "./Rail.css";
import { ProviderBadge } from "./ProviderBadge";
import { UsageCard } from "./UsageCard";
import { useNow } from "../lib/format";
import { useInteractiveRegions } from "../lib/interaction";
import type { ProviderView } from "../lib/usage";
import { openSettingsWindow, type RailSide } from "../lib/settings";

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
  const triggerRef = useRef<HTMLButtonElement>(null);
  const dockRef = useRef<HTMLDivElement>(null);
  const railRef = useRef<HTMLDivElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const bridgeRef = useRef<HTMLSpanElement>(null);
  const settingsRef = useRef<HTMLDivElement>(null);
  const settingsErrorRef = useRef<HTMLParagraphElement>(null);
  const interactiveRefs = useMemo(() => [triggerRef, railRef, cardRef, bridgeRef, settingsRef, settingsErrorRef], []);
  const movingRefs = useMemo(() => [dockRef], []);
  const badgeRefs = useRef(new Map<string, HTMLElement>());

  const [active, setActive] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [openingSettings, setOpeningSettings] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [placement, setPlacement] = useState<Placement>({ top: 0, tailOffset: 0 });
  const reduceMotion = useReducedMotion();

  const activeView = expanded ? views.find((view) => view.provider === active) ?? null : null;

  const cancelClose = () => {
    if (closeTimer.current !== null) clearTimeout(closeTimer.current);
    closeTimer.current = null;
  };
  const reveal = () => {
    cancelClose();
    setExpanded(true);
  };
  const collapse = () => {
    cancelClose();
    setActive(null);
    setExpanded(false);
  };
  const scheduleClose = () => {
    if (closeTimer.current === null) closeTimer.current = setTimeout(collapse, 180);
  };
  const showSettings = async () => {
    if (openingSettings) return;
    setOpeningSettings(true);
    setSettingsError(null);
    try {
      await openSettingsWindow();
      collapse();
    } catch {
      setSettingsError("Could not open settings. Try again.");
    } finally {
      setOpeningSettings(false);
    }
  };
  useEffect(() => () => {
    if (closeTimer.current !== null) clearTimeout(closeTimer.current);
  }, []);

  // Measured after layout, not during render: the card's height depends on how
  // many windows the provider reported, so it cannot be known in advance.
  useLayoutEffect(() => {
    if (!activeView) return;

    const root = rootRef.current;
    const badge = badgeRefs.current.get(activeView.provider);
    const card = cardRef.current;
    if (!root || !badge || !card) return;

    const place = () => {
      const rootBox = root.getBoundingClientRect();
      const badgeBox = badge.getBoundingClientRect();
      const cardHeight = card.offsetHeight;

      const badgeCentre = badgeBox.top + badgeBox.height / 2 - rootBox.top;

      // Centre on the badge, then keep the whole card inside the viewport.
      const lowest = Math.max(rootBox.height - cardHeight - EDGE_MARGIN, EDGE_MARGIN);
      const top = Math.min(Math.max(badgeCentre - cardHeight / 2, EDGE_MARGIN), lowest);
      const tailOffset = Math.max(18, Math.min(badgeCentre - top, cardHeight - 18));

      setPlacement((current) => current.top === top && current.tailOffset === tailOffset
        ? current : { top, tailOffset });
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(root);
    observer.observe(card);
    return () => observer.disconnect();
  }, [activeView, views]);

  useInteractiveRegions(interactiveRefs, `${side}:${expanded}`, movingRefs);

  return (
    <div
      ref={rootRef}
      className="rail-root"
      data-side={side}
      data-expanded={expanded}
      onPointerLeave={scheduleClose}
      onPointerMove={(event) => {
        if (event.target === event.currentTarget) scheduleClose();
        else cancelClose();
      }}
      onFocusCapture={cancelClose}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) scheduleClose();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          collapse();
          requestAnimationFrame(() => triggerRef.current?.focus());
        }
      }}
    >
      <button
        ref={triggerRef}
        type="button"
        className="rail-trigger"
        aria-label="Show usage indicators"
        aria-expanded={expanded}
        aria-controls="provider-rail"
        aria-hidden={expanded}
        inert={expanded}
        onPointerEnter={reveal}
        onClick={() => {
          reveal();
          requestAnimationFrame(() => railRef.current?.querySelector('button')?.focus());
        }}
      />
      <motion.div
        id="usage-card"
        ref={cardRef}
        className="rail-card card-surface"
        animate={{
          y: placement.top,
          opacity: activeView ? 1 : 0,
          scale: activeView ? 1 : 0.97,
        }}
        transition={reduceMotion ? { duration: 0 } : { ...FOLLOW_SPRING, opacity: CROSSFADE, scale: CROSSFADE }}
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
              transition={reduceMotion ? { duration: 0 } : CROSSFADE}
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
          transition={reduceMotion ? { duration: 0 } : FOLLOW_SPRING}
        />
      </motion.div>

      <motion.div
        ref={dockRef}
        className="rail-dock"
        initial={false}
        animate={{ x: expanded ? "0%" : side === "right" ? "100%" : "-100%", y: "-50%" }}
        transition={reduceMotion ? { duration: 0 } : { duration: 0.2, ease: "easeOut" }}
        inert={!expanded}
        aria-hidden={!expanded}
        onPointerEnter={reveal}
      >
      <div ref={railRef} id="provider-rail" className="rail">
        <div className="rail-badges" onScroll={() => setActive(null)}>
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
              cardId="usage-card"
            />
          </div>
        ))}
        </div>
      </div>
      <div ref={settingsRef} className="rail-settings-area">
        <button
          type="button"
          className="rail-settings-button"
          aria-label="Open settings"
          title="Settings"
          disabled={openingSettings}
          onPointerEnter={() => setActive(null)}
          onFocus={() => setActive(null)}
          onClick={() => void showSettings()}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
            strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="m9 3-.5 2.1-1.4.8L5 5.3 2 10l1.6 1.5v1L2 14l3 4.7 2.1-.6 1.4.8L9 21h6l.5-2.1 1.4-.8 2.1.6 3-4.7-1.6-1.5v-1L22 10l-3-4.7-2.1.6-1.4-.8L15 3Z" />
            <circle cx="12" cy="12" r="3.2" />
          </svg>
        </button>
        <p ref={settingsErrorRef} className="rail-settings-error" role="alert"
          aria-hidden={!settingsError} inert={!settingsError}>
          {settingsError}
        </p>
      </div>
      </motion.div>
    </div>
  );
}
