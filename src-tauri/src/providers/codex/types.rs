//! The shape of Codex's usage response.
//!
//! Undocumented, so every field is optional: a payload that drops a field we
//! expect, or adds one we have never seen, must degrade rather than fail.

use serde::Deserialize;

use crate::providers::timestamps::ResetValue;

/// Top-level usage payload.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodexUsageResponse {
    /// Plan name. Its presence is also the marker that the response is a real
    /// usage payload rather than an error body that happened to be valid JSON.
    pub plan_type: Option<String>,
    pub rate_limit: Option<CodexRateLimit>,
}

/// The two rate-limit windows, as the provider positions them.
///
/// The names are positional, not semantic. See
/// [`classify_windows`](super::windows::classify_windows).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodexRateLimit {
    pub primary_window: Option<CodexWindow>,
    pub secondary_window: Option<CodexWindow>,
}

/// One window as the provider expresses it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodexWindow {
    pub used_percent: Option<f64>,
    /// Declared duration of the window. This, not the field name, is what says
    /// whether the window is a session or a weekly limit.
    pub limit_window_seconds: Option<i64>,
    /// Note the singular: this provider spells it `reset_at`, unlike Claude's
    /// `resets_at`.
    pub reset_at: Option<ResetValue>,
}

impl CodexWindow {
    /// The declared duration in whole minutes, rounding up.
    ///
    /// Returns `None` when the provider did not state a duration, which is the
    /// case the classifier has to resolve by other means.
    pub fn window_minutes(&self) -> Option<u32> {
        let seconds = self.limit_window_seconds?;
        if seconds <= 0 {
            return None;
        }

        // Ceiling division, spelled out: `i64::div_ceil` is still unstable.
        // `seconds` is known positive here, so the +59 cannot overflow in
        // practice and saturating protects the pathological case anyway.
        u32::try_from(seconds.saturating_add(59) / 60).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_declared_duration_to_minutes() {
        let window = CodexWindow {
            limit_window_seconds: Some(18_000),
            ..Default::default()
        };
        assert_eq!(window.window_minutes(), Some(300));

        let window = CodexWindow {
            limit_window_seconds: Some(604_800),
            ..Default::default()
        };
        assert_eq!(window.window_minutes(), Some(10_080));
    }

    #[test]
    fn rounds_a_partial_minute_up() {
        // Why: a window reported as 299 minutes and 1 second is a 300-minute
        // window that lost a second in transit, not a 299-minute one.
        let window = CodexWindow {
            limit_window_seconds: Some(17_941),
            ..Default::default()
        };
        assert_eq!(window.window_minutes(), Some(300));
    }

    #[test]
    fn treats_a_missing_or_nonsense_duration_as_unknown() {
        assert_eq!(CodexWindow::default().window_minutes(), None);

        let window = CodexWindow {
            limit_window_seconds: Some(0),
            ..Default::default()
        };
        assert_eq!(window.window_minutes(), None);

        let window = CodexWindow {
            limit_window_seconds: Some(-60),
            ..Default::default()
        };
        assert_eq!(window.window_minutes(), None);
    }

    #[test]
    fn deserializes_the_documented_shape() {
        let payload: CodexUsageResponse = serde_json::from_str(
            r#"{
                "plan_type": "some_plan",
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 21,
                        "limit_window_seconds": 18000,
                        "reset_at": 1788580800
                    },
                    "secondary_window": {
                        "used_percent": 44,
                        "limit_window_seconds": 604800,
                        "reset_at": 1788580800
                    }
                }
            }"#,
        )
        .expect("should parse");

        let rate_limit = payload.rate_limit.expect("rate_limit should be present");
        assert_eq!(payload.plan_type.as_deref(), Some("some_plan"));
        assert_eq!(
            rate_limit.primary_window.unwrap().window_minutes(),
            Some(300)
        );
        assert_eq!(
            rate_limit.secondary_window.unwrap().window_minutes(),
            Some(10_080)
        );
    }

    #[test]
    fn tolerates_a_payload_missing_every_known_field() {
        let payload: CodexUsageResponse =
            serde_json::from_str(r#"{"something_new": true}"#).expect("should parse");

        assert!(payload.plan_type.is_none());
        assert!(payload.rate_limit.is_none());
    }
}
