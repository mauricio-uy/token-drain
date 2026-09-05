import "./App.css";

/**
 * Phase 4 shell probe.
 *
 * Deliberately not the real UI: this is a solid, unmistakable rectangle used to
 * verify the window itself — that it is chromeless, that everything around the
 * rectangle is genuinely transparent rather than white, that Windows draws no
 * shadow of its own, and that it floats above other windows.
 *
 * Replaced by the rail in Phase 5.
 */
function App() {
  return (
    <div className="probe-root">
      <div className="probe-rect">
        <span className="probe-label">tok-ching shell probe</span>
      </div>
    </div>
  );
}

export default App;
