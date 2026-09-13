# Contributing to Token Drain

Thanks for taking the time to contribute. Token Drain is a Windows desktop
application, so changes should preserve the native/WebView boundary, keep
provider credentials in the Rust process, and avoid making undocumented
provider endpoints more fragile than necessary.

## Before you start

For a bug fix, search existing issues first and open an issue if the problem is
not already tracked. For a larger change, describe the proposed behavior in an
issue before investing in an implementation. Do not include credentials,
tokens, private release keys, or unredacted logs in an issue or pull request.

## Development setup

Use Windows 10 version 1809 or later (x64) or Windows 11, Node.js
`^20.19.0` or `>=22.12.0`, Rust 1.98.1 for the `x86_64-pc-windows-msvc`
target, the Visual Studio C++ workload, and a Windows SDK. WebView2 is required
to run the application.

```sh
npm ci
npm run tauri dev
```

Provider credentials are read from the provider CLIs' existing files. The app
does not create or refresh those credentials. See [docs/guide.md](docs/guide.md)
for the exact paths, network hosts, and development flags.

## Making a change

- Keep repository content and user-facing text in English.
- Keep provider-specific behavior inside its provider module and preserve the
  common provider interface.
- Never pass credentials to the WebView, write them to app-owned files, or log
  them.
- Add or update tests for behavior changes.
- Prefer focused commits and explain the user-visible effect in the pull
  request description.

Create a feature branch from `main`, make the change, and open a pull request.
Do not push release tags or alter published release assets from a pull request.

## Checks before opening a pull request

Run the same checks used by continuous integration:

```sh
npm ci
npm run check:version
npm run test:frontend
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
npm run tauri build -- --no-bundle
```

If a check cannot be run locally, say why in the pull request. Live provider
tests are opt-in and must never include their output with credentials or
personal account data.

## Pull requests

Use the pull request template and include the motivation, testing performed,
security or credential-handling impact, and any documentation changes. Keep a
pull request narrowly scoped so it can be reviewed independently. A maintainer
will merge only after required checks pass and review conversations are
resolved.

## Releases

Release preparation is maintainer-only. Follow the release checklist in
[docs/guide.md](docs/guide.md), keep signing secrets out of the repository, and
publish only from a verified stable tag.

## Security issues

Do not report vulnerabilities in a public issue or pull request. Use GitHub's
**Security → Report a vulnerability** flow, as described in [SECURITY.md](SECURITY.md).
