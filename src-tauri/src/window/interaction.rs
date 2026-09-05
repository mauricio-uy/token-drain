//! Deciding when the window accepts the mouse and when the desktop gets it.
//!
//! The window is much larger than the rail it draws, because the hover card has
//! to render inside it. Everything outside the rail is transparent, and a
//! transparent pixel that still swallows clicks is worse than a visible one: the
//! user clicks something they can see and nothing happens.
//!
//! So the window ignores the cursor by default and stops ignoring it only while
//! the pointer is over a region the UI has declared interactive.
//!
//! # Why this is polled rather than event-driven
//!
//! It cannot be event-driven. While the window ignores cursor events the webview
//! receives no mouse events at all, so the UI cannot notice the pointer arriving
//! and ask for control — the very state we need to leave is the state that hides
//! the signal to leave it. The cursor position therefore has to be read from
//! outside the window.

use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewWindow};

/// How often the cursor is sampled.
///
/// Fast enough that entering the rail feels immediate, slow enough to be
/// invisible in a profiler. Reading the cursor position is a single cheap OS
/// call.
const CURSOR_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A rectangle in window-relative **logical** pixels, as the UI measures itself.
///
/// The UI knows its own layout in CSS pixels and nothing about where the window
/// sits or what the display scale is, so that is the coordinate space it reports
/// in. Conversion to the physical, virtual-desktop coordinates the cursor
/// arrives in happens here, once.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl LogicalRect {
    /// Half-open containment, matching how CSS lays rectangles out: the right
    /// and bottom edges belong to the next element.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// The regions the UI currently wants the mouse for.
///
/// Empty means fully click-through, which is the correct state before the UI has
/// reported anything: a window that grabs the mouse over an area it has not yet
/// drawn would block the desktop for no reason.
#[derive(Default)]
pub struct InteractiveRegions(Mutex<Vec<LogicalRect>>);

impl InteractiveRegions {
    pub fn set(&self, regions: Vec<LogicalRect>) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = regions;
        }
    }

    pub fn snapshot(&self) -> Vec<LogicalRect> {
        self.0
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

/// Whether a cursor position falls inside any interactive region.
///
/// Pure, and the only place the coordinate arithmetic lives.
///
/// `cursor` and `window_origin` are physical virtual-desktop pixels; `regions`
/// are window-relative logical pixels.
pub fn cursor_wants_the_window(
    regions: &[LogicalRect],
    cursor: (f64, f64),
    window_origin: (i32, i32),
    scale_factor: f64,
) -> bool {
    if regions.is_empty() || scale_factor <= 0.0 {
        return false;
    }

    let relative_x = (cursor.0 - window_origin.0 as f64) / scale_factor;
    let relative_y = (cursor.1 - window_origin.1 as f64) / scale_factor;

    regions
        .iter()
        .any(|region| region.contains(relative_x, relative_y))
}

/// Watch the cursor and hand the window the mouse only when it is wanted.
pub fn spawn_cursor_watcher(app: AppHandle, window: WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(CURSOR_POLL_INTERVAL);
        // Mirrors the window's state so it is only set when it actually
        // changes. Toggling every tick would be a stream of pointless OS calls.
        let mut ignoring = true;

        let _ = window.set_ignore_cursor_events(true);

        loop {
            ticker.tick().await;

            if window.is_closable().is_err() {
                return;
            }

            let regions = app.state::<InteractiveRegions>().snapshot();

            let Ok(cursor) = app.cursor_position() else {
                continue;
            };
            let Ok(origin) = window.outer_position() else {
                continue;
            };
            let scale = window.scale_factor().unwrap_or(1.0);

            let wanted = cursor_wants_the_window(
                &regions,
                (cursor.x, cursor.y),
                (origin.x, origin.y),
                scale,
            );

            if ignoring == wanted {
                ignoring = !wanted;
                let _ = window.set_ignore_cursor_events(ignoring);
            }
        }
    });
}

