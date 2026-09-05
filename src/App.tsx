import { Rail } from "./components/Rail";
import { useUsage } from "./lib/usage";

function App() {
  const views = useUsage();

  return <Rail views={views} />;
}

export default App;
