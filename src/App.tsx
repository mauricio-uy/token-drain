import { BadgeGallery, BADGE_GALLERY_ENABLED } from "./components/BadgeGallery";
import { Rail } from "./components/Rail";
import { useUsage } from "./lib/usage";

function App() {
  const views = useUsage();

  if (BADGE_GALLERY_ENABLED) {
    return <BadgeGallery />;
  }

  return <Rail views={views} />;
}

export default App;
