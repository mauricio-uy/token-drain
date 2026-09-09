//! The shape of Claude's usage response, and its translation into
//! [`UsageWindow`].
//!
//! This interface is undocumented and has changed before, so **every field is
//! optional**. A payload that drops a field we expected, or adds one we have
//! never seen, must degrade to "that window is unavailable" rather than fail the
//! whole fetch.

use serde::Deserialize;

use crate::providers::timestamps::{parse_reset_timestamp, ResetValue};
use crate::providers::usage::UsageWindow;

/// Top-level usage payload.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClaudeUsageResponse {
    /// The rolling session window.
    pub five_hour: Option<ClaudeUsageWindow>,
    /// The rolling weekly window, across all models.
    pub seven_day: Option<ClaudeUsageWindow>,
    /// Model-scoped limits. Parsed so an unexpected payload still deserializes,
    /// but not mapped in v1: the UI shows the session and weekly windows only.
    pub limits: Option<Vec<ClaudeScopedLimit>>,
}

/// One window as the provider expresses it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClaudeUsageWindow {
    /// Newer name for the consumed percentage.
    pub utilization: Option<f64>,
    /// Older name for the same value. Both have been observed in the wild.
    pub used_percentage: Option<f64>,
    pub resets_at: Option<ResetValue>,
}

