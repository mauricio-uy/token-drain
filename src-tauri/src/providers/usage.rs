//! Provider-agnostic usage types.
//!
//! Every provider reports the same underlying idea — "this window is N percent
//! consumed and resets at T" — behind a different payload shape. Provider
//! modules translate into these types, and everything above this layer works
//! only in terms of them.

use serde::Serialize;

/// A five-hour session window, in minutes.
pub const SESSION_WINDOW_MINUTES: u32 = 300;

/// A seven-day window, in minutes.
pub const WEEKLY_WINDOW_MINUTES: u32 = 10_080;

/// One rate-limit window.
///
/// Note what is deliberately absent: any human-readable rendering of the reset
/// time. "Resets in 51 min" versus "Resets Thu 12:00 AM" is a presentation
/// decision that depends on the viewer's locale and on how much time has passed
/// since the fetch, so it belongs to the UI and is recomputed there on every
/// render. Baking a formatted string in here would freeze a countdown at the
/// moment of the request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    /// Percentage of the window consumed, clamped to `0.0..=100.0`.
    pub used_percent: f64,
    /// Nominal duration of the window in minutes.
    pub window_minutes: u32,
    /// Unix milliseconds at which the window resets, when the provider says so.
    pub resets_at: Option<i64>,
}

/// Which provider a usage snapshot came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    Claude,
    Codex,
}

impl ProviderId {
    /// Stable identifier used in caches and log lines.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

/// A complete usage snapshot for one provider at one moment.
///
/// Both windows are optional independently: a provider may report a session
/// window and no weekly window, or neither, depending on the plan. `None` means
/// "not reported", which the UI renders as absent — never as zero.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub provider: ProviderId,
    pub session: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    /// Plan name, when the provider states one.
    pub plan: Option<String>,
    /// Unix milliseconds at which this snapshot was retrieved. Drives the
    /// "stale" presentation after a restart.
    pub fetched_at: i64,
}

impl UsageWindow {
    /// Build a window from a raw percentage, clamping it into range.
    ///
    /// Returns `None` for a non-finite percentage: a `NaN` that reached the UI
    /// would render as a blank or a broken ring rather than as an honest error
    /// state.
    pub fn new(used_percent: f64, window_minutes: u32, resets_at: Option<i64>) -> Option<Self> {
        if !used_percent.is_finite() {
            return None;
        }

        Some(Self {
            used_percent: used_percent.clamp(0.0, 100.0),
            window_minutes,
            resets_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_out_of_range_percentages() {
        // Why: a provider returning 103% or -0.4 is not an error worth failing
        // the whole fetch over, but it must never reach a progress ring raw.
        assert_eq!(
            UsageWindow::new(103.0, SESSION_WINDOW_MINUTES, None).unwrap().used_percent,
            100.0
        );
        assert_eq!(
            UsageWindow::new(-0.4, SESSION_WINDOW_MINUTES, None).unwrap().used_percent,
            0.0
        );
    }

    #[test]
    fn keeps_in_range_percentages_exact() {
        assert_eq!(
            UsageWindow::new(73.5, SESSION_WINDOW_MINUTES, None).unwrap().used_percent,
            73.5
        );
    }

    #[test]
    fn rejects_non_finite_percentages() {
        assert!(UsageWindow::new(f64::NAN, SESSION_WINDOW_MINUTES, None).is_none());
        assert!(UsageWindow::new(f64::INFINITY, SESSION_WINDOW_MINUTES, None).is_none());
    }

    #[test]
    fn serializes_as_camel_case_for_the_frontend() {
        let window = UsageWindow::new(73.0, WEEKLY_WINDOW_MINUTES, Some(1_788_580_800_000)).unwrap();
        let json = serde_json::to_string(&window).expect("should serialize");

        assert!(json.contains("\"usedPercent\":73.0"));
        assert!(json.contains("\"windowMinutes\":10080"));
        assert!(json.contains("\"resetsAt\":1788580800000"));
    }
}
