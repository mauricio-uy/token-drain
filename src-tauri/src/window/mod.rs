//! Placing and keeping the rail window where it belongs.

pub mod interaction;
pub mod placement;
pub mod settings_window;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Monitor, PhysicalPosition, WebviewWindow};

use crate::diagnostics::{self, Event};
use crate::settings::{Settings, SettingsStore};
use placement::{dock_position, to_physical, vertical_offset_range, RailPlacement, RailSide, Rect};

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
///
/// Both measurements the user can influence are stored in logical pixels and
/// converted here, so the same settings file puts the rail in the same place on
/// a 100% and a 150% display.
pub fn desired_position(window: &WebviewWindow, settings: &Settings) -> Option<(i32, i32)> {
    let geometry = DockGeometry::of(window, settings.rail_side)?;
    let placement = geometry
        .placement
        .with_vertical_offset(to_physical(settings.vertical_offset as f64, geometry.scale));

    Some(dock_position(
        geometry.work_area,
        geometry.width,
        geometry.height,
        placement,
    ))
}

/// The vertical offsets, in logical pixels, that move the rail on its current
/// monitor when docked to `side`, as `(furthest up, furthest down)`.
pub fn offset_range(window: &WebviewWindow, side: RailSide) -> Option<(i32, i32)> {
    let geometry = DockGeometry::of(window, side)?;
    let (up, down) = vertical_offset_range(geometry.work_area, geometry.height);
    // Truncate toward zero, so a converted end never overshoots the limit.
    let to_logical = |physical: i32| (f64::from(physical) / geometry.scale).trunc() as i32;
    Some((to_logical(up), to_logical(down)))
}

/// The physical measurements the docking rules work from.
struct DockGeometry {
    work_area: Rect,
    width: u32,
    height: u32,
    scale: f64,
    /// Centred on the requested side.
    placement: RailPlacement,
}

impl DockGeometry {
    fn of(window: &WebviewWindow, side: RailSide) -> Option<Self> {
        let monitor = target_monitor(window)?;
        let size = window.outer_size().ok()?;
        let area = monitor.work_area();
        let scale = monitor.scale_factor();

        Some(Self {
            work_area: Rect::new(
                area.position.x,
                area.position.y,
                area.size.width,
                area.size.height,
            ),
            width: size.width,
            height: size.height,
            scale,
            placement: RailPlacement::new(side),
        })
    }
}

/// Move the window to its docked position, if it is not already there.
///
/// Returns whether a move was performed. Skipping a no-op move matters: setting
/// the position unconditionally on a timer makes the window flicker on some
/// compositors and shows up as constant work in a profiler.
pub fn dock(window: &WebviewWindow, settings: &Settings) -> bool {
    let Some((x, y)) = desired_position(window, settings) else {
        return false;
    };

    if let Ok(current) = window.outer_position() {
        if current.x == x && current.y == y {
            return false;
        }
    }

    match window.set_position(PhysicalPosition::new(x, y)) {
        Ok(()) => {
            diagnostics::record(Event::DockRecalculated {
                side: settings.rail_side,
                x,
                y,
            });
            true
        }
        Err(_) => {
            diagnostics::record(Event::OperationFailed {
                operation: diagnostics::Operation::WindowPosition,
            });
            false
        }
    }
}

/// Keep the window docked for the lifetime of the app.
///
/// The settings are read on every tick rather than captured once, so changing
/// the side or the offset takes effect without a restart even for the paths
/// that do not re-dock explicitly.
pub fn spawn_dock_watcher(window: WebviewWindow, settings: Arc<SettingsStore>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(REPOSITION_INTERVAL);

        loop {
            ticker.tick().await;

            // A closed window is the signal to stop; there is nothing left to
            // position.
            if window.is_closable().is_err() {
                return;
            }

            dock(&window, &settings.get());
        }
    });
}
