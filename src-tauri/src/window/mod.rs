//! Placing and keeping the rail window where it belongs.

pub mod interaction;
pub mod placement;

use std::time::Duration;

use tauri::{Monitor, PhysicalPosition, WebviewWindow};

use placement::{dock_position, to_physical, Rect, RailSide, MIN_TOP_MARGIN_LOGICAL};

/// How often the window's position is re-checked.
///
/// Windows has no single event that covers everything worth reacting to: a
/// taskbar moved to another edge, a resolution change, a monitor unplugged, the
/// taskbar's auto-hide toggled. Rather than chasing each native message, the
/// position is recomputed periodically and only applied when it actually
/// differs. The work is two monitor queries, so the cost is irrelevant and the
/// behavior covers cases a hand-picked list of events would miss.
const REPOSITION_INTERVAL: Duration = Duration::from_secs(3);

/// The monitor the window is on, falling back to the primary one.
///
/// `current_monitor` returns nothing while the window is off-screen or between
/// displays, which is exactly when repositioning matters most.
fn target_monitor(window: &WebviewWindow) -> Option<Monitor> {
    match window.current_monitor() {
        Ok(Some(monitor)) => Some(monitor),
        _ => window.primary_monitor().ok().flatten(),
    }
}

/// Work out where the window should sit right now, in physical pixels.
pub fn desired_position(window: &WebviewWindow, side: RailSide) -> Option<(i32, i32)> {
    let monitor = target_monitor(window)?;
    let size = window.outer_size().ok()?;

    let area = monitor.work_area();
    let work_area = Rect::new(
        area.position.x,
        area.position.y,
        area.size.width,
        area.size.height,
    );

    let min_top_margin = to_physical(MIN_TOP_MARGIN_LOGICAL, monitor.scale_factor());

    Some(dock_position(
        work_area,
        size.width,
        size.height,
        side,
        min_top_margin,
    ))
}

/// Move the window to its docked position, if it is not already there.
///
/// Returns whether a move was performed. Skipping a no-op move matters: setting
/// the position unconditionally on a timer makes the window flicker on some
/// compositors and shows up as constant work in a profiler.
pub fn dock(window: &WebviewWindow, side: RailSide) -> bool {
    let Some((x, y)) = desired_position(window, side) else {
        return false;
    };

    if let Ok(current) = window.outer_position() {
        if current.x == x && current.y == y {
            return false;
        }
    }

    window.set_position(PhysicalPosition::new(x, y)).is_ok()
}

/// Keep the window docked for the lifetime of the app.
pub fn spawn_dock_watcher(window: WebviewWindow, side: RailSide) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(REPOSITION_INTERVAL);

        loop {
            ticker.tick().await;

            // A closed window is the signal to stop; there is nothing left to
            // position.
            if window.is_closable().is_err() {
                return;
            }

            dock(&window, side);
        }
    });
}