/// Report the regions the UI wants the mouse for, in window-relative logical
/// pixels. Called from the frontend whenever its layout changes.
#[tauri::command]
pub fn set_interactive_regions(
    regions: Vec<LogicalRect>,
    state: tauri::State<'_, InteractiveRegions>,
) {
    state.set(regions);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rail() -> LogicalRect {
        // A 120x320 strip on the right of a 460x520 window.
        LogicalRect {
            x: 340.0,
            y: 100.0,
            width: 120.0,
            height: 320.0,
        }
    }

    const ORIGIN: (i32, i32) = (1460, 256);

    #[test]
    fn no_regions_means_fully_click_through() {
        // The startup state. A window that grabbed the mouse over an area it has
        // not drawn yet would block the desktop for no reason.
        assert!(!cursor_wants_the_window(&[], (1500.0, 300.0), ORIGIN, 1.0));
    }

    #[test]
    fn the_cursor_inside_the_rail_wants_the_window() {
        // Window-relative (400, 200) -> virtual desktop (1860, 456).
        assert!(cursor_wants_the_window(
            &[rail()],
            (1860.0, 456.0),
            ORIGIN,
            1.0
        ));
    }

    #[test]
    fn the_transparent_area_of_the_window_does_not() {
        // Inside the window, outside the rail: this is the case that matters.
        // Claiming it would swallow clicks aimed at whatever is underneath.
        assert!(!cursor_wants_the_window(
            &[rail()],
            (1500.0, 300.0),
            ORIGIN,
            1.0
        ));
    }

    #[test]
    fn a_cursor_outside_the_window_entirely_does_not() {
        assert!(!cursor_wants_the_window(&[rail()], (200.0, 200.0), ORIGIN, 1.0));
    }

    #[test]
    fn scaling_is_applied_to_the_regions() {
        // At 150% the same logical rail covers 1.5x the physical pixels. A point
        // that is inside at 100% can be outside at 150% and vice versa; getting
        // this wrong makes the hot area drift off the visible rail.
        let scaled_origin = (1460, 256);

        // Logical (400, 200) at 1.5x is 600, 300 physical from the origin.
        assert!(cursor_wants_the_window(
            &[rail()],
            (1460.0 + 600.0, 256.0 + 300.0),
            scaled_origin,
            1.5
        ));

        // The same physical point interpreted at 1.0x lands well past the rail.
        assert!(!cursor_wants_the_window(
            &[rail()],
            (1460.0 + 600.0, 256.0 + 300.0),
            scaled_origin,
            1.0
        ));
    }

    #[test]
    fn a_window_at_a_negative_origin_still_works() {
        // A monitor left of the primary. The subtraction has to stay signed.
        let origin = (-1920, 0);

        assert!(cursor_wants_the_window(
            &[rail()],
            (-1920.0 + 400.0, 200.0),
            origin,
            1.0
        ));
    }

    #[test]
    fn several_regions_are_all_honoured() {
        // What the open hover card needs: the rail plus the card, as two
        // separate rectangles with a gap between them.
        let card = LogicalRect {
            x: 20.0,
            y: 180.0,
            width: 280.0,
            height: 140.0,
        };
        let regions = [rail(), card];

        // Inside the card.
        assert!(cursor_wants_the_window(
            &regions,
            (1460.0 + 100.0, 256.0 + 200.0),
            ORIGIN,
            1.0
        ));
        // In the gap between the two.
        assert!(!cursor_wants_the_window(
            &regions,
            (1460.0 + 320.0, 256.0 + 200.0),
            ORIGIN,
            1.0
        ));
    }

    #[test]
    fn region_edges_are_half_open() {
        let region = LogicalRect {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 100.0,
        };

        assert!(region.contains(10.0, 10.0));
        assert!(region.contains(109.9, 109.9));
        assert!(!region.contains(110.0, 50.0));
        assert!(!region.contains(50.0, 110.0));
    }

    #[test]
    fn a_nonsense_scale_factor_is_not_trusted() {
        // Guards a division by zero turning into an infinite coordinate, which
        // would silently make the whole window interactive or none of it.
        assert!(!cursor_wants_the_window(&[rail()], (1860.0, 456.0), ORIGIN, 0.0));
        assert!(!cursor_wants_the_window(&[rail()], (1860.0, 456.0), ORIGIN, -1.0));
    }

    #[test]
    fn regions_round_trip_through_the_shared_state() {
        let state = InteractiveRegions::default();
        assert!(state.snapshot().is_empty());

        state.set(vec![rail()]);
        assert_eq!(state.snapshot(), vec![rail()]);

        state.set(vec![]);
        assert!(state.snapshot().is_empty());
    }
}
