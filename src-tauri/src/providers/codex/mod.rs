//! Codex usage provider.

pub mod credentials;
pub mod fetch;
pub mod types;
pub mod windows;

use std::path::PathBuf;

use reqwest::Client;

use crate::providers::error::UsageError;
use crate::providers::provider::UsageProvider;
use crate::providers::usage::{ProviderId, ProviderUsage};

use credentials::{default_credentials_path, read_credentials_from};
use fetch::fetch_codex_usage;

/// Reads the Codex CLI's stored token and queries the Codex usage endpoint.
pub struct CodexProvider {
    client: Client,
    /// Overridable so tests can point at a fixture instead of the real home
    /// directory. `None` means the CLI's default location.
    credentials_path: Option<PathBuf>,
}

impl CodexProvider {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            credentials_path: None,
        }
    }

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

impl UsageProvider for CodexProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    async fn fetch(&self) -> Result<ProviderUsage, UsageError> {
        let path = self.resolve_credentials_path()?;
        let credentials = read_credentials_from(&path)?;
        let now_ms = chrono::Utc::now().timestamp_millis();

        fetch_codex_usage(&self.client, &credentials, now_ms).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::http::build_client;

    #[test]
    fn reports_its_identity() {
        let provider = CodexProvider::new(build_client().expect("client"));
        assert_eq!(provider.id(), ProviderId::Codex);
    }

    #[tokio::test]
    async fn a_missing_credentials_file_fails_before_any_request() {
        let path = std::env::temp_dir().join("token-drain-absent-codex-credentials.json");
        assert!(!path.exists(), "test precondition");

        let provider =
            CodexProvider::new(build_client().expect("client")).with_credentials_path(&path);

        let error = provider.fetch().await.expect_err("should fail");

        assert!(matches!(error, UsageError::MissingCredentials(_)));
    }
}
