//! Reading the Claude CLI's stored OAuth credentials.
//!
//! The Claude Code CLI persists its OAuth blob to `~/.claude/.credentials.json`
//! under a `claudeAiOauth` key. This module reads that file and nothing else: it
//! performs no network access and never writes, so it cannot disturb the CLI's
//! own session.
//!
//! Security: the token value never appears in an error, a `Debug` rendering, or
//! any log line produced here.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Every distinct way reading the credentials can fail.
///
/// The variants are deliberately fine-grained: the caller renders a different
/// badge state for "you never signed in" than for "the file is corrupt", and
/// collapsing them would make both look like the same problem.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("could not determine the current user's home directory")]
    NoHomeDirectory,

    #[error("no credentials file at {path}")]
    NotFound { path: PathBuf },

    #[error("credentials file at {path} could not be read ({kind:?})")]
    Unreadable { path: PathBuf, kind: io::ErrorKind },

    /// Only the position of the syntax error is carried, never the surrounding
    /// text, so a malformed file can never leak token material into a log.
    #[error("credentials file at {path} is not valid JSON (line {line}, column {column})")]
    Malformed {
        path: PathBuf,
        line: usize,
        column: usize,
    },

    #[error("credentials file at {path} has no claudeAiOauth block")]
    MissingOAuthBlock { path: PathBuf },

    #[error("credentials file at {path} has no access token")]
    MissingAccessToken { path: PathBuf },
}

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
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
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
    let home = dirs::home_dir().ok_or(CredentialError::NoHomeDirectory)?;
    Ok(home.join(".claude").join(".credentials.json"))
}

/// Read credentials from the CLI's default location.
pub fn read_credentials() -> Result<ClaudeCredentials, CredentialError> {
    read_credentials_from(&default_credentials_path()?)
}

/// Read credentials from an explicit path. Separate from [`read_credentials`]
/// so tests can point at a fixture without touching the real home directory.
pub fn read_credentials_from(path: &Path) -> Result<ClaudeCredentials, CredentialError> {
    let raw = fs::read_to_string(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => CredentialError::NotFound {
            path: path.to_path_buf(),
        },
        kind => CredentialError::Unreadable {
            path: path.to_path_buf(),
            kind,
        },
    })?;

    parse_credentials(&raw, path)
}

/// Parse the credentials JSON. Pure, so the parsing rules can be tested without
/// any filesystem involvement.
///
/// `path` is only used to build error messages.
pub fn parse_credentials(raw: &str, path: &Path) -> Result<ClaudeCredentials, CredentialError> {
    let file: CredentialsFile =
        serde_json::from_str(raw).map_err(|error| CredentialError::Malformed {
            path: path.to_path_buf(),
            line: error.line(),
            column: error.column(),
        })?;

    let oauth = file
        .claude_ai_oauth
        .ok_or_else(|| CredentialError::MissingOAuthBlock {
            path: path.to_path_buf(),
        })?;

    // Why: a blank string is the same as absent for our purposes, and treating
    // it as a valid token would turn a clear "not signed in" into a confusing
    // 401 from the server.
    let access_token = oauth
        .access_token
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
        .ok_or_else(|| CredentialError::MissingAccessToken {
            path: path.to_path_buf(),
        })?;

    let refresh_token = oauth
        .refresh_token
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty());

    Ok(ClaudeCredentials {
        access_token,
        refresh_token,
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
        assert_eq!(credentials.refresh_token.as_deref(), Some(FAKE_REFRESH_TOKEN));
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
            Err(CredentialError::MissingAccessToken { .. })
        ));
    }

    #[test]
    fn reports_a_missing_oauth_block() {
        let raw = r#"{"someOtherProvider": {"token": "unrelated"}}"#;

        assert!(matches!(
            parse_credentials(raw, &fixture_path()),
            Err(CredentialError::MissingOAuthBlock { .. })
        ));
    }

    // --- The three filesystem error paths required by the plan ---

    #[test]
    fn reports_a_missing_file() {
        let path = std::env::temp_dir().join("tok-ching-does-not-exist-9d1c250a.json");
        assert!(!path.exists(), "test precondition");

        assert!(matches!(
            read_credentials_from(&path),
            Err(CredentialError::NotFound { .. })
        ));
    }

    #[test]
    fn reports_an_unreadable_file() {
        // A directory standing in for an unreadable file: the read fails with
        // an io error that is not NotFound, which is exactly the branch under
        // test and is reproducible on every platform.
        let dir = std::env::temp_dir().join("tok-ching-unreadable-fixture");
        fs::create_dir_all(&dir).expect("should create fixture directory");

        let result = read_credentials_from(&dir);

        fs::remove_dir_all(&dir).ok();

        assert!(
            matches!(result, Err(CredentialError::Unreadable { .. })),
            "expected Unreadable, got {result:?}"
        );
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
