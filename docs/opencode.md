# OpenCode usage

## Go

Connect **OpenCode Go** through `opencode auth login` (or `/connect` in OpenCode).
Tok-Ching reads the `opencode-go` API credential from
`%USERPROFILE%\.local\share\opencode\auth.json`, respecting an absolute
`XDG_DATA_HOME` override and `OPENCODE_AUTH_CONTENT` when present.
It never changes that file or refreshes its credentials.

The read-only request is `GET https://opencode.ai/zen/go/v1/usage` with bearer
authentication. No model request or billable generation is made. Redirects are
not followed, responses are capped at 2 MB, and request/body details are not logged.

Go shows 5-hour, 7-day and monthly quotas. The two rings represent 5h and 7d;
the additional `1m` value and hover card show the monthly quota. Monthly duration
is nominally 30 days in the data model, but the reset datetime always comes from
the provider, never from adding 30 days. Hover a reset label for its full local
date and time. Missing/rejected credentials show a sign-in state, not zero usage.

Contract reference: [official usage route](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/go/v1/usage.ts).
