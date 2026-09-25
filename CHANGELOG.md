# Changelog

Notable user-facing changes are listed by version, newest first.
This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Appearance setting to size the rail between 70% and 100%. Only the layout
  shrinks; text keeps its size. The rail now starts at 70%, including for
  existing installs, and keeps whatever size the user picks.
- Light theme, chosen under Appearance. Dark remains the default.

### Fixed

- Claude usage keeps working when Claude Code is only used through the Claude
  desktop app: an expired saved login is now renewed automatically instead of
  showing "sign in".
- A provider showing "sign in" recovers within seconds of its saved login
  changing (for example after the Codex desktop app renews it), instead of
  waiting up to 30 minutes.
- The rail can be moved all the way to the top of the screen, and the
  vertical position slider can be dragged and reaches both screen edges.

## [0.1.1] - 2026-09-18

### Security

- Updated the Windows HTTPS stack to include the rustls fix for
  RUSTSEC-2026-0285.

## [0.1.0] - 2026-09-13

First release of Token Drain for Windows.

### Added

- Claude subscription usage: five-hour and weekly quotas from the local
  Claude Code sign-in.
- Codex subscription usage: session and weekly limits from the local
  Codex sign-in.
- OpenCode Go subscription usage: five-hour, weekly, and monthly quotas
  from the local OpenCode Go connection.
- A docked desktop rail with hover details, reset times, and cached
  last-known usage when a provider is unavailable.
- Configurable refresh intervals, threshold notifications, and launch at login.
- Manual update checks and optional automatic updates using signed installers.
- The Outlet logo, a GitHub link in Settings, and per-subscription data guides.
- Bounded local diagnostic logs and an in-app shortcut for support reports.

### Fixed

- A failed settings write no longer changes the configuration used in memory.

### Security

- Provider credentials remain in the native backend and are read without
  modifying the original credential files.
- Downloaded updates must pass signature verification before installation.
