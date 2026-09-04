//! Reading the Codex CLI's stored OAuth credentials.
//!
//! The Codex CLI persists its tokens to `~/.codex/auth.json` under a `tokens`
//! key, and honours a `CODEX_HOME` environment variable that relocates the whole
//! directory. Read-only, like the Claude reader: the file belongs to the CLI.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::providers::credentials::{
    in_home_directory, normalize_token, parse_json, read_json_file, CredentialError,
};

/// Environment variable that relocates the Codex configuration directory.
const CODEX_HOME_VAR: &str = "CODEX_HOME";

/// A usable set of Codex credentials.
///
/// Deliberately does not derive `Debug`; see the manual implementation below.
#[derive(Clone)]
pub struct CodexCredentials {
    pub access_token: String,
    /// Sent as `ChatGPT-Account-Id` when present. Absent on some installs, and
    /// the request is still valid without it.
    pub account_id: Option<String>,
}

impl fmt::Debug for CodexCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodexCredentials")
            .field("access_token", &"<redacted>")
            // The account id is an identifier, not a secret, but it is still
            // account-identifying, so it is not printed either.
            .field("account_id", &self.account_id.as_ref().map(|_| "<present>"))
            .finish()
    }
}

#[derive(Deserialize)]
struct AuthFile {
    tokens: Option<TokensBlob>,
}

#[derive(Deserialize)]
struct TokensBlob {
    access_token: Option<String>,
    account_id: Option<String>,
}

/// Directory holding the Codex CLI's configuration.
///
/// `CODEX_HOME` wins when set, matching the CLI's own resolution order, so an
/// install with a relocated config directory is not reported as "not signed in".
pub fn codex_home() -> Result<PathBuf, CredentialError> {
    match std::env::var(CODEX_HOME_VAR) {
        Ok(value) if !value.trim().is_empty() => Ok(PathBuf::from(value)),
        _ => in_home_directory(&[".codex"]),
    }
}

/// Default location of the CLI's auth file.
pub fn default_credentials_path() -> Result<PathBuf, CredentialError> {
    Ok(codex_home()?.join("auth.json"))
}

/// Read credentials from the CLI's default location.
pub fn read_credentials() -> Result<CodexCredentials, CredentialError> {
    read_credentials_from(&default_credentials_path()?)
}

/// Read credentials from an explicit path, so tests never touch the real home
/// directory.
pub fn read_credentials_from(path: &Path) -> Result<CodexCredentials, CredentialError> {
    let file: AuthFile = read_json_file(path)?;
    into_credentials(file, path)
}

/// Parse the auth JSON. Pure, so the parsing rules are testable without any
/// filesystem involvement.
pub fn parse_credentials(raw: &str, path: &Path) -> Result<CodexCredentials, CredentialError> {
    let file: AuthFile = parse_json(raw, path)?;
    into_credentials(file, path)
}

fn into_credentials(file: AuthFile, path: &Path) -> Result<CodexCredentials, CredentialError> {
    let tokens = file.tokens.ok_or_else(|| CredentialError::MissingField {
        path: path.to_path_buf(),
        field: "tokens block",
    })?;

    let access_token =
        normalize_token(tokens.access_token).ok_or_else(|| CredentialError::MissingField {
            path: path.to_path_buf(),
            field: "access token",
        })?;

    Ok(CodexCredentials {
        access_token,
        account_id: normalize_token(tokens.account_id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_ACCESS_TOKEN: &str = "fake-codex-access-token-for-tests";
    const FAKE_ACCOUNT_ID: &str = "fake-account-id-for-tests";

    fn fixture_path() -> PathBuf {
        PathBuf::from("test-fixture/auth.json")
    }

    #[test]
    fn parses_a_complete_auth_file() {
        let raw = format!(
            r#"{{
                "tokens": {{
                    "access_token": "{FAKE_ACCESS_TOKEN}",
                    "account_id": "{FAKE_ACCOUNT_ID}",
                    "id_token": "unused"
                }},
                "last_refresh": "2026-09-04T00:00:00Z"
            }}"#
        );

        let credentials = parse_credentials(&raw, &fixture_path()).expect("should parse");

        assert_eq!(credentials.access_token, FAKE_ACCESS_TOKEN);
        assert_eq!(credentials.account_id.as_deref(), Some(FAKE_ACCOUNT_ID));
    }

    #[test]
    fn tolerates_an_absent_account_id() {
        // Why: the header is only sent when present, and the request is valid
        // without it. Treating this as a failure would break those installs.
        let raw = format!(r#"{{"tokens": {{"access_token": "{FAKE_ACCESS_TOKEN}"}}}}"#);

        let credentials = parse_credentials(&raw, &fixture_path()).expect("should parse");

        assert_eq!(credentials.access_token, FAKE_ACCESS_TOKEN);
        assert!(credentials.account_id.is_none());
    }

    #[test]
    fn treats_a_blank_account_id_as_absent() {
        let raw = format!(
            r#"{{"tokens": {{"access_token": "{FAKE_ACCESS_TOKEN}", "account_id": "  "}}}}"#
        );

        let credentials = parse_credentials(&raw, &fixture_path()).expect("should parse");

        assert!(credentials.account_id.is_none());
    }

    #[test]
    fn ignores_unknown_fields() {
        let raw = format!(
            r#"{{"tokens": {{"access_token": "{FAKE_ACCESS_TOKEN}", "brand_new": true}}, "extra": 1}}"#
        );

        assert!(parse_credentials(&raw, &fixture_path()).is_ok());
    }

    #[test]
    fn reports_a_missing_tokens_block() {
        let raw = r#"{"last_refresh": "2026-09-04T00:00:00Z"}"#;

        assert!(matches!(
            parse_credentials(raw, &fixture_path()),
            Err(CredentialError::MissingField {
                field: "tokens block",
                ..
            })
        ));
    }

    #[test]
    fn rejects_a_blank_access_token() {
        let raw = r#"{"tokens": {"access_token": "   "}}"#;

        assert!(matches!(
            parse_credentials(raw, &fixture_path()),
            Err(CredentialError::MissingField {
                field: "access token",
                ..
            })
        ));
    }

    #[test]
    fn reports_a_missing_file() {
        let path = std::env::temp_dir().join("tok-ching-absent-codex-auth.json");
        assert!(!path.exists(), "test precondition");

        assert!(matches!(
            read_credentials_from(&path),
            Err(CredentialError::NotFound { .. })
        ));
    }

    #[test]
    fn reports_malformed_json_without_echoing_content() {
        let raw = r#"{"tokens": {"access_token": "#;

        let error = parse_credentials(raw, &fixture_path()).expect_err("should fail");

        assert!(matches!(error, CredentialError::Malformed { .. }));
        assert!(!error.to_string().contains("access_token"));
    }

    #[test]
    fn debug_rendering_redacts_the_token_and_account() {
        let credentials = CodexCredentials {
            access_token: FAKE_ACCESS_TOKEN.to_owned(),
            account_id: Some(FAKE_ACCOUNT_ID.to_owned()),
        };

        let rendered = format!("{credentials:?}");

        assert!(!rendered.contains(FAKE_ACCESS_TOKEN));
        assert!(!rendered.contains(FAKE_ACCOUNT_ID));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn the_auth_path_sits_under_the_codex_home() {
        // Not asserting the absolute path: the home directory differs per
        // machine. What matters is the shape the CLI defines.
        let path = default_credentials_path().expect("should resolve");

        assert!(path.ends_with("auth.json"));
        assert!(path.parent().unwrap().ends_with(".codex") || std::env::var(CODEX_HOME_VAR).is_ok());
    }
}
