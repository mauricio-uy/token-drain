# tok-ching

A Windows desktop widget that shows live usage and rate-limit status for AI
subscriptions as a rail docked to the edge of the screen.

Each provider is a progress ring; hovering one opens a card with the current
session window, the weekly window, and when each resets. The rail sits above
other windows, has no taskbar entry, and passes clicks through everywhere except
the rail and the card themselves.

Claude and Codex are supported today.

---

## Install

Download `tok-ching_<version>_x64-setup.exe` and run it. It installs for the
current user into `%LOCALAPPDATA%\tok-ching` and needs no administrator rights.

An `.msi` is also produced. It installs **per machine** and therefore requires
administrator rights; prefer the `.exe` unless you specifically want a
machine-wide install.

> **Windows will warn you the first time.** The installers are not code-signed,
> so SmartScreen shows *"Windows protected your PC"*. Choose **More info** →
> **Run anyway**. This is expected, and it is worth knowing that a paid
> certificate would not remove that warning either — SmartScreen trust is earned
> through download volume, which a personal tool will never accumulate.

You will also need a provider CLI installed and signed in — the app never asks
you for credentials and cannot log you in.

Uninstall from **Settings → Apps**, or run `uninstall.exe` in the install
directory. That removes the program but **leaves your data** (see below); delete
`%APPDATA%\dev.tokching.app` if you want it gone too.

---

## How it works

The app reads the OAuth access tokens that provider CLIs already store in your
home directory, and queries each provider's usage endpoint directly. It does not
ask you for credentials, and it never creates any of its own.

### Files read

| Path | Purpose |
|---|---|
| `%USERPROFILE%\.claude\.credentials.json` | Claude access token |
| `%USERPROFILE%\.codex\auth.json` (or `%CODEX_HOME%\auth.json`) | Codex access token and account id |

**These files are read only.** The app never writes to them. That is a
deliberate choice rather than an oversight: they belong to the CLIs, which
rotate the tokens inside them, and writing would race that rotation and could
strand a single-use refresh token — signing you out of the tool this app is
meant to watch. It follows that the app cannot refresh an expired token either.
When one expires, the badge says so and you sign in again with the CLI.

### Files written

| Path | Purpose |
|---|---|
| `%APPDATA%\dev.tokching.app\settings.json` | Your preferences |
| `%APPDATA%\dev.tokching.app\usage-cache.json` | Last successful figures, so the rail is not empty at launch |
| `%APPDATA%\dev.tokching.app\alerts.json` | Which thresholds have already been announced |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Only while *Launch at login* is on |

None of these contains a token. If the app data directory cannot be resolved for
any reason, it falls back to a folder under `%TEMP%` and keeps working with a
memory that does not survive a reboot.

### Hosts contacted

| Host | Why |
|---|---|
| `api.anthropic.com` | Claude usage |
| `chatgpt.com` | Codex usage |

**Those two, and nothing else.** The app performs no telemetry, no analytics and
no crash reporting. This was verified rather than assumed: a release build was
run with every TCP connection owned by its process sampled and checked against
the addresses those two hostnames resolve to.

One honest caveat. The app hosts Microsoft's **WebView2** runtime to draw its
interface, and that runtime opens its own HTTPS connections to Microsoft. It is
a separate process (`msedgewebview2.exe`) and not something this project wrote or
can fully switch off. The page it renders is bundled locally and requests
nothing. What has been done about it: the runtime is started with background
networking, component update, domain reliability, hyperlink auditing, sync,
crash reporting and client-side phishing detection all disabled, which measurably
removed one of the two endpoints originally observed. One connection remains, and
it could not be attributed to a named service — the runtime resolves hostnames
itself, so nothing about it appears in the system DNS cache.

### Where tokens live at runtime

Tokens are read and used entirely inside the Rust process. They are never passed
to the WebView, never written to any file this app creates, and never logged.
HTTP failures record a status code only — never a header, a body, or a URL with
a query. Errors from reading a credentials file carry the file's path and, for a
syntax error, the line and column, never the surrounding text.

---

## Caveats

**The usage endpoints this app calls are undocumented.** Providers do not
publish a public API for consumer-plan quota, and these interfaces can change or
disappear without notice. When that happens the app shows an error state rather
than stale or invented numbers, but it will need a code change to work again.

**A failure never shows a number.** Any state other than a successful fetch
draws an empty ring and a word — `sign in`, `offline`, `error` — because a `0%`
you cannot distinguish from a fresh quota is worse than no figure at all. The
last known figures remain available in the hover card, labelled with their age.

If a provider CLI is not installed or not signed in, that provider is shown as
needing sign-in; the others keep working. A provider can also be switched off
entirely in settings, which stops the requests as well as hiding the badge.

**Polling is deliberately unhurried.** The default is every five minutes and the
floor is one minute, including for a manual refresh. The point is a quota
readout, not a live telemetry feed, and hammering an undocumented endpoint is
how it gets closed.

**Exclusive-fullscreen games** may cover the rail. Borderless fullscreen — how
most modern games run — does not.

---

## Development

### Prerequisites

- Node.js 20+
- Rust (stable, `x86_64-pc-windows-msvc` host)
- Microsoft Visual Studio Build Tools with the C++ workload, and a Windows SDK
- WebView2 runtime (preinstalled on Windows 11)

### Commands

```sh
npm install         # install frontend dependencies
npm run tauri dev   # run the app in development
npm run build       # build the frontend only
npm run tauri build # produce installers
cargo test          # run the Rust suite, from src-tauri/
```

The first Rust build compiles the full dependency tree and takes several
minutes. Subsequent builds are incremental.

Two environment flags help when working on the interface:

- `VITE_DEBUG_BADGES=1` renders every badge and card state side by side, which
  is the only practical way to look at states that are hard to produce on
  demand.
- `VITE_DEBUG_RAIL=1` shows a counter proving the hover card is moved between
  badges rather than rebuilt.

There are also two live tests that make real authenticated requests against the
providers. They are ignored by default and print what they parsed:

```sh
cargo test --test live_usage -- --ignored --nocapture
```

---

## Stack

Tauri v2 · Rust · React · TypeScript · Vite

## License

MIT — see [LICENSE](LICENSE).
