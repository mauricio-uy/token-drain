# Claude subscription

Token Drain reads the subscription usage available to a locally signed-in
Claude Code account. It does not measure API-key billing or count tokens from
conversation files.

## Credentials

Sign in through the Claude Code CLI once. On each poll, the native Rust
provider reads `%USERPROFILE%\.claude\.credentials.json` and uses
`claudeAiOauth.accessToken`. Token Drain never runs the CLI for you.

The Claude desktop app authenticates its embedded Claude Code through its own
token store and never updates this file, so someone who works only in the
desktop app is left with an access token that expired long ago. Token Drain
therefore renews it: when the stored `expiresAt` is within five minutes, or a
usage request is rejected with HTTP 401, it exchanges `refreshToken` for a new
token set, the same exchange the CLI performs:

```text
POST https://console.anthropic.com/v1/oauth/token
{"grant_type": "refresh_token", "refresh_token": "<local refresh token>",
 "client_id": "<Claude Code's public OAuth client id>"}
```

The result is written back to the same file, because refresh tokens are single
use and the replacement must reach the CLI too. The write is guarded:

- The file is re-read immediately before writing. If its refresh token changed
  meanwhile, the CLI renewed it first; its tokens are kept and ours discarded.
- Only `accessToken`, `refreshToken`, `expiresAt` and, when the server states
  it, `refreshTokenExpiresAt` are replaced. Every other field is preserved.
- The new contents go to a temporary sibling file that is renamed over the
  original, so the file is never left half-written.

If the refresh token itself is rejected, the badge asks you to sign in again
through Claude Code; the next poll picks up the new file.

Known limitation: when the token endpoint does not state a lifetime for the
replacement refresh token, `refreshTokenExpiresAt` keeps its previous value.
Token Drain does not rely on that field; if the CLI does, it may ask you to
sign in once that stale date passes.

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

The access and refresh tokens stay in the Rust backend and are not returned to
the UI or stored in the usage cache. Neither is logged. The usage request does
not generate a model response. A changed response contract may require an app
update.

Implementation: [credentials](../src-tauri/src/providers/claude/credentials.rs),
[token renewal](../src-tauri/src/providers/claude/refresh.rs),
[request and mapping](../src-tauri/src/providers/claude/fetch.rs),
[response types](../src-tauri/src/providers/claude/types.rs).
