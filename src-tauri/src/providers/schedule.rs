//! When to poll a provider next.
//!
//! Pure policy: every decision here is a function of the previous outcome and a
//! consecutive-failure count, and returns a delay. Nothing reads a clock, so the
//! rules are testable exactly rather than approximately.
//!
//! The endpoints behind this app are private and undocumented. Polling them
//! aggressively is how an application's traffic pattern gets noticed, so the
//! floor below is a hard rule, not a default.

use std::time::Duration;

use crate::providers::error::UsageError;

/// The fastest this app will ever poll a provider, whatever the configuration
/// says. Not adjustable.
pub const MIN_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Default gap between polls of a healthy provider.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(300);

/// Ceiling on the backoff, so a provider that has been failing for hours is
/// still retried occasionally.
pub const MAX_BACKOFF: Duration = Duration::from_secs(1800);

/// Per-provider polling policy.
#[derive(Debug, Clone)]
pub struct RefreshSchedule {
    interval: Duration,
    consecutive_failures: u32,
}

impl RefreshSchedule {
    /// Build a schedule around a configured interval.
    ///
    /// The interval is clamped up to [`MIN_POLL_INTERVAL`]: a configuration
    /// asking for a 5-second poll is a mistake, and honouring it would be worse
    /// than ignoring it.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval: interval.max(MIN_POLL_INTERVAL),
            consecutive_failures: 0,
        }
    }

    /// The configured interval after clamping.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Adopt a new interval, clamped the same way as at construction.
    ///
    /// Backoff state deliberately survives: a provider has not stopped failing
    /// because the user moved a slider, and resetting the escalation would turn
    /// a settings change into a way to retry a dead endpoint at full speed.
    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval.max(MIN_POLL_INTERVAL);
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Record a successful fetch and return the delay before the next one.
    pub fn record_success(&mut self) -> Duration {
        self.consecutive_failures = 0;
        self.interval
    }

    /// Record a failed fetch and return the delay before the next attempt.
    pub fn record_failure(&mut self, error: &UsageError) -> Duration {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);

        match error {
            // The server said exactly how long to wait. Respect it rather than
            // applying our own curve — but never drop below the floor, however
            // short the header says.
            UsageError::RateLimited {
                retry_after_ms: Some(delay_ms),
            } => Duration::from_millis((*delay_ms).max(0) as u64).max(MIN_POLL_INTERVAL),

            // Nothing the user can do will fix these, and nothing we do will
            // either. Go straight to the ceiling instead of escalating through
            // it: an expired token stays expired until the user runs the CLI,
            // and a changed contract stays changed until the code is updated.
            // Retrying at the ceiling means recovery is still automatic once
            // the underlying problem is resolved.
            error if !error.is_transient() => MAX_BACKOFF,

            // Transient: exponential backoff from the configured interval.
            _ => self.backoff(),
        }
    }

    fn backoff(&self) -> Duration {
        let factor = 1u32.checked_shl(self.consecutive_failures).unwrap_or(u32::MAX);

        self.interval
            .saturating_mul(factor)
            .clamp(MIN_POLL_INTERVAL, MAX_BACKOFF)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::credentials::CredentialError;
    use crate::providers::error::NetworkFailure;
    use std::path::PathBuf;

    const INTERVAL: Duration = Duration::from_secs(300);

    fn schedule() -> RefreshSchedule {
        RefreshSchedule::new(INTERVAL)
    }

    fn network_error() -> UsageError {
        UsageError::Network {
            reason: NetworkFailure::Timeout,
        }
    }

    #[test]
    fn a_healthy_provider_polls_at_the_configured_interval() {
        let mut schedule = schedule();
        assert_eq!(schedule.record_success(), INTERVAL);
    }

    #[test]
    fn a_configured_interval_below_the_floor_is_raised_to_it() {
        // Why: the floor is a hard rule about how this app behaves toward a
        // private endpoint, not a default a setting may override.
        let mut schedule = RefreshSchedule::new(Duration::from_secs(5));

        assert_eq!(schedule.interval(), MIN_POLL_INTERVAL);
        assert_eq!(schedule.record_success(), MIN_POLL_INTERVAL);
    }

    #[test]
    fn transient_failures_back_off_exponentially() {
        let mut schedule = schedule();

        assert_eq!(schedule.record_failure(&network_error()), INTERVAL * 2);
        assert_eq!(schedule.record_failure(&network_error()), INTERVAL * 4);
        // INTERVAL * 8 is 2400s, which is past the ceiling, so the curve stops
        // here rather than continuing to double.
        assert_eq!(schedule.record_failure(&network_error()), MAX_BACKOFF);
    }

    #[test]
    fn backoff_stops_at_the_ceiling() {
        let mut schedule = schedule();
        let mut delay = Duration::ZERO;

        for _ in 0..20 {
            delay = schedule.record_failure(&network_error());
        }

        assert_eq!(delay, MAX_BACKOFF);
    }

    #[test]
    fn a_very_long_failure_streak_does_not_overflow() {
        // Guards the shift in the backoff calculation: 2^64 is not a number
        // Duration can hold, and a panic in the scheduler stops all polling.
        let mut schedule = schedule();

        for _ in 0..200 {
            let delay = schedule.record_failure(&network_error());
            assert!(delay <= MAX_BACKOFF);
        }
    }

    #[test]
    fn a_success_clears_the_backoff() {
        let mut schedule = schedule();

        schedule.record_failure(&network_error());
        schedule.record_failure(&network_error());
        assert_eq!(schedule.consecutive_failures(), 2);

        assert_eq!(schedule.record_success(), INTERVAL);
        assert_eq!(schedule.consecutive_failures(), 0);
    }

    #[test]
    fn a_retry_after_delay_is_honoured() {
        let mut schedule = schedule();

        let delay = schedule.record_failure(&UsageError::RateLimited {
            retry_after_ms: Some(900_000),
        });

        assert_eq!(delay, Duration::from_secs(900));
    }

    #[test]
    fn a_short_retry_after_is_still_floored() {
        // The requirement the plan singles out: the server may ask us back in a
        // second, and we still will not poll faster than the floor.
        let mut schedule = schedule();

        let delay = schedule.record_failure(&UsageError::RateLimited {
            retry_after_ms: Some(1_000),
        });

        assert_eq!(delay, MIN_POLL_INTERVAL);
    }

    #[test]
    fn rate_limiting_without_a_header_falls_back_to_backoff() {
        let mut schedule = schedule();

        let delay = schedule.record_failure(&UsageError::RateLimited {
            retry_after_ms: None,
        });

        assert_eq!(delay, INTERVAL * 2);
    }

    #[test]
    fn permanent_failures_go_straight_to_the_ceiling() {
        // Why not escalate: an expired token stays expired until the user runs
        // the CLI. Escalating through the curve just means more requests that
        // cannot succeed, and the ceiling still allows automatic recovery once
        // the user fixes it.
        for error in [
            UsageError::Unauthorized,
            UsageError::Parse,
            UsageError::MissingCredentials(CredentialError::NotFound {
                path: PathBuf::from("nowhere"),
            }),
        ] {
            let mut schedule = schedule();
            assert_eq!(
                schedule.record_failure(&error),
                MAX_BACKOFF,
                "unexpected delay for {error}"
            );
        }
    }

    #[test]
    fn no_outcome_ever_polls_faster_than_the_floor() {
        // A sweep rather than a spot check: whatever the outcome, the returned
        // delay is at or above the floor.
        let mut schedule = RefreshSchedule::new(Duration::from_secs(1));

        let outcomes = [
            UsageError::Unauthorized,
            UsageError::Parse,
            network_error(),
            UsageError::Server { status: 503 },
            UsageError::RateLimited {
                retry_after_ms: Some(0),
            },
            UsageError::RateLimited {
                retry_after_ms: Some(-5_000),
            },
            UsageError::RateLimited {
                retry_after_ms: None,
            },
        ];

        for error in &outcomes {
            assert!(
                schedule.record_failure(error) >= MIN_POLL_INTERVAL,
                "{error} produced a delay below the floor"
            );
        }

        assert!(schedule.record_success() >= MIN_POLL_INTERVAL);
    }
}
