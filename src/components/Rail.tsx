import { useRef } from "react";
import "./Rail.css";
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

  useInteractiveRegions([railRef]);

  return (
    <div className="rail-root">
      <div ref={railRef} className="rail">
        {views.map((view) => (
          <div key={view.provider} className="rail-slot" />
        ))}
      </div>
    </div>
  );
}
