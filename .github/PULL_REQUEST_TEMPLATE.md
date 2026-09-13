## Summary

Describe the user-visible change and why it is needed.

## Testing

- [ ] `npm run check:version`
- [ ] `npm run test:frontend`
- [ ] `npm run build`
- [ ] `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --locked`
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- [ ] `npm run tauri build -- --no-bundle`

If a check was not run, explain why.

## Review checklist

- [ ] Repository content and user-facing copy are in English.
- [ ] Tests and documentation were updated where appropriate.
- [ ] Credentials, tokens, private keys, and sensitive logs are not included.
- [ ] This pull request does not change published release assets or secrets.
