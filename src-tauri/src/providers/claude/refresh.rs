//! Renewing an expired Claude access token.
//!
//! The Claude desktop app authenticates its embedded Claude Code through its
//! own token store and never touches `~/.claude/.credentials.json`. Someone who
//! only uses the desktop app therefore ends up with a file whose access token
//! expired long ago while its refresh token is still valid. Renewing it here —
//! the same exchange the CLI performs — keeps the widget working without the
//! CLI, and leaves the file fresh for the CLI too.
//!
//! Refresh tokens are single use: the exchange invalidates the one presented
//! and returns a replacement. The replacement must reach the file, or the next
//! exchange (ours or the CLI's) fails and the user has to sign in again. Hence:
//!
//! - the file is re-read immediately before writing, and left alone if its
//!   refresh token changed in the meantime (the CLI renewed it first);
//! - every field we do not own is preserved verbatim;
//! - the write goes to a sibling temporary file that is then renamed over the
//!   original, so a crash can never leave a half-written credentials file.
//!
//! Security (S2, S6): tokens go to the token endpoint and nowhere else, and no
//! header or body is ever logged.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::providers::credentials::{normalize_token, CredentialError};
use crate::providers::error::UsageError;
use crate::providers::http::{classify_transport_error, parse_retry_after, retry_after_header};

/// OAuth token endpoint of the Claude Code client.
const TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";

/// Public OAuth client id of Claude Code. Refresh tokens are bound to the
/// client that obtained them, so this must match the CLI's.
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// Renew this long before the recorded expiry, so a token does not expire
/// between the check and the usage request.
pub const EXPIRY_SKEW_MS: i64 = 5 * 60 * 1000;

/// Whether a token with this expiry should be renewed before use.
///
/// An unknown expiry is treated as valid: the usage request will tell us with a
/// 401 if it is not, and that path renews too.
pub fn needs_refresh(expires_at: Option<i64>, now_ms: i64) -> bool {
    expires_at.is_some_and(|expires_at| expires_at <= now_ms.saturating_add(EXPIRY_SKEW_MS))
}

#[derive(Serialize)]
struct RefreshRequest<'a> {
    grant_type: &'static str,
    refresh_token: &'a str,
    client_id: &'static str,
}

/// The fields of a token response this module uses.
#[derive(Deserialize)]
pub struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    /// Lifetime of the replacement refresh token, when the server states it.
    refresh_token_expires_in: Option<i64>,
}

/// A renewed token set, ready to be written back.
pub struct RenewedTokens {
    pub access_token: String,
    /// The replacement refresh token. Absent when the server kept the old one.
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    /// When the replacement refresh token expires, if the server said.
    pub refresh_token_expires_at: Option<i64>,
}

impl TokenResponse {
    /// Validate the response. A response without an access token is a
    /// contract change, not a success.
    pub fn into_renewed(self, now_ms: i64) -> Result<RenewedTokens, UsageError> {
        let access_token = normalize_token(self.access_token).ok_or(UsageError::Parse)?;
        let refresh_token = normalize_token(self.refresh_token);
        let expiry = |seconds: Option<i64>| {
            seconds
                .filter(|seconds| *seconds > 0)
                .map(|seconds| now_ms.saturating_add(seconds.saturating_mul(1000)))
        };
        Ok(RenewedTokens {
            access_token,
            // Only meaningful alongside a replacement: a lifetime with no new
            // refresh token would be attached to the old one.
            refresh_token_expires_at: refresh_token
                .as_ref()
                .and_then(|_| expiry(self.refresh_token_expires_in)),
            refresh_token,
            expires_at: expiry(self.expires_in),
        })
    }
}

/// Exchange a refresh token for a new token set.
pub async fn exchange_refresh_token(
    client: &Client,
    refresh_token: &str,
    now_ms: i64,
) -> Result<RenewedTokens, UsageError> {
    let response = client
        .post(TOKEN_URL)
        .json(&RefreshRequest {
            grant_type: "refresh_token",
            refresh_token,
            client_id: CLIENT_ID,
        })
        .send()
        .await
        .map_err(classify_transport_error)?;

    let status = response.status().as_u16();
    if let Some(error) = classify_refresh_status(status, retry_after_header(&response), now_ms) {
        return Err(error);
    }

    let payload: TokenResponse = response.json().await.map_err(|_| UsageError::Parse)?;
    payload.into_renewed(now_ms)
}

