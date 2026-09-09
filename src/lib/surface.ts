import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Which window this document is.
 *
 * Both windows load the same bundle, so the label is what decides whether to
 * draw the rail or the settings screen. Read synchronously — Tauri injects the
 * label before any script runs — so the first paint is already the right one
 * rather than a flash of the wrong surface.
 *
 * Falls back to the rail outside Tauri, which is what the browser-based badge
 * gallery runs as.
 */
export type Surface = "rail" | "settings";

/** Resolve the current Tauri window to the React surface it owns. */
export function currentSurface(): Surface {
  try {
    return getCurrentWindow().label === "settings" ? "settings" : "rail";
  } catch {
    return "rail";
  }
}
