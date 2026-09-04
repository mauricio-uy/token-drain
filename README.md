# tok-ching

A Windows desktop widget that shows live usage and rate-limit status for AI
subscriptions as a rail docked to the edge of the screen.

Each provider is a progress ring; hovering one opens a card with the current
session window, the weekly window, and when each resets.

> **Status: early development.** Nothing here is usable yet.

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

These files are read only. The app does not modify them.

### Hosts contacted

| Host | Why |
|---|---|
| `api.anthropic.com` | Claude usage |
| `chatgpt.com` | Codex usage |

That is the complete list. The app performs no telemetry, no analytics, and no
crash reporting.

### Where tokens live at runtime

Tokens are read and used entirely inside the Rust process. They are never passed
to the WebView, never written to the on-disk cache, and never logged. HTTP
failures are logged as a status code only.

---

## Caveats

**The usage endpoints this app calls are undocumented.** Providers do not
publish a public API for consumer-plan quota, and these interfaces can change or
disappear without notice. When that happens the app will show an error state
rather than stale or invented numbers, but it will need a code change to work
again.

If a provider CLI is not installed or not signed in, that provider is simply
shown as unavailable; the others keep working.

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
npm run tauri build # produce an installer
```

The first Rust build compiles the full dependency tree and takes several
minutes. Subsequent builds are incremental.

---

## Stack

Tauri v2 · Rust · React · TypeScript · Vite

## License

MIT — see [LICENSE](LICENSE).
