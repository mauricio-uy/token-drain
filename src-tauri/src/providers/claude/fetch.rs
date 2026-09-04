//! The Claude usage request.
//!
//! One authenticated GET against an undocumented endpoint, using the OAuth
//! token the CLI already stored. The request presents the same headers the CLI
//! does, because the endpoint is part of that client's contract and not a
//! public API.
//!
//! Security (S2, S6): the token goes to this one host and nowhere else, and no
//! header or body is ever logged. Everything this module can emit about a
//! failure is a status code or a coarse failure kind.

use std::time::Duration;

use reqwest::{Client, StatusCode};

use crate::providers::claude::types::{map_window, ClaudeUsageResponse};
use crate::providers::error::{NetworkFailure, UsageError};
use crate::providers::usage::{
    ProviderId, ProviderUsage, SESSION_WINDOW_MINUTES, WEEKLY_WINDOW_MINUTES,
};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Opt-in header the endpoint requires for OAuth-authenticated callers.
const ANTHROPIC_BETA: &str = "oauth-2025-04-20";

/// Client identity. This is the CLI's own user agent: the endpoint serves this
/// client, so presenting anything else invites being treated as unknown
/// traffic. Expect to bump it when the contract watcher (Phase 7) reports a
/// change.
const CLIENT_USER_AGENT: &str = "claude-code/2.1.0";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Build the HTTP client used for usage requests.
///
/// Callers should build this once and reuse it: a fresh client per request
/// discards the connection pool and pays a new TLS handshake every poll.
pub fn build_client() -> Result<Client, UsageError> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| UsageError::Network {
            reason: NetworkFailure::Request,
        })
}

/// Fetch the current usage snapshot for the given access token.
pub async fn fetch_claude_usage(
    client: &Client,
    token: &str,
    now_ms: i64,
) -> Result<ProviderUsage, UsageError> {
    let response = client
        .get(USAGE_URL)
        .bearer_auth(token)
        .header("anthropic-beta", ANTHROPIC_BETA)
        .header(reqwest::header::USER_AGENT, CLIENT_USER_AGENT)
        .send()
        .await
        .map_err(classify_transport_error)?;

    let status = response.status();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    if let Some(error) = classify_status(status.as_u16(), retry_after.as_deref(), now_ms) {
        return Err(error);
    }

    let payload: ClaudeUsageResponse = response.json().await.map_err(|_| UsageError::Parse)?;

    Ok(map_response(&payload, now_ms))
}

/// Translate a raw payload into a provider snapshot.
///
/// Pure, so the mapping is tested without a network round trip.
pub fn map_response(payload: &ClaudeUsageResponse, fetched_at: i64) -> ProviderUsage {
    ProviderUsage {
        provider: ProviderId::Claude,
        session: map_window(payload.five_hour.as_ref(), SESSION_WINDOW_MINUTES),
        weekly: map_window(payload.seven_day.as_ref(), WEEKLY_WINDOW_MINUTES),
        // This endpoint does not state a plan name; Codex does.
        plan: None,
        fetched_at,
    }
}

/// Map a response status onto an error, or `None` when the response is usable.
fn classify_status(status: u16, retry_after: Option<&str>, now_ms: i64) -> Option<UsageError> {
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
/// The header is defined as either delta-seconds or an HTTP date; both forms
/// are accepted. A date already in the past yields a zero delay rather than a
/// negative one.
fn parse_retry_after(value: &str, now_ms: i64) -> Option<i64> {
    let trimmed = value.trim();

    if let Ok(seconds) = trimmed.parse::<i64>() {
        return (seconds >= 0).then_some(seconds.saturating_mul(1000));
    }

    let target = chrono::DateTime::parse_from_rfc2822(trimmed).ok()?;
    Some((target.timestamp_millis() - now_ms).max(0))
}

/// Reduce a transport error to a coarse failure kind.
///
/// The underlying error's own message is discarded rather than wrapped: it can
/// embed request detail, and none of it is needed to decide what to display or
/// whether to retry.
fn classify_transport_error(error: reqwest::Error) -> UsageError {
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
        // Two minutes past NOW_MS, expressed as an HTTP date.
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
    fn maps_a_full_payload_into_a_snapshot() {
        let payload: ClaudeUsageResponse = serde_json::from_str(
            r#"{
                "five_hour": {"utilization": 73, "resets_at": "2026-09-04T18:41:00Z"},
                "seven_day": {"utilization": 7, "resets_at": 1788580800}
            }"#,
        )
        .expect("should parse");

        let usage = map_response(&payload, NOW_MS);

        assert_eq!(usage.provider, ProviderId::Claude);
        assert_eq!(usage.fetched_at, NOW_MS);
        assert_eq!(usage.session.as_ref().unwrap().used_percent, 73.0);
        assert_eq!(usage.session.as_ref().unwrap().window_minutes, 300);
        assert_eq!(usage.weekly.as_ref().unwrap().used_percent, 7.0);
        assert_eq!(usage.weekly.as_ref().unwrap().window_minutes, 10_080);
        assert!(usage.plan.is_none());
    }

    #[test]
    fn an_empty_payload_yields_a_snapshot_with_no_windows() {
        // Not an error: the response was valid, it just reported nothing. The
        // UI shows "unavailable", which is honest, rather than 0%.
        let payload: ClaudeUsageResponse = serde_json::from_str("{}").expect("should parse");

        let usage = map_response(&payload, NOW_MS);

        assert!(usage.session.is_none());
        assert!(usage.weekly.is_none());
    }

    #[test]
    fn the_client_builds() {
        assert!(build_client().is_ok());
    }
}
