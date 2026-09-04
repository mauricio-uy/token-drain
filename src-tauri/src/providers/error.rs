//! Failure classification for usage fetches.
//!
//! The UI renders a different badge state for each variant here, so the
//! granularity matters: "sign in again" and "the provider is down" call for
//! different reactions from the user, and neither may be shown as `0%`.
//!
//! Security: no variant carries a token, a request header, or a response body.
//! Only status codes and coarse failure kinds cross this boundary.

use crate::providers::credentials::CredentialError;

/// Why a network request could not be completed.
///
/// Deliberately coarse rather than wrapping the underlying client error: the
/// client's own message can embed request detail, and none of it is needed to
/// decide what to show or whether to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkFailure {
    /// The request exceeded its deadline.
    Timeout,
    /// The connection could not be established (offline, DNS, TLS).
    Connect,
    /// Anything else that prevented the round trip.
    Request,
}

impl std::fmt::Display for NetworkFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Timeout => "timed out",
            Self::Connect => "could not connect",
            Self::Request => "request failed",
        };
        f.write_str(text)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    /// No usable credentials on disk. Carries the specific credential problem
    /// so the UI can distinguish "never signed in" from "file is corrupt".
    #[error("no usable credentials: {0}")]
    MissingCredentials(#[from] CredentialError),

    /// The provider rejected the stored token. The user must reauthenticate
    /// through the CLI; this app does not own the login.
    #[error("the provider rejected the stored token (HTTP 401)")]
    Unauthorized,

    /// The provider is throttling us. `retry_after_ms` is a delay, not an
    /// absolute time, so the scheduler can apply it without a clock read.
    #[error("rate limited by the provider (HTTP 429)")]
    RateLimited { retry_after_ms: Option<i64> },

    /// Any non-success status that is not 401 or 429.
    ///
    /// Named for the 5xx case it usually is, but it deliberately also catches
    /// 403 and 404: an endpoint that has moved or been withdrawn must surface
    /// as a visible failure, never be mistaken for an empty success.
    #[error("the provider returned HTTP {status}")]
    Server { status: u16 },

    #[error("the request to the provider {reason}")]
    Network { reason: NetworkFailure },

    /// The response arrived but did not deserialize. This is the signal that
    /// the undocumented contract has changed.
    #[error("the provider response could not be parsed")]
    Parse,
}

impl UsageError {
    /// Whether retrying later could plausibly succeed without user action.
    ///
    /// Used by the scheduler in Step 3.1: an unauthorized token or a changed
    /// contract will not fix itself, so hammering the endpoint achieves nothing
    /// but a traffic pattern worth flagging.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Server { .. } | Self::Network { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn transient_failures_are_worth_retrying() {
        assert!(UsageError::Network {
            reason: NetworkFailure::Timeout
        }
        .is_transient());
        assert!(UsageError::Server { status: 503 }.is_transient());
        assert!(UsageError::RateLimited {
            retry_after_ms: Some(1_000)
        }
        .is_transient());
    }

    #[test]
    fn permanent_failures_are_not_retried() {
        // Why: neither of these fixes itself. Retrying just generates traffic.
        assert!(!UsageError::Unauthorized.is_transient());
        assert!(!UsageError::Parse.is_transient());
        assert!(!UsageError::MissingCredentials(CredentialError::NotFound {
            path: PathBuf::from("nowhere")
        })
        .is_transient());
    }

    #[test]
    fn rendered_errors_stay_free_of_request_detail() {
        // Guards S6: these strings reach logs, so they may carry a status code
        // and nothing else from the exchange.
        let rendered = UsageError::Server { status: 503 }.to_string();
        assert_eq!(rendered, "the provider returned HTTP 503");

        let rendered = UsageError::Network {
            reason: NetworkFailure::Connect,
        }
        .to_string();
        assert_eq!(rendered, "the request to the provider could not connect");
    }
}
