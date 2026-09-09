//! What the UI receives for each provider.
//!
//! The rest of the app deals in outcomes — a snapshot or a classified error.
//! The rail deals in badges. This module is the single place that translation
//! happens, so the rules about what may and may not appear on screen are stated
//! once and testable.
//!
//! The rule that shapes everything here: **a failure never produces a
//! percentage.** A badge showing `0%` is indistinguishable from a genuinely
//! unused quota, so a provider that could not be reached must look different
//! from one that has consumed nothing.

use serde::Serialize;

use crate::providers::credentials::CredentialError;
use crate::providers::error::UsageError;
use crate::providers::remediation::{remediation_for, Remediation};
use crate::providers::usage::{ProviderId, ProviderUsage};

/// How a provider's badge should present itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BadgeState {
    /// No fetch has completed yet and there is nothing cached to show.
    ///
    /// **Deviation from the plan's five states, deliberately.** Those five
    /// describe outcomes, and the first moment after launch is not an outcome.
    /// Rendering it as `unavailable` or `error` would tell the user something is
    /// wrong when nothing is; this is simply "ask again in a second".
    Pending,
    /// Live figures from a fetch that succeeded.
    Ok,
    /// Figures from the cache, with no live result yet this session.
    Stale,
    /// The saved login is missing, unusable, or was rejected. The user must act.
    Reauth,
    /// A temporary problem the app will retry on its own.
    Unavailable,
    /// Something retrying will not fix and signing in will not fix either.
    Error,
}

impl BadgeState {
    /// Whether this state represents a failure the user should see explained.
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Reauth | Self::Unavailable | Self::Error)
    }
}

/// Everything the rail needs to draw one provider.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub provider: ProviderId,
    pub state: BadgeState,

    /// The figures the ring draws.
    ///
    /// Populated **only** for [`BadgeState::Ok`] and [`BadgeState::Stale`]. A
    /// failure leaves this `None`, which is what keeps a broken provider from
    /// rendering as a real percentage.
    pub usage: Option<ProviderUsage>,

    /// The newest successful snapshot known, whatever the current state.
    ///
    /// Separate from `usage` so a failing provider's card can still say "last
    /// known 55%, two hours ago" without the ring claiming that is the figure
    /// now. `fetched_at` on the snapshot carries the age.
    pub last_known: Option<ProviderUsage>,

    /// What the user can do, present exactly when `state` is a failure.
    pub remediation: Option<Remediation>,
}

/// Build the view for one provider.
///
/// `result` is the most recent fetch outcome, or `None` when no fetch has
/// completed yet this session. `cached` is the newest snapshot restored from
/// disk.
pub fn build_view(
    provider: ProviderId,
    result: Option<&Result<ProviderUsage, UsageError>>,
    cached: Option<&ProviderUsage>,
) -> ProviderView {
    match result {
        Some(Ok(usage)) => ProviderView {
            provider,
            state: BadgeState::Ok,
            usage: Some(usage.clone()),
            last_known: Some(usage.clone()),
            remediation: None,
        },

        Some(Err(error)) => ProviderView {
            provider,
            state: state_for(error),
            // Deliberately not the cached value: a failing provider must not
            // present old figures as current.
            usage: None,
            last_known: cached.cloned(),
            remediation: Some(remediation_for(provider, error)),
        },

        None => match cached {
            Some(usage) => ProviderView {
                provider,
                state: BadgeState::Stale,
                usage: Some(usage.clone()),
                last_known: Some(usage.clone()),
                remediation: None,
            },
            None => ProviderView {
                provider,
                state: BadgeState::Pending,
                usage: None,
                last_known: None,
                remediation: None,
            },
        },
    }
}

