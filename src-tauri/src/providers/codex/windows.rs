//! Deciding which Codex window is the session and which is the weekly limit.
//!
//! The payload names its two windows `primary_window` and `secondary_window`.
//! **Those names are positional, not semantic.** Assuming `primary` is always
//! the session window is the obvious shortcut and it is wrong: the provider is
//! free to order them differently, and a plan may report the weekly window
//! first. Getting this backwards puts a 7-day figure on the session ring and
//! nobody would notice until the numbers looked strange.
//!
//! So the decision is made from `limit_window_seconds`, the duration the
//! provider itself declares.

use crate::providers::codex::types::{CodexRateLimit, CodexWindow};
use crate::providers::usage::{UsageWindow, SESSION_WINDOW_MINUTES, WEEKLY_WINDOW_MINUTES};

/// Boundary between "a session window" and "a longer-term window", in minutes.
///
/// One day sits in the wide empty gap between the two real window lengths: a
/// 5-hour session is far below it and a 7-day limit is far above. Placing the
/// boundary here means an unusual-but-plausible variation — a 6-hour session, a
/// 30-day cap — still lands in the right bucket.
const SESSION_BOUNDARY_MINUTES: u32 = 24 * 60;

/// The two windows, resolved into their meanings.
#[derive(Debug, Default, PartialEq)]
pub struct ClassifiedWindows {
    pub session: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Session,
    Weekly,
}

impl Slot {
    fn default_minutes(self) -> u32 {
        match self {
            Self::Session => SESSION_WINDOW_MINUTES,
            Self::Weekly => WEEKLY_WINDOW_MINUTES,
        }
    }

    fn opposite(self) -> Self {
        match self {
            Self::Session => Self::Weekly,
            Self::Weekly => Self::Session,
        }
    }
}

/// Classify a window by its declared duration alone.
fn slot_from_duration(minutes: Option<u32>) -> Option<Slot> {
    minutes.map(|minutes| {
        if minutes <= SESSION_BOUNDARY_MINUTES {
            Slot::Session
        } else {
            Slot::Weekly
        }
    })
}

/// Resolve `primary_window` and `secondary_window` into session and weekly.
pub fn classify_windows(rate_limit: Option<&CodexRateLimit>) -> ClassifiedWindows {
    let Some(rate_limit) = rate_limit else {
        return ClassifiedWindows::default();
    };

    let primary = rate_limit.primary_window.as_ref();
    let secondary = rate_limit.secondary_window.as_ref();

    let (primary_slot, secondary_slot) = assign_slots(
        primary.and_then(CodexWindow::window_minutes),
        secondary.and_then(CodexWindow::window_minutes),
    );

    let mut classified = ClassifiedWindows::default();
    place(&mut classified, primary, primary_slot);
    place(&mut classified, secondary, secondary_slot);
    classified
}

/// Decide which slot each position occupies.
///
/// Ordered from most to least evidence: declared durations that disagree are
/// conclusive, a single declared duration determines both slots, and only with
/// no duration at all does position become the tie-breaker.
fn assign_slots(primary_minutes: Option<u32>, secondary_minutes: Option<u32>) -> (Slot, Slot) {
    match (primary_minutes, secondary_minutes) {
        // Both declared: the shorter window is the session, whatever order the
        // provider listed them in. This is the case the naming trap hides.
        (Some(primary), Some(secondary)) if primary != secondary => {
            if primary < secondary {
                (Slot::Session, Slot::Weekly)
            } else {
                (Slot::Weekly, Slot::Session)
            }
        }

        // Both declared and identical: no signal to separate them, so fall back
        // to position rather than inventing a distinction.
        (Some(_), Some(_)) => (Slot::Session, Slot::Weekly),

        // One declared: it determines its own slot, and the other takes what is
        // left.
        (Some(primary), None) => {
            let slot = slot_from_duration(Some(primary)).expect("duration is present");
            (slot, slot.opposite())
        }
        (None, Some(secondary)) => {
            let slot = slot_from_duration(Some(secondary)).expect("duration is present");
            (slot.opposite(), slot)
        }

        // Neither declared: position is all that is left.
        (None, None) => (Slot::Session, Slot::Weekly),
    }
}

