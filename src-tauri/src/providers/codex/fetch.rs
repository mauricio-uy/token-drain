//! The Codex usage request.
//!
//! One authenticated GET against an undocumented endpoint, using the token the
//! Codex CLI already stored, and presenting that client's headers.
//!
//! Security (S2, S6): the token goes to this one host and nowhere else, and no
//! header or body is ever logged.

use reqwest::Client;

use crate::providers::codex::credentials::CodexCredentials;
use crate::providers::codex::types::CodexUsageResponse;
use crate::providers::codex::windows::classify_windows;
use crate::providers::error::UsageError;
use crate::providers::http::{classify_status, classify_transport_error, retry_after_header};
use crate::providers::usage::{ProviderId, ProviderUsage};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Client identity headers. As with Claude, this endpoint serves a specific
/// client rather than the public; presenting anything else invites being
/// treated as unknown traffic.
const CLIENT_USER_AGENT: &str = "codex-cli";
const OPENAI_BETA: &str = "codex-1";
const ORIGINATOR: &str = "Codex Desktop";

/// Header carrying the account id, sent only when the CLI stored one.
const ACCOUNT_ID_HEADER: &str = "ChatGPT-Account-Id";

/// Fetch the current usage snapshot for the given credentials.
pub async fn fetch_codex_usage(
    client: &Client,
    credentials: &CodexCredentials,
    now_ms: i64,
) -> Result<ProviderUsage, UsageError> {
    let mut request = client
        .get(USAGE_URL)
        .bearer_auth(&credentials.access_token)
        .header(reqwest::header::USER_AGENT, CLIENT_USER_AGENT)
        .header("OpenAI-Beta", OPENAI_BETA)
        .header("originator", ORIGINATOR);

    if let Some(account_id) = &credentials.account_id {
        request = request.header(ACCOUNT_ID_HEADER, account_id);
    }

    let response = request.send().await.map_err(classify_transport_error)?;

    let status = response.status().as_u16();
    let retry_after = retry_after_header(&response);

    if let Some(error) = classify_status(status, retry_after.as_deref(), now_ms) {
        return Err(error);
    }

    let payload: CodexUsageResponse = response.json().await.map_err(|_| UsageError::Parse)?;

    map_response(&payload, now_ms)
}

/// Translate a raw payload into a provider snapshot.
///
/// Pure, so the mapping is tested without a network round trip.
///
/// Fails when `plan_type` is absent. That field is the marker distinguishing a
/// real usage payload from an error body or a redirect page that happens to be
/// valid JSON — without the check, a signed-out session could deserialize into
/// an empty-but-successful snapshot and render as "no limits reported".
pub fn map_response(
    payload: &CodexUsageResponse,
    fetched_at: i64,
) -> Result<ProviderUsage, UsageError> {
    let Some(plan) = payload.plan_type.clone() else {
        return Err(UsageError::Parse);
    };

    let classified = classify_windows(payload.rate_limit.as_ref());

    Ok(ProviderUsage {
        provider: ProviderId::Codex,
        session: classified.session,
        weekly: classified.weekly,
        monthly: None,
        billing: None,
        plan: Some(plan),
        fetched_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_788_547_260_000;

    #[test]
    fn maps_a_full_payload_into_a_snapshot() {
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

        let usage = map_response(&payload, NOW_MS).expect("should map");

        assert_eq!(usage.provider, ProviderId::Codex);
        assert_eq!(usage.plan.as_deref(), Some("some_plan"));
        assert_eq!(usage.fetched_at, NOW_MS);
        assert_eq!(usage.session.as_ref().unwrap().used_percent, 21.0);
        assert_eq!(usage.session.as_ref().unwrap().window_minutes, 300);
        assert_eq!(usage.weekly.as_ref().unwrap().used_percent, 44.0);
        assert_eq!(usage.weekly.as_ref().unwrap().window_minutes, 10_080);
    }

    #[test]
    fn rejects_a_payload_without_a_plan_type() {
        // Why: an error body or a sign-in redirect can be valid JSON. Without
        // this check it would deserialize into an empty snapshot and render as
        // "no limits reported", which looks like a working account with no
        // usage.
        let payload: CodexUsageResponse =
            serde_json::from_str(r#"{"detail": "unauthorized"}"#).expect("should parse");

        assert!(matches!(
            map_response(&payload, NOW_MS),
            Err(UsageError::Parse)
        ));
    }

    #[test]
    fn accepts_a_plan_with_no_windows_reported() {
        // A real payload that genuinely reports no limits is different from a
        // payload that is not a usage response at all.
        let payload: CodexUsageResponse =
            serde_json::from_str(r#"{"plan_type": "some_plan"}"#).expect("should parse");

        let usage = map_response(&payload, NOW_MS).expect("should map");

        assert_eq!(usage.plan.as_deref(), Some("some_plan"));
        assert!(usage.session.is_none());
        assert!(usage.weekly.is_none());
    }

    #[test]
    fn respects_the_duration_based_classification() {
        // Same data as the first test with the windows swapped: the mapping
        // must be identical.
        let payload: CodexUsageResponse = serde_json::from_str(
            r#"{
                "plan_type": "some_plan",
                "rate_limit": {
                    "primary_window": {"used_percent": 44, "limit_window_seconds": 604800},
                    "secondary_window": {"used_percent": 21, "limit_window_seconds": 18000}
                }
            }"#,
        )
        .expect("should parse");

        let usage = map_response(&payload, NOW_MS).expect("should map");

        assert_eq!(usage.session.as_ref().unwrap().used_percent, 21.0);
        assert_eq!(usage.weekly.as_ref().unwrap().used_percent, 44.0);
    }
}
