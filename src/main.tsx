import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { currentSurface } from "./lib/surface";
import "./styles/tokens.css";
import "./base.css";

// Stamped before the first render so the chromeless-overlay rules apply to the
// rail and to nothing else.
document.documentElement.dataset.surface = currentSurface();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
