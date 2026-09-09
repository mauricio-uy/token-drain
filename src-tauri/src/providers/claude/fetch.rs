//! The Claude usage request.
//!
//! One authenticated GET against an undocumented endpoint, using the OAuth
//! token the CLI already stored. The request presents the same headers the CLI
//! does, because the endpoint is part of that client's contract and not a
//! public API.
//!
//! Security (S2, S6): the token goes to this one host and nowhere else, and no
//! header or body is ever logged.

use reqwest::Client;

use crate::providers::claude::types::{map_window, ClaudeUsageResponse};
use crate::providers::error::UsageError;
use crate::providers::http::{classify_status, classify_transport_error, retry_after_header};
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

    let status = response.status().as_u16();
    let retry_after = retry_after_header(&response);

    if let Some(error) = classify_status(status, retry_after.as_deref(), now_ms) {
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
        monthly: None,
        // This endpoint does not state a plan name; Codex does.
        plan: None,
        fetched_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_788_547_260_000;

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
}
