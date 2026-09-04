//! Shared HTTP plumbing for usage requests.
//!
//! Every provider makes the same shape of call: one authenticated GET with a
//! deadline, whose outcome is either a payload or a classified failure. The
//! endpoint and headers differ; the status handling and the rules for keeping
//! request detail out of logs do not, so they live here.

use std::time::Duration;

use reqwest::{Client, StatusCode};

use crate::providers::error::{NetworkFailure, UsageError};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Build the HTTP client used for usage requests.
///
/// Build this once and share it: a fresh client per request discards the
/// connection pool and pays a new TLS handshake on every poll.
pub fn build_client() -> Result<Client, UsageError> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| UsageError::Network {
            reason: NetworkFailure::Request,
        })
}

/// Map a response status onto an error, or `None` when the response is usable.
pub fn classify_status(status: u16, retry_after: Option<&str>, now_ms: i64) -> Option<UsageError> {
    if StatusCode::from_u16(status)
        .map(|code| code.is_success())
        .unwrap_or(false)
    {
        return None;
    }

    match status {
        401 => Some(UsageError::Unauthorized),
        429 => Some(UsageError::RateLimited {
            retry_after_ms: retry_after.and_then(|value| parse_retry_after(value, now_ms)),
        }),
        other => Some(UsageError::Server { status: other }),
    }
}

/// Interpret a `Retry-After` header as a delay in milliseconds.
///
/// The header is defined as either delta-seconds or an HTTP date; both forms are
/// accepted. A date already in the past yields a zero delay rather than a
/// negative one.
pub fn parse_retry_after(value: &str, now_ms: i64) -> Option<i64> {
    let trimmed = value.trim();

    if let Ok(seconds) = trimmed.parse::<i64>() {
        return (seconds >= 0).then_some(seconds.saturating_mul(1000));
    }

    let target = chrono::DateTime::parse_from_rfc2822(trimmed).ok()?;
    Some((target.timestamp_millis() - now_ms).max(0))
}

/// Read the `Retry-After` header, if the response carries a usable one.
pub fn retry_after_header(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Reduce a transport error to a coarse failure kind.
///
/// The underlying error's own message is discarded rather than wrapped: it can
/// embed request detail, and none of it is needed to decide what to display or
/// whether to retry.
pub fn classify_transport_error(error: reqwest::Error) -> UsageError {
    let reason = if error.is_timeout() {
        NetworkFailure::Timeout
    } else if error.is_connect() {
        NetworkFailure::Connect
    } else {
        NetworkFailure::Request
    };

    UsageError::Network { reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_788_547_260_000;

    #[test]
    fn a_success_status_is_not_an_error() {
        assert!(classify_status(200, None, NOW_MS).is_none());
        assert!(classify_status(204, None, NOW_MS).is_none());
    }

    #[test]
    fn maps_401_to_unauthorized() {
        assert!(matches!(
            classify_status(401, None, NOW_MS),
            Some(UsageError::Unauthorized)
        ));
    }

    #[test]
    fn maps_429_to_rate_limited_with_its_delay() {
        assert!(matches!(
            classify_status(429, Some("120"), NOW_MS),
            Some(UsageError::RateLimited {
                retry_after_ms: Some(120_000)
            })
        ));
    }

    #[test]
    fn maps_429_without_a_retry_after_header() {
        assert!(matches!(
            classify_status(429, None, NOW_MS),
            Some(UsageError::RateLimited {
                retry_after_ms: None
            })
        ));
    }

    #[test]
    fn maps_5xx_to_server() {
        assert!(matches!(
            classify_status(503, None, NOW_MS),
            Some(UsageError::Server { status: 503 })
        ));
    }

    #[test]
    fn a_withdrawn_endpoint_is_a_visible_failure() {
        // Why: 403 and 404 are the shapes a moved or revoked endpoint takes.
        // Neither may be mistaken for an empty but successful response.
        assert!(matches!(
            classify_status(404, None, NOW_MS),
            Some(UsageError::Server { status: 404 })
        ));
        assert!(matches!(
            classify_status(403, None, NOW_MS),
            Some(UsageError::Server { status: 403 })
        ));
    }

    #[test]
    fn parses_retry_after_as_delta_seconds() {
        assert_eq!(parse_retry_after("120", NOW_MS), Some(120_000));
        assert_eq!(parse_retry_after("  0 ", NOW_MS), Some(0));
    }

    #[test]
    fn parses_retry_after_as_an_http_date() {
        let target = chrono::DateTime::from_timestamp_millis(NOW_MS + 120_000)
            .expect("valid timestamp")
            .to_rfc2822();

        assert_eq!(parse_retry_after(&target, NOW_MS), Some(120_000));
    }

    #[test]
    fn a_retry_after_date_in_the_past_becomes_no_delay() {
        let target = chrono::DateTime::from_timestamp_millis(NOW_MS - 60_000)
            .expect("valid timestamp")
            .to_rfc2822();

        assert_eq!(parse_retry_after(&target, NOW_MS), Some(0));
    }

    #[test]
    fn rejects_an_unparseable_retry_after() {
        assert_eq!(parse_retry_after("soon", NOW_MS), None);
        assert_eq!(parse_retry_after("", NOW_MS), None);
        assert_eq!(parse_retry_after("-5", NOW_MS), None);
    }

    #[test]
    fn the_client_builds() {
        assert!(build_client().is_ok());
    }
}
