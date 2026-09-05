import { useRef, useState } from "react";
import "./Rail.css";
import { ProviderBadge } from "./ProviderBadge";
import { useInteractiveRegions } from "../lib/interaction";
import type { ProviderView } from "../lib/usage";

/**
 * The rail: an opaque strip docked to the screen edge, carrying one badge per
 * provider.
 *
 * Everything outside the rail is transparent and click-through, so the rail
 * reports its own bounds as the region that should receive the mouse.
 */
export function Rail({ views }: { views: ProviderView[] }) {
  const railRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState<string | null>(null);

  useInteractiveRegions([railRef]);

  return (
    <div className="rail-root">
      <div
        ref={railRef}
        className="rail"
        onMouseLeave={() => setActive(null)}
      >
        {views.map((view) => (
          <ProviderBadge
            key={view.provider}
            view={view}
            active={active === view.provider}
            onEnter={setActive}
          />
        ))}
      </div>
    </div>
  );
}
