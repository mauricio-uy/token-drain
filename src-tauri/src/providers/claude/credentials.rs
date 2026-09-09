//! Reading the Claude CLI's stored OAuth credentials.
//!
//! The Claude Code CLI persists its OAuth blob to `~/.claude/.credentials.json`
//! under a `claudeAiOauth` key. This module reads that file and nothing else: it
//! performs no network access and never writes, so it cannot disturb the CLI's
//! own session.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::providers::credentials::{
    in_home_directory, normalize_token, parse_json, read_json_file, CredentialError,
};

/// A usable set of Claude OAuth credentials.
///
/// Deliberately does not derive `Debug`; see the manual implementation below.
#[derive(Clone)]
pub struct ClaudeCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix milliseconds, as stored by the CLI. Absent or unparseable means
    /// "expiry unknown", which callers treat as "attempt the request anyway".
    pub expires_at: Option<i64>,
}

/// Redacting `Debug`, so no accidental `{:?}` or `dbg!` can print a token.
impl fmt::Debug for ClaudeCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeCredentials")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuthBlob>,
}

#[derive(Deserialize)]
struct OAuthBlob {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

/// Default location of the CLI's credentials file.
pub fn default_credentials_path() -> Result<PathBuf, CredentialError> {
    in_home_directory(&[".claude", ".credentials.json"])
}

/// Read credentials from the CLI's default location.
pub fn read_credentials() -> Result<ClaudeCredentials, CredentialError> {
    read_credentials_from(&default_credentials_path()?)
}

/// Read credentials from an explicit path. Separate from [`read_credentials`]
/// so tests can point at a fixture without touching the real home directory.
pub fn read_credentials_from(path: &Path) -> Result<ClaudeCredentials, CredentialError> {
    let file: CredentialsFile = read_json_file(path)?;
    into_credentials(file, path)
}

/// Parse the credentials JSON. Pure, so the parsing rules are testable without
/// any filesystem involvement.
pub fn parse_credentials(raw: &str, path: &Path) -> Result<ClaudeCredentials, CredentialError> {
    let file: CredentialsFile = parse_json(raw, path)?;
    into_credentials(file, path)
}

fn into_credentials(
    file: CredentialsFile,
    path: &Path,
) -> Result<ClaudeCredentials, CredentialError> {
    let oauth = file
        .claude_ai_oauth
        .ok_or_else(|| CredentialError::MissingField {
            path: path.to_path_buf(),
            field: "claudeAiOauth block",
        })?;

    let access_token =
        normalize_token(oauth.access_token).ok_or_else(|| CredentialError::MissingField {
            path: path.to_path_buf(),
            field: "access token",
        })?;

    Ok(ClaudeCredentials {
        access_token,
        refresh_token: normalize_token(oauth.refresh_token),
        expires_at: oauth.expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a real token. Every fixture in this file uses obvious placeholders so
    /// no genuine credential can ever end up in the repository.
    const FAKE_ACCESS_TOKEN: &str = "fake-access-token-for-tests";
    const FAKE_REFRESH_TOKEN: &str = "fake-refresh-token-for-tests";

    fn fixture_path() -> PathBuf {
        PathBuf::from("test-fixture/.credentials.json")
    }

    #[test]
    fn parses_a_complete_credentials_blob() {
        let raw = format!(
            r#"{{
                "claudeAiOauth": {{
                    "accessToken": "{FAKE_ACCESS_TOKEN}",
                    "refreshToken": "{FAKE_REFRESH_TOKEN}",
                    "expiresAt": 1788000000000,
                    "scopes": ["user:inference"]
                }}
            }}"#
        );

        let credentials = parse_credentials(&raw, &fixture_path()).expect("should parse");

        assert_eq!(credentials.access_token, FAKE_ACCESS_TOKEN);
        assert_eq!(
            credentials.refresh_token.as_deref(),
            Some(FAKE_REFRESH_TOKEN)
        );
        assert_eq!(credentials.expires_at, Some(1788000000000));
    }

    #[test]
    fn tolerates_a_missing_refresh_token_and_expiry() {
        let raw = format!(r#"{{"claudeAiOauth": {{"accessToken": "{FAKE_ACCESS_TOKEN}"}}}}"#);

        let credentials = parse_credentials(&raw, &fixture_path()).expect("should parse");

        assert_eq!(credentials.access_token, FAKE_ACCESS_TOKEN);
        assert!(credentials.refresh_token.is_none());
        assert!(credentials.expires_at.is_none());
    }

    #[test]
    fn ignores_unknown_fields() {
        // Why: the CLI adds keys over time. An unknown field must never turn a
        // working install into a parse error.
        let raw = format!(
            r#"{{
                "claudeAiOauth": {{"accessToken": "{FAKE_ACCESS_TOKEN}", "somethingNew": 1}},
                "someOtherProvider": {{"token": "unrelated"}}
            }}"#
        );

        assert!(parse_credentials(&raw, &fixture_path()).is_ok());
    }

