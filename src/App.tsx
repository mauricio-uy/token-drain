import { BadgeGallery, BADGE_GALLERY_ENABLED } from "./components/BadgeGallery";
import { Rail } from "./components/Rail";
import { SettingsPanel } from "./components/SettingsPanel";
import { currentSurface } from "./lib/surface";
import { useUsage } from "./lib/usage";

/**
 * The rail. Kept separate from `App` so the usage subscription only runs in the
 * window that draws it — a hook at the top level would open a poll subscription
 * in the settings window too.
 */
function RailSurface() {
  return <Rail views={useUsage()} />;
}

function App() {
  if (BADGE_GALLERY_ENABLED) {
    return <BadgeGallery />;
  }

  return currentSurface() === "settings" ? <SettingsPanel /> : <RailSurface />;
}

export default App;