/// Map a token endpoint status onto an error, or `None` on success.
///
/// OAuth reports a revoked or expired refresh token as 400 `invalid_grant`, so
/// 400 and 401 both mean "sign in again" rather than "the server is broken".
pub fn classify_refresh_status(
    status: u16,
    retry_after: Option<String>,
    now_ms: i64,
) -> Option<UsageError> {
    match status {
        200..=299 => None,
        400 | 401 => Some(UsageError::Unauthorized),
        429 => Some(UsageError::RateLimited {
            retry_after_ms: retry_after.and_then(|value| parse_retry_after(&value, now_ms)),
        }),
        other => Some(UsageError::Server { status: other }),
    }
}

/// What writing a renewed token set back to disk achieved.
#[derive(Debug, PartialEq, Eq)]
pub enum WriteBack {
    /// The file now holds the renewed tokens.
    Written,
    /// The file's refresh token changed since it was read: another client
    /// renewed it first. The file was left untouched.
    Superseded,
}

/// Merge renewed tokens into the raw credentials JSON.
///
/// Returns `None` when the file no longer holds `used_refresh_token`. Pure, so
/// the merge rules are testable without touching the filesystem. Every field
/// other than the three renewed ones is preserved.
pub fn merge_renewed(
    raw: &str,
    used_refresh_token: &str,
    renewed: &RenewedTokens,
    path: &Path,
) -> Result<Option<String>, CredentialError> {
    let mut document: Value =
        serde_json::from_str(raw).map_err(|error| CredentialError::Malformed {
            path: path.to_path_buf(),
            line: error.line(),
            column: error.column(),
        })?;

    let oauth = document
        .get_mut("claudeAiOauth")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| CredentialError::MissingField {
            path: path.to_path_buf(),
            field: "claudeAiOauth block",
        })?;

    let current = oauth
        .get("refreshToken")
        .and_then(Value::as_str)
        .map(str::trim);
    if current != Some(used_refresh_token) {
        return Ok(None);
    }

    oauth.insert(
        "accessToken".to_owned(),
        Value::String(renewed.access_token.clone()),
    );
    if let Some(refresh_token) = &renewed.refresh_token {
        oauth.insert(
            "refreshToken".to_owned(),
            Value::String(refresh_token.clone()),
        );
    }
    if let Some(expires_at) = renewed.expires_at {
        oauth.insert("expiresAt".to_owned(), Value::from(expires_at));
    }
    // Without a stated lifetime the stored one is left as it was: the server
    // may not report it, and guessing could only make it less accurate.
    if let Some(expires_at) = renewed.refresh_token_expires_at {
        oauth.insert("refreshTokenExpiresAt".to_owned(), Value::from(expires_at));
    }

    serde_json::to_string(&document)
        .map(Some)
        .map_err(|_| CredentialError::Unreadable {
            path: path.to_path_buf(),
            kind: io::ErrorKind::InvalidData,
        })
}

/// Write renewed tokens back to the credentials file.
pub fn write_renewed(
    path: &Path,
    used_refresh_token: &str,
    renewed: &RenewedTokens,
) -> Result<WriteBack, CredentialError> {
    let unreadable = |kind| CredentialError::Unreadable {
        path: path.to_path_buf(),
        kind,
    };

    let raw = fs::read_to_string(path).map_err(|error| unreadable(error.kind()))?;
    let Some(updated) = merge_renewed(&raw, used_refresh_token, renewed, path)? else {
        return Ok(WriteBack::Superseded);
    };

    replace_file(path, updated.as_bytes()).map_err(|error| unreadable(error.kind()))?;
    Ok(WriteBack::Written)
}

