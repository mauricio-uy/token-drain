import { invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, type RefObject } from "react";

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
  const x = Math.max(0, rect.x);
  const y = Math.max(0, rect.y);
  return {
    x,
    y,
    width: Math.max(0, Math.min(window.innerWidth, rect.right) - x),
    height: Math.max(0, Math.min(window.innerHeight, rect.bottom) - y),
  };
}

const NO_ANCESTORS: RefObject<Element | null>[] = [];

/**
 * Keep the backend's idea of the interactive area in step with what is drawn.
 *
 * Observe transforms as well as size: a moving card does not trigger a
 * ResizeObserver. Reports are batched once per frame and unchanged geometry
 * never crosses IPC. Inert cards hand their area straight back to the desktop.
 */
export function useInteractiveRegions(
  refs: RefObject<Element | null>[],
  layoutKey?: unknown,
  ancestors: RefObject<Element | null>[] = NO_ANCESTORS,
): void {
  useLayoutEffect(() => {
    let frame = 0;
    let previous = "";

    const report = () => {
      frame = 0;
      const regions = refs
        .map((ref) => ref.current)
        .filter((element): element is Element =>
          element !== null && !element.closest('[inert], [aria-hidden="true"]'))
        .map(measure)
        .filter((rect) => rect.width > 0 && rect.height > 0);

      const signature = JSON.stringify(regions);
      if (signature === previous) return;
      previous = signature;
      void setInteractiveRegions(regions).catch(() => {
        // Allow the next geometry change to retry a failed report.
        previous = "";
      });
    };

    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(report);
    };
    report();

    const observer = new ResizeObserver(schedule);
    const mutations = new MutationObserver(schedule);
    for (const ref of [...refs, ...ancestors]) {
      if (!ref.current) continue;
      observer.observe(ref.current);
      mutations.observe(ref.current, {
        attributes: true,
        attributeFilter: ["style", "inert", "aria-hidden", "class"],
      });
    }
    window.addEventListener("resize", schedule);

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      mutations.disconnect();
      window.removeEventListener("resize", schedule);
      // Hand every region back on unmount, so a closing window cannot leave the
      // desktop with a dead zone on it.
      void setInteractiveRegions([]).catch(() => {});
    };
  }, [refs, layoutKey, ancestors]);
}
