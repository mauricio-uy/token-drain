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

Go shows 5-hour, weekly and monthly quotas. Its badge shows only the 5h and 7d
values; the hover card also shows the monthly quota. Monthly duration is
nominally 30 days in the data model, but the reset datetime always comes from
the provider, never from adding 30 days. Hover a reset label for its full local
date and time. Missing/rejected credentials show a sign-in state, not zero usage.

Contract reference: [official usage route](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/go/v1/usage.ts).

## Zen

Zen is pay as you go. Its badge shows USD balance; the card shows balance,
reported monthly spend, any configured monthly spending limit, and the spend
update date when available. A spending limit is not a subscription quota.
An old or unknown spend update date is shown explicitly; it is not presented as
a verified current-month total. No quota percentage or reset date is invented.

The CLI API key does not grant access to console billing. Configure a web
session locally; do not send it in chat or put it in this repository:

1. Sign in at `https://opencode.ai` and open the intended workspace's billing
   page. Copy its `wrk_...` (or `wk_...`) ID from the URL.
2. In that site's browser developer tools, find its `auth` or `__Host-auth`
   cookie. Treat this as a password: it grants access to your web session.
3. Create `%USERPROFILE%\.config\tok-ching\opencode.credentials.json` with
   string fields `cookie` and `workspaceId`. The cookie string must be in
   `auth=<your local value>` or `__Host-auth=<your local value>` form. Do not
   include unrelated cookies. Keep filesystem access restricted to your Windows
   account; this file is plaintext and is not encrypted by Tok-Ching.
4. Select **Refresh now** from the tray. If the session expires, renew it in that
   file. Tok-Ching does not edit credentials, log in, or make billing changes.

Only `GET https://opencode.ai/workspace/<validated-id>/billing` is requested.
Only allowlisted auth cookies are sent; redirects are not followed. Raw HTML is
size-limited, never logged, never executed and never sent to the widget. The
parser extracts allowlisted numeric billing fields from server-rendered data.
Changed or ambiguous page formats show an error instead of plausible zeros.

The integration is based on the [official console billing query](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/workspace/common.tsx).
This private page format is not a supported public API and can change. A live
Zen billing response must be checked after configuring the session; parser
fixtures alone do not verify a particular account's wire format or permissions.