/// Replace a file's contents atomically: write a sibling, then rename it over
/// the original. `rename` replaces an existing target on every platform we
/// ship on, so readers see either the old file or the new one, never a mix.
fn replace_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let temporary = sibling_temporary(path);

    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();

    if result.is_err() {
        fs::remove_file(&temporary).ok();
    }
    result
}

fn sibling_temporary(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(format!(".tok-ching-{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_788_547_260_000;
    const OLD_REFRESH: &str = "fake-old-refresh-token";
    const NEW_REFRESH: &str = "fake-new-refresh-token";
    const NEW_ACCESS: &str = "fake-new-access-token";

    fn fixture_path() -> PathBuf {
        PathBuf::from("test-fixture/.credentials.json")
    }

    fn renewed() -> RenewedTokens {
        RenewedTokens {
            access_token: NEW_ACCESS.to_owned(),
            refresh_token: Some(NEW_REFRESH.to_owned()),
            expires_at: Some(NOW_MS + 3_600_000),
            refresh_token_expires_at: None,
        }
    }

    fn credentials_file(refresh_token: &str) -> String {
        format!(
            r#"{{
                "claudeAiOauth": {{
                    "accessToken": "fake-old-access-token",
                    "refreshToken": "{refresh_token}",
                    "expiresAt": 1,
                    "scopes": ["user:inference"],
                    "subscriptionType": "max"
                }},
                "otherTool": {{"kept": true}}
            }}"#
        )
    }

    #[test]
    fn renews_expired_and_nearly_expired_tokens_only() {
        assert!(needs_refresh(Some(NOW_MS - 1), NOW_MS));
        assert!(needs_refresh(Some(NOW_MS + EXPIRY_SKEW_MS - 1), NOW_MS));
        assert!(!needs_refresh(
            Some(NOW_MS + EXPIRY_SKEW_MS + 60_000),
            NOW_MS
        ));
        // Unknown expiry: let the usage request decide.
        assert!(!needs_refresh(None, NOW_MS));
    }

    #[test]
    fn converts_a_token_response_into_an_absolute_expiry() {
        let response: TokenResponse = serde_json::from_str(&format!(
            r#"{{"access_token": "{NEW_ACCESS}", "refresh_token": "{NEW_REFRESH}", "expires_in": 28800}}"#
        ))
        .expect("should parse");

        let renewed = response.into_renewed(NOW_MS).expect("should be usable");

        assert_eq!(renewed.access_token, NEW_ACCESS);
        assert_eq!(renewed.refresh_token.as_deref(), Some(NEW_REFRESH));
        assert_eq!(renewed.expires_at, Some(NOW_MS + 28_800_000));
    }

    #[test]
    fn a_stated_refresh_token_lifetime_is_recorded() {
        let response: TokenResponse = serde_json::from_str(&format!(
            r#"{{"access_token": "{NEW_ACCESS}", "refresh_token": "{NEW_REFRESH}",
                "expires_in": 60, "refresh_token_expires_in": 86400}}"#
        ))
        .expect("should parse");

        let renewed = response.into_renewed(NOW_MS).expect("should be usable");
        assert_eq!(renewed.refresh_token_expires_at, Some(NOW_MS + 86_400_000));

        let merged = merge_renewed(
            &credentials_file(OLD_REFRESH),
            OLD_REFRESH,
            &renewed,
            &fixture_path(),
        )
        .expect("should merge")
        .expect("the refresh token still matches");
        let document: Value = serde_json::from_str(&merged).expect("valid JSON");
        assert_eq!(
            document["claudeAiOauth"]["refreshTokenExpiresAt"],
            NOW_MS + 86_400_000
        );
    }

    #[test]
    fn a_refresh_token_lifetime_without_a_replacement_is_ignored() {
        let response: TokenResponse = serde_json::from_str(&format!(
            r#"{{"access_token": "{NEW_ACCESS}", "refresh_token_expires_in": 86400}}"#
        ))
        .expect("should parse");

        let renewed = response.into_renewed(NOW_MS).expect("should be usable");
        assert_eq!(renewed.refresh_token_expires_at, None);
    }

    #[test]
    fn a_token_response_without_an_access_token_is_a_contract_failure() {
        let response: TokenResponse =
            serde_json::from_str(r#"{"expires_in": 60}"#).expect("should parse");

        assert!(matches!(
            response.into_renewed(NOW_MS),
            Err(UsageError::Parse)
        ));
    }

    #[test]
    fn a_rejected_refresh_token_means_signing_in_again() {
        for status in [400, 401] {
            assert!(matches!(
                classify_refresh_status(status, None, NOW_MS),
                Some(UsageError::Unauthorized)
            ));
        }
        assert!(classify_refresh_status(200, None, NOW_MS).is_none());
        assert!(matches!(
            classify_refresh_status(503, None, NOW_MS),
            Some(UsageError::Server { status: 503 })
        ));
    }

    #[test]
    fn merging_replaces_the_tokens_and_preserves_everything_else() {
        let merged = merge_renewed(
            &credentials_file(OLD_REFRESH),
            OLD_REFRESH,
            &renewed(),
            &fixture_path(),
        )
        .expect("should merge")
        .expect("the refresh token still matches");

        let document: Value = serde_json::from_str(&merged).expect("valid JSON");
        let oauth = &document["claudeAiOauth"];
        assert_eq!(oauth["accessToken"], NEW_ACCESS);
        assert_eq!(oauth["refreshToken"], NEW_REFRESH);
        assert_eq!(oauth["expiresAt"], NOW_MS + 3_600_000);
        assert_eq!(oauth["scopes"][0], "user:inference");
        assert_eq!(oauth["subscriptionType"], "max");
        assert_eq!(document["otherTool"]["kept"], true);
    }

    #[test]
    fn merging_keeps_the_old_refresh_token_when_none_was_issued() {
        let mut tokens = renewed();
        tokens.refresh_token = None;

        let merged = merge_renewed(
            &credentials_file(OLD_REFRESH),
            OLD_REFRESH,
            &tokens,
            &fixture_path(),
        )
        .expect("should merge")
        .expect("the refresh token still matches");

        let document: Value = serde_json::from_str(&merged).expect("valid JSON");
        assert_eq!(document["claudeAiOauth"]["refreshToken"], OLD_REFRESH);
    }

    #[test]
    fn merging_backs_off_when_another_client_renewed_first() {
        // The CLI rotated the token while our request was in flight. Its token
        // is the live one; overwriting it would sign the CLI out.
        let merged = merge_renewed(
            &credentials_file("fake-token-the-cli-just-wrote"),
            OLD_REFRESH,
            &renewed(),
            &fixture_path(),
        )
        .expect("should parse");

        assert!(merged.is_none());
    }

    #[test]
    fn writing_back_replaces_the_file_atomically() {
        let path = std::env::temp_dir().join("token-drain-claude-refresh-write.json");
        fs::write(&path, credentials_file(OLD_REFRESH)).expect("should write fixture");

        let outcome = write_renewed(&path, OLD_REFRESH, &renewed()).expect("should write");
        let written = fs::read_to_string(&path).expect("should still exist");
        let leftover = sibling_temporary(&path).exists();
        fs::remove_file(&path).ok();

        assert_eq!(outcome, WriteBack::Written);
        assert!(written.contains(NEW_ACCESS));
        assert!(written.contains(NEW_REFRESH));
        assert!(!leftover, "the temporary file was left behind");
    }

    #[test]
    fn writing_back_leaves_a_superseded_file_untouched() {
        let path = std::env::temp_dir().join("token-drain-claude-refresh-superseded.json");
        let original = credentials_file("fake-token-the-cli-just-wrote");
        fs::write(&path, &original).expect("should write fixture");

        let outcome = write_renewed(&path, OLD_REFRESH, &renewed()).expect("should read");
        let after = fs::read_to_string(&path).expect("should still exist");
        fs::remove_file(&path).ok();

        assert_eq!(outcome, WriteBack::Superseded);
        assert_eq!(after, original);
    }
}
