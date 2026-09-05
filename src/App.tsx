import { useRef, useState } from "react";
import "./App.css";
import { useInteractiveRegions } from "./lib/interaction";

/**
 * Phase 4 shell probe.
 *
 * Deliberately not the real UI. It verifies the window itself: that it is
 * chromeless, that everything around the rectangle is genuinely transparent
 * rather than white, that Windows draws no shadow of its own, that it floats
 * above other windows, and that only the rectangle captures the mouse while the
 * transparent area passes clicks through to the desktop.
 *
 * Replaced by the rail in Phase 5.
 */
function App() {
  const rectRef = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState(false);
  const [clicks, setClicks] = useState(0);

  useInteractiveRegions([rectRef]);

  return (
    <div className="probe-root">
      <div
        ref={rectRef}
        className={`probe-rect ${hovered ? "is-hovered" : ""}`}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        onClick={() => setClicks((count) => count + 1)}
      >
        <span className="probe-label">
          {hovered ? "hover ok" : "shell probe"} · {clicks}
        </span>
      </div>
    </div>
  );
}

export default App;
