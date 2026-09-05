//! Where the rail window sits.
//!
//! Pure geometry: given a monitor's work area and the window size, decide the
//! window's top-left corner. No Tauri types, no clock, no OS calls — so every
//! display arrangement can be tested exactly, including the ones this machine
//! does not have.
//!
//! Everything here is in **physical pixels**. Mixing logical and physical
//! coordinates is the classic way multi-monitor positioning goes wrong, so the
//! conversion happens once at the caller and never inside these rules.

use serde::{Deserialize, Serialize};

/// Distance kept between the top of the work area and the top of the window,
/// in **logical** pixels.
///
/// Not cosmetic. A maximized window's close button lives in the top-right
/// corner, which is exactly where a right-docked rail would otherwise sit. This
/// margin keeps the rail clear of it.
pub const MIN_TOP_MARGIN_LOGICAL: f64 = 120.0;

/// A rectangle in physical pixels.
///
/// `x` and `y` can be negative: on Windows a monitor placed to the left of or
/// above the primary one has a negative origin in the virtual desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.height as i32)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

/// Which screen edge the rail docks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RailSide {
    #[default]
    Right,
    Left,
}

/// Compute the window's top-left corner.
///
/// `work_area` is the monitor area excluding the taskbar, so a taskbar on any
/// edge is respected without the rules needing to know where it is.
pub fn dock_position(
    work_area: Rect,
    window_width: u32,
    window_height: u32,
    side: RailSide,
    min_top_margin: i32,
) -> (i32, i32) {
    let x = match side {
        RailSide::Right => work_area.right().saturating_sub(window_width as i32),
        RailSide::Left => work_area.x,
    };

    let y = vertical_position(work_area, window_height, min_top_margin);

    (x, y)
}

fn vertical_position(work_area: Rect, window_height: u32, min_top_margin: i32) -> i32 {
    let window_height = window_height as i32;

    // A window taller than the space available cannot satisfy any constraint;
    // pinning it to the top of the work area at least keeps it on screen.
    if window_height >= work_area.height as i32 {
        return work_area.y;
    }

    let centred = work_area.y + (work_area.height as i32 - window_height) / 2;

    let highest = work_area.y.saturating_add(min_top_margin);
    let lowest = work_area.bottom().saturating_sub(window_height);

    // Order matters: push down away from the close button first, then pull back
    // up if that would run off the bottom. On a screen too short to honour both,
    // staying on screen wins.
    centred.max(highest).min(lowest).max(work_area.y)
}

