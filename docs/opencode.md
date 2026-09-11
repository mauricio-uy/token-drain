# OpenCode Go subscription

## Credentials

Connect **OpenCode Go** through `opencode auth login` (or `/connect` in OpenCode).
Token Drain reads the `opencode-go` API credential from
`%USERPROFILE%\.local\share\opencode\auth.json`, respecting an absolute
`XDG_DATA_HOME` override and `OPENCODE_AUTH_CONTENT` when present.
It never changes that file or refreshes its credentials.

## Request and mapping

The read-only request is `GET https://opencode.ai/zen/go/v1/usage` with bearer
authentication. No model request or billable generation is made. Redirects are
not followed, responses are capped at 2 MB, and request/body details are not logged.

Go shows 5-hour, weekly and monthly quotas. Its badge shows only the 5h and 7d
values; the hover card also shows the monthly quota. Monthly duration is
nominally 30 days in the data model, but the reset datetime always comes from
the provider, never from adding 30 days. Hover a reset label for its full local
date and time. Missing/rejected credentials show a sign-in state, not zero usage.

Contract reference: [official usage route](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/go/v1/usage.ts).

The response's `usage.rolling`, `usage.weekly`, and `usage.monthly` objects
provide each window's `percent` and RFC 3339 `resetsAt` value. The backend maps
these to the common session, weekly, and monthly usage model and labels the plan
as `Go`. Invalid or incomplete quota payloads produce a parse error rather than
fabricated zero usage. This integration covers Go quotas, not Zen workspace
billing or other OpenCode provider accounts.

## Refresh and privacy

The credential is re-read for each poll. Healthy polling defaults to five
minutes and is configurable with a one-minute minimum. Rate-limited responses
respect `Retry-After`; transient failures use backoff. Successful snapshots are
cached locally, and failures retain the previous figures marked stale. Turning
off OpenCode Go in Settings stops its network requests.

The credential is never returned to the frontend or written to the usage cache.
If authentication fails, reconnect Go in OpenCode and allow the next poll to
read the new credential. The endpoint can change; parser updates may be needed.

Implementation: [credentials](../src-tauri/src/providers/opencode/credentials.rs),
[request and mapping](../src-tauri/src/providers/opencode/go.rs),
[HTTP safeguards](../src-tauri/src/providers/opencode/mod.rs).
