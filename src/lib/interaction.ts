import { invoke } from "@tauri-apps/api/core";
import { useEffect, type RefObject } from "react";

/**
 * A rectangle in window-relative logical pixels.
 *
 * This is exactly what `getBoundingClientRect` returns, which is why the backend
 * accepts this coordinate space rather than physical pixels: the UI knows its
 * own layout and nothing about where the window sits or what the display scale
 * is.
 */
export type LogicalRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

/**
 * Tell the backend which parts of the window should accept the mouse.
 *
 * Everything not reported here is click-through, so the transparent area of the
 * window does not swallow clicks meant for whatever is behind it.
 */
export async function setInteractiveRegions(regions: LogicalRect[]): Promise<void> {
  await invoke("set_interactive_regions", { regions });
}

function measure(element: Element): LogicalRect {
  const rect = element.getBoundingClientRect();
  return {
    x: rect.x,
    y: rect.y,
    width: rect.width,
    height: rect.height,
  };
}

/**
 * Keep the backend's idea of the interactive area in step with what is drawn.
 *
 * Re-measures whenever an element resizes or the window does. Elements that are
 * absent — a hover card that is not open — are skipped, so closing the card
 * hands its area straight back to the desktop.
 */
export function useInteractiveRegions(refs: RefObject<Element | null>[]): void {
  useEffect(() => {
    let cancelled = false;

    const report = () => {
      if (cancelled) return;

      const regions = refs
        .map((ref) => ref.current)
        .filter((element): element is Element => element !== null)
        .map(measure);

      void setInteractiveRegions(regions);
    };

    report();

    const observer = new ResizeObserver(report);
    for (const ref of refs) {
      if (ref.current) observer.observe(ref.current);
    }
    window.addEventListener("resize", report);

    return () => {
      cancelled = true;
      observer.disconnect();
      window.removeEventListener("resize", report);
      // Hand every region back on unmount, so a closing window cannot leave the
      // desktop with a dead zone on it.
      void setInteractiveRegions([]);
    };
    // The ref objects are stable; their contents are watched by the observer.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
