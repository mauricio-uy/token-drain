# Claude subscription

Token Drain reads the subscription usage available to a locally signed-in
Claude Code account. It does not measure API-key billing or count tokens from
conversation files.

## Credentials

Sign in through Claude Code first. On each poll, the native Rust provider reads
`%USERPROFILE%\.claude\.credentials.json` and uses
`claudeAiOauth.accessToken`. The file must contain a usable OAuth access token.
Token Drain does not modify the file, refresh tokens, or run the CLI for you.
If credentials expire or are rejected, sign in again through Claude Code and
let the next poll pick up the updated file.

## Request and mapping

The provider sends one authenticated request:

```text
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <local OAuth access token>
anthropic-beta: oauth-2025-04-20
User-Agent: claude-code/2.1.0
```

This is an undocumented client endpoint, not a guaranteed public API.
The response's `five_hour` and `seven_day` objects become the session and
weekly quota windows. Each window uses `utilization`, falling back to
`used_percentage`, as the consumed percentage. `resets_at` supplies the reset
timestamp. Missing windows remain unavailable rather than becoming zero usage.
Model-specific `limits` are parsed but are not displayed as separate quotas.
The endpoint does not supply a plan name used by this integration.

## Refresh and privacy

Healthy providers are polled every five minutes by default, with a configurable
interval and a one-minute minimum. Rate limits respect `Retry-After`; other
transient failures use backoff. The last successful usage snapshot is cached
locally and shown as stale when fresh data is unavailable. Failed requests do
not overwrite successful cached figures. Turning off Claude in Settings stops
its polling.

The access token stays in the Rust backend and is not returned to the UI or
stored in the usage cache. This read-only usage request does not generate a
model response. A changed response contract may require an app update.

Implementation: [credentials](../src-tauri/src/providers/claude/credentials.rs),
[request and mapping](../src-tauri/src/providers/claude/fetch.rs),
[response types](../src-tauri/src/providers/claude/types.rs).
