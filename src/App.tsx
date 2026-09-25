import { lazy, Suspense, useEffect } from "react";
import { Rail } from "./components/Rail";
import { SettingsPanel } from "./components/SettingsPanel";
import { currentSurface } from "./lib/surface";
import { useUsage } from "./lib/usage";
import { applyTheme, useRailAppearance } from "./lib/settings";
import { useAutomaticUpdates } from "./lib/updates";

// Keep the gallery module and its stylesheet out of production bundles.
const BadgeGallery = import.meta.env.DEV && import.meta.env.VITE_DEBUG_BADGES === "1"
  ? lazy(() => import("./components/BadgeGallery").then((module) => ({ default: module.BadgeGallery })))
  : null;

/**
 * The rail. Kept separate from `App` so the usage subscription only runs in the
 * window that draws it — a hook at the top level would open a poll subscription
 * in the settings window too.
 */
function RailSurface() {
  useAutomaticUpdates();
  const { side, scale, theme } = useRailAppearance();

  // On the root element, where the geometry tokens that consume it are defined.
  useEffect(() => {
    document.documentElement.style.setProperty("--ui-scale", String(scale));
  }, [scale]);

  useEffect(() => applyTheme(theme), [theme]);

  return <Rail views={useUsage()} side={side} />;
}

function App() {
  if (BadgeGallery) {
    return <Suspense fallback={null}><BadgeGallery /></Suspense>;
  }

  return currentSurface() === "settings" ? <SettingsPanel /> : <RailSurface />;
}

export default App;
