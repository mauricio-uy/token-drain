# Token Drain

A Windows desktop widget that shows live usage and rate-limit status for AI
subscriptions as a rail docked to the edge of the screen.

Each provider is a progress ring; hovering one opens a card with the current
session window, the weekly window, and when each resets. The rail sits above
other windows, has no taskbar entry, and passes clicks through everywhere except
the rail and the card themselves.

Claude, Codex, and OpenCode Go are supported today.

---

## Install

Download `Token Drain_<version>_x64-setup.exe` and run it. It installs for the
current user into `%LOCALAPPDATA%\Token Drain` and needs no administrator rights.

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
`%APPDATA%\dev.tokendrain.app` if you want it gone too.

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
| `%APPDATA%\dev.tokendrain.app\settings.json` | Your preferences |
| `%APPDATA%\dev.tokendrain.app\usage-cache.json` | Last successful figures, so the rail is not empty at launch |
| `%APPDATA%\dev.tokendrain.app\alerts.json` | Which thresholds have already been announced |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Only while *Launch at login* is on |

None of these contains a token. If the app data directory cannot be resolved for
any reason, it falls back to a folder under `%TEMP%` and keeps working with a
memory that does not survive a reboot.

### Hosts contacted

| Host | Why |
|---|---|
| `api.anthropic.com` | Claude usage |
| `chatgpt.com` | Codex usage |
| `github.com` | Optional signed-update metadata and release downloads, only when automatic updates are enabled |
| `objects.githubusercontent.com` | GitHub's release-asset host for the optional signed update download |

With automatic updates disabled, the app itself contacts only the two provider
hosts. If you explicitly enable automatic updates in **Settings → Updates**, the
native updater also contacts GitHub Releases at startup. It accepts and installs
only an artifact whose signature matches Token Drain's embedded public release
key. The app performs no telemetry, analytics, or crash reporting. This was
verified rather than assumed: a release build was run with every TCP connection
owned by its process sampled and checked against the addresses those two provider
hostnames resolve to.

One honest caveat. The app hosts Microsoft's **WebView2** runtime to draw its
interface, and that runtime opens its own HTTPS connections to Microsoft. It is
a separate process (`msedgewebview2.exe`) and not something this project wrote or
can fully switch off. The page it renders is bundled locally and requests
nothing. What has been done about it: the runtime is started with background
networking, component update, domain reliability, hyperlink auditing, sync,
crash reporting and client-side phishing detection all disabled, which measurably
removed one of the two endpoints originally observed. One connection remains, to
an address registered to Microsoft, and it could not be attributed to a named
service. Three things were tried: the system DNS cache stays empty because the
runtime resolves hostnames itself; forcing it onto the operating system's
resolver still yields no name; and pointing it at a local proxy yields no
request, because this connection bypasses the runtime's proxy settings and goes
out directly. Worth knowing if you route your browsing through a proxy and
expect this to follow.

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

**Exclusive-fullscreen games** cover the rail entirely, and it costs them
nothing to do so: with the rail running, a fullscreen application keeps its mode
and its frame rate. Borderless fullscreen — how most modern games run — does not
cover it.

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
npm run check:version # ensure release manifests agree
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

### Release checklist

1. Choose the next [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
   version and update it in `package.json`, `src-tauri/Cargo.toml`, and
   `src-tauri/tauri.conf.json`.
2. Run `npm run check:version`; it must report one version across all release
   manifests.
3. Move the user-visible entries from `[Unreleased]` in
   [`CHANGELOG.md`](CHANGELOG.md) into a dated release section using the
   `YYYY-MM-DD` format.
4. Run `npm run build`, then `npm run tauri build` to produce the installers.
5. Test the generated NSIS installer before publishing it. The build must have
   `TAURI_SIGNING_PRIVATE_KEY_PATH` set to the local private-key path and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` set in its environment; neither value
   belongs in a repository file.
6. In GitHub Actions, create the repository secrets
   `TAURI_SIGNING_PRIVATE_KEY` (the complete private-key file) and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep both secret; the public key in
   `src-tauri/tauri.conf.json` is intentionally safe to commit.
7. Push a tag named `v<version>`. The release workflow checks that the tag and
   manifests agree, signs the NSIS updater artifact, and publishes it with
   `latest.json`. It selects the NSIS installer for Windows updates.

### Automatic updates

Automatic updates are off by default. Enable **Settings → Updates → Download
and install updates automatically** to let the rail check the latest GitHub
Release at startup. If a newer signed release is available, it is downloaded,
Windows closes the app while the NSIS installer runs, and the app restarts.

---

## Stack

Tauri v2 · Rust · React · TypeScript · Vite

## License

MIT — see [LICENSE](LICENSE).
