//! Go's read-only JSON usage endpoint: three quotas with authoritative ISO resets.

use super::{credentials, read_response};
use crate::providers::error::UsageError;
use crate::providers::http::classify_transport_error;
use crate::providers::provider::UsageProvider;
use crate::providers::usage::{
    ProviderId, ProviderUsage, UsageWindow, MONTHLY_WINDOW_MINUTES, SESSION_WINDOW_MINUTES,
    WEEKLY_WINDOW_MINUTES,
};
use reqwest::Client;
use serde::Deserialize;

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

pub struct GoProvider {
    client: Client,
}

impl GoProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// The credentials file this provider reads, when it can be located.
    pub fn credentials_path(&self) -> Option<std::path::PathBuf> {
        credentials::api_path().ok()
    }
}

impl UsageProvider for GoProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpencodeGo
    }
    async fn fetch(&self) -> Result<ProviderUsage, UsageError> {
        let credential = credentials::read_api()?;
        let response = self
            .client
            .get(USAGE_URL)
            .header(reqwest::header::AUTHORIZATION, credential.0)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(classify_transport_error)?;
        let body = read_response(response).await?;
        parse_usage(&body, chrono::Utc::now().timestamp_millis())
    }
}

#[derive(Deserialize)]
struct Response {
    usage: Windows,
}
#[derive(Deserialize)]
struct Windows {
    rolling: Window,
    weekly: Window,
    monthly: Window,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    percent: f64,
    resets_at: String,
}

fn window(raw: Window, minutes: u32) -> Result<Option<UsageWindow>, UsageError> {
    let reset = chrono::DateTime::parse_from_rfc3339(&raw.resets_at)
        .map_err(|_| UsageError::Parse)?
        .timestamp_millis();
    UsageWindow::new(raw.percent, minutes, Some(reset))
        .map(Some)
        .ok_or(UsageError::Parse)
}

pub fn parse_usage(body: &[u8], now: i64) -> Result<ProviderUsage, UsageError> {
    let response: Response = serde_json::from_slice(body).map_err(|_| UsageError::Parse)?;
    Ok(ProviderUsage {
        provider: ProviderId::OpencodeGo,
        session: window(response.usage.rolling, SESSION_WINDOW_MINUTES)?,
        weekly: window(response.usage.weekly, WEEKLY_WINDOW_MINUTES)?,
        monthly: window(response.usage.monthly, MONTHLY_WINDOW_MINUTES)?,
        plan: Some("Go".into()),
        fetched_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_three_windows_with_exact_reset_datetimes() {
        let usage = parse_usage(br#"{"usage":{"rolling":{"percent":12.5,"resetsAt":"2026-09-08T12:00:00Z"},"weekly":{"percent":101,"resetsAt":"2026-09-15T12:00:00Z"},"monthly":{"percent":-1,"resetsAt":"2026-10-08T09:00:00-03:00"}}}"#, 42).unwrap();
        assert_eq!(usage.session.unwrap().used_percent, 12.5);
        assert_eq!(usage.weekly.unwrap().used_percent, 100.0);
        let monthly = usage.monthly.unwrap();
        assert_eq!(monthly.used_percent, 0.0);
        assert_eq!(monthly.window_minutes, 43_200);
        assert_eq!(
            monthly.resets_at,
            Some(
                chrono::DateTime::parse_from_rfc3339("2026-10-08T12:00:00Z")
                    .unwrap()
                    .timestamp_millis()
            )
        );
    }
    #[test]
    fn incomplete_or_wrong_contracts_do_not_become_zero_usage() {
        for body in [
            b"{}".as_slice(),
            b"<html>Sign in</html>",
            br#"{"usage":{"rolling":{"percent":1,"resetsAt":"bad"}}}"#,
        ] {
            assert!(matches!(parse_usage(body, 0), Err(UsageError::Parse)));
        }
    }
    #[tokio::test]
    #[ignore = "requires a configured OpenCode Go subscription; calls only its usage endpoint"]
    async fn live_go_usage() {
        let provider = GoProvider::new(super::super::build_client().unwrap());
        let usage = provider.fetch().await.expect("Go usage request failed");
        assert!(usage.session.is_some() && usage.weekly.is_some() && usage.monthly.is_some());
        // The normalized snapshot contains no credential or upstream body.
        println!("{}", serde_json::to_string(&usage).unwrap());
    }
}
