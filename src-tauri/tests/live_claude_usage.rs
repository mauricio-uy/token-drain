//! Live verification of the Claude usage contract.
//!
//! This is the Step 1.5 GO / NO-GO check: it reads the real credentials the CLI
//! stored on this machine and makes one authenticated request to the real
//! endpoint. It is `#[ignore]`d so it never runs as part of the normal suite.
//!
//! Run it deliberately:
//!
//! ```text
//! cargo test --test live_claude_usage -- --ignored --nocapture
//! ```
//!
//! Security: the token is read, sent to the provider, and dropped. It is never
//! printed, and the only output is percentages and reset timestamps.

use tok_ching_lib::providers::claude::credentials::read_credentials;
use tok_ching_lib::providers::claude::fetch::{build_client, fetch_claude_usage};
use tok_ching_lib::providers::usage::UsageWindow;

fn describe(label: &str, window: Option<&UsageWindow>) -> String {
    match window {
        Some(window) => {
            let resets = window
                .resets_at
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|moment| moment.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_owned());

            format!(
                "claude  {label}: {:.0}% used, resets {resets} (window {} min)",
                window.used_percent, window.window_minutes
            )
        }
        None => format!("claude  {label}: not reported"),
    }
}

#[tokio::test]
#[ignore = "makes a live authenticated request against the real provider"]
async fn prints_live_claude_usage() {
    let credentials = match read_credentials() {
        Ok(credentials) => credentials,
        Err(error) => panic!("could not read local credentials: {error}"),
    };

    let client = build_client().expect("client should build");
    let now_ms = chrono::Utc::now().timestamp_millis();

    let usage = match fetch_claude_usage(&client, &credentials.access_token, now_ms).await {
        Ok(usage) => usage,
        Err(error) => panic!("live usage fetch failed: {error}"),
    };

    println!("{}", describe("session", usage.session.as_ref()));
    println!("{}", describe("weekly ", usage.weekly.as_ref()));

    // The GO / NO-GO condition: the response deserialized and carried at least
    // one usable window. A snapshot with neither means the contract has drifted
    // even though the request succeeded.
    assert!(
        usage.session.is_some() || usage.weekly.is_some(),
        "the response parsed but reported no usable window - the contract has drifted"
    );
}
