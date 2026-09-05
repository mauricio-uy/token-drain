import { useRef } from "react";
import "./App.css";
import { useInteractiveRegions } from "./lib/interaction";
import { refreshNow, useUsage, type ProviderView } from "./lib/usage";

/**
 * Phase 4 shell probe.
 *
 * Deliberately not the real UI. It verifies the shell end to end: that the
 * window is chromeless and transparent, that only the drawn panel captures the
 * mouse, and that live figures actually arrive from the Rust side over IPC.
 *
 * Replaced by the rail in Phase 5.
 */
function App() {
  const panelRef = useRef<HTMLDivElement>(null);
  const views = useUsage();

  useInteractiveRegions([panelRef]);

  return (
    <div className="probe-root">
      <div ref={panelRef} className="probe-panel">
        <button className="probe-refresh" onClick={() => void refreshNow()}>
          refresh
        </button>
        {views.length === 0 ? (
          <p className="probe-empty">waiting for the first snapshot…</p>
        ) : (
          views.map((view) => <ProviderRow key={view.provider} view={view} />)
        )}
      </div>
    </div>
  );
}

function ProviderRow({ view }: { view: ProviderView }) {
  // The invariant made visible: a failing provider has no percentage to show,
  // so the row prints its state instead of a number.
  const session = view.usage?.session;
  const weekly = view.usage?.weekly;

  return (
    <div className="probe-provider">
      <div className="probe-provider-head">
        <strong>{view.provider}</strong>
        <span className={`probe-state probe-state--${view.state}`}>{view.state}</span>
      </div>
      {session && <div>session: {session.usedPercent.toFixed(0)}%</div>}
      {weekly && <div>weekly: {weekly.usedPercent.toFixed(0)}%</div>}
      {view.remediation && <div className="probe-note">{view.remediation.message}</div>}
    </div>
  );
}

export default App;
