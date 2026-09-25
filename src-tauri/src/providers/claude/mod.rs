//! Claude usage provider.

pub mod credentials;
pub mod fetch;
pub mod refresh;
pub mod types;

use std::path::PathBuf;

use reqwest::Client;

use crate::providers::error::UsageError;
use crate::providers::provider::UsageProvider;
use crate::providers::usage::{ProviderId, ProviderUsage};

use credentials::{default_credentials_path, read_credentials_from, ClaudeCredentials};
use fetch::fetch_claude_usage;
use refresh::{exchange_refresh_token, needs_refresh, write_renewed, WriteBack};

/// Reads the Claude CLI's stored token and queries the Claude usage endpoint,
/// renewing the token first when it has expired.
pub struct ClaudeProvider {
    client: Client,
    /// Overridable so tests can point at a fixture instead of the real home
    /// directory. `None` means the CLI's default location.
    credentials_path: Option<PathBuf>,
}

impl ClaudeProvider {
    /// The client is passed in rather than built here so every provider shares
    /// one connection pool across the whole polling lifetime.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            credentials_path: None,
        }
    }

    /// Point the provider at a specific credentials file.
    pub fn with_credentials_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.credentials_path = Some(path.into());
        self
    }

    /// The credentials file this provider reads, when it can be located.
    pub fn credentials_path(&self) -> Option<PathBuf> {
        self.resolve_credentials_path().ok()
    }

    fn resolve_credentials_path(&self) -> Result<PathBuf, UsageError> {
        match &self.credentials_path {
            Some(path) => Ok(path.clone()),
            None => Ok(default_credentials_path()?),
        }
    }
}

impl UsageProvider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    async fn fetch(&self) -> Result<ProviderUsage, UsageError> {
        let path = self.resolve_credentials_path()?;
        let mut credentials = read_credentials_from(&path)?;
        let now_ms = chrono::Utc::now().timestamp_millis();

        let mut renewed = false;
        if needs_refresh(credentials.expires_at, now_ms) && credentials.refresh_token.is_some() {
            credentials = self.renew(&path, credentials, now_ms).await?;
            renewed = true;
        }

        match fetch_claude_usage(&self.client, &credentials.access_token, now_ms).await {
            // A 401 with a token we believed valid: the expiry was unknown or
            // wrong. Renew once and retry; a second 401 is final.
            Err(UsageError::Unauthorized) if !renewed && credentials.refresh_token.is_some() => {
                let credentials = self.renew(&path, credentials, now_ms).await?;
                fetch_claude_usage(&self.client, &credentials.access_token, now_ms).await
            }
            result => result,
        }
    }
}

impl ClaudeProvider {
    /// Exchange the stored refresh token and persist the result.
    ///
    /// When another client renewed the file while the exchange was in flight,
    /// its tokens win and are re-read; ours are discarded unwritten.
    async fn renew(
        &self,
        path: &std::path::Path,
        credentials: ClaudeCredentials,
        now_ms: i64,
    ) -> Result<ClaudeCredentials, UsageError> {
        let Some(refresh_token) = credentials.refresh_token.as_deref() else {
            return Ok(credentials);
        };

        let tokens = exchange_refresh_token(&self.client, refresh_token, now_ms).await?;

        match write_renewed(path, refresh_token, &tokens)? {
            WriteBack::Written => Ok(ClaudeCredentials {
                access_token: tokens.access_token,
                refresh_token: tokens
                    .refresh_token
                    .or_else(|| credentials.refresh_token.clone()),
                expires_at: tokens.expires_at,
            }),
            WriteBack::Superseded => Ok(read_credentials_from(path)?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::http::build_client;

    #[test]
    fn reports_its_identity() {
        let provider = ClaudeProvider::new(build_client().expect("client"));
        assert_eq!(provider.id(), ProviderId::Claude);
    }

    #[tokio::test]
    async fn a_missing_credentials_file_fails_before_any_request() {
        // Why: no network call may be attempted when there is nothing to
        // authenticate with. This also keeps the normal suite offline.
        let path = std::env::temp_dir().join("token-drain-absent-claude-credentials.json");
        assert!(!path.exists(), "test precondition");

        let provider =
            ClaudeProvider::new(build_client().expect("client")).with_credentials_path(&path);

        let error = provider.fetch().await.expect_err("should fail");

        assert!(matches!(error, UsageError::MissingCredentials(_)));
    }
}
