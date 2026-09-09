# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- OpenCode Go subscription usage, including five-hour, weekly, and monthly
  quota windows.
- A monochrome application mark, edge reveal, independent usage rings, and a
  floating settings shortcut.

### Changed

- Renamed the application from tok-ching to Token Drain.
- Made the settings panel compact, collapsible, and consistent with the dark
  application surface.
- Simplified OpenCode Go's compact badge while retaining its monthly quota in
  the hover card.

### Fixed

- Preserved the newest settings edit and usage event when asynchronous replies
  arrive out of order.
- Kept the rail and hover card correctly positioned on either screen edge.

### Removed

- OpenCode Zen workspace billing support and its local web-session credentials.

## [0.1.0] - 2026-09-05

### Added

- A Windows desktop rail showing Claude and Codex subscription usage.
- Read-only credential integration with the existing provider CLIs.
- Cached last-known usage, actionable failure states, threshold notifications,
  a tray menu, and launch-at-login settings.
- A contract watcher for the upstream usage integrations and unsigned Windows
  installers for personal use.

### Security

- Kept tokens in the Rust process, never wrote them to application data, and
  restricted network requests to the documented provider endpoints.
