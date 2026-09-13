# Security Policy

## Supported versions

The repository currently declares version `0.1.0` in its package, Cargo, and
Tauri manifests. Public release records, when available, are listed on the
[GitHub Releases](https://github.com/mauricio-uy/token-drain/releases) page.

During the pre-release period, security fixes target the current `0.1.x`
development line. After public releases begin, support is limited to the
latest stable release and, when it exists, the immediately preceding stable
release in the same major version. Older releases, development snapshots, and
modified builds are not supported.

## Scope

This policy covers the Windows desktop application and its release machinery,
including the Tauri/Rust native layer, the WebView and IPC boundary, packaged
installers, and the signed updater workflow.

In particular, report security issues involving:

- Reading local provider credentials, including Claude's
  `~/.claude/.credentials.json`, Codex's `~/.codex/auth.json` (or its
  `CODEX_HOME` equivalent), and OpenCode Go's local `auth.json`.
- Credential exposure through the WebView, IPC commands, UI, cache, errors, or
  other output; credentials must remain in the native layer and must not be
  written back to the provider files.
- Authenticated usage requests, including sending a credential to an
  unintended destination, unsafe redirects, or a provider response being
  treated as trusted data when it is not.
- The opt-in updater: its GitHub Release metadata and installer endpoint,
  Tauri signature verification, update installation, and the release workflow's
  handling of signing secrets.

Issues in a provider's service, a third-party dependency with no
Token Drain-specific impact, or a machine already controlled by an attacker are
outside this policy unless they create a vulnerability in the application or
its release process.

## Responsible reporting

GitHub **Private Vulnerability Reporting is enabled for this repository**. Use
the repository's **Security** tab and **Report a vulnerability** entry to submit
a private report. Do not use a public issue, pull request, discussion, or
release to report a vulnerability.

If private reporting is temporarily unavailable, do not include vulnerability
details in a public post. Contact the maintainers through the support channel
described in [SUPPORT.md](SUPPORT.md) and ask for a private reporting route.

A useful report includes the affected version or commit, Windows version,
installation or update path, reproduction steps, security impact, and
sanitized evidence. Keep the report limited to the information needed to
reproduce and fix the issue, and keep vulnerability details private until
coordinated disclosure is explicitly agreed.

## Secrets and public disclosure

Public disclosure of secrets or tokens is prohibited. Never publish API keys,
OAuth access or refresh tokens, credential-file contents, GitHub Actions
secrets, the updater signing private key or password, or unredacted logs,
screenshots, crash dumps, or configuration files that contain them. This
prohibition applies to issues, pull requests, discussions, releases, gists,
social media, and any other public channel.

Use placeholders and redact sensitive values before sending a private report.
If a secret may have been exposed, revoke or rotate it and report the exposure
without including the secret itself.
