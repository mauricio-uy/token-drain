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
//! Security: tokens are read, sent to their own provider, and dropped. Tokens,
//! plan names, percentages, reset timestamps, and provider error details are
//! never printed.

use token_drain_lib::providers::claude::ClaudeProvider;
use token_drain_lib::providers::codex::CodexProvider;
use token_drain_lib::providers::http::build_client;
use token_drain_lib::providers::provider::UsageProvider;
use token_drain_lib::providers::usage::ProviderUsage;

/// Assert that a live snapshot carries at least one usable window.
///
/// That assertion is the point: without it, a run where the request still
/// succeeds but the payload has drifted could report no usable data and still
/// pass, which is exactly the silent failure these checks exist to catch.
fn assert_live_contract(provider: &str, usage: &ProviderUsage) {
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
        Ok(usage) => assert_live_contract("claude", &usage),
        Err(_) => panic!("live claude usage fetch failed"),
    }
}

#[tokio::test]
#[ignore = "makes a live authenticated request against the real provider"]
async fn prints_live_codex_usage() {
    let provider = CodexProvider::new(build_client().expect("client should build"));

    match provider.fetch().await {
        Ok(usage) => assert_live_contract("codex", &usage),
        Err(_) => panic!("live codex usage fetch failed"),
    }
}