/// Convert a logical measurement to physical pixels for a given scale factor.
pub fn to_physical(logical: f64, scale_factor: f64) -> i32 {
    (logical * scale_factor).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1920x1080 with a 40px taskbar along the bottom.
    fn standard_work_area() -> Rect {
        Rect::new(0, 0, 1920, 1040)
    }

    const WIDTH: u32 = 460;
    const HEIGHT: u32 = 520;
    const MARGIN: i32 = 120;

    #[test]
    fn docks_flush_to_the_right_edge_of_the_work_area() {
        let (x, _) = dock_position(standard_work_area(), WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(x, 1920 - 460);
    }

    #[test]
    fn docks_flush_to_the_left_edge_of_the_work_area() {
        let (x, _) = dock_position(standard_work_area(), WIDTH, HEIGHT, RailSide::Left, MARGIN);

        assert_eq!(x, 0);
    }

    #[test]
    fn centres_vertically_within_the_work_area() {
        let (_, y) = dock_position(standard_work_area(), WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(y, (1040 - 520) / 2);
    }

    #[test]
    fn a_side_taskbar_is_not_covered() {
        // Taskbar 48px wide on the right: the work area stops short of the
        // screen edge, and the rail must stop with it.
        let work_area = Rect::new(0, 0, 1920 - 48, 1080);

        let (x, _) = dock_position(work_area, WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(x, 1920 - 48 - 460);
    }

    #[test]
    fn a_top_taskbar_shifts_the_work_area_origin() {
        let work_area = Rect::new(0, 40, 1920, 1040);

        let (_, y) = dock_position(work_area, WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(y, 40 + (1040 - 520) / 2);
        assert!(y >= 40, "the window started above the work area");
    }

    #[test]
    fn a_monitor_left_of_the_primary_has_a_negative_origin() {
        // The case a naive implementation gets wrong: on Windows a secondary
        // monitor placed to the left sits at negative virtual coordinates.
        let work_area = Rect::new(-1920, 0, 1920, 1040);

        let (x, y) = dock_position(work_area, WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(x, -1920 + 1920 - 460);
        assert_eq!(y, (1040 - 520) / 2);
    }

    #[test]
    fn a_monitor_above_the_primary_has_a_negative_vertical_origin() {
        let work_area = Rect::new(0, -1080, 1920, 1040);

        let (_, y) = dock_position(work_area, WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(y, -1080 + (1040 - 520) / 2);
    }

    #[test]
    fn the_top_margin_is_enforced_on_a_short_screen() {
        // 720px tall: centring would put the window 100px from the top, inside
        // the strip where a maximized window's close button lives.
        let work_area = Rect::new(0, 0, 1280, 720);

        let (_, y) = dock_position(work_area, WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(y, MARGIN);
        assert!(y + HEIGHT as i32 <= 720, "the window ran off the bottom");
    }

    #[test]
    fn staying_on_screen_beats_the_top_margin() {
        // Too short to honour both: the margin is sacrificed rather than
        // letting the window hang off the bottom edge.
        let work_area = Rect::new(0, 0, 1280, 600);

        let (_, y) = dock_position(work_area, WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(y, 600 - 520);
        assert!(y >= 0);
    }

    #[test]
    fn a_window_taller_than_the_work_area_is_pinned_to_the_top() {
        let work_area = Rect::new(0, 0, 1280, 400);

        let (_, y) = dock_position(work_area, WIDTH, HEIGHT, RailSide::Right, MARGIN);

        assert_eq!(y, 0);
    }

    #[test]
    fn scaling_converts_the_margin_to_physical_pixels() {
        // At 150% the same logical margin has to become 150% of the pixels, or
        // the rail creeps up into the close button on a scaled display.
        assert_eq!(to_physical(MIN_TOP_MARGIN_LOGICAL, 1.0), 120);
        assert_eq!(to_physical(MIN_TOP_MARGIN_LOGICAL, 1.5), 180);
        assert_eq!(to_physical(MIN_TOP_MARGIN_LOGICAL, 2.0), 240);
    }

    #[test]
    fn the_window_always_lands_inside_the_work_area() {
        // A sweep over plausible and implausible arrangements, since the real
        // machine only ever exercises one of them.
        let arrangements = [
            Rect::new(0, 0, 1920, 1040),
            Rect::new(0, 40, 1920, 1040),
            Rect::new(-2560, -400, 2560, 1400),
            Rect::new(1920, 0, 3840, 2120),
            Rect::new(0, 0, 1366, 728),
            Rect::new(0, 0, 1024, 600),
        ];

        for work_area in arrangements {
            for side in [RailSide::Right, RailSide::Left] {
                let (x, y) = dock_position(work_area, WIDTH, HEIGHT, side, MARGIN);

                assert!(
                    x >= work_area.x && x + WIDTH as i32 <= work_area.right(),
                    "{side:?} on {work_area:?} placed x={x} outside the work area"
                );
                assert!(
                    y >= work_area.y,
                    "{side:?} on {work_area:?} placed y={y} above the work area"
                );
            }
        }
    }

    #[test]
    fn rect_containment_is_half_open() {
        // Used by the cursor hit test: the right and bottom edges belong to the
        // next pixel, so two adjacent rects never both claim the same point.
        let rect = Rect::new(10, 20, 100, 50);

        assert!(rect.contains(10, 20));
        assert!(rect.contains(109, 69));
        assert!(!rect.contains(110, 69));
        assert!(!rect.contains(109, 70));
        assert!(!rect.contains(9, 20));
    }
}