/// Map one raw window into its slot, defaulting the duration when the provider
/// did not state one.
fn place(classified: &mut ClassifiedWindows, raw: Option<&CodexWindow>, slot: Slot) {
    let Some(raw) = raw else { return };
    let Some(used_percent) = raw.used_percent.filter(|value| value.is_finite()) else {
        return;
    };

    let window_minutes = raw
        .window_minutes()
        .unwrap_or_else(|| slot.default_minutes());
    let resets_at = crate::providers::timestamps::parse_reset_timestamp(raw.reset_at.as_ref());

    let window = UsageWindow::new(used_percent, window_minutes, resets_at);

    match slot {
        Slot::Session => classified.session = window,
        Slot::Weekly => classified.weekly = window,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(used_percent: f64, seconds: Option<i64>) -> CodexWindow {
        CodexWindow {
            used_percent: Some(used_percent),
            limit_window_seconds: seconds,
            reset_at: None,
        }
    }

    const FIVE_HOURS: i64 = 18_000;
    const SEVEN_DAYS: i64 = 604_800;

    #[test]
    fn classifies_windows_in_the_expected_order() {
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(21.0, Some(FIVE_HOURS))),
            secondary_window: Some(window(44.0, Some(SEVEN_DAYS))),
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 21.0);
        assert_eq!(classified.session.as_ref().unwrap().window_minutes, 300);
        assert_eq!(classified.weekly.as_ref().unwrap().used_percent, 44.0);
        assert_eq!(classified.weekly.as_ref().unwrap().window_minutes, 10_080);
    }

    #[test]
    fn classifies_windows_correctly_when_they_are_swapped() {
        // The test the plan calls for, and the whole reason this module exists:
        // the same data with the positions reversed must produce the same
        // meaning. A positional implementation passes the previous test and
        // fails this one.
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(44.0, Some(SEVEN_DAYS))),
            secondary_window: Some(window(21.0, Some(FIVE_HOURS))),
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 21.0);
        assert_eq!(classified.session.as_ref().unwrap().window_minutes, 300);
        assert_eq!(classified.weekly.as_ref().unwrap().used_percent, 44.0);
        assert_eq!(classified.weekly.as_ref().unwrap().window_minutes, 10_080);
    }

    #[test]
    fn an_unusual_but_plausible_duration_still_lands_in_the_right_bucket() {
        // A 6-hour session and a 30-day cap: neither is one of the two values
        // we expect, and both must still classify correctly.
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(10.0, Some(30 * 24 * 3_600))),
            secondary_window: Some(window(90.0, Some(6 * 3_600))),
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 90.0);
        assert_eq!(classified.session.as_ref().unwrap().window_minutes, 360);
        assert_eq!(classified.weekly.as_ref().unwrap().used_percent, 10.0);
        assert_eq!(classified.weekly.as_ref().unwrap().window_minutes, 43_200);
    }

    #[test]
    fn a_single_declared_duration_determines_both_slots() {
        // Only the second window declares a duration, and it is a weekly one.
        // The undeclared window therefore has to be the session.
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(21.0, None)),
            secondary_window: Some(window(44.0, Some(SEVEN_DAYS))),
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 21.0);
        // Duration unknown, so the slot's default is used.
        assert_eq!(classified.session.as_ref().unwrap().window_minutes, 300);
        assert_eq!(classified.weekly.as_ref().unwrap().used_percent, 44.0);
    }

    #[test]
    fn a_single_declared_session_duration_pushes_the_other_to_weekly() {
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(21.0, Some(FIVE_HOURS))),
            secondary_window: Some(window(44.0, None)),
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 21.0);
        assert_eq!(classified.weekly.as_ref().unwrap().used_percent, 44.0);
        assert_eq!(classified.weekly.as_ref().unwrap().window_minutes, 10_080);
    }

    #[test]
    fn falls_back_to_position_when_no_duration_is_declared() {
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(21.0, None)),
            secondary_window: Some(window(44.0, None)),
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 21.0);
        assert_eq!(classified.session.as_ref().unwrap().window_minutes, 300);
        assert_eq!(classified.weekly.as_ref().unwrap().used_percent, 44.0);
        assert_eq!(classified.weekly.as_ref().unwrap().window_minutes, 10_080);
    }

    #[test]
    fn falls_back_to_position_when_both_durations_are_identical() {
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(21.0, Some(FIVE_HOURS))),
            secondary_window: Some(window(44.0, Some(FIVE_HOURS))),
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 21.0);
        assert_eq!(classified.weekly.as_ref().unwrap().used_percent, 44.0);
    }

    #[test]
    fn handles_a_single_present_window() {
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(21.0, Some(FIVE_HOURS))),
            secondary_window: None,
        };

        let classified = classify_windows(Some(&rate_limit));

        assert_eq!(classified.session.as_ref().unwrap().used_percent, 21.0);
        assert!(classified.weekly.is_none());
    }

    #[test]
    fn a_window_with_no_percentage_is_not_reported() {
        // Not zero: absent. Zero would be indistinguishable from real data.
        let rate_limit = CodexRateLimit {
            primary_window: Some(CodexWindow {
                used_percent: None,
                limit_window_seconds: Some(FIVE_HOURS),
                reset_at: None,
            }),
            secondary_window: None,
        };

        assert!(classify_windows(Some(&rate_limit)).session.is_none());
    }

    #[test]
    fn an_absent_rate_limit_yields_nothing() {
        assert_eq!(classify_windows(None), ClassifiedWindows::default());
    }

    #[test]
    fn clamps_an_out_of_range_percentage() {
        let rate_limit = CodexRateLimit {
            primary_window: Some(window(120.0, Some(FIVE_HOURS))),
            secondary_window: None,
        };

        assert_eq!(
            classify_windows(Some(&rate_limit))
                .session
                .unwrap()
                .used_percent,
            100.0
        );
    }
}
