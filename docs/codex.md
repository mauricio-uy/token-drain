# Codex subscription

Token Drain reads the Codex usage limits associated with a locally signed-in
ChatGPT account. It does not track pay-as-you-go API-key spending or estimate
usage from local conversation transcripts.

## Credentials

Sign in through Codex first. The Rust backend reads
`%USERPROFILE%\.codex\auth.json`, or `auth.json` inside `CODEX_HOME` when that
environment variable is set. It uses `tokens.access_token` and, when present,
`tokens.account_id`. API-key-only files and credentials stored only in an OS
keychain are not read by this file-based integration.

Credentials are re-read on each poll. Token Drain does not modify `auth.json`
or refresh the OAuth session. If the token is rejected, sign in again through
Codex; the next poll can use the updated credentials.

## Request and mapping

The provider sends one authenticated request:

```text
GET https://chatgpt.com/backend-api/wham/usage
Authorization: Bearer <local OAuth access token>
ChatGPT-Account-Id: <local account ID, when available>
User-Agent: codex-cli
OpenAI-Beta: codex-1
originator: Codex Desktop
```

This is an undocumented client endpoint whose contract can change. The native
provider maps `rate_limit.primary_window` and `rate_limit.secondary_window`
into session and weekly quotas. Their position is not assumed to identify the
quota: `limit_window_seconds` determines the window's duration, with fallback
rules when duration information is missing. `used_percent` supplies consumption;
reset fields supply the next reset time. `plan_type`, when present, labels the
subscription. A missing window is unavailable, not an unused quota.

## Refresh and privacy

The default polling interval is five minutes, configurable with a one-minute
minimum. HTTP 429 responses honor `Retry-After`; transient failures use backoff.
Successful figures are cached locally. During a failure the UI distinguishes
the current error from last-known, stale figures, and the good cache is retained.
Disabling Codex in Settings stops its requests.

Tokens and account credentials remain in Rust and are never sent to the WebView
or written to the usage cache. Fetching usage does not request model generation.

Implementation: [credentials](../src-tauri/src/providers/codex/credentials.rs),
[request](../src-tauri/src/providers/codex/fetch.rs),
[window classification](../src-tauri/src/providers/codex/windows.rs).
