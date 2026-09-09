//! Live verification of the provider usage contracts.
//!
//! These are the GO / NO-GO checks: each reads the real credentials the
//! corresponding CLI stored on this machine and makes one authenticated request
//! to the real endpoint. They are `#[ignore]`d so they never run as part of the
//! normal suite.
//!
//! Run them deliberately:
//!
//! ```text
//! cargo test --test live_usage -- --ignored --nocapture
//! ```
//!
//! Security: tokens are read, sent to their own provider, and dropped. They are
//! never printed; the only output is percentages and reset timestamps.

use token_drain_lib::providers::claude::ClaudeProvider;
use token_drain_lib::providers::codex::CodexProvider;
use token_drain_lib::providers::http::build_client;
use token_drain_lib::providers::provider::UsageProvider;
use token_drain_lib::providers::usage::{ProviderUsage, UsageWindow};

fn describe(provider: &str, label: &str, window: Option<&UsageWindow>) -> String {
    match window {
        Some(window) => {
            let resets = window
                .resets_at
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|moment| moment.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_owned());

            format!(
                "{provider}  {label}: {:.0}% used, resets {resets} (window {} min)",
                window.used_percent, window.window_minutes
            )
        }
        None => format!("{provider}  {label}: not reported"),
    }
}

/// Print a snapshot and assert it carries at least one usable window.
///
/// That assertion is the point: without it, a run where the request still
/// succeeds but the payload has drifted would print two "not reported" lines and
/// pass, which is exactly the silent failure these checks exist to catch.
fn report(provider: &str, usage: &ProviderUsage) {
    if let Some(plan) = &usage.plan {
        println!("{provider}  plan:    {plan}");
    }
    println!("{}", describe(provider, "session", usage.session.as_ref()));
    println!("{}", describe(provider, "weekly ", usage.weekly.as_ref()));

    assert!(
        usage.session.is_some() || usage.weekly.is_some(),
        "{provider}: the response parsed but reported no usable window - the contract has drifted"
    );
}

#[tokio::test]
#[ignore = "makes a live authenticated request against the real provider"]
async fn prints_live_claude_usage() {
    let provider = ClaudeProvider::new(build_client().expect("client should build"));

    match provider.fetch().await {
        Ok(usage) => report("claude", &usage),
        Err(error) => panic!("live claude usage fetch failed: {error}"),
    }
}

#[tokio::test]
#[ignore = "makes a live authenticated request against the real provider"]
async fn prints_live_codex_usage() {
    let provider = CodexProvider::new(build_client().expect("client should build"));

    match provider.fetch().await {
        Ok(usage) => report("codex ", &usage),
        Err(error) => panic!("live codex usage fetch failed: {error}"),
    }
}
