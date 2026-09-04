//! Claude usage provider.

pub mod credentials;
pub mod fetch;
pub mod types;

use std::path::PathBuf;

use reqwest::Client;

use crate::providers::error::UsageError;
use crate::providers::provider::UsageProvider;
use crate::providers::usage::{ProviderId, ProviderUsage};

use credentials::{default_credentials_path, read_credentials_from};
use fetch::fetch_claude_usage;

/// Reads the Claude CLI's stored token and queries the Claude usage endpoint.
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
        let credentials = read_credentials_from(&path)?;
        let now_ms = chrono::Utc::now().timestamp_millis();

        fetch_claude_usage(&self.client, &credentials.access_token, now_ms).await
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
        let path = std::env::temp_dir().join("tok-ching-absent-claude-credentials.json");
        assert!(!path.exists(), "test precondition");

        let provider =
            ClaudeProvider::new(build_client().expect("client")).with_credentials_path(&path);

        let error = provider.fetch().await.expect_err("should fail");

        assert!(matches!(error, UsageError::MissingCredentials(_)));
    }
}