/// A model-scoped limit entry. Retained for forward compatibility.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClaudeScopedLimit {
    pub kind: Option<String>,
    pub percent: Option<f64>,
    pub resets_at: Option<ResetValue>,
    pub is_active: Option<bool>,
    pub scope: Option<ClaudeLimitScope>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClaudeLimitScope {
    pub model: Option<ClaudeLimitScopeModel>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClaudeLimitScopeModel {
    pub display_name: Option<String>,
}

impl ClaudeUsageWindow {
    /// The consumed percentage, preferring the newer field name.
    ///
    /// Both names have been served by this endpoint. Preferring `utilization`
    /// and falling back to `used_percentage` means a rename in either direction
    /// keeps working.
    fn used_percent(&self) -> Option<f64> {
        self.utilization
            .filter(|value| value.is_finite())
            .or_else(|| self.used_percentage.filter(|value| value.is_finite()))
    }
}

/// Translate one raw window into a [`UsageWindow`].
///
/// Returns `None` when the window is absent or carries no usable percentage.
/// That is a normal outcome, not an error: the provider genuinely does not
/// report every window on every plan.
pub fn map_window(raw: Option<&ClaudeUsageWindow>, window_minutes: u32) -> Option<UsageWindow> {
    let raw = raw?;
    let used_percent = raw.used_percent()?;
    let resets_at = parse_reset_timestamp(raw.resets_at.as_ref());

    UsageWindow::new(used_percent, window_minutes, resets_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::usage::{SESSION_WINDOW_MINUTES, WEEKLY_WINDOW_MINUTES};

    /// Hand-written sample of the documented response shape. Not captured from
    /// a real account.
    const SAMPLE: &str = r#"{
        "five_hour": {
            "utilization": 73,
            "used_percentage": 73,
            "resets_at": "2026-09-04T18:41:00Z"
        },
        "seven_day": {
            "utilization": 7,
            "used_percentage": 7,
            "resets_at": 1788580800
        },
        "limits": [
            {
                "kind": "weekly_scoped",
                "percent": 12,
                "resets_at": "2026-09-05T04:00:00Z",
                "is_active": true,
                "scope": { "model": { "display_name": "Some Model" } }
            }
        ]
    }"#;

    #[test]
    fn maps_both_windows_from_the_documented_shape() {
        let response: ClaudeUsageResponse = serde_json::from_str(SAMPLE).expect("should parse");

        let session = map_window(response.five_hour.as_ref(), SESSION_WINDOW_MINUTES)
            .expect("session window should map");
        let weekly = map_window(response.seven_day.as_ref(), WEEKLY_WINDOW_MINUTES)
            .expect("weekly window should map");

        assert_eq!(session.used_percent, 73.0);
        assert_eq!(session.window_minutes, 300);
        // 2026-09-04T18:41:00Z, cross-checked against an independent Date.parse.
        assert_eq!(session.resets_at, Some(1_788_547_260_000));

        assert_eq!(weekly.used_percent, 7.0);
        assert_eq!(weekly.window_minutes, 10_080);
        // Same field, delivered as a seconds epoch rather than a string.
        assert_eq!(weekly.resets_at, Some(1_788_580_800_000));
    }

    #[test]
    fn prefers_utilization_over_used_percentage() {
        // Why: if the two ever disagree, the newer field is the one to trust.
        let raw = ClaudeUsageWindow {
            utilization: Some(42.0),
            used_percentage: Some(11.0),
            resets_at: None,
        };

        assert_eq!(
            map_window(Some(&raw), SESSION_WINDOW_MINUTES)
                .unwrap()
                .used_percent,
            42.0
        );
    }

    #[test]
    fn falls_back_to_used_percentage_when_utilization_is_absent() {
        let raw: ClaudeUsageWindow =
            serde_json::from_str(r#"{"used_percentage": 64.5}"#).expect("should parse");

        assert_eq!(
            map_window(Some(&raw), SESSION_WINDOW_MINUTES)
                .unwrap()
                .used_percent,
            64.5
        );
    }

    #[test]
    fn falls_back_when_utilization_is_present_but_not_finite() {
        let raw = ClaudeUsageWindow {
            utilization: Some(f64::NAN),
            used_percentage: Some(30.0),
            resets_at: None,
        };

        assert_eq!(
            map_window(Some(&raw), SESSION_WINDOW_MINUTES)
                .unwrap()
                .used_percent,
            30.0
        );
    }

    #[test]
    fn clamps_an_out_of_range_percentage() {
        let raw: ClaudeUsageWindow =
            serde_json::from_str(r#"{"utilization": 104.2}"#).expect("should parse");

        assert_eq!(
            map_window(Some(&raw), SESSION_WINDOW_MINUTES)
                .unwrap()
                .used_percent,
            100.0
        );
    }

    #[test]
    fn maps_a_window_with_no_reset_time() {
        let raw: ClaudeUsageWindow =
            serde_json::from_str(r#"{"utilization": 50}"#).expect("should parse");

        let window = map_window(Some(&raw), SESSION_WINDOW_MINUTES).expect("should map");

        assert_eq!(window.used_percent, 50.0);
        assert_eq!(window.resets_at, None);
    }

    #[test]
    fn returns_none_for_an_absent_or_empty_window() {
        assert!(map_window(None, SESSION_WINDOW_MINUTES).is_none());

        let empty: ClaudeUsageWindow = serde_json::from_str("{}").expect("should parse");
        assert!(map_window(Some(&empty), SESSION_WINDOW_MINUTES).is_none());
    }

    #[test]
    fn tolerates_a_payload_missing_every_known_field() {
        // The whole point of making every field optional: an unrecognized
        // payload yields "unavailable", never a parse failure.
        let response: ClaudeUsageResponse =
            serde_json::from_str(r#"{"something_new": {"nested": true}}"#).expect("should parse");

        assert!(map_window(response.five_hour.as_ref(), SESSION_WINDOW_MINUTES).is_none());
        assert!(map_window(response.seven_day.as_ref(), WEEKLY_WINDOW_MINUTES).is_none());
    }

    #[test]
    fn parses_scoped_limits_without_mapping_them() {
        let response: ClaudeUsageResponse = serde_json::from_str(SAMPLE).expect("should parse");
        let limits = response.limits.expect("limits should be present");

        assert_eq!(limits.len(), 1);
        assert_eq!(limits[0].kind.as_deref(), Some("weekly_scoped"));
        assert_eq!(
            limits[0]
                .scope
                .as_ref()
                .and_then(|scope| scope.model.as_ref())
                .and_then(|model| model.display_name.as_deref()),
            Some("Some Model")
        );
    }
}