    #[test]
    fn rejects_a_blank_access_token() {
        let raw = r#"{"claudeAiOauth": {"accessToken": "   "}}"#;

        assert!(matches!(
            parse_credentials(raw, &fixture_path()),
            Err(CredentialError::MissingField { field: "access token", .. })
        ));
    }

    #[test]
    fn reports_a_missing_oauth_block() {
        let raw = r#"{"someOtherProvider": {"token": "unrelated"}}"#;

        assert!(matches!(
            parse_credentials(raw, &fixture_path()),
            Err(CredentialError::MissingField { field: "claudeAiOauth block", .. })
        ));
    }

    #[test]
    fn reports_a_missing_file() {
        let path = std::env::temp_dir().join("token-drain-does-not-exist-9d1c250a.json");
        assert!(!path.exists(), "test precondition");

        assert!(matches!(
            read_credentials_from(&path),
            Err(CredentialError::NotFound { .. })
        ));
    }

    #[test]
    fn reports_malformed_json() {
        let raw = r#"{"claudeAiOauth": {"accessToken": "#;

        let error = parse_credentials(raw, &fixture_path()).expect_err("should fail");

        assert!(matches!(error, CredentialError::Malformed { .. }));
        // The rendered error must not echo the file contents.
        assert!(!error.to_string().contains("accessToken"));
    }

    #[test]
    fn reading_never_modifies_the_file() {
        // Guards the read-only decision behaviorally rather than by convention.
        // This file belongs to the CLI: writing to it races the CLI's own token
        // rotation and can strand a single-use refresh token, signing the user
        // out of the tool they actually depend on.
        let path = std::env::temp_dir().join("token-drain-readonly-claude.json");
        let original = format!(
            r#"{{"claudeAiOauth": {{"accessToken": "{FAKE_ACCESS_TOKEN}", "expiresAt": 1}}}}"#
        );
        std::fs::write(&path, &original).expect("should write fixture");
        let before = std::fs::metadata(&path).expect("metadata").modified().ok();

        read_credentials_from(&path).expect("should read");

        let after_content = std::fs::read_to_string(&path).expect("should still exist");
        let after = std::fs::metadata(&path).expect("metadata").modified().ok();
        std::fs::remove_file(&path).ok();

        assert_eq!(after_content, original, "the credentials file was rewritten");
        assert_eq!(before, after, "the credentials file was touched");
    }

    #[test]
    fn debug_rendering_redacts_the_token() {
        let credentials = ClaudeCredentials {
            access_token: FAKE_ACCESS_TOKEN.to_owned(),
            refresh_token: Some(FAKE_REFRESH_TOKEN.to_owned()),
            expires_at: None,
        };

        let rendered = format!("{credentials:?}");

        assert!(!rendered.contains(FAKE_ACCESS_TOKEN));
        assert!(!rendered.contains(FAKE_REFRESH_TOKEN));
        assert!(rendered.contains("<redacted>"));
    }
}