/// Classify a failure into the badge state that describes it.
fn state_for(error: &UsageError) -> BadgeState {
    match error {
        UsageError::Unauthorized => BadgeState::Reauth,

        UsageError::MissingCredentials(credential_error) => match credential_error {
            // A sign-in produces a valid file, so these are all "sign in".
            CredentialError::NotFound { .. }
            | CredentialError::MissingField { .. }
            | CredentialError::Malformed { .. } => BadgeState::Reauth,
            // Signing in writes the same file to the same place. If the OS will
            // not hand it over, or there is no home directory, doing that again
            // changes nothing.
            CredentialError::Unreadable { .. } | CredentialError::NoHomeDirectory => {
                BadgeState::Error
            }
        },

        UsageError::RateLimited { .. } | UsageError::Server { .. } | UsageError::Network { .. } => {
            BadgeState::Unavailable
        }

        // The undocumented contract moved. Retrying repeats the failure and
        // signing in is unrelated.
        UsageError::Parse => BadgeState::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::error::NetworkFailure;
    use crate::providers::usage::{UsageWindow, SESSION_WINDOW_MINUTES};
    use std::path::PathBuf;

    fn usage(used_percent: f64) -> ProviderUsage {
        ProviderUsage {
            provider: ProviderId::Claude,
            session: UsageWindow::new(used_percent, SESSION_WINDOW_MINUTES, None),
            weekly: None,
            monthly: None,
            plan: None,
            fetched_at: 1_000,
        }
    }

    fn all_errors() -> Vec<UsageError> {
        vec![
            UsageError::Unauthorized,
            UsageError::Parse,
            UsageError::RateLimited {
                retry_after_ms: Some(1_000),
            },
            UsageError::Server { status: 503 },
            UsageError::Network {
                reason: NetworkFailure::Timeout,
            },
            UsageError::MissingCredentials(CredentialError::NotFound {
                path: PathBuf::from("nowhere"),
            }),
            UsageError::MissingCredentials(CredentialError::Malformed {
                path: PathBuf::from("nowhere"),
                line: 1,
                column: 1,
            }),
            UsageError::MissingCredentials(CredentialError::MissingField {
                path: PathBuf::from("nowhere"),
                field: "access token",
            }),
            UsageError::MissingCredentials(CredentialError::Unreadable {
                path: PathBuf::from("nowhere"),
                kind: std::io::ErrorKind::PermissionDenied,
            }),
            UsageError::MissingCredentials(CredentialError::NoHomeDirectory),
        ]
    }

    // --- Every state is reachable ---

    #[test]
    fn nothing_fetched_and_nothing_cached_is_pending() {
        let view = build_view(ProviderId::Claude, None, None);

        assert_eq!(view.state, BadgeState::Pending);
        assert!(view.usage.is_none());
        assert!(view.remediation.is_none());
    }

    #[test]
    fn cached_data_before_the_first_fetch_is_stale() {
        let cached = usage(55.0);
        let view = build_view(ProviderId::Claude, None, Some(&cached));

        assert_eq!(view.state, BadgeState::Stale);
        assert_eq!(
            view.usage
                .as_ref()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .used_percent,
            55.0
        );
        assert!(view.remediation.is_none());
    }

    #[test]
    fn a_successful_fetch_is_ok() {
        let live = Ok(usage(73.0));
        let view = build_view(ProviderId::Claude, Some(&live), None);

        assert_eq!(view.state, BadgeState::Ok);
        assert_eq!(
            view.usage
                .as_ref()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .used_percent,
            73.0
        );
        assert!(view.remediation.is_none());
    }

    #[test]
    fn a_rejected_token_is_reauth() {
        let failed = Err(UsageError::Unauthorized);
        let view = build_view(ProviderId::Claude, Some(&failed), None);

        assert_eq!(view.state, BadgeState::Reauth);
        assert_eq!(view.remediation.as_ref().unwrap().command, Some("claude"));
    }

    #[test]
    fn a_transient_problem_is_unavailable() {
        for error in [
            UsageError::RateLimited {
                retry_after_ms: None,
            },
            UsageError::Server { status: 500 },
            UsageError::Network {
                reason: NetworkFailure::Connect,
            },
        ] {
            let failed = Err(error);
            let view = build_view(ProviderId::Claude, Some(&failed), None);

            assert_eq!(view.state, BadgeState::Unavailable);
        }
    }

    #[test]
    fn a_changed_contract_is_error() {
        let failed = Err(UsageError::Parse);
        let view = build_view(ProviderId::Claude, Some(&failed), None);

        assert_eq!(view.state, BadgeState::Error);
    }

    #[test]
    fn an_unreadable_credentials_file_is_error_not_reauth() {
        // Why: signing in rewrites the same file to the same place. Sending the
        // user to do that would waste their time on a problem it cannot fix.
        let failed = Err(UsageError::MissingCredentials(
            CredentialError::Unreadable {
                path: PathBuf::from("nowhere"),
                kind: std::io::ErrorKind::PermissionDenied,
            },
        ));
        let view = build_view(ProviderId::Claude, Some(&failed), None);

        assert_eq!(view.state, BadgeState::Error);
    }

    #[test]
    fn a_corrupt_credentials_file_is_reauth() {
        // The opposite case: a sign-in does produce a valid file here.
        let failed = Err(UsageError::MissingCredentials(CredentialError::Malformed {
            path: PathBuf::from("nowhere"),
            line: 1,
            column: 1,
        }));
        let view = build_view(ProviderId::Claude, Some(&failed), None);

        assert_eq!(view.state, BadgeState::Reauth);
    }

    // --- The invariant ---

    #[test]
    fn no_failure_state_ever_carries_a_percentage() {
        // The rule this module exists to enforce, swept across every error, with
        // and without a cache behind it. A badge showing 0% is indistinguishable
        // from an unused quota.
        for error in all_errors() {
            let cached = usage(55.0);
            let failed = Err(error);

            for cache in [None, Some(&cached)] {
                let view = build_view(ProviderId::Claude, Some(&failed), cache);

                assert!(view.state.is_failure(), "{:?} is not a failure", view.state);
                assert!(
                    view.usage.is_none(),
                    "a failure state carried usage: {view:?}"
                );
            }
        }
    }

    #[test]
    fn a_failure_still_remembers_the_last_good_figures() {
        // Separate from `usage` on purpose: the card can say "last known 55%"
        // while the ring shows no reading at all.
        let cached = usage(55.0);
        let failed = Err(UsageError::Unauthorized);

        let view = build_view(ProviderId::Claude, Some(&failed), Some(&cached));

        assert!(view.usage.is_none());
        assert_eq!(
            view.last_known
                .as_ref()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .used_percent,
            55.0
        );
        assert_eq!(view.last_known.as_ref().unwrap().fetched_at, 1_000);
    }

    #[test]
    fn guidance_is_present_exactly_when_the_state_is_a_failure() {
        for error in all_errors() {
            let failed = Err(error);
            let view = build_view(ProviderId::Claude, Some(&failed), None);
            assert!(
                view.remediation.is_some(),
                "{:?} had no guidance",
                view.state
            );
        }

        let live = Ok(usage(10.0));
        assert!(build_view(ProviderId::Claude, Some(&live), None)
            .remediation
            .is_none());
        assert!(build_view(ProviderId::Claude, None, None)
            .remediation
            .is_none());
    }

    #[test]
    fn serializes_for_the_frontend() {
        let failed = Err(UsageError::Unauthorized);
        let cached = usage(55.0);
        let view = build_view(ProviderId::Claude, Some(&failed), Some(&cached));

        let json = serde_json::to_value(&view).expect("should serialize");

        assert_eq!(json["provider"], "claude");
        assert_eq!(json["state"], "reauth");
        assert!(json["usage"].is_null());
        assert_eq!(json["lastKnown"]["session"]["usedPercent"], 55.0);
        assert_eq!(json["remediation"]["command"], "claude");
        assert_eq!(json["remediation"]["resolvesItself"], false);
    }
}
